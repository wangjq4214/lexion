//! Authenticated discovery, pairing and bounded direct-peer learning exchange.
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::wordbooks::repository::replay::{DeviceId, Envelope};
use crate::wordbooks::WordbookRepository;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use snow::{Builder, TransportState};
use tauri::{Emitter, State};

const SERVICE: &str = "_lexion-pair._tcp.local.";
const NOISE: &str = "Noise_XX_25519_ChaChaPoly_SHA256";
const TIMEOUT: Duration = Duration::from_secs(120);
const IO_TIMEOUT: Duration = Duration::from_millis(250);
const MAX_FRAME: usize = 4096;
const MAX_SYNC_FRAME: usize = 60_000;
const MAX_SYNC_MESSAGE: usize = 16 * 1024 * 1024;
const MAX_SYNC_CHANGES: usize = 128;
const MAX_SESSIONS: usize = 8;

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Serialize)]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub trusted: bool,
}
#[derive(Clone, Serialize)]
pub struct PendingView {
    pub id: String,
    pub name: String,
    pub code: String,
}
#[derive(Clone, Serialize)]
pub struct TrustedView {
    pub id: String,
    pub name: String,
}
#[derive(Serialize)]
pub struct LanStatus {
    pub peers: Vec<Peer>,
    pub pending: Vec<PendingView>,
    pub trusted: Vec<TrustedView>,
    pub error: Option<String>,
    pub sync: Vec<SyncView>,
}
#[derive(Clone, Serialize)]
pub struct SyncView {
    pub id: String,
    pub state: String,
    pub detail: Option<String>,
    pub last_sync: Option<String>,
}

