use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use super::model::{ImportedEntry, MistakeEntry, WordEntry, WordbookSummary};

#[path = "schedule.rs"]
mod schedule;

#[derive(Debug)]
pub enum RepositoryError {
    Conflict,
    Validation(&'static str),
    Database(rusqlite::Error),
}

impl From<rusqlite::Error> for RepositoryError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

#[derive(Debug, Clone)]
pub struct WordbookRepository {
    database_path: PathBuf,
}

impl WordbookRepository {
    pub fn open(database_path: impl AsRef<Path>) -> Result<Self, rusqlite::Error> {
        let repository = Self {
            database_path: database_path.as_ref().to_path_buf(),
        };
        repository.initialize()?;
        Ok(repository)
    }

    pub(super) fn connect(&self) -> Result<Connection, rusqlite::Error> {
        let connection = Connection::open(&self.database_path)?;
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(connection)
    }

    fn initialize(&self) -> Result<(), rusqlite::Error> {
        let connection = self.connect()?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS wordbooks (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS entries (
                id INTEGER PRIMARY KEY,
                wordbook_id INTEGER NOT NULL REFERENCES wordbooks(id) ON DELETE CASCADE,
                english TEXT NOT NULL,
                chinese TEXT NOT NULL,
                normalized_english TEXT NOT NULL,
                UNIQUE(wordbook_id, normalized_english, chinese)
            );
            CREATE INDEX IF NOT EXISTS entries_wordbook_id ON entries(wordbook_id);
            CREATE TABLE IF NOT EXISTS favorites (
                id INTEGER PRIMARY KEY,
                english TEXT NOT NULL CHECK(length(english) > 0),
                chinese TEXT NOT NULL CHECK(length(chinese) > 0),
                normalized_english TEXT NOT NULL,
                UNIQUE(normalized_english, chinese)
            );
            CREATE TABLE IF NOT EXISTS mistakes (
                id INTEGER PRIMARY KEY,
                english TEXT NOT NULL CHECK(length(english) > 0),
                chinese TEXT NOT NULL CHECK(length(chinese) > 0),
                normalized_english TEXT NOT NULL,
                error_count INTEGER NOT NULL DEFAULT 1 CHECK(error_count > 0),
                UNIQUE(normalized_english, chinese)
            );
            CREATE TABLE IF NOT EXISTS mistake_submissions (
                submission_id TEXT PRIMARY KEY NOT NULL CHECK(length(trim(submission_id)) > 0),
                normalized_english TEXT NOT NULL,
                chinese TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS review_coverage (
                source TEXT NOT NULL,
                source_id INTEGER NOT NULL,
                normalized_english TEXT NOT NULL,
                chinese TEXT NOT NULL,
                PRIMARY KEY(source, source_id, normalized_english, chinese)
            );
            CREATE TABLE IF NOT EXISTS review_memory (
                normalized_english TEXT NOT NULL,
                chinese TEXT NOT NULL,
                direction TEXT NOT NULL CHECK(direction IN ('zh-to-en', 'en-to-zh')),
                stability REAL NOT NULL CHECK(stability > 0),
                last_reviewed REAL NOT NULL,
                due_at REAL NOT NULL,
                PRIMARY KEY(normalized_english, chinese, direction)
            );
            CREATE TABLE IF NOT EXISTS pending_reviews (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                normalized_english TEXT NOT NULL,
                chinese TEXT NOT NULL,
                direction TEXT NOT NULL CHECK(direction IN ('zh-to-en', 'en-to-zh')),
                completed INTEGER NOT NULL DEFAULT 0 CHECK(completed IN (0, 1)),
                created_at REAL NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS review_settings (
                id INTEGER PRIMARY KEY CHECK(id = 1),
                target_retention REAL NOT NULL CHECK(target_retention > 0 AND target_retention < 1)
            );
            INSERT OR IGNORE INTO review_settings(id, target_retention) VALUES (1, 0.9);",
        )?;
        let has_mistake_created_at: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('mistake_submissions') WHERE name = 'created_at')",
            [],
            |row| row.get(0),
        )?;
        if !has_mistake_created_at {
            connection.execute_batch(
                "ALTER TABLE mistake_submissions ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0;",
            )?;
        }
        connection.execute_batch(
            "CREATE INDEX IF NOT EXISTS mistake_submissions_created_at ON mistake_submissions(created_at);",
        )?;
        let has_created_at: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('pending_reviews') WHERE name = 'created_at')",
            [],
            |row| row.get(0),
        )?;
        if !has_created_at {
            connection.execute_batch(
                "BEGIN;
                 CREATE TABLE pending_reviews_new (
                   id INTEGER PRIMARY KEY AUTOINCREMENT,
                   normalized_english TEXT NOT NULL,
                   chinese TEXT NOT NULL,
                   direction TEXT NOT NULL CHECK(direction IN ('zh-to-en', 'en-to-zh')),
                   completed INTEGER NOT NULL DEFAULT 0 CHECK(completed IN (0, 1)),
                   created_at REAL NOT NULL DEFAULT 0
                 );
                 INSERT INTO pending_reviews_new(id, normalized_english, chinese, direction, completed)
                   SELECT id, normalized_english, chinese, direction, completed FROM pending_reviews;
                 DROP TABLE pending_reviews;
                 ALTER TABLE pending_reviews_new RENAME TO pending_reviews;
                 COMMIT;",
            )?;
        }
        connection.execute_batch(
            "CREATE INDEX IF NOT EXISTS pending_reviews_created_at ON pending_reviews(created_at);",
        )?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<WordbookSummary>, rusqlite::Error> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT wordbooks.id, wordbooks.name, COUNT(entries.id)
             FROM wordbooks
             LEFT JOIN entries ON entries.wordbook_id = wordbooks.id
             GROUP BY wordbooks.id, wordbooks.name
             ORDER BY wordbooks.created_at, wordbooks.id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(WordbookSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                entry_count: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    pub fn replace(
        &self,
        name: &str,
        entries: &[ImportedEntry],
        replace_existing: bool,
    ) -> Result<WordbookSummary, RepositoryError> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        let existing_id: Option<i64> = transaction
            .query_row("SELECT id FROM wordbooks WHERE name = ?1", [name], |row| {
                row.get(0)
            })
            .optional()?;

        if existing_id.is_some() && !replace_existing {
            return Err(RepositoryError::Conflict);
        }

        if let Some(id) = existing_id {
            transaction.execute(
                "DELETE FROM review_coverage WHERE source = 'wordbook' AND source_id = ?1",
                [id],
            )?;
            transaction.execute("DELETE FROM wordbooks WHERE id = ?1", [id])?;
        }

        transaction.execute("INSERT INTO wordbooks(name) VALUES (?1)", [name])?;
        let wordbook_id = transaction.last_insert_rowid();
        Self::insert_entries(&transaction, wordbook_id, entries)?;
        transaction.commit()?;

        Ok(WordbookSummary {
            id: wordbook_id,
            name: name.to_owned(),
            entry_count: entries.len() as i64,
        })
    }

