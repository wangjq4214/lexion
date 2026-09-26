use super::*;
use rusqlite::OptionalExtension;
use tempfile::tempdir;

struct MapProjection;
fn decode(change: &Envelope) -> Result<Vec<&str>, ReplayError> {
    let text =
        std::str::from_utf8(&change.content).map_err(|_| ReplayError::Projection("utf8".into()))?;
    let parts: Vec<_> = text.split(':').collect();
    match parts.as_slice() {
        ["set", _, _] | ["del", _] => Ok(parts),
        _ => Err(ReplayError::Projection("unknown test operation".into())),
    }
}
impl Projection for MapProjection {
    fn validate(&self, change: &Envelope) -> Result<(), ReplayError> {
        decode(change).map(|_| ())
    }
    fn reset(&self, tx: &Transaction<'_>) -> Result<(), ReplayError> {
        tx.execute_batch("CREATE TABLE IF NOT EXISTS replay_test_map (key TEXT PRIMARY KEY, value TEXT); DELETE FROM replay_test_map;")?;
        Ok(())
    }
    fn apply(&self, tx: &Transaction<'_>, change: &Envelope) -> Result<(), ReplayError> {
        match decode(change)?.as_slice() {
            ["set", key, value] => {
                tx.execute("INSERT INTO replay_test_map VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key,value])?;
            }
            ["del", key] => {
                tx.execute("DELETE FROM replay_test_map WHERE key=?1", [key])?;
            }
            _ => unreachable!(),
        }
        Ok(())
    }
}
fn value(repo: &WordbookRepository) -> Option<String> {
    let conn = repo.connect().unwrap();
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='replay_test_map')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    if !exists {
        return None;
    }
    conn.query_row("SELECT value FROM replay_test_map WHERE key='x'", [], |r| {
        r.get(0)
    })
    .optional()
    .unwrap()
}
fn ledger(repo: &WordbookRepository) -> Vec<(String, i64, Vec<u8>, bool)> {
    let conn = repo.connect().unwrap();
    let mut stmt = conn.prepare("SELECT device, sequence, content, applied FROM replay_changes ORDER BY device, sequence").unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}
fn remote(device: &str, sequence: u64, text: &str, dependencies: BTreeSet<ChangeId>) -> Envelope {
    Envelope {
        id: ChangeId {
            device: DeviceId(device.into()),
            sequence,
        },
        version: 1,
        content: text.as_bytes().to_vec(),
        dependencies,
        occurred_at: None,
    }
}
fn put(repo: &WordbookRepository, text: &str) -> Envelope {
    repo.append_local(text.as_bytes().to_vec(), None, &MapProjection)
        .unwrap()
}
fn repo(path: &std::path::Path, name: &str) -> WordbookRepository {
    WordbookRepository::open(path.join(name)).unwrap()
}

#[test]
fn three_devices_late_delete_converge_and_clock_never_orders() {
    let dir = tempdir().unwrap();
    let a = repo(dir.path(), "a");
    let b = repo(dir.path(), "b");
    let c = repo(dir.path(), "c");
    let first = a
        .append_local(b"set:x:old".to_vec(), Some(900), &MapProjection)
        .unwrap();
    b.ingest(&first, &MapProjection).unwrap();
    c.ingest(&first, &MapProjection).unwrap();
    let deletion = a
        .append_local(b"del:x".to_vec(), Some(800), &MapProjection)
        .unwrap();
    let edit = b
        .append_local(b"set:x:new".to_vec(), Some(-1000), &MapProjection)
        .unwrap();
    let extra = c
        .append_local(b"set:y:other".to_vec(), Some(123), &MapProjection)
        .unwrap();
    assert!(edit.dependencies.contains(&first.id));
    assert!(!edit.dependencies.contains(&deletion.id));
    for change in [&edit, &extra, &deletion] {
        if change.id.device != a.replay_device_id().unwrap() {
            a.ingest(change, &MapProjection).unwrap();
        }
    }
    for change in [&deletion, &extra] {
        b.ingest(change, &MapProjection).unwrap();
    }
    for change in [&edit, &deletion] {
        c.ingest(change, &MapProjection).unwrap();
    }
    let expected = if deletion.id < edit.id {
        Some("new".to_string())
    } else {
        None
    };
    for database in [&a, &b, &c] {
        assert_eq!(value(database), expected);
        let export = database.changes_since(&BTreeMap::new(), 20).unwrap();
        let map: BTreeMap<_, _> = export.into_iter().map(|e| (e.id.clone(), e)).collect();
        let order = canonical_order(&map).unwrap().0;
        assert_eq!(order.len(), 4);
        assert!(
            order.iter().position(|id| id == &first.id)
                < order.iter().position(|id| id == &deletion.id)
        );
    }
    assert!(!b.ingest(&edit, &MapProjection).unwrap().inserted); // exact local echo
    assert!(!a.ingest(&edit, &MapProjection).unwrap().inserted);
    let mut conflict = edit.clone();
    conflict.occurred_at = Some(42);
    assert!(matches!(
        a.ingest(&conflict, &MapProjection),
        Err(ReplayError::Conflict(_))
    ));
    assert_eq!(value(&a), expected);
}

