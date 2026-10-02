//! Regression boundaries: actual Noise/SQLite exchange plus deterministic feedback transitions.
use super::tests::{connect_pair, mutual_trust, network_peer, protocol_transport_pair};
use super::*;

#[test]
fn retry_exhaustion_keeps_failure_visible_until_success() {
    let dir = tempfile::tempdir().unwrap();
    let other_dir = tempfile::tempdir().unwrap();
    let (a, _) = network_peer(dir.path());
    let (b, _) = network_peer(other_dir.path());
    mutual_trust(&a, &b);
    a.set_sync(&b.local_id, 0, "synced", None, true);
    let previous = a.status().sync[0].last_sync.clone();
    let generation = a.state.lock().unwrap().successes[&b.local_id];
    for failure in 1..=4 {
        a.set_sync(&b.local_id, 0, "syncing", None, false);
        a.record_failure(&b.local_id, 0, generation, "timeout");
        let view = &a.status().sync[0];
        assert_eq!(view.last_sync, previous);
        assert_eq!(view.retry_attempt, failure.min(3));
        if failure <= 3 {
            assert_eq!(view.state, "retrying");
            assert!(view.detail.is_none());
        } else {
            assert_eq!(view.state, "error");
            assert_eq!(view.detail.as_deref(), Some("timeout"));
        }
    }
    a.set_sync(&b.local_id, 0, "syncing", None, false);
    assert_eq!(
        a.status().sync[0].state,
        "error",
        "recovery probe must not dismiss exhausted failure"
    );
    a.record_failure(&b.local_id, 0, generation, "Peer disconnected");
    assert_eq!(a.status().sync[0].state, "error");
    a.set_sync(&b.local_id, 0, "synced", None, true);
    let status = a.status();
    assert_eq!(status.sync[0].state, "synced");
    assert_eq!(status.sync[0].progress, Some(100));
    assert_eq!(status.sync[0].retry_attempt, 0);
    assert!(status.sync[0].detail.is_none());
    a.record_failure(&b.local_id, 0, generation, "timeout");
    assert_eq!(
        a.status().sync[0].state,
        "synced",
        "older failure cannot overwrite newer success"
    );
    let generation = a.state.lock().unwrap().successes[&b.local_id];
    a.record_failure(&b.local_id, 0, generation, "timeout");
    assert_eq!(
        a.status().sync[0].state,
        "retrying",
        "successful recovery resets the retry budget"
    );
}

#[test]
fn fatal_errors_are_immediate_and_feedback_cannot_revive_removed_peers() {
    let dir = tempfile::tempdir().unwrap();
    let other_dir = tempfile::tempdir().unwrap();
    let (a, _) = network_peer(dir.path());
    let (b, _) = network_peer(other_dir.path());
    mutual_trust(&a, &b);
    for error in [
        "Sync protocol version mismatch: local 4, remote 3",
        "Replay origin is not bound to authenticated Noise key",
        "Sync ingest: database failure",
    ] {
        a.set_sync(&b.local_id, 0, "synced", None, true);
        let generation = a.state.lock().unwrap().successes[&b.local_id];
        a.record_failure(&b.local_id, 0, generation, error);
        assert_eq!(a.status().sync[0].state, "error");
        assert_eq!(a.status().sync[0].detail.as_deref(), Some(error));
        a.set_sync(&b.local_id, 0, "syncing", None, false);
        assert_eq!(a.status().sync[0].detail.as_deref(), Some(error));
    }
    a.remove(&hex::encode(snow_public(&b.key).unwrap()))
        .unwrap();
    a.record_failure(&b.local_id, 0, 0, "timeout");
    a.set_sync(&b.local_id, 0, "synced", None, true);
    assert!(a.status().sync.is_empty());
    assert!(a.state.lock().unwrap().failures.is_empty());
}

#[test]
fn percentages_count_both_directions_without_overflow_or_early_completion() {
    let x = DeviceId("a".repeat(32));
    let y = DeviceId("b".repeat(32));
    let mut round = SyncRound::new(
        BTreeMap::from([(x.clone(), 10), (y.clone(), 4)]),
        BTreeMap::from([(x.clone(), 6), (y.clone(), 10)]),
    );
    assert_eq!(round.total, 10);
    assert_eq!(round.percentage(), 0);
    round.completed = 4;
    assert_eq!(round.percentage(), 40);
    round.completed = 10;
    assert_eq!(round.percentage(), 99);
    let empty = SyncRound::new(BTreeMap::new(), BTreeMap::new());
    assert_eq!(empty.percentage(), 0);
    let huge = SyncRound::new(
        BTreeMap::from([(x, i64::MAX as u64), (y, i64::MAX as u64)]),
        BTreeMap::new(),
    );
    assert_eq!(huge.total, 2 * i64::MAX as u128);
}