    fn insert_entries(
        transaction: &Transaction<'_>,
        wordbook_id: i64,
        entries: &[ImportedEntry],
    ) -> Result<(), rusqlite::Error> {
        let mut statement = transaction.prepare(
            "INSERT INTO entries(wordbook_id, english, chinese, normalized_english)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for entry in entries {
            statement.execute(params![
                wordbook_id,
                entry.english,
                entry.chinese,
                entry.english.to_lowercase()
            ])?;
        }
        Ok(())
    }

    pub fn add_favorite(&self, english: &str, chinese: &str) -> Result<WordEntry, RepositoryError> {
        let (english, chinese) = favorite_pair(english, chinese)?;
        let connection = self.connect()?;
        connection.execute(
            "INSERT OR IGNORE INTO favorites(english, chinese, normalized_english)
             VALUES (?1, ?2, ?3)",
            params![english, chinese, english.to_lowercase()],
        )?;
        Ok(connection.query_row(
            "SELECT id, english, chinese FROM favorites
             WHERE normalized_english = ?1 AND chinese = ?2",
            params![english.to_lowercase(), chinese],
            word_entry,
        )?)
    }

    pub fn remove_favorite(&self, english: &str, chinese: &str) -> Result<bool, RepositoryError> {
        let (english, chinese) = favorite_pair(english, chinese)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        let removed = transaction.execute(
            "DELETE FROM favorites WHERE normalized_english = ?1 AND chinese = ?2",
            params![english.to_lowercase(), chinese],
        )? != 0;
        if removed {
            transaction.execute(
                "DELETE FROM review_coverage WHERE source = 'favorites' AND normalized_english = ?1 AND chinese = ?2",
                params![english.to_lowercase(), chinese],
            )?;
        }
        transaction.commit()?;
        Ok(removed)
    }