#[derive(Clone)]
struct Discovered {
    fullname: String,
    name: String,
    addresses: Vec<SocketAddr>,
}
struct Pending {
    name: String,
    code: String,
    decision: Option<bool>,
    expires: Instant,
}
struct Shared {
    discovered: HashMap<String, Discovered>,
    pending: HashMap<String, Pending>,
    trusted: HashMap<String, String>,
    error: Option<String>,
    authenticated: std::collections::HashSet<String>,
    reconnect_attempts: HashMap<String, Instant>,
    sync: HashMap<String, SyncView>,
    active_sessions: usize,
    sync_ids: HashMap<String, String>,
}
pub struct LanService {
    local_id: String,
    key: [u8; 32],
    trust_path: PathBuf,
    state: Arc<Mutex<Shared>>,
    repository: Option<WordbookRepository>,
    app: Option<tauri::AppHandle>,
}
pub struct LanBackend {
    service: Option<Arc<LanService>>,
    error: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Identity {
    key: String,
}
#[derive(Serialize, Deserialize)]
struct TrustFile {
    peers: HashMap<String, String>,
    #[serde(default)]
    sync_ids: HashMap<String, String>,
}

fn identity_id(key: &[u8]) -> String {
    hex::encode(Sha256::digest(key))
}
/// Replay origin is verifiably tied to the authenticated Noise static public key.
fn replay_origin(key: &[u8]) -> DeviceId {
    DeviceId(identity_id(key)[..32].to_owned())
}
fn pairing_code(hash: &[u8]) -> String {
    let number = u64::from_be_bytes(hash[..8].try_into().expect("Noise hash")) % 100_000_000;
    format!("{number:08}")
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut suffix = [0u8; 16];
    getrandom::fill(&mut suffix).map_err(|e| e.to_string())?;
    let temp = path.with_extension(format!("tmp-{}", hex::encode(suffix)));
    let result = (|| -> std::io::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(|e| e.to_string())
}
fn load_identity(dir: &Path) -> Result<[u8; 32]> {
    let path = dir.join("lan-identity.json");
    let key_hex = if path.exists() {
        serde_json::from_slice::<Identity>(&fs::read(&path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?
            .key
    } else {
        let keypair = Builder::new(NOISE.parse().map_err(|e: snow::Error| e.to_string())?)
            .generate_keypair()
            .map_err(|e| e.to_string())?;
        let key_hex = hex::encode(keypair.private);
        let bytes = serde_json::to_vec(&Identity {
            key: key_hex.clone(),
        })
        .map_err(|e| e.to_string())?;
        let mut suffix = [0u8; 16];
        getrandom::fill(&mut suffix).map_err(|e| e.to_string())?;
        let temp = path.with_extension(format!("tmp-{}", hex::encode(suffix)));
        let result = (|| -> std::io::Result<()> {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            // Install only if absent: two app instances must never rotate each other's key.
            fs::hard_link(&temp, &path)
        })();
        let _ = fs::remove_file(&temp);
        match result {
            Ok(()) => key_hex,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return load_identity(dir),
            Err(e) => return Err(e.to_string()),
        }
    };
    let bytes = hex::decode(key_hex).map_err(|e| e.to_string())?;
    bytes
        .try_into()
        .map_err(|_| "Invalid LAN identity key length".to_string())
}
fn load_trust(path: &Path) -> Result<TrustFile> {
    if !path.exists() {
        return Ok(TrustFile {
            peers: HashMap::new(),
            sync_ids: HashMap::new(),
        });
    }
    let file = serde_json::from_slice::<TrustFile>(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let peers = &file.peers;
    if peers
        .keys()
        .any(|id| hex::decode(id).map_or(true, |key| key.len() != 32))
    {
        return Err("Invalid LAN trust identity".into());
    }
    if file
        .sync_ids
        .values()
        .collect::<std::collections::HashSet<_>>()
        .len()
        != file.sync_ids.len()
        || file.sync_ids.iter().any(|(key, id)| {
            !peers.keys().any(|trusted_key| {
                hex::decode(trusted_key).is_ok_and(|bytes| identity_id(&bytes) == *key)
            }) || id.len() != 32
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        })
    {
        return Err("Invalid pinned replay identity".into());
    }
    Ok(file)
}
impl LanService {
    #[cfg(test)]
    fn open(dir: &Path) -> Result<Arc<Self>> {
        Self::open_with_repository(dir, None, None)
    }
    fn open_with_repository(
        dir: &Path,
        repository: Option<WordbookRepository>,
        app: Option<tauri::AppHandle>,
    ) -> Result<Arc<Self>> {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let key = load_identity(dir)?;
        let trust_path = dir.join("lan-trust.json");
        let trust = load_trust(&trust_path)?;
        if let Some(repo) = &repository {
            let public = snow_public(&key)?;
            repo.bind_replay_origin(&replay_origin(&public))
                .map_err(|e| format!("LAN replay origin binding: {e:?}"))?;
        }
        Ok(Arc::new(Self {
            local_id: identity_id(&snow_public(&key)?),
            key,
            trust_path,
            state: Arc::new(Mutex::new(Shared {
                discovered: HashMap::new(),
                pending: HashMap::new(),
                trusted: trust.peers,
                error: None,
                authenticated: Default::default(),
                reconnect_attempts: HashMap::new(),
                sync: HashMap::new(),
                active_sessions: 0,
                sync_ids: trust.sync_ids,
            })),
            repository,
            app,
        }))
    }
    fn status(&self) -> LanStatus {
        let state = self.state.lock().unwrap();
        let mut peers: Vec<_> = state
            .discovered
            .iter()
            .map(|(id, peer)| Peer {
                id: id.clone(),
                name: peer.name.clone(),
                trusted: state.authenticated.contains(id)
                    && state
                        .trusted
                        .keys()
                        .any(|key| hex::decode(key).is_ok_and(|bytes| identity_id(&bytes) == *id)),
            })
            .collect();
        peers.sort_by(|a, b| a.id.cmp(&b.id));
        let mut pending: Vec<_> = state
            .pending
            .iter()
            .filter(|(_, p)| p.expires > Instant::now())
            .map(|(id, p)| PendingView {
                id: id.clone(),
                name: p.name.clone(),
                code: p.code.clone(),
            })
            .collect();
        pending.sort_by(|a, b| a.id.cmp(&b.id));
        let mut trusted: Vec<_> = state
            .trusted
            .iter()
            .map(|(id, name)| TrustedView {
                id: id.clone(),
                name: name.clone(),
            })
            .collect();
        trusted.sort_by(|a, b| a.id.cmp(&b.id));
        let mut sync: Vec<_> = state.sync.values().cloned().collect();
        // Sync state is keyed by the Noise key hash; trusted views use the raw key.
        for view in &mut sync {
            if let Some(key) = state
                .trusted
                .keys()
                .find(|key| hex::decode(key).is_ok_and(|bytes| identity_id(&bytes) == view.id))
            {
                view.id = key.clone();
            }
        }
        sync.sort_by(|a, b| a.id.cmp(&b.id));
        LanStatus {
            peers,
            pending,
            trusted,
            error: state.error.clone(),
            sync,
        }
    }
    fn start(self: &Arc<Self>) -> Result<()> {
        let listener = TcpListener::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let mdns = ServiceDaemon::new().map_err(|e| e.to_string())?;
        let hostname = format!("lexion-{}.local.", &self.local_id[..12]);
        let display_name = local_name();
        let service = ServiceInfo::new(
            SERVICE,
            &self.local_id[..12],
            &hostname,
            "",
            port,
            &[
                ("id", self.local_id.as_str()),
                ("name", display_name.as_str()),
            ][..],
        )
        .map_err(|e| e.to_string())?
        .enable_addr_auto();
        mdns.register(service).map_err(|e| e.to_string())?;
        let events = mdns.browse(SERVICE).map_err(|e| e.to_string())?;
        let owner = self.clone();
        thread::spawn(move || {
            // Keep daemon alive for the duration of discovery.
            let _daemon = mdns;
            loop {
                let event = events.recv_timeout(Duration::from_secs(10));
                if event.is_err() && events.is_disconnected() {
                    break;
                }
                let mut state = owner.state.lock().unwrap();
                match event {
                    Ok(ServiceEvent::ServiceResolved(info)) => {
                        let Some(id) = info.get_property_val_str("id") else {
                            continue;
                        };
                        if id == owner.local_id || hex::decode(id).map_or(true, |v| v.len() != 32) {
                            continue;
                        }
                        let addresses = info
                            .get_addresses()
                            .iter()
                            .map(|ip| SocketAddr::new(*ip, info.get_port()))
                            .collect();
                        state.discovered.insert(
                            id.to_string(),
                            Discovered {
                                fullname: info.get_fullname().into(),
                                name: info
                                    .get_property_val_str("name")
                                    .unwrap_or(id)
                                    .chars()
                                    .take(80)
                                    .collect(),
                                addresses,
                            },
                        );
                    }
                    Ok(ServiceEvent::ServiceRemoved(_, fullname)) => {
                        remove_discovered(&mut state, &fullname);
                    }
                    _ => {}
                }
                // Retry cached, directly trusted discoveries after outages even if mDNS
                // does not emit another resolved event. Every attempt verifies the Noise key.
                let trusted_ids: Vec<_> = state
                    .trusted
                    .keys()
                    .filter_map(|key| hex::decode(key).ok().map(|bytes| identity_id(&bytes)))
                    .collect();
                for id in trusted_ids {
                    if state.discovered.contains_key(&id)
                        && state
                            .reconnect_attempts
                            .get(&id)
                            .is_none_or(|last| last.elapsed() >= Duration::from_secs(30))
                    {
                        state.reconnect_attempts.insert(id.clone(), Instant::now());
                        let owner = owner.clone();
                        thread::spawn(move || {
                            let _ = owner.pair(&id);
                        });
                    }
                }
            }
            owner.state.lock().unwrap().error = Some("LAN discovery stopped".into());
        });
        let owner = self.clone();
        thread::spawn(move || {
            for connection in listener.incoming() {
                match connection {
                    Ok(stream) => {
                        let owner = owner.clone();
                        thread::spawn(move || {
                            let _ = owner.session(stream, false, None);
                        });
                    }
                    Err(e) => {
                        owner.state.lock().unwrap().error = Some(format!("LAN listener: {e}"));
                        break;
                    }
                }
            }
        });
        Ok(())
    }
    fn pair(self: &Arc<Self>, peer_id: &str) -> Result<()> {
        let peer = self
            .state
            .lock()
            .unwrap()
            .discovered
            .get(peer_id)
            .cloned()
            .ok_or("Peer is not discovered")?;
        let addresses = peer.addresses;
        let expected = peer_id.to_string();
        let owner = self.clone();
        thread::spawn(move || {
            let mut last_error = "No reachable address".to_string();
            for address in addresses {
                match TcpStream::connect_timeout(&address, Duration::from_secs(3)) {
                    Ok(stream) => {
                        match owner.session(stream, true, Some(&expected)) {
                            Ok(()) => {
                                owner.state.lock().unwrap().error = None;
                                return;
                            }
                            Err(e) if e.starts_with("Pairing decision: ") => {
                                owner.state.lock().unwrap().error = Some(e);
                                return; // Never restart a cancelled or failed user decision.
                            }
                            Err(e) => last_error = e,
                        }
                    }
                    Err(e) => last_error = e.to_string(),
                }
            }
            owner.state.lock().unwrap().error =
                Some(format!("Pairing connection failed: {last_error}"));
        });
        Ok(())
    }
    fn decide(&self, id: &str, code: Option<&str>) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        let pending = state
            .pending
            .get_mut(id)
            .ok_or("Pairing is no longer pending")?;
        if pending.expires <= Instant::now() {
            return Err("Pairing expired".into());
        }
        if code.is_none() {
            // A user may withdraw consent while waiting for the remote confirmation.
            pending.decision = Some(false);
            return Ok(());
        }
        if pending.decision.is_some() {
            return Err("Pairing already decided".into());
        }
        pending.decision = Some(code == Some(pending.code.as_str()));
        if pending.decision == Some(false) {
            return Err("Pairing code does not match; request cancelled".into());
        }
        Ok(())
    }
    fn trust(&self, key: &[u8], name: &str) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        let mut peers = state.trusted.clone();
        peers.insert(hex::encode(key), name.to_owned());
        atomic_write(
            &self.trust_path,
            &serde_json::to_vec(&TrustFile {
                peers: peers.clone(),
                sync_ids: state.sync_ids.clone(),
            })
            .map_err(|e| e.to_string())?,
        )?;
        state.trusted = peers;
        Ok(())
    }
    fn session(&self, stream: TcpStream, initiator: bool, expected: Option<&str>) -> Result<()> {
        {
            let mut state = self.state.lock().unwrap();
            if state.active_sessions >= MAX_SESSIONS {
                return Err("Too many LAN sessions".into());
            }
            state.active_sessions += 1;
        }
        struct SessionGuard(Arc<Mutex<Shared>>);
        impl Drop for SessionGuard {
            fn drop(&mut self) {
                self.0.lock().unwrap().active_sessions -= 1;
            }
        }
        let _guard = SessionGuard(self.state.clone());
        self.session_inner(stream, initiator, expected)
    }
    fn session_inner(
        &self,
        mut stream: TcpStream,
        initiator: bool,
        expected: Option<&str>,
    ) -> Result<()> {
        stream
            .set_read_timeout(Some(IO_TIMEOUT))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .map_err(|e| e.to_string())?;
        let builder = Builder::new(NOISE.parse().map_err(|e: snow::Error| e.to_string())?)
            .local_private_key(&self.key);
        let mut handshake = if initiator {
            builder.build_initiator()
        } else {
            builder.build_responder()
        }
        .map_err(|e| e.to_string())?;
        let mut buffer = [0u8; MAX_FRAME];
        let deadline = Instant::now() + Duration::from_secs(12);
        for step in 0..3 {
            if (step % 2 == 0) == initiator {
                let len = handshake
                    .write_message(&[], &mut buffer)
                    .map_err(|e| e.to_string())?;
                send_frame(&mut stream, &buffer[..len])?;
            } else {
                let frame = read_frame(&mut stream, deadline)?;
                handshake
                    .read_message(&frame, &mut buffer)
                    .map_err(|e| e.to_string())?;
            }
        }
        let remote = handshake
            .get_remote_static()
            .ok_or("Missing authenticated Noise static key")?
            .to_vec();
        if remote.len() != 32 {
            return Err("Invalid Noise static key".into());
        }
        let peer_id = identity_id(&remote);
        if expected.is_some_and(|id| id != peer_id) {
            return Err("Discovered peer identity does not match Noise key".into());
        }
        let code = pairing_code(handshake.get_handshake_hash());
        let mut transport = handshake.into_transport_mode().map_err(|e| e.to_string())?;
        let already_trusted = self
            .state
            .lock()
            .unwrap()
            .trusted
            .contains_key(&hex::encode(&remote));
        let name = local_name();
        send_message(
            &mut stream,
            &mut transport,
            &Message::Hello {
                name,
                trusted: already_trusted,
            },
        )?;
        let Message::Hello {
            name,
            trusted: remote_trusted,
        } = receive_message(&mut stream, &mut transport, deadline)?
        else {
            return Err("Expected peer introduction".into());
        };
        let name: String = name.chars().take(80).collect();
        if already_trusted && remote_trusted {
            self.state
                .lock()
                .unwrap()
                .authenticated
                .insert(peer_id.clone());
            return self.synchronize(&peer_id, &mut stream, &mut transport, initiator);
        }
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(|e| e.to_string())?;
        let request_id = hex::encode(random);
        self.state.lock().unwrap().pending.insert(
            request_id.clone(),
            Pending {
                name,
                code,
                decision: None,
                expires: Instant::now() + TIMEOUT,
            },
        );
        let result = self.finish_pairing(&request_id, &remote, &mut stream, &mut transport);
        self.state.lock().unwrap().pending.remove(&request_id);
        result.map_err(|e| format!("Pairing decision: {e}"))?;
        self.synchronize(&peer_id, &mut stream, &mut transport, initiator)
    }
    fn synchronize(
        &self,
        id: &str,
        stream: &mut TcpStream,
        transport: &mut TransportState,
        initiator: bool,
    ) -> Result<()> {
        let Some(repo) = &self.repository else {
            return Ok(());
        };
        let peer = DeviceId(id.to_owned());
        self.set_sync(id, "syncing", None, false);
        let result = self.exchange(repo, &peer, stream, transport, initiator);
        match &result {
            Ok(()) => self.set_sync(id, "synced", None, true),
            Err(error) => {
                let when = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |time| time.as_secs());
                self.set_sync(id, "error", Some(format!("{when}: {error}")), false);
            }
        }
        result
    }
    fn set_sync(&self, id: &str, status: &str, detail: Option<String>, success: bool) {
        let mut state = self.state.lock().unwrap();
        // Removal can race the final encrypted frame; completion must not mark an
        // already-removed discovery as online. A new session resets it to syncing.
        let removed_during_session = status != "syncing"
            && state
                .sync
                .get(id)
                .is_some_and(|view| view.state == "offline");
        let previous = state.sync.get(id).and_then(|view| view.last_sync.clone());
        let last_sync = if success {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()
                .map(|time| time.as_secs().to_string())
        } else {
            previous
        };
        state.sync.insert(
            id.to_owned(),
            SyncView {
                id: id.to_owned(),
                state: if removed_during_session {
                    "offline"
                } else {
                    status
                }
                .to_owned(),
                detail,
                last_sync,
            },
        );
    }
    fn pinned_origin(
        &self,
        id: &str,
        repo: &WordbookRepository,
        stream: &mut TcpStream,
        transport: &mut TransportState,
        initiator: bool,
    ) -> Result<DeviceId> {
        let local = repo
            .replay_device_id()
            .map_err(|e| format!("Replay identity: {e:?}"))?;
        let deadline = Instant::now() + Duration::from_secs(12);
        let remote = if initiator {
            send_sync(stream, transport, &SyncMessage::Identity(local.clone()))?;
            match receive_sync(stream, transport, deadline)? {
                SyncMessage::Identity(id) => id,
                _ => return Err("Expected sync identity".into()),
            }
        } else {
            let remote = match receive_sync(stream, transport, deadline)? {
                SyncMessage::Identity(id) => id,
                _ => return Err("Expected sync identity".into()),
            };
            send_sync(stream, transport, &SyncMessage::Identity(local.clone()))?;
            remote
        };
        if remote == local
            || remote.0.len() != 32
            || !remote
                .0
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("Invalid replay identity".into());
        }
        let mut state = self.state.lock().unwrap();
        let authenticated_key = state
            .trusted
            .keys()
            .filter_map(|key| hex::decode(key).ok())
            .find(|key| identity_id(key) == id)
            .ok_or("Untrusted sync identity")?;
        if remote != replay_origin(&authenticated_key) {
            return Err("Replay origin is not bound to authenticated Noise key".into());
        }
        if state.sync_ids.get(id).is_some_and(|old| old != &remote.0)
            || state
                .sync_ids
                .iter()
                .any(|(key, old)| key != id && old == &remote.0)
        {
            return Err("Replay identity does not match paired device".into());
        }
        if !state.sync_ids.contains_key(id) {
            let mut sync_ids = state.sync_ids.clone();
            sync_ids.insert(id.to_owned(), remote.0.clone());
            atomic_write(
                &self.trust_path,
                &serde_json::to_vec(&TrustFile {
                    peers: state.trusted.clone(),
                    sync_ids: sync_ids.clone(),
                })
                .map_err(|e| e.to_string())?,
            )?;
            state.sync_ids = sync_ids;
        }
        Ok(remote)
    }
    fn exchange(
        &self,
        repo: &WordbookRepository,
        peer: &DeviceId,
        stream: &mut TcpStream,
        transport: &mut TransportState,
        initiator: bool,
    ) -> Result<()> {
        let peer = self.pinned_origin(&peer.0, repo, stream, transport, initiator)?;
        let cursor = repo
            .exchange_cursors(&peer)
            .map_err(|e| format!("Sync cursor: {e:?}"))?[&peer];
        let deadline = Instant::now() + Duration::from_secs(120);
        let remote_cursor = if initiator {
            send_sync(
                stream,
                transport,
                &SyncMessage::Start { version: 1, cursor },
            )?;
            match receive_sync(stream, transport, deadline)? {
                SyncMessage::Start { version: 1, cursor } => cursor,
                _ => return Err("Invalid sync introduction".into()),
            }
        } else {
            let received = match receive_sync(stream, transport, deadline)? {
                SyncMessage::Start { version: 1, cursor } => cursor,
                _ => return Err("Invalid sync introduction".into()),
            };
            send_sync(
                stream,
                transport,
                &SyncMessage::Start { version: 1, cursor },
            )?;
            received
        };
        let local = repo
            .replay_device_id()
            .map_err(|e| format!("Replay identity: {e:?}"))?;
        let local_cursor = repo
            .exchange_cursors(&peer)
            .map_err(|e| format!("Sync cursor: {e:?}"))?[&local];
        if remote_cursor > local_cursor {
            return Err("Peer reported an impossible sync cursor".into());
        }
        if initiator {
            self.send_changes(repo, &peer, remote_cursor, stream, transport)?;
            self.receive_changes(repo, &peer, stream, transport, deadline)?;
        } else {
            self.receive_changes(repo, &peer, stream, transport, deadline)?;
            self.send_changes(repo, &peer, remote_cursor, stream, transport)?;
        }
        Ok(())
    }
    fn send_changes(
        &self,
        repo: &WordbookRepository,
        peer: &DeviceId,
        mut cursor: u64,
        stream: &mut TcpStream,
        transport: &mut TransportState,
    ) -> Result<()> {
        for _ in 0..MAX_SYNC_CHANGES {
            let mut batch = repo
                .exchange_batch(peer, cursor, 1)
                .map_err(|e| format!("Sync export: {e:?}"))?;
            let Some(change) = batch.pop() else {
                send_sync(stream, transport, &SyncMessage::Done { complete: true })?;
                return Ok(());
            };
            cursor = change.id.sequence;
            send_sync(stream, transport, &SyncMessage::Change(change))?;
        }
        // A capped session is never acknowledged as fully synchronized.
        let complete = repo
            .exchange_batch(peer, cursor, 1)
            .map_err(|e| format!("Sync export: {e:?}"))?
            .is_empty();
        send_sync(stream, transport, &SyncMessage::Done { complete })?;
        if complete {
            Ok(())
        } else {
            Err("Sync page limit reached; retry required".into())
        }
    }
    fn receive_changes(
        &self,
        repo: &WordbookRepository,
        peer: &DeviceId,
        stream: &mut TcpStream,
        transport: &mut TransportState,
        deadline: Instant,
    ) -> Result<()> {
        for index in 0..=MAX_SYNC_CHANGES {
            match receive_sync(stream, transport, deadline)? {
                SyncMessage::Change(change) => {
                    if index == MAX_SYNC_CHANGES {
                        return Err("Sync page limit exceeded".into());
                    }
                    let result = repo
                        .exchange_ingest(peer, &change)
                        .map_err(|e| format!("Sync ingest: {e:?}"))?;
                    if result.inserted {
                        if let Some(app) = &self.app {
                            let _ = app.emit("learning-data-synced", ());
                        }
                    }
                }
                SyncMessage::Done { complete: true } => return Ok(()),
                SyncMessage::Done { complete: false } => {
                    return Err("Peer has more changes; retry required".into())
                }
                _ => return Err("Unexpected sync message".into()),
            }
        }
        Err("Sync page limit exceeded".into())
    }
    fn finish_pairing(
        &self,
        id: &str,
        remote: &[u8],
        stream: &mut TcpStream,
        transport: &mut TransportState,
    ) -> Result<()> {
        let deadline = Instant::now() + TIMEOUT;
        let mut sent = false;
        let mut received = false;
        while Instant::now() < deadline {
            let decision = self
                .state
                .lock()
                .unwrap()
                .pending
                .get(id)
                .and_then(|p| p.decision);
            match decision {
                Some(false) => {
                    let _ = send_message(stream, transport, &Message::Cancel);
                    return Err("Pairing cancelled".into());
                }
                Some(true) if !sent => {
                    send_message(stream, transport, &Message::Confirm)?;
                    sent = true;
                }
                _ => {}
            }
            // The peer may already have sent Commit after our Confirm. Once both
            // confirmations are present, do not read that Commit as a pairing message.
            if !(sent && received) {
                match receive_message(stream, transport, Instant::now() + IO_TIMEOUT) {
                    Ok(Message::Confirm) => received = true,
                    Ok(Message::Cancel) => return Err("Remote device cancelled pairing".into()),
                    Ok(_) => return Err("Unexpected pairing message".into()),
                    Err(e) if e == "timeout" => {}
                    Err(e) => return Err(e),
                }
            }
            if sent && received {
                // Crossing the commit boundary removes the request under the same lock
                // used by cancel. A successful cancellation can never be followed by trust.
                let name = {
                    let mut state = self.state.lock().unwrap();
                    let pending = state.pending.get(id).ok_or("Pairing expired")?;
                    let confirmed = pending.decision == Some(true);
                    let name = pending.name.clone();
                    state.pending.remove(id);
                    if confirmed {
                        Some(name)
                    } else {
                        None
                    }
                };
                let Some(name) = name else {
                    let _ = send_message(stream, transport, &Message::Cancel);
                    return Err("Pairing cancelled".into());
                };
                send_message(stream, transport, &Message::Commit)?;
                match receive_message(stream, transport, deadline)? {
                    Message::Commit => {
                        self.trust(remote, &name)?;
                        self.state
                            .lock()
                            .unwrap()
                            .authenticated
                            .insert(identity_id(remote));
                        return Ok(());
                    }
                    _ => return Err("Remote did not commit pairing".into()),
                }
            }
        }
        Err("Pairing timed out".into())
    }
}
fn remove_discovered(state: &mut Shared, fullname: &str) {
    let removed: Vec<_> = state
        .discovered
        .iter()
        .filter(|(_, peer)| peer.fullname == fullname)
        .map(|(id, _)| id.clone())
        .collect();
    for id in removed {
        state.discovered.remove(&id);
        state.authenticated.remove(&id);
        state.reconnect_attempts.remove(&id);
        if let Some(sync) = state.sync.get_mut(&id) {
            sync.state = "offline".into();
            sync.detail = None;
        }
    }
}

fn snow_public(private: &[u8; 32]) -> Result<Vec<u8>> {
    // Noise's 25519 static DH public key, derived from the durable private key.
    Ok(
        x25519_dalek::PublicKey::from(&x25519_dalek::StaticSecret::from(*private))
            .as_bytes()
            .to_vec(),
    )
}
fn local_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "Lexicon device".into())
        .chars()
        .take(80)
        .collect()
}
#[derive(Serialize, Deserialize)]
enum Message {
    Hello { name: String, trusted: bool },
    Confirm,
    Cancel,
    Commit,
}
#[derive(Debug, Serialize, Deserialize)]
enum SyncMessage {
    Identity(DeviceId),
    Start { version: u32, cursor: u64 },
    Change(Envelope),
    // Followed by encrypted raw chunks containing exactly length bytes of one Change.
    ChangeStart { length: usize },
    Done { complete: bool },
}
fn send_frame(stream: &mut TcpStream, bytes: &[u8]) -> Result<()> {
    send_bounded_frame(stream, bytes, MAX_FRAME)
}
fn send_bounded_frame(stream: &mut TcpStream, bytes: &[u8], max: usize) -> Result<()> {
    if bytes.is_empty() || bytes.len() > max {
        return Err("Invalid frame size".into());
    }
    stream
        .write_all(&(bytes.len() as u16).to_be_bytes())
        .and_then(|_| stream.write_all(bytes))
        .map_err(|e| e.to_string())
}
fn read_frame(stream: &mut TcpStream, deadline: Instant) -> Result<Vec<u8>> {
    read_bounded_frame(stream, deadline, MAX_FRAME)
}
fn read_bounded_frame(stream: &mut TcpStream, deadline: Instant, max: usize) -> Result<Vec<u8>> {
    let mut length = [0u8; 2];
    // An idle socket can be polled; after one byte arrives, a partial frame must
    // complete or close rather than lose framing on the next poll.
    read_exact_until(stream, &mut length[..1], deadline)?;
    let frame_deadline = deadline.max(Instant::now() + Duration::from_secs(3));
    read_exact_until(stream, &mut length[1..], frame_deadline)?;
    let size = u16::from_be_bytes(length) as usize;
    if size == 0 || size > max {
        return Err("Invalid frame size".into());
    }
    let mut bytes = vec![0; size];
    read_exact_until(stream, &mut bytes, frame_deadline)?;
    Ok(bytes)
}
fn read_exact_until(stream: &mut TcpStream, mut bytes: &mut [u8], deadline: Instant) -> Result<()> {
    while !bytes.is_empty() {
        if Instant::now() >= deadline {
            return Err("timeout".into());
        }
        match stream.read(bytes) {
            Ok(0) => return Err("Peer disconnected".into()),
            Ok(n) => bytes = &mut bytes[n..],
            Err(e)
                if e.kind() == std::io::ErrorKind::TimedOut
                    || e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}
fn send_message(
    stream: &mut TcpStream,
    transport: &mut TransportState,
    message: &Message,
) -> Result<()> {
    let plaintext = serde_json::to_vec(message).map_err(|e| e.to_string())?;
    let mut ciphertext = [0u8; MAX_FRAME];
    let len = transport
        .write_message(&plaintext, &mut ciphertext)
        .map_err(|e| e.to_string())?;
    send_frame(stream, &ciphertext[..len])
}
fn receive_message(
    stream: &mut TcpStream,
    transport: &mut TransportState,
    deadline: Instant,
) -> Result<Message> {
    let frame = read_frame(stream, deadline)?;
    let mut plaintext = [0u8; MAX_FRAME];
    let len = transport
        .read_message(&frame, &mut plaintext)
        .map_err(|e| e.to_string())?;
    serde_json::from_slice(&plaintext[..len]).map_err(|e| e.to_string())
}
fn send_sync_frame(
    stream: &mut TcpStream,
    transport: &mut TransportState,
    plain: &[u8],
) -> Result<()> {
    if plain.is_empty() || plain.len() > MAX_SYNC_FRAME - 16 {
        return Err("Invalid sync frame size".into());
    }
    let mut encrypted = vec![0; MAX_SYNC_FRAME];
    let len = transport
        .write_message(plain, &mut encrypted)
        .map_err(|e| e.to_string())?;
    send_bounded_frame(stream, &encrypted[..len], MAX_SYNC_FRAME)
}
fn receive_sync_frame(
    stream: &mut TcpStream,
    transport: &mut TransportState,
    deadline: Instant,
) -> Result<Vec<u8>> {
    let frame = read_bounded_frame(stream, deadline, MAX_SYNC_FRAME)?;
    let mut plain = vec![0; MAX_SYNC_FRAME];
    let len = transport
        .read_message(&frame, &mut plain)
        .map_err(|e| e.to_string())?;
    if len == 0 {
        return Err("Empty sync chunk".into());
    }
    plain.truncate(len);
    Ok(plain)
}
fn send_sync(
    stream: &mut TcpStream,
    transport: &mut TransportState,
    message: &SyncMessage,
) -> Result<()> {
    let plain = serde_json::to_vec(message).map_err(|e| e.to_string())?;
    if plain.len() > MAX_SYNC_MESSAGE {
        return Err("Sync operation exceeds message limit".into());
    }
    if plain.len() <= MAX_SYNC_FRAME - 16 {
        return send_sync_frame(stream, transport, &plain);
    }
    if !matches!(message, SyncMessage::Change(_)) {
        return Err("Sync control exceeds frame limit".into());
    }
    let start = serde_json::to_vec(&SyncMessage::ChangeStart {
        length: plain.len(),
    })
    .map_err(|e| e.to_string())?;
    send_sync_frame(stream, transport, &start)?;
    for chunk in plain.chunks(MAX_SYNC_FRAME - 16) {
        send_sync_frame(stream, transport, chunk)?;
    }
    Ok(())
}
fn receive_sync(
    stream: &mut TcpStream,
    transport: &mut TransportState,
    deadline: Instant,
) -> Result<SyncMessage> {
    let plain = receive_sync_frame(stream, transport, deadline)?;
    let message: SyncMessage = serde_json::from_slice(&plain).map_err(|e| e.to_string())?;
    let SyncMessage::ChangeStart { length } = message else {
        return Ok(message);
    };
    if length <= MAX_SYNC_FRAME - 16 || length > MAX_SYNC_MESSAGE {
        return Err("Invalid sync operation length".into());
    }
    let mut bytes = Vec::with_capacity(length);
    while bytes.len() < length {
        let chunk = receive_sync_frame(stream, transport, deadline)?;
        if chunk.len() > length - bytes.len() {
            return Err("Sync operation exceeds declared length".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    match serde_json::from_slice(&bytes).map_err(|e| e.to_string())? {
        change @ SyncMessage::Change(_) => Ok(change),
        _ => Err("Expected chunked sync operation".into()),
    }
}
impl LanBackend {
    pub fn open(dir: &Path, repository: WordbookRepository, app: tauri::AppHandle) -> Self {
        match LanService::open_with_repository(dir, Some(repository), Some(app)) {
            Ok(service) => {
                let error = service.start().err();
                Self {
                    service: Some(service),
                    error,
                }
            }
            Err(error) => Self {
                service: None,
                error: Some(format!("LAN unavailable: {error}")),
            },
        }
    }
    fn service(&self) -> Result<&Arc<LanService>> {
        self.service
            .as_ref()
            .ok_or_else(|| self.error.clone().unwrap_or("LAN unavailable".into()))
    }
}
#[tauri::command]
pub fn lan_status(backend: State<'_, LanBackend>) -> LanStatus {
    match &backend.service {
        Some(service) => {
            let mut status = service.status();
            if backend.error.is_some() {
                status.error = backend.error.clone();
            }
            status
        }
        None => LanStatus {
            peers: vec![],
            pending: vec![],
            trusted: vec![],
            error: backend.error.clone(),
            sync: vec![],
        },
    }
}
#[tauri::command]
pub fn lan_pair(backend: State<'_, LanBackend>, peer_id: String) -> Result<()> {
    backend.service()?.pair(&peer_id)
}
#[tauri::command]
pub fn lan_confirm(backend: State<'_, LanBackend>, id: String, code: String) -> Result<()> {
    backend.service()?.decide(&id, Some(&code))
}
#[tauri::command]
pub fn lan_cancel(backend: State<'_, LanBackend>, id: String) -> Result<()> {
    backend.service()?.decide(&id, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_identity_and_fail_closed_storage() {
        let dir = tempfile::tempdir().unwrap();
        let a = LanService::open(dir.path()).unwrap();
        assert_eq!(a.local_id, LanService::open(dir.path()).unwrap().local_id);
        fs::write(dir.path().join("lan-trust.json"), b"broken").unwrap();
        assert!(LanService::open(dir.path()).is_err());
        fs::remove_file(dir.path().join("lan-trust.json")).unwrap();
        fs::write(dir.path().join("lan-identity.json"), b"broken").unwrap();
        assert!(LanService::open(dir.path()).is_err());
        assert!(LanService::open(dir.path()).is_err()); // Never starts listener on invalid keys.
    }
    #[test]
    fn direct_trust_only_and_sas_binding() {
        let a_dir = tempfile::tempdir().unwrap();
        let b_dir = tempfile::tempdir().unwrap();
        let a = LanService::open(a_dir.path()).unwrap();
        let b = LanService::open(b_dir.path()).unwrap();
        a.trust(&snow_public(&b.key).unwrap(), "B").unwrap();
        a.trust(&snow_public(&b.key).unwrap(), "B renamed").unwrap();
        assert!(LanService::open(a_dir.path())
            .unwrap()
            .status()
            .trusted
            .iter()
            .any(|p| p.name == "B renamed"));
        assert!(b.status().trusted.is_empty());
        let c_dir = tempfile::tempdir().unwrap();
        let c = LanService::open(c_dir.path()).unwrap();
        b.trust(&snow_public(&a.key).unwrap(), "A").unwrap();
        b.trust(&snow_public(&c.key).unwrap(), "C").unwrap();
        c.trust(&snow_public(&b.key).unwrap(), "B").unwrap();
        assert_eq!(a.status().trusted.len(), 1);
        assert!(!a
            .status()
            .trusted
            .iter()
            .any(|p| p.id == hex::encode(snow_public(&c.key).unwrap())));
        assert_ne!(pairing_code(&[0; 32]), pairing_code(&[1; 32]));
    }
    #[test]
    fn persisted_sync_pin_must_belong_to_directly_trusted_noise_key() {
        let dir = tempfile::tempdir().unwrap();
        let a = LanService::open(dir.path()).unwrap();
        let b_dir = tempfile::tempdir().unwrap();
        let c_dir = tempfile::tempdir().unwrap();
        let b = LanService::open(b_dir.path()).unwrap();
        let c = LanService::open(c_dir.path()).unwrap();
        let b_key = snow_public(&b.key).unwrap();
        a.trust(&b_key, "B").unwrap();
        let path = dir.path().join("lan-trust.json");
        let mut saved = load_trust(&path).unwrap();
        let origin = replay_origin(&b_key).0;
        saved.sync_ids.insert(identity_id(&b_key), origin.clone());
        fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
        assert_eq!(
            LanService::open(dir.path())
                .unwrap()
                .state
                .lock()
                .unwrap()
                .sync_ids
                .len(),
            1
        );
        saved.sync_ids.clear();
        saved.sync_ids.insert(c.local_id.clone(), origin);
        fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
        assert!(
            matches!(LanService::open(dir.path()), Err(message) if message == "Invalid pinned replay identity")
        );
    }
    #[test]
    fn asymmetric_trust_reopens_pairing_on_both_sides() {
        let a_dir = tempfile::tempdir().unwrap();
        let b_dir = tempfile::tempdir().unwrap();
        let a = LanService::open(a_dir.path()).unwrap();
        let b = LanService::open(b_dir.path()).unwrap();
        a.trust(&snow_public(&b.key).unwrap(), "B").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let responder = {
            let b = b.clone();
            thread::spawn(move || b.session(listener.accept().unwrap().0, false, None))
        };
        let initiator = {
            let a = a.clone();
            let expected = b.local_id.clone();
            thread::spawn(move || {
                a.session(TcpStream::connect(address).unwrap(), true, Some(&expected))
            })
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        while a.status().pending.is_empty() || b.status().pending.is_empty() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        let a_pending = &a.status().pending[0];
        let b_pending = &b.status().pending[0];
        assert_eq!(a_pending.code, b_pending.code);
        a.decide(&a_pending.id, Some(&a_pending.code)).unwrap();
        b.decide(&b_pending.id, Some(&b_pending.code)).unwrap();
        let initiator = initiator.join().unwrap();
        assert!(initiator.is_ok(), "{initiator:?}");
        let responder = responder.join().unwrap();
        assert!(responder.is_ok(), "{responder:?}");
        assert_eq!(b.status().trusted.len(), 1);
        assert_eq!(
            LanService::open(b_dir.path())
                .unwrap()
                .status()
                .trusted
                .len(),
            1
        );
    }

    #[test]
    fn loopback_requires_both_confirmations_and_rejects_forged_discovery_id() {
        let a_dir = tempfile::tempdir().unwrap();
        let b_dir = tempfile::tempdir().unwrap();
        let a = LanService::open(a_dir.path()).unwrap();
        let b = LanService::open(b_dir.path()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let responder = {
            let b = b.clone();
            thread::spawn(move || b.session(listener.accept().unwrap().0, false, None))
        };
        let initiator = {
            let a = a.clone();
            let b_id = b.local_id.clone();
            thread::spawn(move || {
                a.session(TcpStream::connect(address).unwrap(), true, Some(&b_id))
            })
        };
        let limit = Instant::now() + Duration::from_secs(5);
        while a.status().pending.is_empty() || b.status().pending.is_empty() {
            assert!(
                Instant::now() < limit,
                "Noise handshake did not create pending sessions"
            );
            thread::sleep(Duration::from_millis(10));
        }
        let a_pending = &a.status().pending[0];
        let b_pending = &b.status().pending[0];
        assert_eq!(a_pending.code, b_pending.code);
        a.decide(&a_pending.id, Some(&a_pending.code)).unwrap();
        thread::sleep(Duration::from_millis(100));
        assert!(a.status().trusted.is_empty());
        assert!(b.status().trusted.is_empty());
        b.decide(&b_pending.id, Some(&b_pending.code)).unwrap();
        assert!(initiator.join().unwrap().is_ok());
        assert!(responder.join().unwrap().is_ok());
        assert_eq!(a.status().trusted.len(), 1);
        assert_eq!(b.status().trusted.len(), 1);
        // A remembered, directly trusted key reconnects without another pending code.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let responder = {
            let b = b.clone();
            thread::spawn(move || b.session(listener.accept().unwrap().0, false, None))
        };
        assert!(a
            .session(
                TcpStream::connect(address).unwrap(),
                true,
                Some(&b.local_id)
            )
            .is_ok());
        assert!(responder.join().unwrap().is_ok());
        assert!(a.status().pending.is_empty() && b.status().pending.is_empty());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let responder = {
            let b = b.clone();
            thread::spawn(move || b.session(listener.accept().unwrap().0, false, None))
        };
        assert!(a
            .session(
                TcpStream::connect(address).unwrap(),
                true,
                Some(&a.local_id)
            )
            .is_err());
        assert!(responder.join().unwrap().is_err());
        assert!(a.status().pending.is_empty());
    }
    #[test]
    fn cancellation_clears_both_pending_without_trust() {
        let a_dir = tempfile::tempdir().unwrap();
        let b_dir = tempfile::tempdir().unwrap();
        let a = LanService::open(a_dir.path()).unwrap();
        let b = LanService::open(b_dir.path()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let responder = {
            let b = b.clone();
            thread::spawn(move || b.session(listener.accept().unwrap().0, false, None))
        };
        let initiator = {
            let a = a.clone();
            let b_id = b.local_id.clone();
            thread::spawn(move || {
                a.session(TcpStream::connect(address).unwrap(), true, Some(&b_id))
            })
        };
        let limit = Instant::now() + Duration::from_secs(5);
        while a.status().pending.is_empty() || b.status().pending.is_empty() {
            assert!(Instant::now() < limit);
            thread::sleep(Duration::from_millis(10));
        }
        let pending = &a.status().pending[0];
        a.decide(&pending.id, Some(&pending.code)).unwrap();
        a.decide(&pending.id, None).unwrap();
        assert!(initiator.join().unwrap().is_err());
        assert!(responder.join().unwrap().is_err());
        assert!(a.status().pending.is_empty() && b.status().pending.is_empty());
        assert!(a.status().trusted.is_empty() && b.status().trusted.is_empty());
    }
    #[test]
    fn pair_tries_next_address_after_failed_handshake() {
        let a_dir = tempfile::tempdir().unwrap();
        let b_dir = tempfile::tempdir().unwrap();
        let a = LanService::open(a_dir.path()).unwrap();
        let b = LanService::open(b_dir.path()).unwrap();
        let stale = TcpListener::bind("127.0.0.1:0").unwrap();
        let good = TcpListener::bind("127.0.0.1:0").unwrap();
        let stale_addr = stale.local_addr().unwrap();
        let good_addr = good.local_addr().unwrap();
        let stale_thread = thread::spawn(move || {
            drop(stale.accept().unwrap());
        });
        let responder = {
            let b = b.clone();
            thread::spawn(move || b.session(good.accept().unwrap().0, false, None))
        };
        a.state.lock().unwrap().discovered.insert(
            b.local_id.clone(),
            Discovered {
                fullname: "test.local.".into(),
                name: "B".into(),
                addresses: vec![stale_addr, good_addr],
            },
        );
        a.pair(&b.local_id).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while a.status().pending.is_empty() || b.status().pending.is_empty() {
            assert!(Instant::now() < deadline, "did not retry usable address");
            thread::sleep(Duration::from_millis(10));
        }
        let ap = &a.status().pending[0];
        let bp = &b.status().pending[0];
        a.decide(&ap.id, Some(&ap.code)).unwrap();
        b.decide(&bp.id, Some(&bp.code)).unwrap();
        stale_thread.join().unwrap();
        let responder = responder.join().unwrap();
        assert!(responder.is_ok(), "{responder:?}");
        let deadline = Instant::now() + Duration::from_secs(5);
        while a.status().trusted.is_empty() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn confirmation_can_be_withdrawn_before_remote_confirms() {
        let dir = tempfile::tempdir().unwrap();
        let service = LanService::open(dir.path()).unwrap();
        service.state.lock().unwrap().pending.insert(
            "request".into(),
            Pending {
                name: "B".into(),
                code: "12345678".into(),
                decision: None,
                expires: Instant::now() + TIMEOUT,
            },
        );
        service.decide("request", Some("12345678")).unwrap();
        service.decide("request", None).unwrap();
        assert_eq!(
            service.state.lock().unwrap().pending["request"].decision,
            Some(false)
        );
    }

    fn network_peer(dir: &Path) -> (Arc<LanService>, WordbookRepository) {
        let repo = WordbookRepository::open(dir.join("words.sqlite")).unwrap();
        let service = LanService::open_with_repository(dir, Some(repo.clone()), None).unwrap();
        (service, repo)
    }
    fn connect_pair(a: &Arc<LanService>, b: &Arc<LanService>) -> (Result<()>, Result<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let responder = {
            let b = b.clone();
            thread::spawn(move || b.session(listener.accept().unwrap().0, false, None))
        };
        let initiator = a.session(
            TcpStream::connect(address).unwrap(),
            true,
            Some(&b.local_id),
        );
        (initiator, responder.join().unwrap())
    }
    fn mutual_trust(a: &LanService, b: &LanService) {
        a.trust(&snow_public(&b.key).unwrap(), "peer").unwrap();
        b.trust(&snow_public(&a.key).unwrap(), "peer").unwrap();
    }
    #[test]
    fn first_confirmed_pair_exchanges_fresh_learning_data() {
        let ad = tempfile::tempdir().unwrap();
        let bd = tempfile::tempdir().unwrap();
        let (a, ar) = network_peer(ad.path());
        let (b, br) = network_peer(bd.path());
        ar.add_favorite("alpha", "甲").unwrap();
        br.record_mistake("beta", "乙").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let responder = {
            let b = b.clone();
            thread::spawn(move || b.session(listener.accept().unwrap().0, false, None))
        };
        let initiator = {
            let a = a.clone();
            let expected = b.local_id.clone();
            thread::spawn(move || {
                a.session(TcpStream::connect(address).unwrap(), true, Some(&expected))
            })
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        while a.status().pending.is_empty() || b.status().pending.is_empty() {
            assert!(
                Instant::now() < deadline,
                "first pairing did not reach confirmation"
            );
            thread::sleep(Duration::from_millis(10));
        }
        let ap = &a.status().pending[0];
        let bp = &b.status().pending[0];
        assert_eq!(ap.code, bp.code);
        a.decide(&ap.id, Some(&ap.code)).unwrap();
        b.decide(&bp.id, Some(&bp.code)).unwrap();
        let initiator = initiator.join().unwrap();
        assert!(initiator.is_ok(), "{initiator:?}");
        let responder = responder.join().unwrap();
        assert!(responder.is_ok(), "{responder:?}");
        assert_eq!(br.list_favorites().unwrap().len(), 1);
        assert_eq!(ar.list_mistakes().unwrap()[0].error_count, 1);
        assert_eq!(
            LanService::open(ad.path()).unwrap().status().trusted.len(),
            1
        );
    }

    #[test]
    fn existing_unbound_history_disables_lan_without_clearing_local_data() {
        let dir = tempfile::tempdir().unwrap();
        let repo = WordbookRepository::open(dir.path().join("words.sqlite")).unwrap();
        repo.add_favorite("old", "旧").unwrap();
        assert!(LanService::open_with_repository(dir.path(), Some(repo.clone()), None).is_err());
        assert_eq!(repo.list_favorites().unwrap().len(), 1);
    }

    #[test]
    fn paired_peer_cannot_claim_third_party_replay_origin_on_first_exchange() {
        let ad = tempfile::tempdir().unwrap();
        let bd = tempfile::tempdir().unwrap();
        let cd = tempfile::tempdir().unwrap();
        let (a, ar) = network_peer(ad.path());
        let (b, _) = network_peer(bd.path());
        let (c, _) = network_peer(cd.path());
        mutual_trust(&a, &b);
        let forged = replay_origin(&snow_public(&c.key).unwrap());
        rusqlite::Connection::open(bd.path().join("words.sqlite"))
            .unwrap()
            .execute(
                "UPDATE replay_identity SET device=?1 WHERE singleton=1",
                [&forged.0],
            )
            .unwrap();
        let (left, right) = connect_pair(&a, &b);
        assert!(left.is_err() || right.is_err());
        assert!(ar.list_favorites().unwrap().is_empty());
        assert!(!a.state.lock().unwrap().sync_ids.contains_key(&b.local_id));
    }

    #[test]
    fn network_reconnect_replays_business_changes_and_duplicates() {
        let ad = tempfile::tempdir().unwrap();
        let bd = tempfile::tempdir().unwrap();
        let (a, ar) = network_peer(ad.path());
        let (b, br) = network_peer(bd.path());
        mutual_trust(&a, &b);
        ar.add_favorite("alpha", "甲").unwrap();
        ar.record_mistake("alpha", "甲").unwrap();
        br.add_favorite("beta", "乙").unwrap();
        for _ in 0..2 {
            let (left, right) = connect_pair(&a, &b);
            assert!(left.is_ok(), "{left:?}");
            assert!(right.is_ok(), "{right:?}");
        }
        assert_eq!(ar.list_favorites().unwrap().len(), 2);
        assert_eq!(br.list_favorites().unwrap().len(), 2);
        assert_eq!(br.list_mistakes().unwrap()[0].error_count, 1);
        drop(a);
        drop(b);
        let (a, ar) = network_peer(ad.path());
        let (b, br) = network_peer(bd.path());
        br.record_mistake("alpha", "甲").unwrap();
        let (left, right) = connect_pair(&a, &b);
        assert!(left.is_ok(), "{left:?}");
        assert!(right.is_ok(), "{right:?}");
        assert_eq!(ar.list_mistakes().unwrap()[0].error_count, 2);
        assert_eq!(br.list_mistakes().unwrap()[0].error_count, 2);
        assert_eq!(a.status().sync[0].state, "synced");
    }
    #[test]
    fn network_transfers_full_book_across_encrypted_frames() {
        let ad = tempfile::tempdir().unwrap();
        let bd = tempfile::tempdir().unwrap();
        let (a, ar) = network_peer(ad.path());
        let (b, br) = network_peer(bd.path());
        mutual_trust(&a, &b);
        let entries: Vec<_> = (0..2000)
            .map(|n| {
                serde_json::json!({
                    "english": format!("word-{n:04}-long-vocabulary-entry"),
                    "chinese": format!("meaning-{n:04}-long-definition"),
                })
            })
            .collect();
        // The same atomic import operation emitted by replace(), without exposing its private input type.
        ar.append_local(
            serde_json::to_vec(&serde_json::json!({
                "type": "import", "name": "Full book", "entries": entries,
            }))
            .unwrap(),
            None,
            &crate::wordbooks::repository::SyncProjection,
        )
        .unwrap();
        let change = ar
            .exchange_batch(&br.replay_device_id().unwrap(), 0, 1)
            .unwrap();
        assert!(
            serde_json::to_vec(&SyncMessage::Change(change[0].clone()))
                .unwrap()
                .len()
                > MAX_SYNC_FRAME
        );
        let (left, right) = connect_pair(&a, &b);
        assert!(left.is_ok(), "{left:?}");
        assert!(right.is_ok(), "{right:?}");
        let books = br.list().unwrap();
        assert!(books.iter().any(|book| book.name == "Full book"));
        let book = books.iter().find(|book| book.name == "Full book").unwrap();
        assert_eq!(br.list_wordbook_entries(book.id).unwrap().len(), 2000);
    }

    #[test]
    fn sync_rejects_oversized_and_truncated_chunked_operations() {
        // Construct real Noise transport states so the framing checks cover authenticated bytes.
        let a_key = [1u8; 32];
        let b_key = [2u8; 32];
        let mut initiator = Builder::new(NOISE.parse().unwrap())
            .local_private_key(&a_key)
            .build_initiator()
            .unwrap();
        let mut responder = Builder::new(NOISE.parse().unwrap())
            .local_private_key(&b_key)
            .build_responder()
            .unwrap();
        let mut buffer = [0u8; MAX_FRAME];
        let len = initiator.write_message(&[], &mut buffer).unwrap();
        responder
            .read_message(&buffer[..len], &mut [0u8; MAX_FRAME])
            .unwrap();
        let len = responder.write_message(&[], &mut buffer).unwrap();
        initiator
            .read_message(&buffer[..len], &mut [0u8; MAX_FRAME])
            .unwrap();
        let len = initiator.write_message(&[], &mut buffer).unwrap();
        responder
            .read_message(&buffer[..len], &mut [0u8; MAX_FRAME])
            .unwrap();
        let mut sender = initiator.into_transport_mode().unwrap();
        let mut receiver = responder.into_transport_mode().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut outgoing = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut incoming, _) = listener.accept().unwrap();
        incoming.set_read_timeout(Some(IO_TIMEOUT)).unwrap();
        let oversize = SyncMessage::Change(Envelope {
            id: crate::wordbooks::repository::replay::ChangeId {
                device: DeviceId("abcd1234abcd1234abcd1234abcd1234".into()),
                sequence: 1,
            },
            version: 1,
            content: vec![255; MAX_SYNC_MESSAGE / 3],
            dependencies: Default::default(),
            occurred_at: None,
        });
        assert_eq!(
            send_sync(&mut outgoing, &mut sender, &oversize).unwrap_err(),
            "Sync operation exceeds message limit"
        );
        send_sync_frame(
            &mut outgoing,
            &mut sender,
            &serde_json::to_vec(&SyncMessage::ChangeStart {
                length: MAX_SYNC_MESSAGE + 1,
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            receive_sync(
                &mut incoming,
                &mut receiver,
                Instant::now() + Duration::from_secs(2)
            )
            .unwrap_err(),
            "Invalid sync operation length"
        );
        send_sync_frame(
            &mut outgoing,
            &mut sender,
            &serde_json::to_vec(&SyncMessage::ChangeStart {
                length: MAX_SYNC_FRAME,
            })
            .unwrap(),
        )
        .unwrap();
        send_sync_frame(&mut outgoing, &mut sender, b"incomplete").unwrap();
        drop(outgoing);
        assert_eq!(
            receive_sync(
                &mut incoming,
                &mut receiver,
                Instant::now() + Duration::from_secs(2)
            )
            .unwrap_err(),
            "Peer disconnected"
        );
    }

    #[test]
    fn removed_service_marks_trusted_sync_offline_and_preserves_last_success() {
        let dir = tempfile::tempdir().unwrap();
        let service = LanService::open(dir.path()).unwrap();
        let other = [7u8; 32];
        service.trust(&other, "remote").unwrap();
        let id = identity_id(&other);
        service.set_sync(&id, "synced", None, true);
        let prior = service.status().sync[0].last_sync.clone();
        {
            let mut state = service.state.lock().unwrap();
            state.discovered.insert(
                id.clone(),
                Discovered {
                    fullname: "remote.local.".into(),
                    name: "remote".into(),
                    addresses: vec![],
                },
            );
            state.authenticated.insert(id.clone());
            remove_discovered(&mut state, "remote.local.");
        }
        // A session that finishes after mDNS removal cannot restore a stale online state.
        service.set_sync(&id, "synced", None, true);
        let status = service.status();
        assert!(status.peers.is_empty());
        assert_eq!(status.sync[0].id, hex::encode(other));
        assert_eq!(status.sync[0].state, "offline");
        assert_eq!(status.sync[0].last_sync, prior);
        assert!(!service.state.lock().unwrap().authenticated.contains(&id));
        service.set_sync(&id, "syncing", None, false);
        service.set_sync(&id, "synced", None, true);
        assert_eq!(service.status().sync[0].state, "synced");
    }

    #[test]
    fn network_rejects_third_party_and_unpaired_peer() {
        let ad = tempfile::tempdir().unwrap();
        let bd = tempfile::tempdir().unwrap();
        let cd = tempfile::tempdir().unwrap();
        let (a, ar) = network_peer(ad.path());
        let (b, br) = network_peer(bd.path());
        let (c, cr) = network_peer(cd.path());
        mutual_trust(&a, &b);
        mutual_trust(&b, &c);
        cr.add_favorite("secret", "丙").unwrap();
        let (left, right) = connect_pair(&b, &c);
        assert!(left.is_ok(), "{left:?}");
        assert!(right.is_ok(), "{right:?}");
        assert!(ar.list_favorites().unwrap().is_empty());
        let (left, right) = connect_pair(&a, &b);
        assert!(left.is_ok(), "{left:?}");
        assert!(right.is_ok(), "{right:?}");
        assert!(ar.list_favorites().unwrap().is_empty());
        br.add_favorite("dependent", "乙").unwrap();
        let (left, right) = connect_pair(&a, &b);
        assert!(left.is_err() || right.is_err());
        assert!(ar.list_favorites().unwrap().is_empty());
        assert!(matches!(
            br.exchange_batch(&ar.replay_device_id().unwrap(), 0, 16),
            Err(crate::wordbooks::repository::replay::ReplayError::UnshareableDependency(_))
        ));
        assert!(!a
            .state
            .lock()
            .unwrap()
            .trusted
            .contains_key(&hex::encode(snow_public(&c.key).unwrap())));
        assert!(ar.list_favorites().unwrap().is_empty());
        let forged = cr
            .exchange_batch(&br.replay_device_id().unwrap(), 0, 1)
            .unwrap()
            .remove(0);
        assert!(matches!(
            ar.exchange_ingest(&br.replay_device_id().unwrap(), &forged),
            Err(crate::wordbooks::repository::replay::ReplayError::UnauthorizedOrigin)
        ));
    }
    #[test]
    fn mismatch_cancels() {
        let dir = tempfile::tempdir().unwrap();
        let a = LanService::open(dir.path()).unwrap();
        a.state.lock().unwrap().pending.insert(
            "request".into(),
            Pending {
                name: "B".into(),
                code: "12345678".into(),
                decision: None,
                expires: Instant::now() + TIMEOUT,
            },
        );
        assert!(a.decide("request", Some("87654321")).is_err());
        assert_eq!(
            a.state.lock().unwrap().pending["request"].decision,
            Some(false)
        );
        assert!(a.status().trusted.is_empty());
    }
}
