//! Internal durable operation log. Business projections own their own tables.
//! Remote replay never appends locally.
use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{params, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};

use super::{SyncProjection, WordbookRepository};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct DeviceId(pub String);

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct ChangeId {
    pub device: DeviceId,
    pub sequence: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub id: ChangeId,
    pub version: u32,
    pub content: Vec<u8>,
    pub dependencies: BTreeSet<ChangeId>,
    /// Business event occurrence, never used to order changes.
    pub occurred_at: Option<i64>,
}

#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error("database error: {0}")]
    Database(rusqlite::Error),
    #[error("invalid replay operation: {0}")]
    Invalid(&'static str),
    #[error("conflicting change: {0:?}")]
    Conflict(ChangeId),
    #[error("business conflict")]
    BusinessConflict,
    #[error("legacy baseline")]
    LegacyBaseline,
    #[error("sequence exhausted")]
    SequenceExhausted,
    #[error("cyclic dependencies")]
    Cycle,
    #[cfg(test)]
    #[error("projection error: {0}")]
    Projection(String),
    #[error("unauthorized origin")]
    UnauthorizedOrigin,
    #[error("unshareable dependency: {0:?}")]
    UnshareableDependency(ChangeId),
}
impl From<rusqlite::Error> for ReplayError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Database(value)
    }
}

/// A projection owns its reset and apply SQL; both run inside the log transaction.
/// A decoder must reject unknown content rather than silently mark it applied.
pub trait Projection {
    /// Decode a known operation without writing; pending changes must be valid on ingress.
    fn validate(&self, change: &Envelope) -> Result<(), ReplayError>;
    fn reset(&self, tx: &Transaction<'_>) -> Result<(), ReplayError>;
    fn apply(&self, tx: &Transaction<'_>, change: &Envelope) -> Result<(), ReplayError>;
    /// Restore local-only projection state after a reordered full replay, excluding new invalidations.
    fn finish_rebuild(
        &self,
        _tx: &Transaction<'_>,
        _newly_applied: &[Envelope],
    ) -> Result<(), ReplayError> {
        Ok(())
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct IngestResult {
    pub inserted: bool,
    pub missing: BTreeSet<ChangeId>,
    pub unsupported_versions: BTreeSet<ChangeId>,
}

fn valid(envelope: &Envelope) -> Result<(), ReplayError> {
    let id = &envelope.id;
    if id.sequence == 0
        || id.sequence > i64::MAX as u64
        || id.device.0.len() != 32
        || !id
            .device
            .0
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || envelope.version == 0
        || envelope.dependencies.contains(id)
        || envelope.dependencies.iter().any(|dep| {
            dep.sequence == 0
                || dep.sequence > i64::MAX as u64
                || dep.device.0.len() != 32
                || !dep
                    .device
                    .0
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                || (dep.device == id.device && dep.sequence >= id.sequence)
        })
    {
        return Err(ReplayError::Invalid(
            "invalid change identity or dependencies",
        ));
    }
    Ok(())
}

/// Canonical Kahn order: each available node with the smallest (device bytes, sequence)
/// wins. Same-origin sequence continuity is an implicit dependency, even if omitted.
/// Missing parents leave a durable unapplied suffix; a closed cycle is an error.
pub fn canonical_order(
    changes: &BTreeMap<ChangeId, Envelope>,
) -> Result<(Vec<ChangeId>, BTreeSet<ChangeId>), ReplayError> {
    let mut ordered = Vec::new();
    let mut done = BTreeSet::new();
    let mut missing = BTreeSet::new();
    loop {
        let next = changes.iter().find(|(id, change)| {
            !done.contains(*id)
                && change.dependencies.iter().all(|dep| done.contains(dep))
                && (id.sequence == 1
                    || done.contains(&ChangeId {
                        device: id.device.clone(),
                        sequence: id.sequence - 1,
                    }))
        });
        if let Some((id, _)) = next {
            done.insert(id.clone());
            ordered.push(id.clone());
        } else {
            break;
        }
    }
    for (id, change) in changes {
        if done.contains(id) {
            continue;
        }
        for dep in &change.dependencies {
            if !changes.contains_key(dep) {
                missing.insert(dep.clone());
            }
        }
        if id.sequence > 1 {
            let predecessor = ChangeId {
                device: id.device.clone(),
                sequence: id.sequence - 1,
            };
            if !changes.contains_key(&predecessor) {
                missing.insert(predecessor);
            }
        }
    }
    // Missing parents may strand a suffix, but cannot excuse a cycle among nodes
    // already present. Ignore only absent edges when checking the residual graph.
    if done.len() != changes.len() {
        let mut visited = done;
        loop {
            let next = changes.iter().find(|(id, change)| {
                !visited.contains(*id)
                    && change
                        .dependencies
                        .iter()
                        .all(|dep| !changes.contains_key(dep) || visited.contains(dep))
                    && (id.sequence == 1 || {
                        let predecessor = ChangeId {
                            device: id.device.clone(),
                            sequence: id.sequence - 1,
                        };
                        !changes.contains_key(&predecessor) || visited.contains(&predecessor)
                    })
            });
            if let Some((id, _)) = next {
                visited.insert(id.clone());
            } else {
                break;
            }
        }
        if visited.len() != changes.len() {
            return Err(ReplayError::Cycle);
        }
    }
    Ok((ordered, missing))
}

pub(super) fn has_legacy_data(tx: &Transaction<'_>) -> rusqlite::Result<bool> {
    // Literals only. Old schemas may have any subset of these tables.
    for table in [
        "wordbooks",
        "entries",
        "favorites",
        "mistakes",
        "pending_reviews",
        "review_memory",
        "review_coverage",
    ] {
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
            [table],
            |r| r.get(0),
        )?;
        if exists
            && tx.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table})"), [], |r| {
                r.get::<_, bool>(0)
            })?
        {
            return Ok(true);
        }
    }
    let settings: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='review_settings')",
        [],
        |r| r.get(0),
    )?;
    if settings
        && tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM review_settings WHERE id != 1 OR target_retention != 0.9)",
            [],
            |r| r.get::<_, bool>(0),
        )?
    {
        return Ok(true);
    }
    Ok(false)
}