    pub fn is_favorite(&self, english: &str, chinese: &str) -> Result<bool, RepositoryError> {
        let (english, chinese) = favorite_pair(english, chinese)?;
        let connection = self.connect()?;
        Ok(connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM favorites WHERE normalized_english = ?1 AND chinese = ?2)",
            params![english.to_lowercase(), chinese],
            |row| row.get(0),
        )?)
    }

    pub fn list_favorites(&self) -> Result<Vec<WordEntry>, rusqlite::Error> {
        let connection = self.connect()?;
        let mut statement =
            connection.prepare("SELECT id, english, chinese FROM favorites ORDER BY id")?;
        let rows = statement.query_map([], word_entry)?;
        rows.collect()
    }

    pub fn sample_favorites(&self, limit: u8) -> Result<Vec<WordEntry>, RepositoryError> {
        if limit == 0 {
            return Err(RepositoryError::Validation("练习数量必须大于零"));
        }
        let connection = self.connect()?;
        let mut statement = connection
            .prepare("SELECT id, english, chinese FROM favorites ORDER BY RANDOM() LIMIT ?1")?;
        let rows = statement.query_map([i64::from(limit)], word_entry)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn record_mistake(
        &self,
        english: &str,
        chinese: &str,
    ) -> Result<MistakeEntry, RepositoryError> {
        let (english, chinese) = favorite_pair(english, chinese)?;
        let connection = self.connect()?;
        // A single UPSERT avoids losing increments when two submissions use separate connections.
        Ok(connection.query_row(
            "INSERT INTO mistakes(english, chinese, normalized_english, error_count)
             VALUES (?1, ?2, ?3, 1)
             ON CONFLICT(normalized_english, chinese)
             DO UPDATE SET error_count = error_count + 1
             RETURNING id, english, chinese, error_count",
            params![english, chinese, english.to_lowercase()],
            mistake_entry,
        )?)
    }

    pub fn record_mistake_once(
        &self,
        english: &str,
        chinese: &str,
        submission_id: &str,
    ) -> Result<MistakeEntry, RepositoryError> {
        let (english, chinese) = favorite_pair(english, chinese)?;
        if submission_id.trim().is_empty() {
            return Err(RepositoryError::Validation("提交 ID 不能为空"));
        }
        let normalized_english = english.to_lowercase();
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        // The UI keeps retry IDs only for the active question; bound stale idempotency claims.
        transaction.execute(
            "DELETE FROM mistake_submissions WHERE created_at < unixepoch() - 2592000",
            [],
        )?;
        // Claim the ID before updating the count. SQLite serializes writers, so concurrent
        // retries cannot both increment, and a failed UPSERT rolls back the claim as well.
        let claimed = transaction.execute(
            "INSERT OR IGNORE INTO mistake_submissions(submission_id, normalized_english, chinese, created_at)
             VALUES (?1, ?2, ?3, unixepoch())",
            params![submission_id, normalized_english, chinese],
        )? != 0;
        if claimed {
            transaction.execute(
                "INSERT INTO mistakes(english, chinese, normalized_english, error_count)
                 VALUES (?1, ?2, ?3, 1)
                 ON CONFLICT(normalized_english, chinese)
                 DO UPDATE SET error_count = error_count + 1",
                params![english, chinese, normalized_english],
            )?;
        } else {
            let stored_pair: (String, String) = transaction.query_row(
                "SELECT normalized_english, chinese FROM mistake_submissions WHERE submission_id = ?1",
                [submission_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if stored_pair != (normalized_english.clone(), chinese.to_owned()) {
                return Err(RepositoryError::Conflict);
            }
        }
        let entry = transaction.query_row(
            "SELECT id, english, chinese, error_count FROM mistakes
             WHERE normalized_english = ?1 AND chinese = ?2",
            params![normalized_english, chinese],
            mistake_entry,
        )?;
        transaction.commit()?;
        Ok(entry)
    }

    pub fn list_mistakes(&self) -> Result<Vec<MistakeEntry>, rusqlite::Error> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare("SELECT id, english, chinese, error_count FROM mistakes ORDER BY id")?;
        let rows = statement.query_map([], mistake_entry)?;
        rows.collect()
    }

    pub fn sample_mistakes(&self, limit: u8) -> Result<Vec<WordEntry>, RepositoryError> {
        if limit == 0 {
            return Err(RepositoryError::Validation("练习数量必须大于零"));
        }
        let connection = self.connect()?;
        let mut statement = connection
            .prepare("SELECT id, english, chinese FROM mistakes ORDER BY RANDOM() LIMIT ?1")?;
        let rows = statement.query_map([i64::from(limit)], word_entry)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn sample(&self, wordbook_id: i64, limit: u8) -> Result<Vec<WordEntry>, rusqlite::Error> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, english, chinese
             FROM entries
             WHERE wordbook_id = ?1
             ORDER BY RANDOM()
             LIMIT ?2",
        )?;
        let rows = statement.query_map(params![wordbook_id, i64::from(limit)], |row| {
            Ok(WordEntry {
                id: row.get(0)?,
                english: row.get(1)?,
                chinese: row.get(2)?,
            })
        })?;
        rows.collect()
    }
}