#[test]
fn gaps_restart_export_and_local_frontier() {
    let dir = tempdir().unwrap();
    let a = repo(dir.path(), "a");
    let path = dir.path().join("b");
    let b = WordbookRepository::open(&path).unwrap();
    let one = put(&a, "set:x:one");
    let two = put(&a, "set:x:two");
    let three = put(&a, "del:x");
    let pending = b.ingest(&three, &MapProjection).unwrap();
    assert!(pending.missing.contains(&two.id));
    assert!(value(&b).is_none());
    assert!(b.changes_since(&BTreeMap::new(), 10).unwrap().is_empty());
    let own = put(&b, "set:y:own");
    assert!(!own.dependencies.contains(&three.id));
    drop(b);
    let b = WordbookRepository::open(&path).unwrap();
    assert_eq!(b.replay_device_id().unwrap(), own.id.device);
    b.ingest(&one, &MapProjection).unwrap();
    assert_eq!(value(&b), Some("one".into()));
    assert_eq!(
        b.changes_since(&BTreeMap::new(), 10)
            .unwrap()
            .iter()
            .filter(|e| e.id.device == one.id.device)
            .count(),
        1
    );
    b.ingest(&two, &MapProjection).unwrap();
    assert_eq!(value(&b), None);
    let page = b.changes_since(&BTreeMap::new(), 1).unwrap();
    assert_eq!(page.len(), 1);
    let mut cursor = BTreeMap::new();
    cursor.insert(page[0].id.device.clone(), page[0].id.sequence);
    let remainder = b.changes_since(&cursor, 10).unwrap();
    assert!(!remainder.iter().any(|e| e.id == page[0].id));
    assert_eq!(remainder.len(), 3);
    assert!(!b.ingest(&three, &MapProjection).unwrap().inserted);
}

#[test]
fn rollback_unknown_version_cycle_and_legacy_preservation() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("fresh");
    let a = WordbookRepository::open(&path).unwrap();
    let original = a.replay_device_id().unwrap();
    assert!(matches!(
        a.append_local(b"bad".to_vec(), None, &MapProjection),
        Err(ReplayError::Projection(_))
    ));
    assert!(a.changes_since(&BTreeMap::new(), 10).unwrap().is_empty());
    drop(a);
    let a = WordbookRepository::open(&path).unwrap();
    assert_eq!(a.replay_device_id().unwrap(), original);
    assert_eq!(put(&a, "set:x:ok").id.sequence, 1);
    let b = repo(dir.path(), "b");
    let unknown = Envelope {
        id: ChangeId {
            device: b.replay_device_id().unwrap(),
            sequence: 1,
        },
        version: 9,
        content: b"future".to_vec(),
        dependencies: BTreeSet::new(),
        occurred_at: None,
    };
    assert!(a
        .ingest(&unknown, &MapProjection)
        .unwrap()
        .unsupported_versions
        .contains(&unknown.id));
    assert_eq!(value(&a), Some("ok".into()));
    assert!(a
        .changes_since(&BTreeMap::new(), 10)
        .unwrap()
        .iter()
        .all(|e| e.id != unknown.id));
    assert!(a
        .append_local(b"set:x:next".to_vec(), None, &MapProjection)
        .is_err());
    let legacy_path = dir.path().join("legacy");
    let db = rusqlite::Connection::open(&legacy_path).unwrap();
    db.execute_batch("CREATE TABLE favorites(id INTEGER PRIMARY KEY, english TEXT, chinese TEXT, normalized_english TEXT); INSERT INTO favorites VALUES (1,'real','真实','real');").unwrap();
    drop(db);
    let legacy = WordbookRepository::open(&legacy_path).unwrap();
    assert_eq!(legacy.list_favorites().unwrap().len(), 1);
    assert!(matches!(
        legacy.append_local(b"set:x:no".to_vec(), None, &MapProjection),
        Err(ReplayError::LegacyBaseline)
    ));
    assert!(matches!(
        legacy.ingest(&unknown, &MapProjection),
        Err(ReplayError::LegacyBaseline)
    ));
    assert_eq!(
        WordbookRepository::open(&legacy_path)
            .unwrap()
            .list_favorites()
            .unwrap()
            .len(),
        1
    );
    let empty = repo(dir.path(), "empty");
    assert!(empty
        .append_local(b"set:x:yes".to_vec(), None, &MapProjection)
        .is_ok());
}

