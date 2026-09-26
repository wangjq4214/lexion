//! Version-1 wordbook and favorite operations. Wire identities are names and word pairs,
//! never device-local SQLite row IDs.
use std::collections::HashSet;

use rusqlite::{params, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use super::replay::{Envelope, Projection, ReplayError};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Entry {
    pub english: String,
    pub chinese: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum ContentChange {
    Import {
        name: String,
        entries: Vec<Entry>,
    },
    DeleteBook {
        name: String,
    },
    DeleteEntry {
        name: String,
        normalized_english: String,
        chinese: String,
    },
    AddFavorite {
        english: String,
        chinese: String,
    },
    RemoveFavorite {
        normalized_english: String,
        chinese: String,
    },
}

impl ContentChange {
    pub(super) fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("content operations are JSON serializable")
    }

    fn decode(change: &Envelope) -> Result<Self, ReplayError> {
        if change.version != 1 {
            return Err(ReplayError::Invalid("unsupported content version"));
        }
        let payload: Self = serde_json::from_slice(&change.content)
            .map_err(|_| ReplayError::Invalid("unknown or malformed content operation"))?;
        payload.check()?;
        Ok(payload)
    }

    fn check(&self) -> Result<(), ReplayError> {
        match self {
            Self::Import { name, entries } => {
                required(name)?;
                let mut pairs = HashSet::new();
                for entry in entries {
                    required(&entry.english)?;
                    required(&entry.chinese)?;
                    if !pairs.insert((entry.english.to_lowercase(), entry.chinese.as_str())) {
                        return Err(ReplayError::Invalid("duplicate imported word pair"));
                    }
                }
            }
            Self::DeleteBook { name } => required(name)?,
            Self::DeleteEntry {
                name,
                normalized_english,
                chinese,
            } => {
                required(name)?;
                normalized(normalized_english)?;
                required(chinese)?;
            }
            Self::AddFavorite { english, chinese } => {
                required(english)?;
                required(chinese)?;
            }
            Self::RemoveFavorite {
                normalized_english,
                chinese,
            } => {
                normalized(normalized_english)?;
                required(chinese)?;
            }
        }
        Ok(())
    }
}

fn required(value: &str) -> Result<(), ReplayError> {
    if value.is_empty() || value.trim() != value {
        return Err(ReplayError::Invalid(
            "content fields must be nonempty and trimmed",
        ));
    }
    Ok(())
}

fn normalized(value: &str) -> Result<(), ReplayError> {
    required(value)?;
    if value.to_lowercase() != value {
        return Err(ReplayError::Invalid("normalized English must be lowercase"));
    }
    Ok(())
}

/// Stable, local-only IDs survive replay resets and reappearance of a deleted pair.
/// Mapping rows are not included in the wire format or deleted by reset.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ContentProjection;

impl ContentProjection {
    fn tables(tx: &Transaction<'_>) -> Result<(), ReplayError> {
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS sync_book_ids (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE
            );
            CREATE TABLE IF NOT EXISTS sync_entry_ids (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                normalized_english TEXT NOT NULL,
                chinese TEXT NOT NULL,
                UNIQUE(name, normalized_english, chinese)
            );",
        )?;
        Ok(())
    }

    fn book_id(tx: &Transaction<'_>, name: &str) -> Result<i64, ReplayError> {
        if let Some(id) = tx
            .query_row(
                "SELECT id FROM sync_book_ids WHERE name = ?1",
                [name],
                |r| r.get(0),
            )
            .optional()?
        {
            return Ok(id);
        }
        // Explicit high-water allocation also avoids collisions if an older local row exists.
        tx.execute(
            "INSERT INTO sync_book_ids(id, name)
             VALUES ((SELECT max(
                 coalesce((SELECT max(id) FROM sync_book_ids), 0),
                 coalesce((SELECT max(id) FROM wordbooks), 0)
             ) + 1), ?1)",
            [name],
        )?;
        Ok(tx.last_insert_rowid())
    }

    fn entry_id(
        tx: &Transaction<'_>,
        name: &str,
        normalized_english: &str,
        chinese: &str,
    ) -> Result<i64, ReplayError> {
        if let Some(id) = tx
            .query_row(
                "SELECT id FROM sync_entry_ids
                 WHERE name = ?1 AND normalized_english = ?2 AND chinese = ?3",
                params![name, normalized_english, chinese],
                |r| r.get(0),
            )
            .optional()?
        {
            return Ok(id);
        }
        tx.execute(
            "INSERT INTO sync_entry_ids(id, name, normalized_english, chinese)
             VALUES ((SELECT max(
                 coalesce((SELECT max(id) FROM sync_entry_ids), 0),
                 coalesce((SELECT max(id) FROM entries), 0)
             ) + 1), ?1, ?2, ?3)",
            params![name, normalized_english, chinese],
        )?;
        Ok(tx.last_insert_rowid())
    }

    fn apply_content(tx: &Transaction<'_>, payload: ContentChange) -> Result<(), ReplayError> {
        match payload {
            ContentChange::Import { name, entries } => {
                let id = Self::book_id(tx, &name)?;
                tx.execute(
                    "DELETE FROM review_coverage WHERE source = 'wordbook' AND source_id = ?1",
                    [id],
                )?;
                tx.execute("DELETE FROM entries WHERE wordbook_id = ?1", [id])?;
                tx.execute(
                    "INSERT OR IGNORE INTO wordbooks(id, name) VALUES (?1, ?2)",
                    params![id, name],
                )?;
                for entry in entries {
                    let normalized_english = entry.english.to_lowercase();
                    let entry_id = Self::entry_id(tx, &name, &normalized_english, &entry.chinese)?;
                    tx.execute(
                        "INSERT INTO entries(id, wordbook_id, english, chinese, normalized_english)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![
                            entry_id,
                            id,
                            entry.english,
                            entry.chinese,
                            normalized_english
                        ],
                    )?;
                }
            }
            ContentChange::DeleteBook { name } => {
                let id: Option<i64> = tx
                    .query_row("SELECT id FROM wordbooks WHERE name = ?1", [&name], |r| {
                        r.get(0)
                    })
                    .optional()?;
                if let Some(id) = id {
                    tx.execute(
                        "DELETE FROM review_coverage WHERE source = 'wordbook' AND source_id = ?1",
                        [id],
                    )?;
                    tx.execute("DELETE FROM wordbooks WHERE id = ?1", [id])?;
                }
            }
            ContentChange::DeleteEntry {
                name,
                normalized_english,
                chinese,
            } => {
                let id: Option<i64> = tx
                    .query_row("SELECT id FROM wordbooks WHERE name = ?1", [&name], |r| {
                        r.get(0)
                    })
                    .optional()?;
                if let Some(id) = id {
                    let deleted = tx.execute(
                        "DELETE FROM entries WHERE wordbook_id = ?1 AND normalized_english = ?2 AND chinese = ?3",
                        params![id, normalized_english, chinese],
                    )?;
                    if deleted != 0 {
                        tx.execute(
                            "DELETE FROM review_coverage WHERE source = 'wordbook' AND source_id = ?1
                             AND normalized_english = ?2 AND chinese = ?3",
                            params![id, normalized_english, chinese],
                        )?;
                    }
                }
            }
            ContentChange::AddFavorite { english, chinese } => {
                let normalized_english = english.to_lowercase();
                tx.execute(
                    "INSERT OR IGNORE INTO favorites(english, chinese, normalized_english)
                     VALUES (?1, ?2, ?3)",
                    params![english, chinese, normalized_english],
                )?;
            }
            ContentChange::RemoveFavorite {
                normalized_english,
                chinese,
            } => {
                let deleted = tx.execute(
                    "DELETE FROM favorites WHERE normalized_english = ?1 AND chinese = ?2",
                    params![normalized_english, chinese],
                )?;
                if deleted != 0 {
                    tx.execute(
                        "DELETE FROM review_coverage WHERE source = 'favorites'
                         AND normalized_english = ?1 AND chinese = ?2",
                        params![normalized_english, chinese],
                    )?;
                }
            }
        }
        Ok(())
    }
}

