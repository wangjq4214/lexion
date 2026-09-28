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
fn binding_replay_origin_requires_empty_history_and_survives_restart() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("bound.sqlite");
    let repo = WordbookRepository::open(&path).unwrap();
    let bound = DeviceId("abcd1234abcd1234abcd1234abcd1234".into());
    repo.bind_replay_origin(&bound).unwrap();
    assert_eq!(repo.replay_device_id().unwrap(), bound);
    repo.add_favorite("alpha", "甲").unwrap();
    assert_eq!(
        WordbookRepository::open(&path)
            .unwrap()
            .replay_device_id()
            .unwrap(),
        bound
    );
    assert!(matches!(
        repo.bind_replay_origin(&DeviceId("ffff1234abcd1234abcd1234abcd1234".into())),
        Err(ReplayError::Invalid(
            "existing replay history is not bound to LAN identity"
        ))
    ));
    assert_eq!(repo.list_favorites().unwrap().len(), 1);
}

#[test]
fn exchange_accepts_relay_but_rejects_missing_dependencies_gaps_and_legacy() {
    let dir = tempdir().unwrap();
    let a = repo(dir.path(), "a");
    let b = repo(dir.path(), "b");
    let c = repo(dir.path(), "c");
    b.add_favorite("first", "一").unwrap();
    let first = b
        .exchange_batch(&a.replay_device_id().unwrap(), &BTreeMap::new(), 1)
        .unwrap()
        .remove(0);
    c.add_favorite("third", "三").unwrap();
    let c_change = c
        .exchange_batch(&b.replay_device_id().unwrap(), &BTreeMap::new(), 1)
        .unwrap()
        .remove(0);
    assert!(
        a.exchange_ingest(&b.replay_device_id().unwrap(), &c_change)
            .unwrap()
            .inserted
    );
    let mut forged = first.clone();
    forged.dependencies.insert(ChangeId {
        device: c_change.id.device.clone(),
        sequence: 2,
    });
    assert!(matches!(
        a.exchange_ingest(&b.replay_device_id().unwrap(), &forged),
        Err(ReplayError::Invalid("unavailable causal predecessor"))
    ));
    assert_eq!(a.list_favorites().unwrap().len(), 1);
    assert!(
        a.exchange_ingest(&b.replay_device_id().unwrap(), &first)
            .unwrap()
            .inserted
    );
    assert!(
        !a.exchange_ingest(&b.replay_device_id().unwrap(), &first)
            .unwrap()
            .inserted
    );
    let mut gap = first.clone();
    gap.id.sequence = 3;
    assert!(matches!(
        a.exchange_ingest(&b.replay_device_id().unwrap(), &gap),
        Err(ReplayError::Invalid("non-contiguous exchange operation"))
    ));
    assert_eq!(a.list_favorites().unwrap().len(), 2);
    let legacy_path = dir.path().join("legacy");
    {
        let conn = rusqlite::Connection::open(&legacy_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE favorites (english TEXT); INSERT INTO favorites VALUES ('old');",
        )
        .unwrap();
    }
    let legacy = WordbookRepository::open(&legacy_path).unwrap();
    assert!(matches!(
        legacy.exchange_cursors(&b.replay_device_id().unwrap()),
        Err(ReplayError::LegacyBaseline)
    ));
}

