use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use super::model::{ImportedEntry, WordEntry, WordbookSummary};

#[derive(Debug)]
pub enum RepositoryError {
    Conflict,
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

    fn connect(&self) -> Result<Connection, rusqlite::Error> {
        let connection = Connection::open(&self.database_path)?;
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(connection)
    }

    fn initialize(&self) -> Result<(), rusqlite::Error> {
        self.connect()?.execute_batch(
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
            CREATE INDEX IF NOT EXISTS entries_wordbook_id ON entries(wordbook_id);",
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

    pub fn sample(&self, wordbook_id: i64, limit: u8) -> Result<Vec<WordEntry>, rusqlite::Error> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, english, chinese
             FROM entries
             WHERE wordbook_id = ?1
             ORDER BY RANDOM()
             LIMIT ?2",
        )?;
        let rows = statement.query_map(params![wordbook_id, i64::from(limit.min(10))], |row| {
            Ok(WordEntry {
                id: row.get(0)?,
                english: row.get(1)?,
                chinese: row.get(2)?,
            })
        })?;
        rows.collect()
    }
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
            .replace("第一册", &entries("first", 12), false)
            .unwrap();
        repository
            .replace("第二册", &entries("second", 2), false)
            .unwrap();

        let reopened = WordbookRepository::open(&path).unwrap();
        let listed = reopened.list().unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].entry_count, 12);

        let sample = reopened.sample(first.id, 50).unwrap();
        assert_eq!(sample.len(), 10);
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
    fn schema_initialization_is_idempotent() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("wordbooks.sqlite");
        WordbookRepository::open(&path).unwrap();
        WordbookRepository::open(&path).unwrap();
    }
}