impl Projection for ContentProjection {
    fn validate(&self, change: &Envelope) -> Result<(), ReplayError> {
        ContentChange::decode(change).map(|_| ())
    }

    fn reset(&self, tx: &Transaction<'_>) -> Result<(), ReplayError> {
        Self::tables(tx)?;
        tx.execute_batch(
            "DROP TABLE IF EXISTS temp.sync_saved_coverage;
             CREATE TEMP TABLE sync_saved_coverage AS
               SELECT source,source_id,normalized_english,chinese FROM review_coverage
               WHERE source IN ('wordbook', 'favorites');
             DELETE FROM review_coverage WHERE source IN ('wordbook', 'favorites');
             DELETE FROM wordbooks;
             DELETE FROM favorites;",
        )?;
        Ok(())
    }

    fn apply(&self, tx: &Transaction<'_>, change: &Envelope) -> Result<(), ReplayError> {
        let payload = ContentChange::decode(change)?;
        Self::tables(tx)?;
        Self::apply_content(tx, payload)
    }

    fn finish_rebuild(
        &self,
        tx: &Transaction<'_>,
        newly_applied: &[Envelope],
    ) -> Result<(), ReplayError> {
        let mut invalid_books = HashSet::new();
        let mut invalid_entries = HashSet::new();
        let mut invalid_favorites = HashSet::new();
        for change in newly_applied {
            match ContentChange::decode(change)? {
                ContentChange::Import { name, .. } | ContentChange::DeleteBook { name } => {
                    invalid_books.insert(name);
                }
                ContentChange::DeleteEntry {
                    name,
                    normalized_english,
                    chinese,
                } => {
                    invalid_entries.insert((name, normalized_english, chinese));
                }
                ContentChange::RemoveFavorite {
                    normalized_english,
                    chinese,
                } => {
                    invalid_favorites.insert((normalized_english, chinese));
                }
                ContentChange::AddFavorite { .. } => {}
            }
        }
        let saved: Vec<(String, i64, String, String)> = {
            let mut stmt = tx.prepare(
                "SELECT source,source_id,normalized_english,chinese FROM temp.sync_saved_coverage",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for (source, source_id, english, chinese) in saved {
            let keep = if source == "wordbook" {
                let name: Option<String> = tx
                    .query_row(
                        "SELECT name FROM sync_book_ids WHERE id=?1",
                        [source_id],
                        |r| r.get(0),
                    )
                    .optional()?;
                if let Some(name) = name {
                    !invalid_books.contains(&name)
                        && !invalid_entries.contains(&(name, english.clone(), chinese.clone()))
                        && tx.query_row(
                            "SELECT EXISTS(SELECT 1 FROM entries WHERE wordbook_id=?1 AND normalized_english=?2 AND chinese=?3)",
                            params![source_id, english, chinese], |r| r.get::<_, bool>(0),
                        )?
                } else {
                    false
                }
            } else {
                !invalid_favorites.contains(&(english.clone(), chinese.clone()))
                    && tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM favorites WHERE normalized_english=?1 AND chinese=?2)",
                        params![english, chinese], |r| r.get::<_, bool>(0),
                    )?
            };
            if keep {
                tx.execute(
                    "INSERT OR IGNORE INTO review_coverage(source,source_id,normalized_english,chinese) VALUES (?1,?2,?3,?4)",
                    params![source, source_id, english, chinese],
                )?;
            }
        }
        tx.execute_batch("DROP TABLE temp.sync_saved_coverage")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wordbooks::repository::replay::{ChangeId, DeviceId};
    use std::collections::BTreeSet;

    fn envelope(payload: &ContentChange) -> Envelope {
        Envelope {
            id: ChangeId {
                device: DeviceId("a".repeat(32)),
                sequence: 1,
            },
            version: 1,
            content: payload.encode(),
            dependencies: BTreeSet::new(),
            occurred_at: None,
        }
    }

    #[test]
    fn validates_wire_fields_and_duplicate_normalized_pairs() {
        let projection = ContentProjection;
        let import = ContentChange::Import {
            name: "book".into(),
            entries: vec![
                Entry {
                    english: "Apple".into(),
                    chinese: "苹果".into(),
                },
                Entry {
                    english: "apple".into(),
                    chinese: "苹果".into(),
                },
            ],
        };
        assert!(projection.validate(&envelope(&import)).is_err());
        let mut change = envelope(&ContentChange::DeleteBook {
            name: "book".into(),
        });
        change.content = br#"{"type":"delete_book","name":"book","id":9}"#.to_vec();
        assert!(projection.validate(&change).is_err());
        change.content = r#"{"type":"delete_entry","name":"book","normalized_english":"Apple","chinese":"苹果"}"#.as_bytes().to_vec();
        assert!(projection.validate(&change).is_err());
    }

    #[test]
    fn reset_preserves_ids_and_unowned_data() {
        let directory = tempfile::tempdir().unwrap();
        let repository = crate::wordbooks::repository::WordbookRepository::open(
            directory.path().join("content.sqlite"),
        )
        .unwrap();
        let mut connection = repository.connect().unwrap();
        let tx = connection.transaction().unwrap();
        let projection = ContentProjection;
        let import = ContentChange::Import {
            name: "book".into(),
            entries: vec![Entry {
                english: "Apple".into(),
                chinese: "苹果".into(),
            }],
        };
        projection.reset(&tx).unwrap();
        projection.apply(&tx, &envelope(&import)).unwrap();
        let ids: (i64, i64) = tx.query_row(
            "SELECT wordbooks.id, entries.id FROM wordbooks JOIN entries ON entries.wordbook_id = wordbooks.id",
            [], |row| Ok((row.get(0)?, row.get(1)?))
        ).unwrap();
        tx.execute("INSERT INTO mistakes(english,chinese,normalized_english) VALUES ('Apple','苹果','apple')", []).unwrap();
        tx.execute(
            "INSERT INTO review_coverage VALUES ('mistakes',0,'apple','苹果')",
            [],
        )
        .unwrap();
        projection
            .apply(
                &tx,
                &envelope(&ContentChange::AddFavorite {
                    english: "Apple".into(),
                    chinese: "苹果".into(),
                }),
            )
            .unwrap();
        projection.reset(&tx).unwrap();
        projection.apply(&tx, &envelope(&import)).unwrap();
        let replayed: (i64, i64) = tx.query_row(
            "SELECT wordbooks.id, entries.id FROM wordbooks JOIN entries ON entries.wordbook_id = wordbooks.id",
            [], |row| Ok((row.get(0)?, row.get(1)?))
        ).unwrap();
        assert_eq!(ids, replayed);
        assert_eq!(
            tx.query_row("SELECT count(*) FROM mistakes", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            tx.query_row(
                "SELECT count(*) FROM review_coverage WHERE source='mistakes'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            tx.query_row("SELECT count(*) FROM favorites", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