fn favorite_pair<'a>(
    english: &'a str,
    chinese: &'a str,
) -> Result<(&'a str, &'a str), RepositoryError> {
    let (english, chinese) = (english.trim(), chinese.trim());
    if english.is_empty() || chinese.is_empty() {
        return Err(RepositoryError::Validation("英文和中文释义都不能为空"));
    }
    Ok((english, chinese))
}

fn mistake_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<MistakeEntry> {
    Ok(MistakeEntry {
        id: row.get(0)?,
        english: row.get(1)?,
        chinese: row.get(2)?,
        error_count: row.get(3)?,
    })
}

fn word_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<WordEntry> {
    Ok(WordEntry {
        id: row.get(0)?,
        english: row.get(1)?,
        chinese: row.get(2)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn entries(prefix: &str, count: usize) -> Vec<ImportedEntry> {
        (0..count)
            .map(|index| ImportedEntry {
                english: format!("{prefix}-{index}"),
                chinese: format!("释义-{index}"),
            })
            .collect()
    }

    #[test]
    fn persists_multiple_wordbooks_and_scopes_samples() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("wordbooks.sqlite");
        let repository = WordbookRepository::open(&path).unwrap();
        let first = repository
            .replace("第一册", &entries("first", 60), false)
            .unwrap();
        repository
            .replace("第二册", &entries("second", 2), false)
            .unwrap();

        let reopened = WordbookRepository::open(&path).unwrap();
        let listed = reopened.list().unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].entry_count, 60);

        let sample = reopened.sample(first.id, 50).unwrap();
        assert_eq!(sample.len(), 50);
        assert_eq!(reopened.sample(first.id, 255).unwrap().len(), 60);
        assert_eq!(reopened.sample(first.id, 5).unwrap().len(), 5);
        assert!(sample
            .iter()
            .all(|entry| entry.english.starts_with("first-")));
        assert_eq!(
            sample
                .iter()
                .map(|entry| entry.id)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            sample.len()
        );
        assert_eq!(reopened.sample(listed[1].id, 50).unwrap().len(), 2);
    }

    #[test]
    fn replacement_requires_confirmation_and_is_atomic() {
        let directory = tempdir().unwrap();
        let repository =
            WordbookRepository::open(directory.path().join("wordbooks.sqlite")).unwrap();
        let original = repository
            .replace("同名", &entries("old", 2), false)
            .unwrap();

        assert!(matches!(
            repository.replace("同名", &entries("new", 3), false),
            Err(RepositoryError::Conflict)
        ));
        assert!(repository
            .sample(original.id, 10)
            .unwrap()
            .iter()
            .all(|entry| entry.english.starts_with("old-")));

        let replaced = repository
            .replace("同名", &entries("new", 3), true)
            .unwrap();
        assert_eq!(replaced.entry_count, 3);
        assert!(repository
            .sample(replaced.id, 10)
            .unwrap()
            .iter()
            .all(|entry| entry.english.starts_with("new-")));
    }

    #[test]
    fn failed_replacement_rolls_back_the_original_wordbook() {
        let directory = tempdir().unwrap();
        let repository =
            WordbookRepository::open(directory.path().join("wordbooks.sqlite")).unwrap();
        let original = repository
            .replace("同名", &entries("old", 2), false)
            .unwrap();
        let duplicate_entries = vec![
            ImportedEntry {
                english: "same".to_owned(),
                chinese: "相同".to_owned(),
            },
            ImportedEntry {
                english: "SAME".to_owned(),
                chinese: "相同".to_owned(),
            },
        ];

        assert!(matches!(
            repository.replace("同名", &duplicate_entries, true),
            Err(RepositoryError::Database(_))
        ));
        let listed = repository.list().unwrap();
        assert_eq!(listed, vec![original.clone()]);
        assert!(repository
            .sample(original.id, 10)
            .unwrap()
            .iter()
            .all(|entry| entry.english.starts_with("old-")));
    }

    #[test]
    fn favorites_deduplicate_pairs_and_keep_first_display_text() {
        let directory = tempdir().unwrap();
        let repository = WordbookRepository::open(directory.path().join("words.sqlite")).unwrap();
        let first = repository.add_favorite(" Apple ", " 苹果 ").unwrap();
        let duplicate = repository.add_favorite("apple", "苹果").unwrap();
        assert_eq!(first, duplicate);
        assert_eq!(first.english, "Apple");
        assert_eq!(first.chinese, "苹果");
        let other = repository.add_favorite("APPLE", "水果").unwrap();
        assert_ne!(first.id, other.id);
        assert_eq!(repository.list_favorites().unwrap(), vec![first, other]);
        assert!(repository.is_favorite(" aPpLe ", " 苹果 ").unwrap());
        assert!(!repository.is_favorite("apple", "其他").unwrap());
    }

    #[test]
    fn favorites_survive_replacement_and_reopening() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("words.sqlite");
        let repository = WordbookRepository::open(&path).unwrap();
        repository
            .replace("同名", &entries("old", 2), false)
            .unwrap();
        let favorite = repository.add_favorite("old-0", "释义-0").unwrap();
        repository
            .replace("同名", &entries("new", 2), true)
            .unwrap();
        drop(repository);

        let reopened = WordbookRepository::open(&path).unwrap();
        assert_eq!(reopened.list_favorites().unwrap(), vec![favorite.clone()]);
        assert!(reopened.is_favorite("OLD-0", "释义-0").unwrap());
        assert_eq!(reopened.sample_favorites(10).unwrap(), vec![favorite]);
    }

    #[test]
    fn favorite_sampling_is_bounded_unique_and_removal_is_idempotent() {
        use std::collections::HashSet;
        let directory = tempdir().unwrap();
        let repository = WordbookRepository::open(directory.path().join("words.sqlite")).unwrap();
        assert!(repository.sample_favorites(10).unwrap().is_empty());
        for index in 0..60 {
            repository
                .add_favorite(&format!("word-{index}"), "释义")
                .unwrap();
        }
        for limit in [1, 5, 50, 255] {
            let sample = repository.sample_favorites(limit).unwrap();
            assert_eq!(sample.len(), usize::from(limit).min(60));
            assert_eq!(
                sample
                    .iter()
                    .map(|entry| entry.id)
                    .collect::<HashSet<_>>()
                    .len(),
                sample.len()
            );
        }
        assert!(repository.remove_favorite(" WORD-0 ", " 释义 ").unwrap());
        assert!(!repository.remove_favorite("word-0", "释义").unwrap());
        assert!(!repository.is_favorite("word-0", "释义").unwrap());
        assert_eq!(repository.list_favorites().unwrap().len(), 59);
    }

    #[test]
    fn favorites_reject_empty_fields_and_zero_limit_without_writing() {
        let directory = tempdir().unwrap();
        let repository = WordbookRepository::open(directory.path().join("words.sqlite")).unwrap();
        assert!(matches!(
            repository.add_favorite("  ", "释义"),
            Err(RepositoryError::Validation(_))
        ));
        assert!(matches!(
            repository.add_favorite("word", "  "),
            Err(RepositoryError::Validation(_))
        ));
        assert!(matches!(
            repository.remove_favorite("", "释义"),
            Err(RepositoryError::Validation(_))
        ));
        assert!(matches!(
            repository.is_favorite("word", ""),
            Err(RepositoryError::Validation(_))
        ));
        assert!(matches!(
            repository.sample_favorites(0),
            Err(RepositoryError::Validation(_))
        ));
        assert!(repository.list_favorites().unwrap().is_empty());
    }

    #[test]
    fn mistakes_increment_by_normalized_pair_and_preserve_first_display_text() {
        let directory = tempdir().unwrap();
        let repository = WordbookRepository::open(directory.path().join("words.sqlite")).unwrap();
        let first = repository.record_mistake(" Apple ", " 苹果 ").unwrap();
        assert_eq!(first.error_count, 1);
        let repeated = repository.record_mistake("apple", "苹果").unwrap();
        assert_eq!(repeated.id, first.id);
        assert_eq!(repeated.english, "Apple");
        assert_eq!(repeated.chinese, "苹果");
        assert_eq!(repeated.error_count, 2);
        let another = repository.record_mistake("APPLE", "水果").unwrap();
        assert_ne!(another.id, first.id);
        assert_eq!(another.error_count, 1);
        assert_eq!(
            repository.list_mistakes().unwrap(),
            vec![repeated.clone(), another]
        );
        assert_eq!(
            serde_json::to_value(repeated).unwrap(),
            serde_json::json!({"id": first.id, "english": "Apple", "chinese": "苹果", "errorCount": 2})
        );
    }

    #[test]
    fn mistakes_survive_wordbook_replacement_and_reopening() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("words.sqlite");
        let repository = WordbookRepository::open(&path).unwrap();
        repository
            .replace("同名", &entries("old", 2), false)
            .unwrap();
        let mistake = repository.record_mistake("old-0", "释义-0").unwrap();
        repository
            .replace("同名", &entries("new", 2), true)
            .unwrap();
        drop(repository);

        let reopened = WordbookRepository::open(&path).unwrap();
        assert_eq!(reopened.list_mistakes().unwrap(), vec![mistake.clone()]);
        assert_eq!(
            reopened
                .record_mistake("OLD-0", "释义-0")
                .unwrap()
                .error_count,
            2
        );
        assert_eq!(
            reopened.sample_mistakes(10).unwrap(),
            vec![WordEntry {
                id: mistake.id,
                english: mistake.english,
                chinese: mistake.chinese,
            }]
        );
    }

    #[test]
    fn mistake_sampling_is_bounded_unique_and_validated() {
        use std::collections::HashSet;
        let directory = tempdir().unwrap();
        let repository = WordbookRepository::open(directory.path().join("words.sqlite")).unwrap();
        assert!(repository.sample_mistakes(10).unwrap().is_empty());
        for (english, chinese) in [(" ", "释义"), ("word", " ")] {
            assert!(matches!(
                repository.record_mistake(english, chinese),
                Err(RepositoryError::Validation(_))
            ));
        }
        assert!(matches!(
            repository.sample_mistakes(0),
            Err(RepositoryError::Validation(_))
        ));
        assert!(repository.list_mistakes().unwrap().is_empty());
        for index in 0..60 {
            repository
                .record_mistake(&format!("word-{index}"), "释义")
                .unwrap();
        }
        for limit in [1, 5, 50, 255] {
            let sample = repository.sample_mistakes(limit).unwrap();
            assert_eq!(sample.len(), usize::from(limit).min(60));
            assert_eq!(
                sample
                    .iter()
                    .map(|entry| entry.id)
                    .collect::<HashSet<_>>()
                    .len(),
                sample.len()
            );
        }
    }

    #[test]
    fn submission_ids_survive_reopening_and_increment_only_for_new_submissions() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("words.sqlite");
        let repository = WordbookRepository::open(&path).unwrap();
        let first = repository
            .record_mistake_once(" Apple ", " 苹果 ", "submission-1")
            .unwrap();
        assert_eq!(first.error_count, 1);
        assert_eq!(
            repository
                .record_mistake_once("apple", "苹果", "submission-1")
                .unwrap(),
            first
        );
        drop(repository);

        let reopened = WordbookRepository::open(&path).unwrap();
        assert_eq!(
            reopened
                .record_mistake_once("APPLE", "苹果", "submission-1")
                .unwrap(),
            first
        );
        let second = reopened
            .record_mistake_once("apple", "苹果", "submission-2")
            .unwrap();
        assert_eq!(second.id, first.id);
        assert_eq!(second.error_count, 2);
        assert_eq!(second.english, first.english);
        assert_eq!(reopened.list_mistakes().unwrap(), vec![second.clone()]);
        assert_eq!(
            reopened
                .record_mistake_once("apple", "苹果", "submission-1")
                .unwrap(),
            second
        );
    }

    #[test]
    fn expires_old_submission_claims_without_discarding_mistake_counts() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("words.sqlite");
        let repository = WordbookRepository::open(&path).unwrap();
        repository
            .record_mistake_once("apple", "苹果", "old")
            .unwrap();
        repository
            .connect()
            .unwrap()
            .execute(
                "UPDATE mistake_submissions SET created_at = 0 WHERE submission_id = 'old'",
                [],
            )
            .unwrap();
        repository
            .record_mistake_once("apple", "苹果", "new")
            .unwrap();
        let remaining: i64 = repository
            .connect()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM mistake_submissions", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(remaining, 1);
        assert_eq!(repository.list_mistakes().unwrap()[0].error_count, 2);
        drop(repository);
        let reopened = WordbookRepository::open(path).unwrap();
        assert_eq!(reopened.list_mistakes().unwrap()[0].error_count, 2);
    }

    #[test]
    fn concurrent_retries_claim_one_submission() {
        use std::sync::{Arc, Barrier};

        let directory = tempdir().unwrap();
        let repository = WordbookRepository::open(directory.path().join("words.sqlite")).unwrap();
        let barrier = Arc::new(Barrier::new(8));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let repository = repository.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    repository.record_mistake_once("word", "释义", "one-id")
                })
            })
            .collect();
        for handle in handles {
            assert_eq!(handle.join().unwrap().unwrap().error_count, 1);
        }
        assert_eq!(repository.list_mistakes().unwrap()[0].error_count, 1);
    }

    #[test]
    fn submission_ids_reject_invalid_input_and_pair_reuse_without_writes() {
        let directory = tempdir().unwrap();
        let repository = WordbookRepository::open(directory.path().join("words.sqlite")).unwrap();
        for (english, chinese, id) in [
            (" ", "释义", "valid"),
            ("word", " ", "valid"),
            ("word", "释义", " \t "),
        ] {
            assert!(matches!(
                repository.record_mistake_once(english, chinese, id),
                Err(RepositoryError::Validation(_))
            ));
        }
        assert!(repository.list_mistakes().unwrap().is_empty());
        let first = repository
            .record_mistake_once("word", "释义", "valid")
            .unwrap();
        for (english, chinese) in [("other", "释义"), ("word", "其他")] {
            assert!(matches!(
                repository.record_mistake_once(english, chinese, "valid"),
                Err(RepositoryError::Conflict)
            ));
        }
        assert_eq!(repository.list_mistakes().unwrap(), vec![first]);
    }

    #[test]
    fn old_mistake_database_upgrades_and_legacy_recording_still_increments() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("words.sqlite");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE mistakes (
                    id INTEGER PRIMARY KEY,
                    english TEXT NOT NULL,
                    chinese TEXT NOT NULL,
                    normalized_english TEXT NOT NULL,
                    error_count INTEGER NOT NULL,
                    UNIQUE(normalized_english, chinese)
                );
                INSERT INTO mistakes(english, chinese, normalized_english, error_count)
                VALUES ('Apple', '苹果', 'apple', 2);",
            )
            .unwrap();
        drop(connection);
        let repository = WordbookRepository::open(&path).unwrap();
        let once = repository
            .record_mistake_once("apple", "苹果", "new-id")
            .unwrap();
        assert_eq!(once.error_count, 3);
        assert_eq!(
            repository
                .record_mistake("APPLE", "苹果")
                .unwrap()
                .error_count,
            4
        );
        assert_eq!(
            repository
                .record_mistake_once("Apple", "苹果", "new-id")
                .unwrap()
                .error_count,
            4
        );
    }

    #[test]
    fn schema_initialization_is_idempotent() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("wordbooks.sqlite");
        WordbookRepository::open(&path).unwrap();
        WordbookRepository::open(&path).unwrap();
    }
}