pub(super) fn initialize(tx: &Transaction<'_>, legacy: bool) -> rusqlite::Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS replay_identity (
        singleton INTEGER PRIMARY KEY CHECK(singleton=1), device TEXT NOT NULL,
        next_sequence INTEGER NOT NULL CHECK(next_sequence > 0),
        legacy INTEGER NOT NULL CHECK(legacy IN (0,1))
    );
    CREATE TABLE IF NOT EXISTS replay_changes (
        device TEXT NOT NULL, sequence INTEGER NOT NULL CHECK(sequence > 0),
        version INTEGER NOT NULL, content BLOB NOT NULL, dependencies TEXT NOT NULL,
        occurred_at INTEGER, applied INTEGER NOT NULL DEFAULT 0 CHECK(applied IN (0,1)),
        PRIMARY KEY(device,sequence)
    );
    CREATE TRIGGER IF NOT EXISTS replay_no_delete BEFORE DELETE ON replay_changes
    BEGIN SELECT RAISE(ABORT, 'replay changes are immutable'); END;
    CREATE TRIGGER IF NOT EXISTS replay_no_edit BEFORE UPDATE OF device,sequence,version,content,dependencies,occurred_at ON replay_changes
    BEGIN SELECT RAISE(ABORT, 'replay changes are immutable'); END;")?;
    let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM replay_identity)", [], |r| {
        r.get(0)
    })?;
    if !exists {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).map_err(|error| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(
                error.to_string(),
            )))
        })?;
        let device = bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        tx.execute(
            "INSERT INTO replay_identity VALUES (1, ?1, 1, ?2)",
            params![device, legacy],
        )?;
    }
    Ok(())
}

fn enabled(tx: &Transaction<'_>) -> Result<(DeviceId, u64), ReplayError> {
    let (device, sequence, legacy): (String, i64, bool) = tx.query_row(
        "SELECT device,next_sequence,legacy FROM replay_identity WHERE singleton=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    if legacy {
        return Err(ReplayError::LegacyBaseline);
    }
    Ok((DeviceId(device), sequence as u64))
}

fn load(tx: &Transaction<'_>) -> Result<BTreeMap<ChangeId, Envelope>, ReplayError> {
    let mut statement = tx.prepare(
        "SELECT device,sequence,version,content,dependencies,occurred_at FROM replay_changes",
    )?;
    let rows = statement.query_map([], |r| {
        let dependencies: String = r.get(4)?;
        let dependencies = serde_json::from_str(&dependencies).map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(err))
        })?;
        Ok(Envelope {
            id: ChangeId {
                device: DeviceId(r.get(0)?),
                sequence: r.get::<_, i64>(1)? as u64,
            },
            version: r.get(2)?,
            content: r.get(3)?,
            dependencies,
            occurred_at: r.get(5)?,
        })
    })?;
    let mut result = BTreeMap::new();
    for row in rows {
        let change = row?;
        valid(&change)?;
        result.insert(change.id.clone(), change);
    }
    Ok(result)
}

