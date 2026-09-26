use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use super::model::{ExamQuestion, ImportedEntry, MistakeEntry, WordEntry, WordbookSummary};

#[path = "repository/content.rs"]
mod content;
mod learning;
pub(crate) mod replay;

/// Composite projection used by every production append and network ingress.
pub(crate) struct SyncProjection;
impl replay::Projection for SyncProjection {
    fn validate(&self, change: &replay::Envelope) -> Result<(), replay::ReplayError> {
        if is_learning(change) {
            replay::Projection::validate(&learning::LearningProjection, change)
        } else {
            replay::Projection::validate(&content::ContentProjection, change)
        }
    }
    fn reset(&self, tx: &Transaction<'_>) -> Result<(), replay::ReplayError> {
        replay::Projection::reset(&learning::LearningProjection, tx)?;
        replay::Projection::reset(&content::ContentProjection, tx)
    }
    fn apply(
        &self,
        tx: &Transaction<'_>,
        change: &replay::Envelope,
    ) -> Result<(), replay::ReplayError> {
        if is_learning(change) {
            replay::Projection::apply(&learning::LearningProjection, tx, change)
        } else {
            replay::Projection::apply(&content::ContentProjection, tx, change)
        }
    }
    fn finish_rebuild(
        &self,
        tx: &Transaction<'_>,
        changes: &[replay::Envelope],
    ) -> Result<(), replay::ReplayError> {
        let content: Vec<_> = changes
            .iter()
            .filter(|change| !is_learning(change))
            .cloned()
            .collect();
        replay::Projection::finish_rebuild(&content::ContentProjection, tx, &content)
    }
}

fn is_learning(change: &replay::Envelope) -> bool {
    serde_json::from_slice::<serde_json::Value>(&change.content)
        .ok()
        .and_then(|v| v.get("type")?.as_str().map(str::to_owned))
        .is_some_and(|kind| {
            matches!(
                kind.as_str(),
                "mistake" | "schedule" | "complete" | "target"
            )
        })
}
#[path = "schedule.rs"]
mod schedule;

