use super::{replay::Envelope, ImportedEntry, RepositoryError, SyncProjection, WordbookRepository};
use std::collections::BTreeMap;
use tempfile::tempdir;

fn exchange(from: &WordbookRepository, to: &WordbookRepository) {
    for change in from.changes_since(&BTreeMap::new(), 100).unwrap() {
        to.ingest(&change, &SyncProjection).unwrap();
    }
}

#[test]
fn remove_mistake_preserves_other_meanings_content_and_shared_memory() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("db");
    let repo = WordbookRepository::open(&path).unwrap();
    let book = repo
        .replace(
            "book",
            &[ImportedEntry {
                english: "Apple".into(),
                chinese: "苹果".into(),
            }],
            false,
        )
        .unwrap();
    repo.add_favorite("Apple", "苹果").unwrap();
    repo.record_mistake_once("Apple", "苹果", "old").unwrap();
    repo.record_mistake("APPLE", "苹果").unwrap();
    repo.record_mistake("Apple", "水果").unwrap();
    let questions = repo
        .schedule_practice("wordbook", Some(book.id), 1, "zh-to-en")
        .unwrap();
    repo.complete_review(questions[0].review_id, 1, 0, false)
        .unwrap();
    let memory_before: (f64, f64, f64) = repo.connect().unwrap().query_row(
        "SELECT stability,last_reviewed,due_at FROM review_memory WHERE normalized_english='apple' AND chinese='苹果'",
        [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    ).unwrap();
    assert!(repo.remove_mistake(" APPLE ", " 苹果 ").unwrap());
    assert!(!repo.remove_mistake("apple", "苹果").unwrap());
    assert_eq!(repo.list_mistakes().unwrap().len(), 1);
    assert_eq!(repo.list_mistakes().unwrap()[0].chinese, "水果");
    assert_eq!(repo.list_wordbook_entries(book.id).unwrap().len(), 1);
    assert!(repo.is_favorite("apple", "苹果").unwrap());
    let memory_after: (f64, f64, f64) = repo.connect().unwrap().query_row(
        "SELECT stability,last_reviewed,due_at FROM review_memory WHERE normalized_english='apple' AND chinese='苹果'",
        [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    ).unwrap();
    assert_eq!(memory_before, memory_after);
    // An old submission remains claimed even though there is no result row to return.
    assert!(repo.record_mistake_once("Apple", "苹果", "old").is_err());
    assert_eq!(repo.list_mistakes().unwrap().len(), 1);
    let reopened = WordbookRepository::open(&path).unwrap();
    assert_eq!(reopened.list_mistakes().unwrap().len(), 1);
    assert_eq!(
        reopened
            .record_mistake_once("apple", "苹果", "new")
            .unwrap()
            .error_count,
        1
    );
    assert_eq!(reopened.list_mistakes().unwrap().len(), 2);
}

#[test]
fn remove_mistake_allows_completion_of_current_mistake_question() {
    let dir = tempdir().unwrap();
    let repo = WordbookRepository::open(dir.path().join("db")).unwrap();
    repo.record_mistake("apple", "苹果").unwrap();
    let questions = repo
        .schedule_practice("mistakes", None, 1, "en-to-zh")
        .unwrap();
    repo.remove_mistake("apple", "苹果").unwrap();
    repo.complete_review(questions[0].review_id, 1, 0, false)
        .unwrap();
    assert!(repo.list_mistakes().unwrap().is_empty());
    assert!(repo
        .schedule_practice("mistakes", None, 1, "mixed")
        .unwrap()
        .is_empty());
    let (coverage, pending, memory): (i64, i64, i64) = repo
        .connect()
        .unwrap()
        .query_row(
            "SELECT (SELECT count(*) FROM review_coverage WHERE source='mistakes'),
                (SELECT count(*) FROM outstanding_reviews WHERE source='mistakes'),
                (SELECT count(*) FROM review_memory)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((coverage, pending, memory), (0, 0, 1));
}

#[test]
fn remove_mistake_replays_across_peers_and_later_answers_readd() {
    let dir = tempdir().unwrap();
    let a = WordbookRepository::open(dir.path().join("a")).unwrap();
    let b = WordbookRepository::open(dir.path().join("b")).unwrap();
    let c = WordbookRepository::open(dir.path().join("c")).unwrap();
    a.record_mistake_once("Apple", "苹果", "first").unwrap();
    exchange(&a, &b);
    b.remove_mistake("APPLE", "苹果").unwrap();
    exchange(&b, &a);
    assert!(a.list_mistakes().unwrap().is_empty());
    assert!(b.list_mistakes().unwrap().is_empty());
    // Out-of-order delivery, duplicate ingress, and a subsequent rebuild must not resurrect it.
    let changes: Vec<Envelope> = b.changes_since(&BTreeMap::new(), 100).unwrap();
    for change in changes.iter().rev().chain(changes.iter()) {
        c.ingest(change, &SyncProjection).unwrap();
    }
    c.add_favorite("unrelated", "其他").unwrap();
    assert!(c.list_mistakes().unwrap().is_empty());
    a.record_mistake_once("apple", "苹果", "second").unwrap();
    exchange(&a, &b);
    exchange(&b, &c);
    for repo in [&a, &b, &c] {
        assert_eq!(repo.list_mistakes().unwrap().len(), 1);
        assert_eq!(repo.list_mistakes().unwrap()[0].error_count, 1);
    }
}

#[test]
fn remove_mistake_validates_input_and_wire_payload() {
    let dir = tempdir().unwrap();
    let a = WordbookRepository::open(dir.path().join("a")).unwrap();
    let b = WordbookRepository::open(dir.path().join("b")).unwrap();
    assert!(matches!(
        a.remove_mistake(" ", "苹果"),
        Err(RepositoryError::Validation(_))
    ));
    assert!(matches!(
        a.remove_mistake("apple", " "),
        Err(RepositoryError::Validation(_))
    ));
    a.remove_mistake("apple", "苹果").unwrap();
    let mut change = a.changes_since(&BTreeMap::new(), 100).unwrap().remove(0);
    change.content = r#"{"type":"remove_mistake","english":" ","chinese":"苹果"}"#
        .as_bytes()
        .to_vec();
    assert!(b.ingest(&change, &SyncProjection).is_err());
    assert!(b.changes_since(&BTreeMap::new(), 100).unwrap().is_empty());
}

#[test]
fn remove_mistake_supports_legacy_databases() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("legacy");
    rusqlite::Connection::open(&path).unwrap().execute_batch(
        "CREATE TABLE mistakes(id INTEGER PRIMARY KEY, english TEXT NOT NULL, chinese TEXT NOT NULL,
          normalized_english TEXT NOT NULL, error_count INTEGER NOT NULL, UNIQUE(normalized_english,chinese));
         INSERT INTO mistakes VALUES(1,'Apple','苹果','apple',4);"
    ).unwrap();
    let repo = WordbookRepository::open(&path).unwrap();
    assert!(!repo.sync_enabled().unwrap());
    assert!(repo.remove_mistake(" APPLE ", " 苹果 ").unwrap());
    assert!(!repo.remove_mistake("apple", "苹果").unwrap());
    assert!(WordbookRepository::open(&path)
        .unwrap()
        .list_mistakes()
        .unwrap()
        .is_empty());
    assert_eq!(
        repo.record_mistake_once("apple", "苹果", "fresh")
            .unwrap()
            .error_count,
        1
    );
}