#[test]
fn pure_cycle_and_rejected_pending_payload_leave_ledger_unchanged() {
    let dir = tempdir().unwrap();
    let a = repo(dir.path(), "a");
    let b = repo(dir.path(), "b");
    let first = put(&a, "set:x:safe");
    b.ingest(&first, &MapProjection).unwrap();
    let device = b.replay_device_id().unwrap();
    let x = ChangeId {
        device: first.id.device.clone(),
        sequence: 2,
    };
    let y = ChangeId {
        device,
        sequence: 1,
    };
    let a2 = Envelope {
        id: x.clone(),
        version: 1,
        content: b"del:x".to_vec(),
        dependencies: BTreeSet::from([y.clone()]),
        occurred_at: None,
    };
    let b1 = Envelope {
        id: y.clone(),
        version: 1,
        content: b"set:x:unsafe".to_vec(),
        dependencies: BTreeSet::from([x.clone()]),
        occurred_at: None,
    };
    let map = BTreeMap::from([
        (first.id.clone(), first.clone()),
        (x.clone(), a2),
        (y.clone(), b1),
    ]);
    assert!(matches!(canonical_order(&map), Err(ReplayError::Cycle)));
    let corrupt = Envelope {
        id: ChangeId {
            device: first.id.device.clone(),
            sequence: 3,
        },
        version: 1,
        content: b"bad".to_vec(),
        dependencies: BTreeSet::new(),
        occurred_at: None,
    };
    assert!(matches!(
        b.ingest(&corrupt, &MapProjection),
        Err(ReplayError::Projection(_))
    ));
    assert_eq!(value(&b), Some("safe".into()));
    assert_eq!(ledger(&b).len(), 1);
    let second = put(&a, "set:x:good");
    b.ingest(&second, &MapProjection).unwrap();
    let third = put(&a, "del:x");
    assert_eq!(third.id, corrupt.id);
    b.ingest(&third, &MapProjection).unwrap();
    assert_eq!(value(&b), None);
}

#[test]
fn cycle_with_missing_implicit_predecessor_rejected_and_recoverable() {
    let dir = tempdir().unwrap();
    let receiver = repo(dir.path(), "receiver");
    let a = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let b = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let a1 = remote(a, 1, "set:x:first", BTreeSet::new());
    let b1 = remote(b, 1, "set:x:second", BTreeSet::new());
    let a2 = remote(a, 2, "del:x", BTreeSet::from([b1.id.clone()]));
    let mut cyclic_b1 = b1.clone();
    cyclic_b1.dependencies.insert(a2.id.clone());
    assert!(receiver
        .ingest(&a2, &MapProjection)
        .unwrap()
        .missing
        .contains(&a1.id));
    let before = ledger(&receiver);
    assert!(matches!(
        receiver.ingest(&cyclic_b1, &MapProjection),
        Err(ReplayError::Cycle)
    ));
    assert_eq!(ledger(&receiver), before);
    assert_eq!(value(&receiver), None);
    receiver.ingest(&a1, &MapProjection).unwrap();
    receiver.ingest(&b1, &MapProjection).unwrap();
    assert_eq!(value(&receiver), None);
    assert_eq!(ledger(&receiver).len(), 3);
}