#[test]
fn relay_exports_causal_pages_with_original_ids_and_deduplicates_echoes() {
    let dir = tempdir().unwrap();
    let a = repo(dir.path(), "relay_a");
    let b = repo(dir.path(), "relay_b");
    let c = repo(dir.path(), "relay_c");
    let a_id = a.replay_device_id().unwrap();
    let b_id = b.replay_device_id().unwrap();
    let c_id = c.replay_device_id().unwrap();
    c.add_favorite("relay", "中").unwrap();
    let c_change = c
        .exchange_batch(&b_id, &BTreeMap::new(), 1)
        .unwrap()
        .remove(0);
    assert_eq!(c_change.id.device, c_id);
    b.exchange_ingest(&c_id, &c_change).unwrap();
    b.add_favorite("later", "后").unwrap();
    let cursors = b.exchange_cursors(&a_id).unwrap();
    assert_eq!(cursors[&c_id], 1);
    assert_eq!(cursors[&b_id], 1);
    assert_eq!(cursors[&a_id], 0);
    let first = b
        .exchange_batch(&a_id, &BTreeMap::new(), 1)
        .unwrap()
        .remove(0);
    assert_eq!(first, c_change);
    let mut after = BTreeMap::from([(c_id.clone(), 1)]);
    let second = b.exchange_batch(&a_id, &after, 1).unwrap().remove(0);
    assert_eq!(second.id.device, b_id);
    assert!(second.dependencies.contains(&first.id));
    assert!(matches!(
        a.exchange_ingest(&b_id, &second),
        Err(ReplayError::Invalid("unavailable causal predecessor"))
    ));
    assert!(a.exchange_ingest(&b_id, &first).unwrap().inserted);
    assert!(a.exchange_ingest(&b_id, &second).unwrap().inserted);
    assert!(!a.exchange_ingest(&b_id, &first).unwrap().inserted);
    assert!(!a.exchange_ingest(&b_id, &second).unwrap().inserted);
    assert_eq!(a.list_favorites().unwrap().len(), 2);
    assert_eq!(a.exchange_cursors(&b_id).unwrap()[&c_id], 1);
    after.insert(b_id.clone(), 1);
    assert!(b.exchange_batch(&a_id, &after, 16).unwrap().is_empty());
    let mut forged = first.clone();
    forged.occurred_at = Some(42);
    assert!(matches!(
        a.exchange_ingest(&b_id, &forged),
        Err(ReplayError::Conflict(_))
    ));
    assert!(c.exchange_ingest(&b_id, &second).unwrap().inserted);
    assert_eq!(c.list_favorites().unwrap().len(), 2);
}

#[test]
fn exchange_validates_cursors_gaps_and_local_identity() {
    let dir = tempdir().unwrap();
    let a = repo(dir.path(), "cursor_a");
    let b = repo(dir.path(), "cursor_b");
    let a_id = a.replay_device_id().unwrap();
    let b_id = b.replay_device_id().unwrap();
    a.add_favorite("own", "己").unwrap();
    let own = a
        .exchange_batch(&b_id, &BTreeMap::new(), 1)
        .unwrap()
        .remove(0);
    assert!(matches!(
        a.exchange_batch(&b_id, &BTreeMap::from([(a_id.clone(), 2)]), 1),
        Err(ReplayError::Invalid(_))
    ));
    assert!(matches!(
        a.exchange_batch(&b_id, &BTreeMap::from([(DeviceId("invalid".into()), 1)]), 1),
        Err(ReplayError::Invalid(_))
    ));
    assert!(matches!(
        a.exchange_batch(&b_id, &BTreeMap::new(), 0),
        Err(ReplayError::Invalid(_))
    ));
    assert!(matches!(
        a.exchange_batch(&b_id, &BTreeMap::new(), 17),
        Err(ReplayError::Invalid(_))
    ));
    assert!(!a.exchange_ingest(&b_id, &own).unwrap().inserted);
    let mut forged = own.clone();
    forged.content.push(0);
    assert!(matches!(
        a.exchange_ingest(&b_id, &forged),
        Err(ReplayError::Invalid(_))
    ));
    forged.id.sequence = 2;
    assert!(matches!(
        a.exchange_ingest(&b_id, &forged),
        Err(ReplayError::Invalid(_))
    ));
    let third_party = DeviceId("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into());
    let hole = remote(&third_party.0, 2, "set:x:hole", BTreeSet::new());
    assert!(matches!(
        a.exchange_ingest(&b_id, &hole),
        Err(ReplayError::Invalid("non-contiguous exchange operation"))
    ));
    assert!(!a
        .exchange_cursors(&b_id)
        .unwrap()
        .contains_key(&third_party));
    assert_eq!(ledger(&a).len(), 1);
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
