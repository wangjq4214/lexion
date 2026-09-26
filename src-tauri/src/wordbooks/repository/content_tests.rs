use super::{content::ContentProjection, replay::Envelope, ImportedEntry, WordbookRepository};
use std::collections::BTreeMap;
use tempfile::tempdir;

fn pair(english: &str, chinese: &str) -> ImportedEntry {
    ImportedEntry {
        english: english.into(),
        chinese: chinese.into(),
    }
}

fn snapshot(
    repo: &WordbookRepository,
) -> (Vec<(String, Vec<(String, String)>)>, Vec<(String, String)>) {
    let mut books: Vec<_> = repo
        .list()
        .unwrap()
        .into_iter()
        .map(|book| {
            let mut words: Vec<_> = repo
                .list_wordbook_entries(book.id)
                .unwrap()
                .into_iter()
                .map(|entry| (entry.english, entry.chinese))
                .collect();
            words.sort();
            (book.name, words)
        })
        .collect();
    books.sort();
    let mut favorites: Vec<_> = repo
        .list_favorites()
        .unwrap()
        .into_iter()
        .map(|entry| (entry.english, entry.chinese))
        .collect();
    favorites.sort();
    (books, favorites)
}

fn exchange(from: &WordbookRepository, to: &WordbookRepository) {
    for change in from.changes_since(&BTreeMap::new(), 100).unwrap() {
        to.ingest(&change, &ContentProjection).unwrap();
    }
}

#[test]
fn offline_books_and_favorites_exchange_and_preserve_independent_records() {
    let dir = tempdir().unwrap();
    let a = WordbookRepository::open(dir.path().join("a.sqlite")).unwrap();
    let b = WordbookRepository::open(dir.path().join("b.sqlite")).unwrap();
    a.replace("A", &[pair("Apple", "苹果"), pair("APPLE", "水果")], false)
        .unwrap();
    b.replace("B", &[pair("Pear", "梨")], false).unwrap();
    a.add_favorite(" Apple ", "苹果").unwrap();
    b.add_favorite("APPLE", "水果").unwrap();
    // Mistakes must not disappear when the content projection rebuilds.
    a.record_mistake("Apple", "苹果").unwrap();
    exchange(&a, &b);
    exchange(&b, &a);
    assert_eq!(snapshot(&a), snapshot(&b));
    assert_eq!(a.list_mistakes().unwrap().len(), 1);
    let book = a
        .list()
        .unwrap()
        .into_iter()
        .find(|book| book.name == "A")
        .unwrap();
    a.delete_wordbook(book.id).unwrap();
    exchange(&a, &b);
    assert_eq!(snapshot(&a), snapshot(&b));
    assert_eq!(a.list_favorites().unwrap().len(), 2);
    assert_eq!(a.list_mistakes().unwrap().len(), 1);
}

#[test]
fn concurrent_replacement_and_deletion_use_pair_not_remote_row_id() {
    let dir = tempdir().unwrap();
    let a = WordbookRepository::open(dir.path().join("a.sqlite")).unwrap();
    let b = WordbookRepository::open(dir.path().join("b.sqlite")).unwrap();
    // Preallocate unrelated rows on B so the shared book has different local IDs.
    b.replace("other", &[pair("Other", "其他")], false).unwrap();
    let original = a
        .replace("same", &[pair("Apple", "苹果"), pair("Pear", "梨")], false)
        .unwrap();
    exchange(&a, &b);
    let target = b
        .list_wordbook_entries(
            b.list()
                .unwrap()
                .into_iter()
                .find(|x| x.name == "same")
                .unwrap()
                .id,
        )
        .unwrap()
        .into_iter()
        .find(|x| x.english == "Apple")
        .unwrap();
    b.delete_wordbook_entry(
        b.list()
            .unwrap()
            .into_iter()
            .find(|x| x.name == "same")
            .unwrap()
            .id,
        target.id,
    )
    .unwrap();
    assert!(matches!(
        a.replace("same", &[pair("new", "新")], false),
        Err(super::RepositoryError::Conflict)
    ));
    a.replace("same", &[pair("Apple", "水果"), pair("Pear", "梨")], true)
        .unwrap();
    // Different receipt orders still produce the same canonical replay.
    let from_a: Vec<Envelope> = a.changes_since(&BTreeMap::new(), 100).unwrap();
    let from_b: Vec<Envelope> = b.changes_since(&BTreeMap::new(), 100).unwrap();
    for change in from_b.iter().rev() {
        a.ingest(change, &ContentProjection).unwrap();
    }
    for change in from_a.iter().rev() {
        b.ingest(change, &ContentProjection).unwrap();
    }
    assert_eq!(snapshot(&a), snapshot(&b));
    assert_eq!(
        a.list_wordbook_entries(original.id)
            .unwrap()
            .iter()
            .filter(|e| e.chinese == "新")
            .count(),
        0
    );
    let before = snapshot(&b);
    for change in &from_a {
        b.ingest(change, &ContentProjection).unwrap();
    }
    assert_eq!(snapshot(&b), before);
}