#[test]
fn snapshot_export_excludes_later_writes_before_limiting_the_batch() {
    let dir = tempfile::tempdir().unwrap();
    let other_dir = tempfile::tempdir().unwrap();
    let (_, ar) = network_peer(dir.path());
    let (_, br) = network_peer(other_dir.path());
    let peer = br.replay_device_id().unwrap();
    ar.add_favorite("first", "一").unwrap();
    let snapshot = ar.exchange_cursors(&peer).unwrap();
    ar.add_favorite("later", "二").unwrap();
    let batch = ar
        .exchange_batch_until(&peer, &BTreeMap::new(), &snapshot, 16)
        .unwrap();
    assert_eq!(batch.len(), 1);
    let after = BTreeMap::from([(batch[0].id.device.clone(), batch[0].id.sequence)]);
    assert!(ar
        .exchange_batch_until(&peer, &after, &snapshot, 16)
        .unwrap()
        .is_empty());
    assert_eq!(ar.exchange_batch(&peer, &after, 16).unwrap().len(), 1);
}

#[test]
fn sending_advances_only_after_remote_application_acknowledgement() {
    let dir = tempfile::tempdir().unwrap();
    let other_dir = tempfile::tempdir().unwrap();
    let (a, ar) = network_peer(dir.path());
    let (b, br) = network_peer(other_dir.path());
    mutual_trust(&a, &b);
    ar.add_favorite("sent", "发送").unwrap();
    br.add_favorite("received", "接收").unwrap();
    let peer = br.replay_device_id().unwrap();
    let origin = ar.replay_device_id().unwrap();
    let local = ar.exchange_cursors(&peer).unwrap();
    let remote = br.exchange_cursors(&origin).unwrap();
    let (mut left, mut tx, mut right, mut rx) = protocol_transport_pair();
    let sender = {
        let a = a.clone();
        let id = b.local_id.clone();
        thread::spawn(move || {
            let mut round = SyncRound::new(local, remote.clone());
            a.set_sync(&id, 0, "syncing", None, false);
            a.update_progress(&id, 0, &round);
            a.send_changes(
                &ar,
                &id,
                0,
                &peer,
                remote,
                &mut round,
                &mut left,
                &mut tx,
                Instant::now() + Duration::from_secs(5),
            )
        })
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    let SyncMessage::Change(change) = receive_sync(&mut right, &mut rx, deadline).unwrap() else {
        panic!("expected change")
    };
    assert_eq!(a.status().sync[0].progress, Some(0));
    br.exchange_ingest(&origin, &change).unwrap();
    send_sync(&mut right, &mut rx, &SyncMessage::Applied).unwrap();
    assert!(matches!(
        receive_sync(&mut right, &mut rx, deadline).unwrap(),
        SyncMessage::Done { complete: true }
    ));
    assert_eq!(a.status().sync[0].progress, Some(50));
    send_sync(&mut right, &mut rx, &SyncMessage::Applied).unwrap();
    assert!(sender.join().unwrap().is_ok());
    assert_eq!(
        a.status().sync[0].state,
        "syncing",
        "one direction cannot finish the round"
    );
}

#[test]
fn multiple_pages_sync_in_both_directions_without_retry_errors() {
    let dir = tempfile::tempdir().unwrap();
    let other_dir = tempfile::tempdir().unwrap();
    let (a, ar) = network_peer(dir.path());
    let (b, br) = network_peer(other_dir.path());
    mutual_trust(&a, &b);
    for _ in 0..(MAX_SYNC_CHANGES + 2) {
        ar.record_mistake("large", "多页").unwrap();
        br.record_mistake("reverse", "反向").unwrap();
    }
    let (left, right) = connect_pair(&a, &b);
    assert!(left.is_ok(), "{left:?}");
    assert!(right.is_ok(), "{right:?}");
    assert_eq!(
        br.list_mistakes()
            .unwrap()
            .iter()
            .find(|word| word.english == "large")
            .unwrap()
            .error_count as usize,
        MAX_SYNC_CHANGES + 2
    );
    assert_eq!(ar.list_mistakes().unwrap().len(), 2);
    for service in [&a, &b] {
        let status = service.status();
        assert_eq!(status.sync[0].state, "synced");
        assert_eq!(status.sync[0].progress, Some(100));
        assert_eq!(status.sync[0].retry_attempt, 0);
        assert!(status.sync[0].detail.is_none());
        assert!(status.error.is_none());
    }
    let (left, right) = connect_pair(&a, &b);
    assert!(left.is_ok() && right.is_ok(), "empty subsequent round");
    assert_eq!(a.status().sync[0].progress, Some(100));
}

#[test]
fn automatic_connection_failures_count_address_cycles_not_addresses() {
    let dir = tempfile::tempdir().unwrap();
    let other_dir = tempfile::tempdir().unwrap();
    let (a, _) = network_peer(dir.path());
    let (b, _) = network_peer(other_dir.path());
    mutual_trust(&a, &b);
    a.state.lock().unwrap().discovered.insert(
        b.local_id.clone(),
        Discovered {
            fullname: "b.local.".into(),
            name: "B".into(),
            addresses: vec!["127.0.0.1:0".parse().unwrap(); 3],
        },
    );
    for failure in 1..=4 {
        a.pair(&b.local_id).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let state = a.state.lock().unwrap();
            if !state.connecting.contains(&b.local_id) {
                break;
            }
            drop(state);
            assert!(Instant::now() < deadline, "connection cycle did not finish");
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(a.state.lock().unwrap().failures[&b.local_id], failure);
        assert!(
            a.status().error.is_none(),
            "automatic failure must not leak into global pairing error"
        );
        assert_eq!(
            a.status().sync[0].state,
            if failure <= 3 { "retrying" } else { "error" }
        );
    }
}

#[test]
fn overlapping_connection_triggers_do_not_consume_multiple_retries() {
    let dir = tempfile::tempdir().unwrap();
    let other_dir = tempfile::tempdir().unwrap();
    let (a, _) = network_peer(dir.path());
    let (b, _) = network_peer(other_dir.path());
    mutual_trust(&a, &b);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    a.state.lock().unwrap().discovered.insert(
        b.local_id.clone(),
        Discovered {
            fullname: "b.local.".into(),
            name: "B".into(),
            addresses: vec![listener.local_addr().unwrap()],
        },
    );
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_frame(&mut stream, Instant::now() + Duration::from_secs(5)).unwrap();
        ready_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    a.pair(&b.local_id).unwrap();
    ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    a.pair(&b.local_id).unwrap();
    assert_eq!(a.state.lock().unwrap().connecting.len(), 1);
    release_tx.send(()).unwrap();
    server.join().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while a.state.lock().unwrap().connecting.contains(&b.local_id) {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(a.state.lock().unwrap().failures[&b.local_id], 1);
}

#[test]
fn simultaneous_dials_converge_without_a_persistent_failure() {
    let dir = tempfile::tempdir().unwrap();
    let other_dir = tempfile::tempdir().unwrap();
    let (a, ar) = network_peer(dir.path());
    let (b, br) = network_peer(other_dir.path());
    mutual_trust(&a, &b);
    ar.add_favorite("left", "左").unwrap();
    br.add_favorite("right", "右").unwrap();
    let al = TcpListener::bind("127.0.0.1:0").unwrap();
    let bl = TcpListener::bind("127.0.0.1:0").unwrap();
    for (owner, peer, address) in [
        (&a, &b, bl.local_addr().unwrap()),
        (&b, &a, al.local_addr().unwrap()),
    ] {
        owner.state.lock().unwrap().discovered.insert(
            peer.local_id.clone(),
            Discovered {
                fullname: format!("{}.local.", peer.local_id),
                name: "peer".into(),
                addresses: vec![address],
            },
        );
    }
    let barrier = Arc::new(std::sync::Barrier::new(3));
    let servers: Vec<_> = [(a.clone(), al), (b.clone(), bl)]
        .into_iter()
        .map(|(owner, listener)| {
            let barrier = barrier.clone();
            thread::spawn(move || {
                let (stream, _) = listener.accept().unwrap();
                barrier.wait();
                owner.session(stream, false, None)
            })
        })
        .collect();
    a.pair(&b.local_id).unwrap();
    b.pair(&a.local_id).unwrap();
    barrier.wait();
    for server in servers {
        let _ = server.join().unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    for owner in [&a, &b] {
        loop {
            let state = owner.state.lock().unwrap();
            if state.connecting.is_empty() && state.active_peers.is_empty() {
                break;
            }
            drop(state);
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(owner.status().sync[0].state, "synced");
        assert!(owner.status().sync[0].detail.is_none());
        assert!(owner.status().error.is_none());
    }
    assert_eq!(ar.list_favorites().unwrap().len(), 2);
    assert_eq!(br.list_favorites().unwrap().len(), 2);
}

#[test]
fn premature_completion_and_out_of_snapshot_changes_are_rejected_without_application() {
    for premature_done in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let other_dir = tempfile::tempdir().unwrap();
        let (a, ar) = network_peer(dir.path());
        let (b, br) = network_peer(other_dir.path());
        mutual_trust(&a, &b);
        br.add_favorite("first", "一").unwrap();
        let origin = br.replay_device_id().unwrap();
        let local = ar.exchange_cursors(&origin).unwrap();
        let remote = br
            .exchange_cursors(&ar.replay_device_id().unwrap())
            .unwrap();
        br.add_favorite("outside", "二").unwrap();
        let changes = br
            .exchange_batch(&ar.replay_device_id().unwrap(), &BTreeMap::new(), 16)
            .unwrap();
        let (mut left, mut rx, mut right, mut tx) = protocol_transport_pair();
        let id = b.local_id.clone();
        let receiver = thread::spawn(move || {
            let mut round = SyncRound::new(local, remote);
            a.receive_changes(
                &ar,
                &id,
                0,
                &origin,
                &mut round,
                &mut left,
                &mut rx,
                Instant::now() + Duration::from_secs(5),
            )
        });
        let message = if premature_done {
            SyncMessage::Done { complete: true }
        } else {
            SyncMessage::Change(changes[1].clone())
        };
        send_sync(&mut right, &mut tx, &message).unwrap();
        let error = receiver.join().unwrap().unwrap_err();
        assert!(error.contains(if premature_done {
            "ended before"
        } else {
            "outside synchronization snapshot"
        }));
        let repo = WordbookRepository::open(dir.path().join("words.sqlite")).unwrap();
        assert!(repo.list_favorites().unwrap().is_empty());
    }
}

#[test]
fn older_sessions_cannot_overwrite_a_newer_rounds_progress_or_result() {
    let dir = tempfile::tempdir().unwrap();
    let other_dir = tempfile::tempdir().unwrap();
    let (a, _) = network_peer(dir.path());
    let (b, _) = network_peer(other_dir.path());
    mutual_trust(&a, &b);
    a.state
        .lock()
        .unwrap()
        .feedback_sessions
        .insert(b.local_id.clone(), 2);
    a.set_sync_for_session(&b.local_id, 0, Some(2), "syncing", None, false);
    let mut round = SyncRound::new(
        BTreeMap::from([(DeviceId("a".repeat(32)), 10)]),
        BTreeMap::new(),
    );
    round.session = Some(2);
    round.completed = 4;
    a.update_progress(&b.local_id, 0, &round);
    round.session = Some(1);
    round.completed = 8;
    a.update_progress(&b.local_id, 0, &round);
    a.set_sync_for_session(&b.local_id, 0, Some(1), "synced", None, true);
    a.record_failure_for_session(&b.local_id, 0, 0, Some(1), "Sync ingest: bad data");
    assert_eq!(a.status().sync[0].state, "syncing");
    assert_eq!(a.status().sync[0].progress, Some(40));
    a.set_sync_for_session(&b.local_id, 0, Some(2), "synced", None, true);
    round.completed = 9;
    a.update_progress(&b.local_id, 0, &round);
    assert_eq!(a.status().sync[0].progress, Some(100));
}

#[test]
fn incoming_failure_during_an_outgoing_cycle_is_visible_but_counted_only_once() {
    let dir = tempfile::tempdir().unwrap();
    let other_dir = tempfile::tempdir().unwrap();
    let (a, _) = network_peer(dir.path());
    let (b, _) = network_peer(other_dir.path());
    mutual_trust(&a, &b);
    a.state
        .lock()
        .unwrap()
        .connecting
        .insert(b.local_id.clone());
    let (mut left, mut transport, right, remote_transport) = protocol_transport_pair();
    drop(right);
    drop(remote_transport);
    let error = a
        .synchronize(&b.local_id, &mut left, &mut transport, false, 0, true)
        .unwrap_err();
    assert_eq!(
        a.status().sync[0].state,
        "retrying",
        "latest incoming failure must not leave feedback stuck syncing"
    );
    assert_eq!(a.state.lock().unwrap().failures[&b.local_id], 1);
    a.record_failure(&b.local_id, 0, 0, &error);
    assert_eq!(
        a.state.lock().unwrap().failures[&b.local_id],
        1,
        "the outgoing cycle must not count the same round failure twice"
    );
    a.set_sync(&b.local_id, 0, "synced", None, true);
    assert!(a.state.lock().unwrap().failed_attempts.is_empty());
}