fn store(tx: &Transaction<'_>, change: &Envelope) -> Result<bool, ReplayError> {
    valid(change)?;
    let changes = load(tx)?;
    if let Some(existing) = changes.get(&change.id) {
        if existing != change {
            return Err(ReplayError::Conflict(change.id.clone()));
        }
        return Ok(false);
    }
    tx.execute("INSERT INTO replay_changes(device,sequence,version,content,dependencies,occurred_at) VALUES (?1,?2,?3,?4,?5,?6)",
        params![change.id.device.0, change.id.sequence as i64, change.version,
            change.content, serde_json::to_string(&change.dependencies).expect("serializing IDs cannot fail"), change.occurred_at])?;
    Ok(true)
}

fn rebuild(
    tx: &Transaction<'_>,
    projection: &impl Projection,
) -> Result<IngestResult, ReplayError> {
    let changes = load(tx)?;
    let (ordered, missing) = canonical_order(&changes)?;
    let unsupported_versions: BTreeSet<_> = ordered
        .iter()
        .filter(|id| changes[*id].version != 1)
        .cloned()
        .collect();
    let result = IngestResult {
        inserted: false,
        missing,
        unsupported_versions,
    };
    // Never claim partial application when a known operation cannot be decoded.
    if !result.unsupported_versions.is_empty() {
        return Ok(result);
    }
    let currently_applied: BTreeSet<ChangeId> = {
        let mut stmt = tx.prepare("SELECT device,sequence FROM replay_changes WHERE applied=1")?;
        let rows = stmt.query_map([], |r| {
            Ok(ChangeId {
                device: DeviceId(r.get(0)?),
                sequence: r.get::<_, i64>(1)? as u64,
            })
        })?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let ready: BTreeSet<_> = ordered.iter().cloned().collect();
    if currently_applied != ready {
        // An extension of the already-applied canonical prefix can be projected without
        // clearing unrelated local review coverage. A late operation before that prefix
        // still requires a full deterministic rebuild.
        let prefix = if currently_applied.is_empty() {
            false
        } else {
            let previous: BTreeMap<_, _> = changes
                .iter()
                .filter(|(id, _)| currently_applied.contains(*id))
                .map(|(id, change)| (id.clone(), change.clone()))
                .collect();
            let (previous_order, _) = canonical_order(&previous)?;
            ordered.starts_with(&previous_order)
        };
        if !prefix {
            projection.reset(tx)?;
        }
        for id in ordered
            .iter()
            .skip(if prefix { currently_applied.len() } else { 0 })
        {
            projection.apply(tx, &changes[id])?;
        }
        if !prefix {
            let new: Vec<_> = ordered
                .iter()
                .filter(|id| !currently_applied.contains(*id))
                .map(|id| changes[id].clone())
                .collect();
            projection.finish_rebuild(tx, &new)?;
        }
        tx.execute("UPDATE replay_changes SET applied=0", [])?;
        for id in &ordered {
            tx.execute(
                "UPDATE replay_changes SET applied=1 WHERE device=?1 AND sequence=?2",
                params![id.device.0, id.sequence as i64],
            )?;
        }
    }
    Ok(result)
}

impl WordbookRepository {
    pub(crate) fn replay_device_id(&self) -> Result<DeviceId, ReplayError> {
        let conn = self.connect()?;
        Ok(DeviceId(conn.query_row(
            "SELECT device FROM replay_identity WHERE singleton=1",
            [],
            |r| r.get(0),
        )?))
    }

    /// Commits a version-1 local operation and its projection atomically. Dependencies
    /// include only fully applied operations, not merely received or pending records.
    pub(crate) fn append_local(
        &self,
        content: Vec<u8>,
        occurred_at: Option<i64>,
        projection: &impl Projection,
    ) -> Result<Envelope, ReplayError> {
        self.append_local_checked(content, occurred_at, projection, |_| Ok(()))
    }

    /// Check business preconditions under the same write lock as the operation and log.
    pub(crate) fn append_local_checked(
        &self,
        content: Vec<u8>,
        occurred_at: Option<i64>,
        projection: &impl Projection,
        check: impl FnOnce(&Transaction<'_>) -> Result<(), ReplayError>,
    ) -> Result<Envelope, ReplayError> {
        self.append_local_build(occurred_at, projection, |tx| {
            check(tx)?;
            Ok(content)
        })
    }

    pub(crate) fn append_local_build(
        &self,
        occurred_at: Option<i64>,
        projection: &impl Projection,
        build: impl FnOnce(&Transaction<'_>) -> Result<Vec<u8>, ReplayError>,
    ) -> Result<Envelope, ReplayError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let content = build(&tx)?;
        let (device, next) = enabled(&tx)?;
        if next >= i64::MAX as u64 {
            return Err(ReplayError::SequenceExhausted);
        }
        let applied = {
            let mut stmt =
                tx.prepare("SELECT device,sequence FROM replay_changes WHERE applied=1")?;
            let rows = stmt.query_map([], |r| {
                Ok(ChangeId {
                    device: DeviceId(r.get(0)?),
                    sequence: r.get::<_, i64>(1)? as u64,
                })
            })?;
            rows.collect::<rusqlite::Result<BTreeSet<_>>>()?
        };
        let changes = load(&tx)?;
        let mut dependencies = applied.clone();
        for change in changes.values() {
            if applied.contains(&change.id) {
                for dep in &change.dependencies {
                    dependencies.remove(dep);
                }
                if change.id.sequence > 1 {
                    dependencies.remove(&ChangeId {
                        device: change.id.device.clone(),
                        sequence: change.id.sequence - 1,
                    });
                }
            }
        }
        let envelope = Envelope {
            id: ChangeId {
                device,
                sequence: next,
            },
            version: 1,
            content,
            dependencies,
            occurred_at,
        };
        projection.validate(&envelope)?;
        store(&tx, &envelope)?;
        tx.execute(
            "UPDATE replay_identity SET next_sequence=?1 WHERE singleton=1",
            [next as i64 + 1],
        )?;
        let result = rebuild(&tx, projection)?;
        if !result.unsupported_versions.is_empty() {
            return Err(ReplayError::Invalid(
                "unsupported operation version blocks local commit",
            ));
        }
        tx.commit()?;
        Ok(envelope)
    }

    pub(crate) fn ingest(
        &self,
        change: &Envelope,
        projection: &impl Projection,
    ) -> Result<IngestResult, ReplayError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (local, _) = enabled(&tx)?;
        // An exact echo of an already committed local operation is harmless;
        // a new or altered claim on our identity is never accepted.
        if change.id.device == local {
            valid(change)?;
            if load(&tx)?.get(&change.id) != Some(change) {
                return Err(ReplayError::Invalid("remote change claims local identity"));
            }
        } else if change.version == 1 {
            valid(change)?;
            projection.validate(change)?;
        }
        let inserted = store(&tx, change)?;
        let mut result = rebuild(&tx, projection)?;
        result.inserted = inserted;
        tx.commit()?;
        Ok(result)
    }

    /// A cursor is a per-origin *contiguous* watermark, not a global arrival index.
    /// Only applied operations are exported; missing predecessors never leak as complete.
    /// Internal unrestricted export for replay tests; network export uses `exchange_batch`.
    #[cfg(test)]
    pub(crate) fn changes_since(
        &self,
        cursors: &BTreeMap<DeviceId, u64>,
        limit: usize,
    ) -> Result<Vec<Envelope>, ReplayError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        enabled(&tx)?;
        let changes = load(&tx)?;
        let mut stmt = tx.prepare("SELECT device,sequence FROM replay_changes WHERE applied=1")?;
        let applied: BTreeSet<ChangeId> = stmt
            .query_map([], |r| {
                Ok(ChangeId {
                    device: DeviceId(r.get(0)?),
                    sequence: r.get::<_, i64>(1)? as u64,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let mut watermarks = cursors.clone();
        let mut output = Vec::new();
        for (id, change) in changes {
            let watermark = watermarks.entry(id.device.clone()).or_default();
            if id.sequence <= *watermark {
                continue;
            }
            if output.len() < limit
                && id.sequence == watermark.saturating_add(1)
                && applied.contains(&id)
            {
                *watermark = id.sequence;
                output.push(change);
            }
        }
        Ok(output)
    }

    /// Bind a fresh replay identity to the authenticated LAN static key before any
    /// local operation is written. Historical operations are never rewritten or cleared.
    pub(crate) fn bind_replay_origin(&self, expected: &DeviceId) -> Result<(), ReplayError> {
        if expected.0.len() != 32
            || !expected
                .0
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(ReplayError::Invalid("invalid bound replay identity"));
        }
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (current, _) = enabled(&tx)?;
        if current != *expected {
            let count: i64 =
                tx.query_row("SELECT COUNT(*) FROM replay_changes", [], |row| row.get(0))?;
            if count != 0 {
                return Err(ReplayError::Invalid(
                    "existing replay history is not bound to LAN identity",
                ));
            }
            tx.execute(
                "UPDATE replay_identity SET device=?1 WHERE singleton=1",
                [&expected.0],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Applied, contiguous watermarks for only the two directly authorized origins.
    pub(crate) fn exchange_cursors(
        &self,
        peer: &DeviceId,
    ) -> Result<BTreeMap<DeviceId, u64>, ReplayError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let (local, _) = enabled(&tx)?;
        if *peer == local {
            return Err(ReplayError::UnauthorizedOrigin);
        }
        let mut result = BTreeMap::new();
        for origin in [&local, peer] {
            let mut stmt = tx.prepare("SELECT sequence FROM replay_changes WHERE device=?1 AND applied=1 ORDER BY sequence")?;
            let mut rows = stmt.query([&origin.0])?;
            let mut contiguous = 0u64;
            while let Some(row) = rows.next()? {
                let sequence = row.get::<_, i64>(0)? as u64;
                if sequence != contiguous + 1 {
                    break;
                }
                contiguous = sequence;
            }
            result.insert(origin.clone(), contiguous);
        }
        Ok(result)
    }

    /// Export only our own operations. Reject an unshareable causal suffix rather
    /// than forwarding third-party history or pretending its cursor advanced.
    pub(crate) fn exchange_batch(
        &self,
        peer: &DeviceId,
        after: u64,
        limit: usize,
    ) -> Result<Vec<Envelope>, ReplayError> {
        if limit == 0 || limit > 16 {
            return Err(ReplayError::Invalid("invalid batch limit"));
        }
        if after > i64::MAX as u64 {
            return Err(ReplayError::Invalid("invalid exchange cursor"));
        }
        let local = self.replay_device_id()?;
        if local == *peer {
            return Err(ReplayError::UnauthorizedOrigin);
        }
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        enabled(&tx)?;
        let changes = load(&tx)?;
        let mut output = Vec::new();
        for (next, change) in (after.checked_add(1).ok_or(ReplayError::SequenceExhausted)?..).zip(
            changes
                .values()
                .filter(|change| change.id.device == local && change.id.sequence > after),
        ) {
            if output.len() == limit {
                break;
            }
            if change.id.sequence != next {
                break;
            }
            let applied: bool = tx.query_row(
                "SELECT applied FROM replay_changes WHERE device=?1 AND sequence=?2",
                params![local.0, next as i64],
                |row| row.get(0),
            )?;
            if !applied {
                break;
            }
            if let Some(dependency) = change
                .dependencies
                .iter()
                .find(|dep| dep.device != local && dep.device != *peer)
            {
                return Err(ReplayError::UnshareableDependency(dependency.clone()));
            }
            output.push(change.clone());
        }
        Ok(output)
    }

    /// Authenticated peer identity is supplied by the Noise boundary, never by the wire.
    /// Reject foreign origins and causal predecessors before opening a write transaction.
    pub(crate) fn exchange_ingest(
        &self,
        peer: &DeviceId,
        change: &Envelope,
    ) -> Result<IngestResult, ReplayError> {
        let local = self.replay_device_id()?;
        if *peer == local || change.id.device != *peer {
            return Err(ReplayError::UnauthorizedOrigin);
        }
        valid(change)?;
        if let Some(dependency) = change
            .dependencies
            .iter()
            .find(|dep| dep.device != local && dep.device != *peer)
        {
            return Err(ReplayError::UnshareableDependency(dependency.clone()));
        }
        if change.version != 1 {
            return Err(ReplayError::Invalid("unsupported operation version"));
        }
        let cursors = self.exchange_cursors(peer)?;
        let current = cursors[peer];
        if change.id.sequence > current.saturating_add(1) {
            return Err(ReplayError::Invalid("non-contiguous exchange operation"));
        }
        if change
            .dependencies
            .iter()
            .any(|dep| dep.sequence > cursors[&dep.device])
        {
            return Err(ReplayError::Invalid("unavailable causal predecessor"));
        }
        let result = self.ingest(change, &SyncProjection)?;
        if !result.unsupported_versions.is_empty() || !result.missing.is_empty() {
            return Err(ReplayError::Invalid("incomplete exchange operation"));
        }
        Ok(result)
    }
}

#[cfg(test)]
#[path = "replay_tests.rs"]
mod tests;