#[derive(Debug)]
pub enum RepositoryError {
    Conflict,
    Validation(&'static str),
    Database(rusqlite::Error),
    Replay(replay::ReplayError),
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

    fn sync_enabled(&self) -> Result<bool, RepositoryError> {
        let connection = self.connect()?;
        Ok(!connection.query_row(
            "SELECT legacy FROM replay_identity WHERE singleton=1",
            [],
            |row| row.get::<_, bool>(0),
        )?)
    }

    fn record_content(&self, change: content::ContentChange) -> Result<(), RepositoryError> {
        self.append_local(change.encode(), None, &SyncProjection)
            .map_err(RepositoryError::Replay)?;
        Ok(())
    }

    fn initialize(&self) -> Result<(), rusqlite::Error> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        // Check before adding default settings: a legacy database has no reconstructible history.
        let legacy = replay::has_legacy_data(&transaction)?;
        transaction.execute_batch(
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
        let has_mistake_created_at: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('mistake_submissions') WHERE name = 'created_at')",
            [],
            |row| row.get(0),
        )?;
        if !has_mistake_created_at {
            transaction.execute_batch(
                "ALTER TABLE mistake_submissions ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0;",
            )?;
        }
        transaction.execute_batch(
            "CREATE INDEX IF NOT EXISTS mistake_submissions_created_at ON mistake_submissions(created_at);",
        )?;
        let has_created_at: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('pending_reviews') WHERE name = 'created_at')",
            [],
            |row| row.get(0),
        )?;
        if !has_created_at {
            transaction.execute_batch(
                "CREATE TABLE pending_reviews_new (
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
                 ALTER TABLE pending_reviews_new RENAME TO pending_reviews;",
            )?;
        }
        transaction.execute_batch(
            "CREATE INDEX IF NOT EXISTS pending_reviews_created_at ON pending_reviews(created_at);",
        )?;
        transaction.execute_batch("CREATE TABLE IF NOT EXISTS sync_submissions (
            device TEXT NOT NULL, token TEXT NOT NULL, normalized_english TEXT NOT NULL, chinese TEXT NOT NULL,
            created_at INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY(device,token));
            CREATE TABLE IF NOT EXISTS sync_completions (
            device TEXT NOT NULL, sequence INTEGER NOT NULL, item_index INTEGER NOT NULL,
            PRIMARY KEY(device,sequence,item_index));
            CREATE TABLE IF NOT EXISTS sync_retired_reviews (
            device TEXT NOT NULL, sequence INTEGER NOT NULL, item_index INTEGER NOT NULL,
            PRIMARY KEY(device,sequence,item_index));")?;
        let has_sync_created_at: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('sync_submissions') WHERE name='created_at')",
            [],
            |row| row.get(0),
        )?;
        if !has_sync_created_at {
            transaction.execute_batch(
                "ALTER TABLE sync_submissions ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0;",
            )?;
        }
        for (column, definition) in [
            ("origin_device", "TEXT"),
            ("origin_sequence", "INTEGER"),
            ("origin_index", "INTEGER"),
        ] {
            let exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('pending_reviews') WHERE name=?1)",
                [column],
                |row| row.get(0),
            )?;
            if !exists {
                transaction.execute_batch(&format!(
                    "ALTER TABLE pending_reviews ADD COLUMN {column} {definition};"
                ))?;
            }
        }
        transaction.execute_batch("CREATE UNIQUE INDEX IF NOT EXISTS pending_origin ON pending_reviews(origin_device,origin_sequence,origin_index);")?;
        replay::initialize(&transaction, legacy)?;
        transaction.commit()?;
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

    pub fn list_wordbook_entries(
        &self,
        wordbook_id: i64,
    ) -> Result<Vec<WordEntry>, RepositoryError> {
        if wordbook_id <= 0 {
            return Err(RepositoryError::Validation("单词本 ID 必须大于零"));
        }
        let connection = self.connect()?;
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM wordbooks WHERE id = ?1)",
            [wordbook_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(RepositoryError::Validation("单词本不存在"));
        }
        let mut statement = connection.prepare(
            "SELECT id, english, chinese FROM entries WHERE wordbook_id = ?1 ORDER BY id",
        )?;
        let rows = statement.query_map([wordbook_id], word_entry)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn replace(
        &self,
        name: &str,
        entries: &[ImportedEntry],
        replace_existing: bool,
    ) -> Result<WordbookSummary, RepositoryError> {
        let mut connection = self.connect()?;
        if self.sync_enabled()? {
            let name = name.trim();
            if name.is_empty() {
                return Err(RepositoryError::Validation("单词本名称不能为空"));
            }
            let change = content::ContentChange::Import {
                name: name.to_owned(),
                entries: entries
                    .iter()
                    .map(|entry| content::Entry {
                        english: entry.english.clone(),
                        chinese: entry.chinese.clone(),
                    })
                    .collect(),
            };
            self.append_local_checked(change.encode(), None, &SyncProjection, |tx| {
                if !replace_existing
                    && tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM wordbooks WHERE name=?1)",
                        [name],
                        |row| row.get::<_, bool>(0),
                    )?
                {
                    return Err(replay::ReplayError::BusinessConflict);
                }
                Ok(())
            })
            .map_err(|error| match error {
                replay::ReplayError::BusinessConflict => RepositoryError::Conflict,
                other => RepositoryError::Replay(other),
            })?;
            return Ok(self
                .list()?
                .into_iter()
                .find(|book| book.name == name)
                .expect("imported wordbook must exist"));
        }
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

    pub fn delete_wordbook(&self, wordbook_id: i64) -> Result<bool, RepositoryError> {
        if wordbook_id <= 0 {
            return Err(RepositoryError::Validation("单词本 ID 必须大于零"));
        }
        let mut connection = self.connect()?;
        if self.sync_enabled()? {
            let name: Option<String> = self
                .connect()?
                .query_row(
                    "SELECT name FROM wordbooks WHERE id=?1",
                    [wordbook_id],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(name) = name else {
                return Ok(false);
            };
            self.record_content(content::ContentChange::DeleteBook { name })?;
            return Ok(true);
        }
        let transaction = connection.transaction()?;
        let deleted = transaction.execute("DELETE FROM wordbooks WHERE id = ?1", [wordbook_id])?;
        if deleted != 0 {
            transaction.execute(
                "DELETE FROM review_coverage WHERE source = 'wordbook' AND source_id = ?1",
                [wordbook_id],
            )?;
        }
        transaction.commit()?;
        Ok(deleted != 0)
    }
    pub fn delete_wordbook_entry(
        &self,
        wordbook_id: i64,
        entry_id: i64,
    ) -> Result<bool, RepositoryError> {
        if wordbook_id <= 0 || entry_id <= 0 {
            return Err(RepositoryError::Validation("单词本和词条 ID 必须大于零"));
        }
        let mut connection = self.connect()?;
        if self.sync_enabled()? {
            let target: Option<(String, String, String)> = self
                .connect()?
                .query_row(
                    "SELECT wordbooks.name, entries.normalized_english, entries.chinese
                 FROM entries JOIN wordbooks ON entries.wordbook_id=wordbooks.id
                 WHERE entries.id=?1 AND wordbooks.id=?2",
                    params![entry_id, wordbook_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            let Some((name, normalized_english, chinese)) = target else {
                return Ok(false);
            };
            self.record_content(content::ContentChange::DeleteEntry {
                name,
                normalized_english,
                chinese,
            })?;
            return Ok(true);
        }
        let transaction = connection.transaction()?;
        let pair: Option<(String, String)> = transaction
            .query_row(
                "SELECT normalized_english, chinese FROM entries WHERE id = ?1 AND wordbook_id = ?2",
                params![entry_id, wordbook_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((normalized_english, chinese)) = pair else {
            return Ok(false);
        };
        transaction.execute(
            "DELETE FROM entries WHERE id = ?1 AND wordbook_id = ?2",
            params![entry_id, wordbook_id],
        )?;
        transaction.execute(
            "DELETE FROM review_coverage WHERE source = 'wordbook' AND source_id = ?1
             AND normalized_english = ?2 AND chinese = ?3",
            params![wordbook_id, normalized_english, chinese],
        )?;
        // Pending review tokens are pair-scoped, not source-scoped. Leave them to their
        // normal expiration so deleting one source cannot invalidate another's review.
        transaction.commit()?;
        Ok(true)
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
        if self.sync_enabled()? {
            if !self.is_favorite(english, chinese)? {
                self.record_content(content::ContentChange::AddFavorite {
                    english: english.to_owned(),
                    chinese: chinese.to_owned(),
                })?;
            }
            return Ok(self.connect()?.query_row(
                "SELECT id,english,chinese FROM favorites WHERE normalized_english=?1 AND chinese=?2",
                params![english.to_lowercase(), chinese], word_entry,
            )?);
        }
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
        if self.sync_enabled()? {
            if !self.is_favorite(english, chinese)? {
                return Ok(false);
            }
            self.record_content(content::ContentChange::RemoveFavorite {
                normalized_english: english.to_lowercase(),
                chinese: chinese.to_owned(),
            })?;
            return Ok(true);
        }
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
        if self.sync_enabled()? {
            self.append_local(
                learning::LearningChange::Mistake {
                    english: english.to_owned(),
                    chinese: chinese.to_owned(),
                    submission: None,
                }
                .encode(),
                Some(rusqlite::Connection::open(&self.database_path)?.query_row(
                    "SELECT unixepoch()",
                    [],
                    |r| r.get(0),
                )?),
                &SyncProjection,
            )
            .map_err(RepositoryError::Replay)?;
            return Ok(self.connect()?.query_row("SELECT id,english,chinese,error_count FROM mistakes WHERE normalized_english=?1 AND chinese=?2",
                params![english.to_lowercase(),chinese], mistake_entry)?);
        }
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
        if self.sync_enabled()? {
            let device = self.replay_device_id().map_err(RepositoryError::Replay)?;
            let normalized = normalized_english.clone();
            let token = submission_id.to_owned();
            let change = learning::LearningChange::Mistake {
                english: english.to_owned(),
                chinese: chinese.to_owned(),
                submission: Some(token.clone()),
            };
            let occurred_at: i64 = self
                .connect()?
                .query_row("SELECT unixepoch()", [], |r| r.get(0))?;
            match self.append_local_build(Some(occurred_at), &SyncProjection, |tx| {
                tx.execute("DELETE FROM mistake_submissions WHERE created_at < unixepoch()-2592000 AND submission_id NOT LIKE 'exam:%'", [])?;
                let previous: Option<(String, String, i64)> = tx.query_row(
                    "SELECT normalized_english,chinese,created_at FROM sync_submissions WHERE device=?1 AND token=?2",
                    params![device.0, token],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                ).optional()?;
                if let Some((old_english, old_chinese, old_time)) = previous {
                    if token.starts_with("exam:") || occurred_at.saturating_sub(old_time) <= 2_592_000 {
                        if (old_english, old_chinese) != (normalized.clone(), chinese.to_owned()) {
                            return Err(replay::ReplayError::BusinessConflict);
                        }
                        // A retry must not allocate another event.
                        return Err(replay::ReplayError::Invalid("already submitted"));
                    }
                }
                Ok(change.encode())
            }) {
                Ok(_) | Err(replay::ReplayError::Invalid("already submitted")) => {},
                Err(replay::ReplayError::BusinessConflict) => return Err(RepositoryError::Conflict),
                Err(other) => return Err(RepositoryError::Replay(other)),
            }
            return Ok(self.connect()?.query_row("SELECT id,english,chinese,error_count FROM mistakes WHERE normalized_english=?1 AND chinese=?2",
                params![normalized_english,chinese], mistake_entry)?);
        }
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        // Practice retry tokens expire after 30 days. Exam tokens must remain claimed:
        // a timed-out final check may be retried after the exam has been left open.
        transaction.execute(
            "DELETE FROM mistake_submissions WHERE created_at < unixepoch() - 2592000 AND submission_id NOT LIKE 'exam:%'",
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

    pub fn sample_exam(
        &self,
        source: &str,
        wordbook_id: Option<i64>,
        en_to_zh_count: u32,
        zh_to_en_count: u32,
    ) -> Result<Vec<ExamQuestion>, RepositoryError> {
        let total = u64::from(en_to_zh_count) + u64::from(zh_to_en_count);
        if total == 0 {
            return Err(RepositoryError::Validation("考试题数必须大于零"));
        }
        // Table names are selected only from these literals; user input is never interpolated.
        let (table, source_id) = match (source, wordbook_id) {
            ("wordbook", Some(id)) if id > 0 => ("entries", id),
            ("favorites", None) => ("favorites", 0),
            ("mistakes", None) => ("mistakes", 0),
            _ => return Err(RepositoryError::Validation("无效的考试来源或单词本")),
        };
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        if source == "wordbook" {
            let exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM wordbooks WHERE id = ?1)",
                [source_id],
                |row| row.get(0),
            )?;
            if !exists {
                return Err(RepositoryError::Validation("单词本不存在"));
            }
        }
        let where_clause = if source == "wordbook" {
            " WHERE wordbook_id = ?1"
        } else {
            " WHERE ?1 = 0"
        };
        let available: i64 = transaction.query_row(
            &format!("SELECT COUNT(*) FROM {table}{where_clause}"),
            [source_id],
            |row| row.get(0),
        )?;
        if total > available as u64 {
            return Err(RepositoryError::Validation(
                "考试题数超过来源可用词条数，请减少题数",
            ));
        }
        let entries: Vec<WordEntry> = {
            let mut statement = transaction.prepare(&format!(
                "SELECT id, english, chinese FROM {table}{where_clause} ORDER BY RANDOM() LIMIT ?2"
            ))?;
            let rows = statement.query_map(params![source_id, total as i64], word_entry)?;
            rows.collect::<Result<_, _>>()?
        };
        // One randomized draw, then assign each sampled pair exactly one direction.
        let questions = entries
            .into_iter()
            .enumerate()
            .map(|(index, entry)| ExamQuestion {
                entry,
                direction: if (index as u64) < u64::from(en_to_zh_count) {
                    "en-to-zh"
                } else {
                    "zh-to-en"
                }
                .to_owned(),
            })
            .collect();
        Ok(questions)
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
#[path = "repository/content_tests.rs"]
mod content_tests;
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
    fn exam_samples_exact_unique_directions_from_each_source() {
        use std::collections::HashSet;

        let directory = tempdir().unwrap();
        let repository = WordbookRepository::open(directory.path().join("words.sqlite")).unwrap();
        let first = repository
            .replace("first", &entries("first", 300), false)
            .unwrap();
        repository
            .replace("second", &entries("second", 3), false)
            .unwrap();
        let questions = repository
            .sample_exam("wordbook", Some(first.id), 160, 140)
            .unwrap();
        assert_eq!(questions.len(), 300);
        assert_eq!(
            questions
                .iter()
                .filter(|q| q.direction == "en-to-zh")
                .count(),
            160
        );
        assert_eq!(
            questions
                .iter()
                .filter(|q| q.direction == "zh-to-en")
                .count(),
            140
        );
        assert_eq!(
            questions
                .iter()
                .map(|q| (&q.entry.english, &q.entry.chinese))
                .collect::<HashSet<_>>()
                .len(),
            300
        );
        assert!(questions
            .iter()
            .all(|q| q.entry.english.starts_with("first-")));
        assert_eq!(
            serde_json::to_value(&questions[0]).unwrap(),
            serde_json::json!({
                "entry": questions[0].entry, "direction": questions[0].direction
            })
        );

        // Independent collections remain usable without their originating wordbook.
        repository.add_favorite("standalone", "独立").unwrap();
        repository.add_favorite("another", "另一项").unwrap();
        repository.record_mistake("standalone", "独立").unwrap();
        repository.record_mistake("another", "另一项").unwrap();
        repository.delete_wordbook(first.id).unwrap();
        let second_id = repository.list().unwrap()[0].id;
        repository.delete_wordbook(second_id).unwrap();
        for source in ["favorites", "mistakes"] {
            let questions = repository.sample_exam(source, None, 1, 1).unwrap();
            assert_eq!(questions.len(), 2);
            assert_ne!(questions[0].entry.id, questions[1].entry.id);
            assert_eq!(
                questions
                    .iter()
                    .filter(|q| q.direction == "en-to-zh")
                    .count(),
                1
            );
            assert_eq!(
                questions
                    .iter()
                    .filter(|q| q.direction == "zh-to-en")
                    .count(),
                1
            );
            assert!(questions
                .iter()
                .all(|q| q.entry.english == "standalone" || q.entry.english == "another"));
        }
    }

    #[test]
    fn exam_rejects_invalid_sources_and_insufficient_pool_without_shortening() {
        let directory = tempdir().unwrap();
        let repository = WordbookRepository::open(directory.path().join("words.sqlite")).unwrap();
        let book = repository
            .replace("book", &entries("one", 2), false)
            .unwrap();
        for (source, id, en, zh) in [
            ("wordbook", Some(book.id), 0, 0),
            ("wordbook", Some(book.id), 2, 1),
            ("wordbook", Some(book.id), u32::MAX, u32::MAX),
            ("favorites", None, 1, 0),
            ("mistakes", None, 0, 1),
            ("wordbook", None, 1, 0),
            ("wordbook", Some(0), 1, 0),
            ("wordbook", Some(i64::MAX), 1, 0),
            ("favorites", Some(book.id), 1, 0),
            ("invalid", None, 1, 0),
        ] {
            assert!(matches!(
                repository.sample_exam(source, id, en, zh),
                Err(RepositoryError::Validation(_))
            ));
        }
        assert_eq!(
            repository
                .sample_exam("wordbook", Some(book.id), 0, 2)
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            repository
                .sample_exam("wordbook", Some(book.id), 2, 0)
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn exam_sampling_never_changes_review_tables_or_mistakes() {
        let directory = tempdir().unwrap();
        let repository = WordbookRepository::open(directory.path().join("words.sqlite")).unwrap();
        let book = repository
            .replace("book", &entries("one", 2), false)
            .unwrap();
        repository.add_favorite("independent", "独立").unwrap();
        repository.record_mistake("independent", "独立").unwrap();
        let scheduled = repository
            .schedule_practice("wordbook", Some(book.id), 1, "en-to-zh")
            .unwrap();
        repository
            .complete_review(scheduled[0].review_id, 0, 0, false)
            .unwrap();
        let snapshot = || {
            let connection = repository.connect().unwrap();
            let tables = [
                "review_coverage",
                "review_memory",
                "pending_reviews",
                "mistakes",
            ];
            tables.map(|table| {
                let mut statement = connection
                    .prepare(&format!("SELECT * FROM {table} ORDER BY 1"))
                    .unwrap();
                let columns = statement.column_count();
                statement
                    .query_map([], |row| {
                        (0..columns)
                            .map(|i| row.get::<_, rusqlite::types::Value>(i))
                            .collect::<rusqlite::Result<Vec<_>>>()
                    })
                    .unwrap()
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .unwrap()
            })
        };
        let before = snapshot();
        repository
            .sample_exam("wordbook", Some(book.id), 1, 1)
            .unwrap();
        repository.sample_exam("favorites", None, 1, 0).unwrap();
        repository.sample_exam("mistakes", None, 0, 1).unwrap();
        assert!(matches!(
            repository.sample_exam("wordbook", Some(book.id), 2, 1),
            Err(RepositoryError::Validation(_))
        ));
        assert_eq!(snapshot(), before);
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
    fn listing_wordbook_entries_returns_all_in_id_order_and_only_selected_book() {
        let directory = tempdir().unwrap();
        let repository = WordbookRepository::open(directory.path().join("words.sqlite")).unwrap();
        let first = repository
            .replace("first", &entries("first", 300), false)
            .unwrap();
        let second = repository
            .replace("second", &entries("second", 2), false)
            .unwrap();
        let empty = repository.replace("empty", &[], false).unwrap();

        let listed = repository.list_wordbook_entries(first.id).unwrap();
        assert_eq!(listed.len(), 300);
        for (index, entry) in listed.iter().enumerate() {
            assert!(entry.id > 0);
            assert_eq!(entry.english, format!("first-{index}"));
            assert_eq!(entry.chinese, format!("释义-{index}"));
        }
        assert!(listed.windows(2).all(|pair| pair[0].id < pair[1].id));
        assert_eq!(
            repository
                .list_wordbook_entries(second.id)
                .unwrap()
                .iter()
                .map(|entry| entry.english.as_str())
                .collect::<Vec<_>>(),
            vec!["second-0", "second-1"]
        );
        assert!(repository
            .list_wordbook_entries(empty.id)
            .unwrap()
            .is_empty());
        for id in [0, -1, i64::MAX] {
            assert!(matches!(
                repository.list_wordbook_entries(id),
                Err(RepositoryError::Validation(_))
            ));
        }
    }

    #[test]
    fn deleting_wordbook_is_scoped_and_preserves_independent_data() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("words.sqlite");
        let repository = WordbookRepository::open(&path).unwrap();
        let pair = [ImportedEntry {
            english: "Apple".into(),
            chinese: "苹果".into(),
        }];
        let first = repository.replace("first", &pair, false).unwrap();
        let second = repository.replace("second", &pair, false).unwrap();
        repository.add_favorite("Apple", "苹果").unwrap();
        repository.record_mistake("Apple", "苹果").unwrap();
        let connection = repository.connect().unwrap();
        for id in [first.id, second.id] {
            connection.execute(
                "INSERT INTO review_coverage(source, source_id, normalized_english, chinese) VALUES ('wordbook', ?1, 'apple', '苹果')",
                [id],
            ).unwrap();
        }
        drop(connection);

        assert!(matches!(
            repository.delete_wordbook(0),
            Err(RepositoryError::Validation(_))
        ));
        assert!(!repository.delete_wordbook(i64::MAX).unwrap());
        assert!(repository.delete_wordbook(first.id).unwrap());
        assert!(!repository.delete_wordbook(first.id).unwrap());
        let reopened = WordbookRepository::open(&path).unwrap();
        assert_eq!(reopened.list().unwrap().len(), 1);
        assert_eq!(reopened.list().unwrap()[0].id, second.id);
        assert!(reopened.sample(first.id, 10).unwrap().is_empty());
        assert_eq!(reopened.sample(second.id, 10).unwrap().len(), 1);
        assert_eq!(reopened.list_favorites().unwrap().len(), 1);
        assert_eq!(reopened.list_mistakes().unwrap().len(), 1);
        let connection = reopened.connect().unwrap();
        let coverage: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM review_coverage WHERE source = 'wordbook' AND source_id = ?1",
                [first.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(coverage, 0);
        let other_coverage: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM review_coverage WHERE source = 'wordbook' AND source_id = ?1",
                [second.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(other_coverage, 1);
    }

    #[test]
    fn deleting_entry_scopes_coverage_and_preserves_independent_collections() {
        let directory = tempdir().unwrap();
        let repository = WordbookRepository::open(directory.path().join("words.sqlite")).unwrap();
        let first = repository
            .replace(
                "first",
                &[
                    ImportedEntry {
                        english: "Apple".into(),
                        chinese: "苹果".into(),
                    },
                    ImportedEntry {
                        english: "Pear".into(),
                        chinese: "梨".into(),
                    },
                ],
                false,
            )
            .unwrap();
        let second = repository
            .replace(
                "second",
                &[ImportedEntry {
                    english: "apple".into(),
                    chinese: "苹果".into(),
                }],
                false,
            )
            .unwrap();
        let target = repository
            .sample(first.id, 10)
            .unwrap()
            .into_iter()
            .find(|entry| entry.english == "Apple")
            .unwrap();
        let other = repository.sample(second.id, 10).unwrap()[0].clone();
        let favorite = repository.add_favorite("Apple", "苹果").unwrap();
        let mistake = repository.record_mistake("Apple", "苹果").unwrap();
        let first_review = repository
            .schedule_practice("wordbook", Some(first.id), 2, "zh-to-en")
            .unwrap();
        let target_review = first_review
            .iter()
            .find(|question| question.entry.id == target.id)
            .unwrap();
        repository
            .complete_review(target_review.review_id, 0, 0, false)
            .unwrap();
        repository
            .schedule_practice("wordbook", Some(second.id), 1, "zh-to-en")
            .unwrap();
        repository
            .schedule_practice("favorites", None, 1, "zh-to-en")
            .unwrap();
        repository
            .schedule_practice("mistakes", None, 1, "zh-to-en")
            .unwrap();
        let connection = repository.connect().unwrap();
        let coverage = |source: &str, source_id: i64, english: &str| -> i64 {
            connection.query_row(
                "SELECT COUNT(*) FROM review_coverage WHERE source = ?1 AND source_id = ?2 AND normalized_english = ?3 AND chinese = '苹果'",
                params![source, source_id, english], |row| row.get(0),
            ).unwrap()
        };
        assert_eq!(coverage("wordbook", first.id, "apple"), 1);
        assert_eq!(coverage("wordbook", second.id, "apple"), 1);
        assert_eq!(coverage("favorites", 0, "apple"), 1);
        assert_eq!(coverage("mistakes", 0, "apple"), 1);
        let memory_before: (f64, f64, f64) = connection.query_row(
            "SELECT stability, last_reviewed, due_at FROM review_memory WHERE normalized_english = 'apple' AND chinese = '苹果' AND direction = 'zh-to-en'",
            [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).unwrap();
        drop(connection);

        // A valid entry belonging to another wordbook and a missing entry are no-ops.
        assert!(!repository
            .delete_wordbook_entry(first.id, other.id)
            .unwrap());
        assert!(!repository
            .delete_wordbook_entry(first.id, i64::MAX)
            .unwrap());
        assert_eq!(repository.sample(first.id, 10).unwrap().len(), 2);
        assert_eq!(coverage_count(&repository, first.id, "apple"), 1);
        assert!(repository
            .delete_wordbook_entry(first.id, target.id)
            .unwrap());
        assert!(!repository
            .delete_wordbook_entry(first.id, target.id)
            .unwrap());
        assert_eq!(repository.sample(first.id, 10).unwrap().len(), 1);
        assert_eq!(repository.sample(second.id, 10).unwrap(), vec![other]);
        assert_eq!(repository.list().unwrap()[0].entry_count, 1);
        assert_eq!(coverage_count(&repository, first.id, "apple"), 0);
        assert_eq!(coverage_count(&repository, second.id, "apple"), 1);
        assert_eq!(repository.list_favorites().unwrap(), vec![favorite]);
        assert_eq!(repository.list_mistakes().unwrap(), vec![mistake]);
        let connection = repository.connect().unwrap();
        for source in ["favorites", "mistakes"] {
            assert_eq!(connection.query_row(
                "SELECT COUNT(*) FROM review_coverage WHERE source = ?1 AND normalized_english = 'apple' AND chinese = '苹果'",
                [source], |row| row.get::<_, i64>(0),
            ).unwrap(), 1);
        }
        assert_eq!(connection.query_row(
            "SELECT stability, last_reviewed, due_at FROM review_memory WHERE normalized_english = 'apple' AND chinese = '苹果' AND direction = 'zh-to-en'",
            [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).unwrap(), memory_before);
    }

    fn coverage_count(repository: &WordbookRepository, wordbook_id: i64, english: &str) -> i64 {
        repository.connect().unwrap().query_row(
            "SELECT COUNT(*) FROM review_coverage WHERE source = 'wordbook' AND source_id = ?1 AND normalized_english = ?2",
            params![wordbook_id, english], |row| row.get(0),
        ).unwrap()
    }

    #[test]
    fn deletion_rejects_nonpositive_ids_without_writes() {
        let directory = tempdir().unwrap();
        let repository = WordbookRepository::open(directory.path().join("words.sqlite")).unwrap();
        let book = repository
            .replace("book", &entries("word", 1), false)
            .unwrap();
        let entry = repository.sample(book.id, 1).unwrap()[0].clone();
        repository
            .schedule_practice("wordbook", Some(book.id), 1, "zh-to-en")
            .unwrap();
        for (book_id, entry_id) in [(0, entry.id), (-1, entry.id), (book.id, 0), (book.id, -1)] {
            assert!(matches!(
                repository.delete_wordbook_entry(book_id, entry_id),
                Err(RepositoryError::Validation(_))
            ));
        }
        assert_eq!(repository.sample(book.id, 1).unwrap(), vec![entry]);
        assert_eq!(coverage_count(&repository, book.id, "word-0"), 1);
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
            Err(RepositoryError::Replay(replay::ReplayError::Invalid(_)))
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
    fn exam_submission_claims_survive_practice_token_cleanup() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("words.sqlite");
        let repository = WordbookRepository::open(&path).unwrap();
        repository
            .record_mistake_once("apple", "苹果", "exam:one")
            .unwrap();
        repository
            .connect()
            .unwrap()
            .execute(
                "UPDATE mistake_submissions SET created_at = 0 WHERE submission_id = 'exam:one'",
                [],
            )
            .unwrap();
        repository
            .record_mistake_once("book", "书", "practice:new")
            .unwrap();
        drop(repository);
        let reopened = WordbookRepository::open(path).unwrap();
        let repeated = reopened
            .record_mistake_once("apple", "苹果", "exam:one")
            .unwrap();
        assert_eq!(repeated.error_count, 1);
        assert_eq!(reopened.list_mistakes().unwrap().len(), 2);
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
