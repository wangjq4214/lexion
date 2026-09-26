//! Discovery and first-pairing only. No learning data is accepted on this protocol.
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use snow::{Builder, TransportState};
use tauri::State;

const SERVICE: &str = "_lexion-pair._tcp.local.";
const NOISE: &str = "Noise_XX_25519_ChaChaPoly_SHA256";
const TIMEOUT: Duration = Duration::from_secs(120);
const IO_TIMEOUT: Duration = Duration::from_millis(250);
const MAX_FRAME: usize = 4096;

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
}
pub struct LanService {
    local_id: String,
    key: [u8; 32],
    trust_path: PathBuf,
    state: Arc<Mutex<Shared>>,
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
}

fn identity_id(key: &[u8]) -> String {
    hex::encode(Sha256::digest(key))
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
fn load_trust(path: &Path) -> Result<HashMap<String, String>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let peers = serde_json::from_slice::<TrustFile>(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?
        .peers;
    if peers
        .keys()
        .any(|id| hex::decode(id).map_or(true, |key| key.len() != 32))
    {
        return Err("Invalid LAN trust identity".into());
    }
    Ok(peers)
}
impl LanService {
    fn open(dir: &Path) -> Result<Arc<Self>> {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let key = load_identity(dir)?;
        let trust_path = dir.join("lan-trust.json");
        let trusted = load_trust(&trust_path)?;
        Ok(Arc::new(Self {
            local_id: identity_id(&snow_public(&key)?),
            key,
            trust_path,
            state: Arc::new(Mutex::new(Shared {
                discovered: HashMap::new(),
                pending: HashMap::new(),
                trusted,
                error: None,
                authenticated: Default::default(),
                reconnect_attempts: HashMap::new(),
            })),
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
        LanStatus {
            peers,
            pending,
            trusted,
            error: state.error.clone(),
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
                        let removed: Vec<_> = state
                            .discovered
                            .iter()
                            .filter(|(_, p)| p.fullname == fullname)
                            .map(|(id, _)| id.clone())
                            .collect();
                        for id in removed {
                            state.discovered.remove(&id);
                            state.authenticated.remove(&id);
                            state.reconnect_attempts.remove(&id);
                        }
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
            })
            .map_err(|e| e.to_string())?,
        )?;
        state.trusted = peers;
        Ok(())
    }
    fn session(
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
            self.state.lock().unwrap().authenticated.insert(peer_id);
            return Ok(()); // Both sides remember the authenticated key; no data transport yet.
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
        result.map_err(|e| format!("Pairing decision: {e}"))
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
            match receive_message(stream, transport, Instant::now() + IO_TIMEOUT) {
                Ok(Message::Confirm) => received = true,
                Ok(Message::Cancel) => return Err("Remote device cancelled pairing".into()),
                Ok(_) => return Err("Unexpected pairing message".into()),
                Err(e) if e == "timeout" => {}
                Err(e) => return Err(e),
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
fn send_frame(stream: &mut TcpStream, bytes: &[u8]) -> Result<()> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err("Invalid frame size".into());
    }
    stream
        .write_all(&(bytes.len() as u16).to_be_bytes())
        .and_then(|_| stream.write_all(bytes))
        .map_err(|e| e.to_string())
}
fn read_frame(stream: &mut TcpStream, deadline: Instant) -> Result<Vec<u8>> {
    let mut length = [0u8; 2];
    // An idle socket can be polled; after one byte arrives, a partial frame must
    // complete or close rather than lose framing on the next poll.
    read_exact_until(stream, &mut length[..1], deadline)?;
    let frame_deadline = deadline.max(Instant::now() + Duration::from_secs(3));
    read_exact_until(stream, &mut length[1..], frame_deadline)?;
    let size = u16::from_be_bytes(length) as usize;
    if size == 0 || size > MAX_FRAME {
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
impl LanBackend {
    pub fn open(dir: &Path) -> Self {
        match LanService::open(dir) {
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
        let backend = LanBackend::open(dir.path());
        assert!(backend.service.is_none()); // Never starts the listener or mDNS on invalid keys.
        assert!(backend.error.is_some());
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
        assert!(initiator.join().unwrap().is_ok());
        assert!(responder.join().unwrap().is_ok());
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
        assert!(responder.join().unwrap().is_ok());
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