#[test]
fn local_frontier_removes_implicit_foreign_predecessor() {
    let dir = tempdir().unwrap();
    let receiver = repo(dir.path(), "receiver");
    let a = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let one = remote(a, 1, "set:x:one", BTreeSet::new());
    let two = remote(a, 2, "set:x:two", BTreeSet::new());
    receiver.ingest(&one, &MapProjection).unwrap();
    receiver.ingest(&two, &MapProjection).unwrap();
    let own = put(&receiver, "set:x:own");
    assert_eq!(own.dependencies, BTreeSet::from([two.id]));
}

#[test]
fn pending_malformed_payload_is_rejected_before_ledger_write() {
    let dir = tempdir().unwrap();
    let receiver = repo(dir.path(), "receiver");
    let a = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let first = remote(a, 1, "set:x:good", BTreeSet::new());
    let malformed = remote(a, 2, "bad", BTreeSet::new());
    assert!(matches!(
        receiver.ingest(&malformed, &MapProjection),
        Err(ReplayError::Projection(_))
    ));
    assert!(ledger(&receiver).is_empty());
    receiver.ingest(&first, &MapProjection).unwrap();
    assert_eq!(value(&receiver), Some("good".into()));
    assert_eq!(ledger(&receiver).len(), 1);
}

#[test]
fn local_echo_requires_exact_preexisting_envelope() {
    let dir = tempdir().unwrap();
    let receiver = repo(dir.path(), "receiver");
    let original = put(&receiver, "set:x:good");
    let before = ledger(&receiver);
    assert!(!receiver.ingest(&original, &MapProjection).unwrap().inserted);
    let mut altered = original.clone();
    altered.content = b"set:x:forged".to_vec();
    let unknown = Envelope {
        id: ChangeId {
            device: original.id.device.clone(),
            sequence: 2,
        },
        ..original.clone()
    };
    for forged in [&altered, &unknown] {
        assert!(matches!(
            receiver.ingest(forged, &MapProjection),
            Err(ReplayError::Invalid(_))
        ));
    }
    assert_eq!(ledger(&receiver), before);
    assert_eq!(value(&receiver), Some("good".into()));
}

struct RecordingProjection {
    calls: std::cell::RefCell<Vec<ChangeId>>,
    fail_on: Option<ChangeId>,
}
impl Projection for RecordingProjection {
    fn validate(&self, change: &Envelope) -> Result<(), ReplayError> {
        MapProjection.validate(change)
    }
    fn reset(&self, tx: &Transaction<'_>) -> Result<(), ReplayError> {
        MapProjection.reset(tx)
    }
    fn apply(&self, tx: &Transaction<'_>, change: &Envelope) -> Result<(), ReplayError> {
        MapProjection.apply(tx, change)?;
        self.calls.borrow_mut().push(change.id.clone());
        if self.fail_on.as_ref() == Some(&change.id) {
            return Err(ReplayError::Projection("injected rebuild failure".into()));
        }
        Ok(())
    }
}

#[test]
fn late_earlier_concurrent_change_rebuilds_and_failure_rolls_back() {
    let dir = tempdir().unwrap();
    let receiver = repo(dir.path(), "receiver");
    let low = remote(
        "00000000000000000000000000000000",
        1,
        "set:x:early",
        BTreeSet::new(),
    );
    let high = remote(
        "ffffffffffffffffffffffffffffffff",
        1,
        "set:x:late",
        BTreeSet::new(),
    );
    receiver.ingest(&high, &MapProjection).unwrap();
    let before = ledger(&receiver);
    let failing = RecordingProjection {
        calls: std::cell::RefCell::new(Vec::new()),
        fail_on: Some(high.id.clone()),
    };
    assert!(matches!(
        receiver.ingest(&low, &failing),
        Err(ReplayError::Projection(_))
    ));
    assert_eq!(
        *failing.calls.borrow(),
        vec![low.id.clone(), high.id.clone()]
    );
    assert_eq!(ledger(&receiver), before); // new row and applied flags rolled back
    assert_eq!(value(&receiver), Some("late".into())); // reset and partial apply rolled back
    let recording = RecordingProjection {
        calls: std::cell::RefCell::new(Vec::new()),
        fail_on: None,
    };
    receiver.ingest(&low, &recording).unwrap();
    assert_eq!(
        *recording.calls.borrow(),
        vec![low.id.clone(), high.id.clone()]
    );
    assert_eq!(value(&receiver), Some("late".into()));
    assert_eq!(ledger(&receiver).len(), 2);
}