#[test]
fn late_unrelated_change_keeps_existing_review_coverage() {
    let dir = tempdir().unwrap();
    let a = WordbookRepository::open(dir.path().join("a.sqlite")).unwrap();
    let b = WordbookRepository::open(dir.path().join("b.sqlite")).unwrap();
    let (early, late) = if a.replay_device_id().unwrap() < b.replay_device_id().unwrap() {
        (&a, &b)
    } else {
        (&b, &a)
    };
    early
        .replace("unrelated", &[pair("Pear", "梨")], false)
        .unwrap();
    let book = late
        .replace("studied", &[pair("Apple", "苹果")], false)
        .unwrap();
    late.schedule_practice("wordbook", Some(book.id), 1, "zh-to-en")
        .unwrap();
    assert_eq!(coverage(late, book.id), 1);
    exchange(early, late);
    assert_eq!(coverage(late, book.id), 1);
}

fn coverage(repo: &WordbookRepository, id: i64) -> i64 {
    repo.connect()
        .unwrap()
        .query_row(
            "SELECT count(*) FROM review_coverage WHERE source='wordbook' AND source_id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn concurrent_same_name_import_requires_confirmation_for_all_but_one() {
    use std::sync::{Arc, Barrier};
    let dir = tempdir().unwrap();
    let repo = Arc::new(WordbookRepository::open(dir.path().join("race.sqlite")).unwrap());
    let barrier = Arc::new(Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|index| {
            let repo = Arc::clone(&repo);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                repo.replace("race", &[pair(&format!("word-{index}"), "词")], false)
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Err(super::RepositoryError::Conflict)))
            .count(),
        7
    );
    assert_eq!(repo.changes_since(&BTreeMap::new(), 100).unwrap().len(), 1);
}

#[test]
fn invalid_import_and_failed_projection_leave_log_and_data_unchanged() {
    let dir = tempdir().unwrap();
    let a = WordbookRepository::open(dir.path().join("a.sqlite")).unwrap();
    a.replace("book", &[pair("old", "旧")], false).unwrap();
    let baseline = a.changes_since(&BTreeMap::new(), 100).unwrap().len();
    assert!(a
        .replace(
            "book",
            &[pair("Apple", "苹果"), pair("APPLE", "苹果")],
            true
        )
        .is_err());
    assert_eq!(
        a.changes_since(&BTreeMap::new(), 100).unwrap().len(),
        baseline
    );
    assert_eq!(
        a.list_wordbook_entries(a.list().unwrap()[0].id).unwrap()[0].english,
        "old"
    );
    a.add_favorite("Apple", "苹果").unwrap();
    a.add_favorite("apple", "苹果").unwrap();
    assert_eq!(a.list_favorites().unwrap().len(), 1);
    assert!(a.remove_favorite("apple", "苹果").unwrap());
    assert!(!a.remove_favorite("apple", "苹果").unwrap());
}
