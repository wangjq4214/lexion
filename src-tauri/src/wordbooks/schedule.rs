use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, OptionalExtension};

use super::{RepositoryError, WordbookRepository};
use crate::wordbooks::model::{ScheduledQuestion, WordEntry};

// Calibration knobs: seconds throughout. These are predictions, not measured recall probabilities.
// A review can be retried after an ambiguous response, but abandoned round tokens need not live forever.
const REVIEW_TOKEN_RETENTION_SECONDS: f64 = 30.0 * 24.0 * 60.0 * 60.0;
#[derive(Clone, Copy)]
pub(super) struct Curve {
    pub(super) target: f64,
    pub(super) initial_stability: f64,
    pub(super) success_growth: f64,
    pub(super) error_weight: f64,
    pub(super) hint_weight: f64,
    pub(super) skip_factor: f64,
    pub(super) minimum_stability: f64,
    pub(super) maximum_stability: f64,
}

pub(super) const CURVE: Curve = Curve {
    target: 0.9,
    initial_stability: 864_000.0,
    success_growth: 1.6,
    error_weight: 0.4,
    hint_weight: 0.25,
    skip_factor: 0.5,
    minimum_stability: 60.0,
    maximum_stability: 864_000_000.0,
};

impl Curve {
    pub(super) fn interval(self, stability: f64) -> f64 {
        -stability * self.target.ln()
    }

    #[cfg(test)]
    fn retention(self, elapsed: f64, stability: f64) -> f64 {
        (-elapsed.max(0.0) / stability).exp()
    }

    pub(super) fn updated(self, previous: f64, errors: u32, hints: u8, skipped: bool) -> f64 {
        let difficulty =
            1.0 + self.error_weight * f64::from(errors) + self.hint_weight * f64::from(hints);
        let multiplier = if skipped {
            self.skip_factor / difficulty
        } else {
            self.success_growth / difficulty
        };
        (previous * multiplier).clamp(self.minimum_stability, self.maximum_stability)
    }
}

fn now_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

struct Candidate {
    entry: WordEntry,
    normalized: String,
    covered: bool,
    pending_order: Option<i64>,
    due_at: Option<f64>,
    direction: &'static str,
}

type PendingReviewOrigin = (Option<String>, Option<i64>, Option<i64>, bool);

fn select_candidates(candidates: Vec<Candidate>, capacity: usize, now: f64) -> Vec<Candidate> {
    let mut pending = Vec::new();
    let mut unseen = Vec::new();
    let mut due = Vec::new();
    let mut other = Vec::new();
    for candidate in candidates {
        if candidate.pending_order.is_some() {
            pending.push(candidate);
        } else if !candidate.covered {
            unseen.push(candidate);
        } else if candidate.due_at.is_some_and(|time| time <= now) {
            due.push(candidate);
        } else {
            other.push(candidate);
        }
    }
    pending.sort_by_key(|item| item.pending_order.unwrap());
    due.sort_by(|a, b| {
        a.due_at
            .unwrap()
            .total_cmp(&b.due_at.unwrap())
            .then(a.entry.id.cmp(&b.entry.id))
    });
    let mut selected = Vec::with_capacity(capacity);
    selected.extend(pending.drain(..pending.len().min(capacity)));
    let remaining = capacity - selected.len();
    let reserve = usize::from(!unseen.is_empty());
    selected.extend(due.drain(..due.len().min(remaining.saturating_sub(reserve))));
    selected.extend(unseen.drain(..unseen.len().min(capacity - selected.len())));
    selected.extend(due.drain(..due.len().min(capacity - selected.len())));
    selected.extend(other.drain(..other.len().min(capacity - selected.len())));
    selected
}

impl WordbookRepository {
    pub fn review_target(&self) -> Result<f64, RepositoryError> {
        let connection = self.connect()?;
        Ok(connection.query_row(
            "SELECT target_retention FROM review_settings WHERE id = 1",
            [],
            |row| row.get(0),
        )?)
    }
    /// Count distinct word-and-meaning pairs due before tomorrow among current wordbooks.
    pub fn due_word_count(
        &self,
        wordbook_id: Option<i64>,
        tomorrow: f64,
    ) -> Result<u64, RepositoryError> {
        if wordbook_id.is_some_and(|id| id <= 0) || !tomorrow.is_finite() || tomorrow <= 0.0 {
            return Err(RepositoryError::Validation("无效的待复习查询"));
        }
        let connection = self.connect()?;
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM (
                SELECT DISTINCT e.normalized_english, e.chinese FROM entries e
                WHERE (?1 IS NULL OR e.wordbook_id = ?1)
                  AND EXISTS (
                    SELECT 1 FROM review_memory m
                    WHERE m.normalized_english = e.normalized_english AND m.chinese = e.chinese
                      AND m.due_at < ?2
                  )
            )",
            params![wordbook_id, tomorrow],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    pub fn set_review_target(&self, target: f64) -> Result<(), RepositoryError> {
        if !target.is_finite() || !(0.0..1.0).contains(&target) || target == 0.0 {
            return Err(RepositoryError::Validation("目标保持率必须在 0 和 1 之间"));
        }
        if self.sync_enabled()? {
            self.append_local(
                super::learning::LearningChange::Target { value: target }.encode(),
                Some(now_seconds() as i64),
                &super::SyncProjection,
            )
            .map_err(RepositoryError::Replay)?;
            return Ok(());
        }
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "UPDATE review_settings SET target_retention = ?1 WHERE id = 1",
            [target],
        )?;
        transaction.execute(
            "UPDATE review_memory SET due_at = last_reviewed + stability * ?1",
            [-target.ln()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn schedule_practice(
        &self,
        source: &str,
        wordbook_id: Option<i64>,
        limit: u8,
        mode: &str,
    ) -> Result<Vec<ScheduledQuestion>, RepositoryError> {
        use std::hash::{Hash, Hasher};
        let now = now_seconds();
        self.schedule_at(source, wordbook_id, limit, mode, now, |english, chinese| {
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            english.hash(&mut hash);
            chinese.hash(&mut hash);
            now.to_bits().hash(&mut hash);
            hash.finish() & 1 != 0
        })
    }

    // The injected clock and mixed-direction choice keep repository tests deterministic.
    fn schedule_at(
        &self,
        source: &str,
        wordbook_id: Option<i64>,
        limit: u8,
        mode: &str,
        now: f64,
        mixed_direction: impl Fn(&str, &str) -> bool,
    ) -> Result<Vec<ScheduledQuestion>, RepositoryError> {
        if limit == 0 {
            return Err(RepositoryError::Validation("练习数量必须大于零"));
        }
        if !now.is_finite() || now < 0.0 {
            return Err(RepositoryError::Validation("无效的复习时间"));
        }
        if !matches!(mode, "zh-to-en" | "en-to-zh" | "mixed") {
            return Err(RepositoryError::Validation("无效的练习模式"));
        }
        let (table, source_id) = match source {
            "wordbook" if wordbook_id.is_some_and(|id| id > 0) => ("entries", wordbook_id.unwrap()),
            "favorites" if wordbook_id.is_none() => ("favorites", 0),
            "mistakes" if wordbook_id.is_none() => ("mistakes", 0),
            _ => return Err(RepositoryError::Validation("无效的练习来源或单词本")),
        };
        if self.sync_enabled()? {
            return self.schedule_sync_at(source, source_id, limit, mode, now, &mixed_direction);
        }
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "DELETE FROM pending_reviews WHERE created_at < ?1",
            [now - REVIEW_TOKEN_RETENTION_SECONDS],
        )?;
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
        let source_key: String = if source == "wordbook" {
            transaction.query_row("SELECT name FROM wordbooks WHERE id=?1", [source_id], |r| {
                r.get(0)
            })?
        } else {
            String::new()
        };
        // Fixed table names from the validated source, never user-provided SQL.
        let sql = format!(
            "SELECT e.id, e.english, e.chinese, e.normalized_english,
                    c.source IS NOT NULL, zh.due_at, en.due_at, o.rowid
             FROM {table} e
             LEFT JOIN outstanding_reviews o ON o.source=?1 AND o.source_key=?3
               AND o.normalized_english=e.normalized_english AND o.chinese=e.chinese
             LEFT JOIN review_coverage c ON c.source = ?1 AND c.source_id = ?2
               AND c.normalized_english = e.normalized_english AND c.chinese = e.chinese
             LEFT JOIN review_memory zh ON zh.normalized_english = e.normalized_english
               AND zh.chinese = e.chinese AND zh.direction = 'zh-to-en'
             LEFT JOIN review_memory en ON en.normalized_english = e.normalized_english
               AND en.chinese = e.chinese AND en.direction = 'en-to-zh'
             {} ORDER BY e.id",
            if source == "wordbook" {
                "WHERE e.wordbook_id = ?2"
            } else {
                ""
            }
        );
        let mut candidates: Vec<Candidate> = {
            let mut statement = transaction.prepare(&sql)?;
            let rows = statement.query_map(params![source, source_id, source_key], |row| {
                let english: String = row.get(3)?;
                let chinese: String = row.get(2)?;
                let direction = match mode {
                    "mixed" if mixed_direction(&english, &chinese) => "en-to-zh",
                    "en-to-zh" => "en-to-zh",
                    _ => "zh-to-en",
                };
                Ok(Candidate {
                    entry: WordEntry {
                        id: row.get(0)?,
                        english: row.get(1)?,
                        chinese,
                    },
                    normalized: english,
                    covered: row.get(4)?,
                    pending_order: row.get(7)?,
                    due_at: row.get(if direction == "zh-to-en" { 5 } else { 6 })?,
                    direction,
                })
            })?;
            rows.collect::<Result<_, _>>()?
        };
        let selected = select_candidates(std::mem::take(&mut candidates), usize::from(limit), now);

        let mut result = Vec::with_capacity(selected.len());
        for candidate in selected {
            transaction.execute(
                "INSERT INTO pending_reviews(normalized_english, chinese, direction, created_at, source, source_key)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    candidate.normalized,
                    candidate.entry.chinese,
                    candidate.direction,
                    now, source, source_key
                ],
            )?;
            let token_id = transaction.last_insert_rowid();
            transaction.execute("INSERT INTO outstanding_reviews(source,source_key,normalized_english,chinese,token_id)
                VALUES (?1,?2,?3,?4,?5)
                ON CONFLICT(source,source_key,normalized_english,chinese) DO UPDATE SET token_id=excluded.token_id,
                    origin_device=NULL,origin_sequence=NULL,origin_index=NULL",
                params![source, source_key, candidate.normalized, candidate.entry.chinese, token_id])?;
            result.push(ScheduledQuestion {
                review_id: token_id,
                entry: candidate.entry,
                direction: candidate.direction.to_owned(),
            });
        }
        transaction.commit()?;
        Ok(result)
    }
    fn schedule_sync_at(
        &self,
        source: &str,
        source_id: i64,
        limit: u8,
        mode: &str,
        now: f64,
        mixed_direction: &impl Fn(&str, &str) -> bool,
    ) -> Result<Vec<ScheduledQuestion>, RepositoryError> {
        use super::learning::{LearningChange, Selection};
        // `source` was validated by schedule_at before calling this method.
        let table = match source {
            "wordbook" => "entries",
            "favorites" => "favorites",
            "mistakes" => "mistakes",
            _ => unreachable!("validated practice source"),
        };
        let chosen = std::cell::RefCell::new(Vec::<(WordEntry, String)>::new());
        let envelope = self.append_local_build(Some((now * 1000.0) as i64), &super::SyncProjection, |tx| {
            tx.execute("INSERT OR IGNORE INTO sync_retired_reviews SELECT origin_device,origin_sequence,origin_index
                FROM pending_reviews WHERE origin_device IS NOT NULL AND created_at < ?1",
                [now - REVIEW_TOKEN_RETENTION_SECONDS])?;
            tx.execute("DELETE FROM pending_reviews WHERE created_at < ?1", [now - REVIEW_TOKEN_RETENTION_SECONDS])?;
            let book: Option<String> = if source == "wordbook" {
                Some(tx.query_row("SELECT name FROM wordbooks WHERE id=?1", [source_id], |r| r.get(0))
                    .map_err(|_| super::replay::ReplayError::Invalid("单词本不存在"))?)
            } else { None };
            let sql = format!("SELECT e.id,e.english,e.chinese,e.normalized_english,
                c.source IS NOT NULL,zh.due_at,en.due_at,o.rowid FROM {table} e
                LEFT JOIN outstanding_reviews o ON o.source=?1 AND o.source_key=?3
                  AND o.normalized_english=e.normalized_english AND o.chinese=e.chinese
                LEFT JOIN review_coverage c ON c.source=?1 AND c.source_id=?2
                  AND c.normalized_english=e.normalized_english AND c.chinese=e.chinese
                LEFT JOIN review_memory zh ON zh.normalized_english=e.normalized_english
                  AND zh.chinese=e.chinese AND zh.direction='zh-to-en'
                LEFT JOIN review_memory en ON en.normalized_english=e.normalized_english
                  AND en.chinese=e.chinese AND en.direction='en-to-zh'
                {} ORDER BY e.id", if source == "wordbook" { "WHERE e.wordbook_id=?2" } else { "" });
            let mut candidates: Vec<Candidate> = tx.prepare(&sql)?.query_map(params![source,source_id,book.as_deref().unwrap_or("")], |row| {
                let normalized: String = row.get(3)?;
                let chinese: String = row.get(2)?;
                let direction = match mode {
                    "mixed" if mixed_direction(&normalized, &chinese) => "en-to-zh",
                    "en-to-zh" => "en-to-zh",
                    _ => "zh-to-en",
                };
                Ok(Candidate {
                    entry: WordEntry { id: row.get(0)?, english: row.get(1)?, chinese },
                    normalized, covered: row.get(4)?, pending_order: row.get(7)?,
                    due_at: row.get(if direction == "zh-to-en" { 5 } else { 6 })?, direction,
                })
            })?.collect::<Result<_, _>>()?;
            let selected = select_candidates(std::mem::take(&mut candidates), usize::from(limit), now);
            let payload = LearningChange::Schedule { source: source.to_owned(), book,
                selected: selected.iter().map(|item| Selection {
                    english: item.normalized.clone(), chinese: item.entry.chinese.clone(),
                    direction: item.direction.to_owned(),
                }).collect() };
            *chosen.borrow_mut() = selected.into_iter().map(|item| (item.entry, item.direction.to_owned())).collect();
            Ok(payload.encode())
        }).map_err(|error| match error {
            super::replay::ReplayError::Invalid("单词本不存在") => RepositoryError::Validation("单词本不存在"),
            other => RepositoryError::Replay(other),
        })?;
        let connection = self.connect()?;
        let mut result = Vec::new();
        for (index, (entry, direction)) in chosen.into_inner().into_iter().enumerate() {
            let review_id = connection.query_row(
                "SELECT id FROM pending_reviews WHERE origin_device=?1 AND origin_sequence=?2 AND origin_index=?3",
                params![envelope.id.device.0,envelope.id.sequence as i64,index as i64],
                |r| r.get(0))?;
            result.push(ScheduledQuestion {
                review_id,
                entry,
                direction,
            });
        }
        Ok(result)
    }

    pub fn complete_review(
        &self,
        review_id: i64,
        error_count: u32,
        hint_count: u8,
        skipped: bool,
    ) -> Result<(), RepositoryError> {
        self.complete_at(review_id, error_count, hint_count, skipped, now_seconds())
    }

    fn complete_at(
        &self,
        review_id: i64,
        error_count: u32,
        hint_count: u8,
        skipped: bool,
        now: f64,
    ) -> Result<(), RepositoryError> {
        if review_id <= 0 || hint_count > 3 || !now.is_finite() || now < 0.0 {
            return Err(RepositoryError::Validation("无效的复习结果"));
        }
        if self.sync_enabled()? {
            let connection = self.connect()?;
            let origin: Option<PendingReviewOrigin> = connection.query_row(
                "SELECT origin_device,origin_sequence,origin_index,completed FROM pending_reviews WHERE id=?1",
                [review_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
            let Some((device, sequence, index, completed)) = origin else {
                return Err(RepositoryError::Validation("复习记录不存在"));
            };
            if completed {
                return Ok(());
            }
            let (Some(device), Some(sequence), Some(index)) = (device, sequence, index) else {
                return Err(RepositoryError::Validation("复习记录不存在"));
            };
            let change = super::learning::LearningChange::Complete {
                schedule: super::replay::ChangeId {
                    device: super::replay::DeviceId(device),
                    sequence: sequence as u64,
                },
                index: index as usize,
                errors: error_count,
                hints: hint_count,
                skipped,
            };
            match self.append_local_checked(
                change.encode(),
                Some((now * 1000.0) as i64),
                &super::SyncProjection,
                |tx| {
                    let completed: bool = tx.query_row(
                        "SELECT completed FROM pending_reviews WHERE id=?1",
                        [review_id],
                        |r| r.get(0),
                    )?;
                    if completed {
                        return Err(super::replay::ReplayError::BusinessConflict);
                    }
                    Ok(())
                },
            ) {
                Ok(_) | Err(super::replay::ReplayError::BusinessConflict) => {}
                Err(other) => return Err(RepositoryError::Replay(other)),
            }
            return Ok(());
        }
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        let pending: Option<(String, String, String, bool, Option<String>, Option<String>)> = transaction.query_row(
            "SELECT normalized_english, chinese, direction, completed, source, source_key FROM pending_reviews WHERE id = ?1",
            [review_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
        ).optional()?;
        let Some((english, chinese, direction, completed, source, source_key)) = pending else {
            return Err(RepositoryError::Validation("复习记录不存在"));
        };
        if completed {
            return Ok(());
        }
        let previous: Option<(f64, f64)> = transaction
            .query_row(
                "SELECT stability, last_reviewed FROM review_memory
             WHERE normalized_english = ?1 AND chinese = ?2 AND direction = ?3",
                params![english, chinese, direction],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let target: f64 = transaction.query_row(
            "SELECT target_retention FROM review_settings WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        let curve = Curve { target, ..CURVE };
        let stability = previous.map_or(CURVE.initial_stability, |(value, _)| value);
        let effective_now = previous.map_or(now, |(_, last)| now.max(last));
        let updated = curve.updated(stability, error_count, hint_count, skipped);
        transaction.execute(
            "INSERT INTO review_memory(normalized_english, chinese, direction, stability, last_reviewed, due_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(normalized_english, chinese, direction) DO UPDATE SET
                stability = excluded.stability, last_reviewed = excluded.last_reviewed,
                due_at = excluded.due_at",
            params![english, chinese, direction, updated, effective_now, effective_now + curve.interval(updated)],
        )?;
        if let (Some(source), Some(source_key)) = (source, source_key) {
            let source_id: Option<i64> = if source == "wordbook" {
                transaction
                    .query_row(
                        "SELECT id FROM wordbooks WHERE name=?1",
                        [&source_key],
                        |r| r.get(0),
                    )
                    .optional()?
            } else {
                Some(0)
            };
            if let Some(source_id) = source_id {
                let table = match source.as_str() {
                    "wordbook" => "entries",
                    "favorites" => "favorites",
                    "mistakes" => "mistakes",
                    _ => "",
                };
                if !table.is_empty() {
                    let membership = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE normalized_english=?1 AND chinese=?2 {})",
                        if source == "wordbook" { "AND wordbook_id=?3" } else { "AND ?3=0" });
                    let present: bool = transaction.query_row(
                        &membership,
                        params![english, chinese, source_id],
                        |r| r.get(0),
                    )?;
                    if present {
                        transaction.execute(
                            "INSERT OR IGNORE INTO review_coverage VALUES (?1,?2,?3,?4)",
                            params![source, source_id, english, chinese],
                        )?;
                    }
                }
            }
            transaction.execute(
                "DELETE FROM outstanding_reviews WHERE source=?1 AND source_key=?2
                AND normalized_english=?3 AND chinese=?4 AND token_id=?5",
                params![source, source_key, english, chinese, review_id],
            )?;
        }
        transaction.execute(
            "UPDATE pending_reviews SET completed = 1 WHERE id = ?1",
            [review_id],
        )?;
        transaction.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wordbooks::model::ImportedEntry;
    use tempfile::tempdir;

    fn words(count: usize) -> Vec<ImportedEntry> {
        (0..count)
            .map(|n| ImportedEntry {
                english: format!("word-{n}"),
                chinese: format!("释义-{n}"),
            })
            .collect()
    }

    fn memory(
        repo: &WordbookRepository,
        english: &str,
        chinese: &str,
        direction: &str,
    ) -> Option<(f64, f64)> {
        repo.connect().unwrap().query_row(
            "SELECT stability, due_at FROM review_memory WHERE normalized_english = ?1 AND chinese = ?2 AND direction = ?3",
            params![english.to_lowercase(), chinese, direction],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional().unwrap()
    }

    #[test]
    fn due_word_count_filters_membership_time_and_distinct_pairs() {
        let dir = tempdir().unwrap();
        let repo = WordbookRepository::open(dir.path().join("db")).unwrap();
        let first = repo
            .replace(
                "first",
                &[
                    ImportedEntry {
                        english: "Apple".into(),
                        chinese: "苹果".into(),
                    },
                    ImportedEntry {
                        english: "apple".into(),
                        chinese: "水果".into(),
                    },
                    ImportedEntry {
                        english: "unseen".into(),
                        chinese: "未练".into(),
                    },
                ],
                false,
            )
            .unwrap();
        let second = repo
            .replace(
                "second",
                &[
                    ImportedEntry {
                        english: "APPLE".into(),
                        chinese: "苹果".into(),
                    },
                    ImportedEntry {
                        english: "pear".into(),
                        chinese: "梨".into(),
                    },
                ],
                false,
            )
            .unwrap();
        repo.add_favorite("favorite", "收藏").unwrap();
        assert_eq!(repo.due_word_count(None, 200.0).unwrap(), 0);
        let token = repo
            .schedule_at("wordbook", Some(first.id), 1, "zh-to-en", 100.0, |_, _| {
                false
            })
            .unwrap();
        // Scheduling alone does not create a completed review.
        assert_eq!(repo.due_word_count(None, 200.0).unwrap(), 0);
        repo.complete_at(token[0].review_id, 0, 0, false, 100.0)
            .unwrap();
        let connection = repo.connect().unwrap();
        // Fixed due times isolate boundary and direction from the curve coefficients.
        connection.execute("UPDATE review_memory SET due_at = 200 WHERE normalized_english = 'apple' AND chinese = '苹果'", []).unwrap();
        connection
            .execute(
                "INSERT INTO review_memory VALUES ('apple', '苹果', 'en-to-zh', 100, 100, 150)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO review_memory VALUES ('apple', '水果', 'zh-to-en', 100, 100, 180)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO review_memory VALUES ('pear', '梨', 'zh-to-en', 100, 100, 250)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO review_memory VALUES ('favorite', '收藏', 'zh-to-en', 100, 100, 150)",
                [],
            )
            .unwrap();
        assert_eq!(repo.due_word_count(None, 150.0).unwrap(), 0);
        assert_eq!(repo.due_word_count(None, 151.0).unwrap(), 1);
        assert_eq!(repo.due_word_count(None, 201.0).unwrap(), 2);
        assert_eq!(repo.due_word_count(Some(first.id), 201.0).unwrap(), 2);
        assert_eq!(repo.due_word_count(Some(second.id), 201.0).unwrap(), 1);
        assert_eq!(repo.due_word_count(None, 251.0).unwrap(), 3);
        assert_eq!(repo.due_word_count(Some(999), 251.0).unwrap(), 0);
        assert!(matches!(
            repo.due_word_count(Some(0), 200.0),
            Err(RepositoryError::Validation(_))
        ));
        assert!(matches!(
            repo.due_word_count(None, f64::NAN),
            Err(RepositoryError::Validation(_))
        ));
    }

    #[test]
    fn unfinished_local_token_survives_remote_rebuild_without_peer_token_collision() {
        use std::collections::BTreeMap;
        let dir = tempdir().unwrap();
        let a = WordbookRepository::open(dir.path().join("a")).unwrap();
        let b = WordbookRepository::open(dir.path().join("b")).unwrap();
        a.add_favorite("apple", "苹果").unwrap();
        b.add_favorite("pear", "梨").unwrap();
        let token = a
            .schedule_at("favorites", None, 1, "zh-to-en", 100.0, |_, _| false)
            .unwrap()[0]
            .review_id;
        let peer = b
            .schedule_at("favorites", None, 1, "zh-to-en", 90.0, |_, _| false)
            .unwrap()[0]
            .review_id;
        for event in b.changes_since(&BTreeMap::new(), 100).unwrap().iter().rev() {
            a.ingest(event, &super::super::SyncProjection).unwrap();
        }
        assert!(a
            .connect()
            .unwrap()
            .query_row(
                "SELECT completed=0 FROM pending_reviews WHERE id=?1",
                [token],
                |r| r.get::<_, bool>(0)
            )
            .unwrap());
        a.complete_at(token, 0, 0, false, 110.0).unwrap();
        assert!(memory(&a, "apple", "苹果", "zh-to-en").is_some());
        assert!(memory(&a, "pear", "梨", "zh-to-en").is_none());
        // Numeric review IDs on peers are not wire identities.
        assert_eq!(token, peer);
    }
    #[test]
    fn ingress_rejects_foreign_completion_and_non_normalized_schedule() {
        use super::super::learning::{LearningChange, Selection};
        use super::super::replay::{ChangeId, DeviceId, Envelope};
        let dir = tempdir().unwrap();
        let a = WordbookRepository::open(dir.path().join("a")).unwrap();
        a.add_favorite("apple", "苹果").unwrap();
        let token = a
            .schedule_at("favorites", None, 1, "zh-to-en", 100.0, |_, _| false)
            .unwrap()[0]
            .review_id;
        let schedule = a
            .changes_since(&Default::default(), 10)
            .unwrap()
            .pop()
            .unwrap();
        let peer = DeviceId("f".repeat(32));
        let forged = |content: Vec<u8>| Envelope {
            id: ChangeId {
                device: peer.clone(),
                sequence: 1,
            },
            version: 1,
            content,
            dependencies: Default::default(),
            occurred_at: Some(101_000),
        };
        let foreign = forged(
            LearningChange::Complete {
                schedule: schedule.id,
                index: 0,
                errors: 1,
                hints: 0,
                skipped: false,
            }
            .encode(),
        );
        assert!(a.ingest(&foreign, &super::super::SyncProjection).is_err());
        let malformed = forged(
            LearningChange::Schedule {
                source: "favorites".into(),
                book: None,
                selected: vec![Selection {
                    english: "APPLE".into(),
                    chinese: "苹果".into(),
                    direction: "zh-to-en".into(),
                }],
            }
            .encode(),
        );
        assert!(a.ingest(&malformed, &super::super::SyncProjection).is_err());
        a.complete_at(token, 0, 0, false, 102.0).unwrap();
        assert!(memory(&a, "apple", "苹果", "zh-to-en").is_some());
    }

    #[test]
    fn practice_submission_identity_expires_after_thirty_days_but_exam_does_not() {
        use super::super::learning::LearningChange;
        let dir = tempdir().unwrap();
        let repo = WordbookRepository::open(dir.path().join("db")).unwrap();
        for token in ["practice:reused", "exam:fixed"] {
            repo.append_local(
                LearningChange::Mistake {
                    english: "apple".into(),
                    chinese: "苹果".into(),
                    submission: Some(token.into()),
                }
                .encode(),
                Some(1),
                &super::super::SyncProjection,
            )
            .unwrap();
        }
        repo.record_mistake_once("apple", "水果", "practice:reused")
            .unwrap();
        assert!(matches!(
            repo.record_mistake_once("apple", "水果", "exam:fixed"),
            Err(RepositoryError::Conflict)
        ));
        assert_eq!(repo.list_mistakes().unwrap().len(), 2);
        let count: i64 = repo
            .connect()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM replay_changes WHERE applied=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 3);
        // Rebuild from canonical history must retain both distinct practice submissions.
        let other = WordbookRepository::open(dir.path().join("peer")).unwrap();
        for change in repo.changes_since(&Default::default(), 10).unwrap() {
            other
                .ingest(&change, &super::super::SyncProjection)
                .unwrap();
        }
        assert_eq!(
            other.list_mistakes().unwrap(),
            repo.list_mistakes().unwrap()
        );
    }

    #[test]
    fn two_sources_two_meanings_exam_and_skip_converge() {
        use std::collections::BTreeMap;
        let dir = tempdir().unwrap();
        let a = WordbookRepository::open(dir.path().join("a")).unwrap();
        let b = WordbookRepository::open(dir.path().join("b")).unwrap();
        let original = a.replace("book", &[], false).unwrap();
        // Reimporting keeps the content identity while resetting only this source's coverage.
        let book = a
            .replace(
                "book",
                &[
                    ImportedEntry {
                        english: "Apple".into(),
                        chinese: "苹果".into(),
                    },
                    ImportedEntry {
                        english: "Apple".into(),
                        chinese: "水果".into(),
                    },
                ],
                true,
            )
            .unwrap();
        assert_eq!(book.id, original.id);
        b.add_favorite("apple", "苹果").unwrap();
        b.add_favorite("apple", "水果").unwrap();
        let favorite_question = b
            .schedule_at("favorites", None, 1, "en-to-zh", 99.0, |_, _| false)
            .unwrap();
        b.complete_at(favorite_question[0].review_id, 0, 0, false, 99.0)
            .unwrap();
        let questions = a
            .schedule_at("wordbook", Some(book.id), 2, "zh-to-en", 100.0, |_, _| {
                false
            })
            .unwrap();
        let skipped = questions
            .iter()
            .find(|q| q.entry.chinese == "苹果")
            .unwrap();
        a.complete_at(skipped.review_id, 0, 0, true, 101.0).unwrap();
        a.record_mistake_once("Apple", "水果", "exam:blank")
            .unwrap();
        let completed_other = questions
            .iter()
            .find(|q| q.entry.chinese == "水果")
            .unwrap();
        a.complete_at(completed_other.review_id, 1, 0, false, 101.0)
            .unwrap();
        b.record_mistake_once("apple", "苹果", "exam:wrong")
            .unwrap();
        let from_a = a.changes_since(&BTreeMap::new(), 100).unwrap();
        let from_b = b.changes_since(&BTreeMap::new(), 100).unwrap();
        for change in from_b.iter().rev() {
            a.ingest(change, &super::super::SyncProjection).unwrap();
        }
        for change in from_a.iter().rev() {
            b.ingest(change, &super::super::SyncProjection).unwrap();
        }
        let coverage = |repo: &WordbookRepository| -> Vec<(String, i64, String, String)> {
            let connection = repo.connect().unwrap();
            let mut statement = connection.prepare("SELECT source,source_id,normalized_english,chinese FROM review_coverage ORDER BY source,source_id,normalized_english,chinese").unwrap();
            statement
                .query_map([], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(coverage(&a), coverage(&b));
        assert_eq!(coverage(&a).len(), 3);
        assert_eq!(
            coverage(&a)
                .iter()
                .filter(|row| row.0 == "wordbook")
                .count(),
            2
        );
        assert_eq!(
            coverage(&a)
                .iter()
                .filter(|row| row.0 == "favorites")
                .count(),
            1
        );
        assert_eq!(a.list_mistakes().unwrap(), b.list_mistakes().unwrap());
        assert_eq!(a.list_mistakes().unwrap().len(), 2);
        assert!(memory(&a, "apple", "苹果", "zh-to-en").is_some());
        assert_eq!(
            memory(&a, "apple", "苹果", "zh-to-en"),
            memory(&b, "apple", "苹果", "zh-to-en")
        );
        assert!(memory(&a, "apple", "水果", "zh-to-en").is_some());
        assert!(memory(&a, "apple", "苹果", "en-to-zh").is_some());
        assert_eq!(a.list_mistakes().unwrap()[0].error_count, 1);
    }

    #[test]
    fn pending_only_legacy_database_keeps_local_completion_path() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("legacy");
        rusqlite::Connection::open(&path).unwrap().execute_batch(
            "CREATE TABLE pending_reviews (id INTEGER PRIMARY KEY, normalized_english TEXT NOT NULL, chinese TEXT NOT NULL, direction TEXT NOT NULL, completed INTEGER NOT NULL DEFAULT 0);
             INSERT INTO pending_reviews VALUES (42, 'apple', '苹果', 'zh-to-en', 0);",
        ).unwrap();
        let repo = WordbookRepository::open(path).unwrap();
        assert!(!repo.sync_enabled().unwrap());
        repo.complete_at(42, 0, 0, false, 100.0).unwrap();
        assert!(memory(&repo, "apple", "苹果", "zh-to-en").is_some());
    }

    #[test]
    fn learning_exchange_reorders_and_preserves_local_review_tokens() {
        use std::collections::BTreeMap;
        let dir = tempdir().unwrap();
        let a_path = dir.path().join("a.sqlite");
        let b_path = dir.path().join("b.sqlite");
        let a = WordbookRepository::open(&a_path).unwrap();
        let b = WordbookRepository::open(&b_path).unwrap();
        let book = a
            .replace(
                "book",
                &[ImportedEntry {
                    english: "Apple".into(),
                    chinese: "苹果".into(),
                }],
                false,
            )
            .unwrap();
        b.add_favorite("apple", "苹果").unwrap();
        a.record_mistake_once("Apple", "苹果", "same-token")
            .unwrap();
        b.record_mistake_once("apple", "苹果", "same-token")
            .unwrap();
        let a_token = a
            .schedule_at("wordbook", Some(book.id), 1, "zh-to-en", 100.0, |_, _| {
                false
            })
            .unwrap()[0]
            .review_id;
        let b_token = b
            .schedule_at("favorites", None, 1, "en-to-zh", 90.0, |_, _| false)
            .unwrap()[0]
            .review_id;
        a.complete_at(a_token, 0, 0, false, 100.0).unwrap();
        b.complete_at(b_token, 1, 0, false, 90.0).unwrap();
        a.set_review_target(0.8).unwrap();
        b.set_review_target(0.7).unwrap();
        let a_changes = a.changes_since(&BTreeMap::new(), 100).unwrap();
        let b_changes = b.changes_since(&BTreeMap::new(), 100).unwrap();
        for change in b_changes.iter().rev() {
            a.ingest(change, &super::super::SyncProjection).unwrap();
        }
        for change in a_changes.iter().rev() {
            b.ingest(change, &super::super::SyncProjection).unwrap();
        }
        for change in &a_changes {
            b.ingest(change, &super::super::SyncProjection).unwrap();
        }
        for change in &b_changes {
            a.ingest(change, &super::super::SyncProjection).unwrap();
        }
        assert_eq!(a.list_mistakes().unwrap()[0].error_count, 2);
        assert_eq!(b.list_mistakes().unwrap()[0].error_count, 2);
        assert_eq!(a.review_target().unwrap(), b.review_target().unwrap());
        assert_eq!(
            memory(&a, "apple", "苹果", "zh-to-en"),
            memory(&b, "apple", "苹果", "zh-to-en")
        );
        assert_eq!(
            memory(&a, "apple", "苹果", "en-to-zh"),
            memory(&b, "apple", "苹果", "en-to-zh")
        );
        assert_eq!(
            a.record_mistake_once("APPLE", "苹果", "same-token")
                .unwrap()
                .error_count,
            2
        );
        assert!(matches!(
            a.record_mistake_once("Apple", "水果", "same-token"),
            Err(RepositoryError::Conflict)
        ));
        assert_eq!(a.list_mistakes().unwrap()[0].error_count, 2);
        a.complete_at(a_token, 2, 3, true, 200.0).unwrap();
        b.complete_at(b_token, 2, 3, true, 200.0).unwrap();
        assert_eq!(a.list_mistakes().unwrap()[0].error_count, 2);
        let a = WordbookRepository::open(a_path).unwrap();
        let b = WordbookRepository::open(b_path).unwrap();
        assert_eq!(a.review_target().unwrap(), b.review_target().unwrap());
        assert_eq!(
            memory(&a, "apple", "苹果", "zh-to-en"),
            memory(&b, "apple", "苹果", "zh-to-en")
        );
    }

    #[test]
    fn migrates_legacy_review_tokens_without_reusing_retired_ids() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("db");
        let repo = WordbookRepository::open(&path).unwrap();
        repo.connect()
            .unwrap()
            .execute_batch(
                "DROP TABLE pending_reviews;
             CREATE TABLE pending_reviews (
               id INTEGER PRIMARY KEY,
               normalized_english TEXT NOT NULL,
               chinese TEXT NOT NULL,
               direction TEXT NOT NULL,
               completed INTEGER NOT NULL DEFAULT 0
             );
             INSERT INTO pending_reviews VALUES (42, 'apple', '苹果', 'zh-to-en', 0);",
            )
            .unwrap();
        let migrated = WordbookRepository::open(path).unwrap();
        migrated.add_favorite("apple", "苹果").unwrap();
        let selected = migrated
            .schedule_at("favorites", None, 1, "zh-to-en", 3_000_000.0, |_, _| false)
            .unwrap();
        assert!(selected[0].review_id > 42);
        assert!(matches!(
            migrated.complete_at(42, 0, 0, false, 3_000_000.0),
            Err(RepositoryError::Validation(_))
        ));
    }

    #[test]
    fn target_retention_is_adjustable_persistent_and_reschedules_existing_memory() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("db");
        let repo = WordbookRepository::open(&path).unwrap();
        assert_eq!(repo.review_target().unwrap(), 0.9);
        repo.add_favorite("apple", "苹果").unwrap();
        let question = repo
            .schedule_at("favorites", None, 1, "zh-to-en", 100.0, |_, _| false)
            .unwrap();
        repo.complete_at(question[0].review_id, 0, 0, false, 100.0)
            .unwrap();
        let (stability, old_due) = memory(&repo, "apple", "苹果", "zh-to-en").unwrap();
        assert!((old_due - (100.0 - stability * 0.9_f64.ln())).abs() < 0.01);
        assert!(repo.set_review_target(0.8).is_ok());
        let (current_stability, due) = memory(&repo, "apple", "苹果", "zh-to-en").unwrap();
        assert_eq!(current_stability, stability);
        assert!((due - (100.0 - stability * 0.8_f64.ln())).abs() < 0.01);
        for invalid in [0.0, 1.0, f64::NAN, f64::INFINITY] {
            assert!(matches!(
                repo.set_review_target(invalid),
                Err(RepositoryError::Validation(_))
            ));
        }
        assert_eq!(
            WordbookRepository::open(path)
                .unwrap()
                .review_target()
                .unwrap(),
            0.8
        );
    }

    #[test]
    fn abandoned_and_completed_review_tokens_expire_on_future_schedule() {
        let dir = tempdir().unwrap();
        let repo = WordbookRepository::open(dir.path().join("db")).unwrap();
        repo.add_favorite("apple", "苹果").unwrap();
        let first = repo
            .schedule_at("favorites", None, 1, "zh-to-en", 100.0, |_, _| false)
            .unwrap();
        let abandoned = first[0].review_id;
        let second = repo
            .schedule_at("favorites", None, 1, "zh-to-en", 101.0, |_, _| false)
            .unwrap();
        repo.complete_at(second[0].review_id, 0, 0, false, 101.0)
            .unwrap();
        repo.schedule_at(
            "favorites",
            None,
            1,
            "zh-to-en",
            101.0 + REVIEW_TOKEN_RETENTION_SECONDS + 1.0,
            |_, _| false,
        )
        .unwrap();
        let retained: i64 = repo
            .connect()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM pending_reviews", [], |row| row.get(0))
            .unwrap();
        assert_eq!(retained, 1);
        assert!(matches!(
            repo.complete_at(
                abandoned,
                0,
                0,
                false,
                101.0 + REVIEW_TOKEN_RETENTION_SECONDS + 1.0
            ),
            Err(RepositoryError::Validation(_))
        ));
    }

    #[test]
    fn wordbooks_cover_even_with_continuously_due_reviews_and_a_single_slot() {
        let dir = tempdir().unwrap();
        let repo = WordbookRepository::open(dir.path().join("db")).unwrap();
        let book = repo.replace("book", &words(12), false).unwrap();
        let mut seen = std::collections::HashSet::new();
        for round in 0..12 {
            let picked = repo
                .schedule_at(
                    "wordbook",
                    Some(book.id),
                    1,
                    "zh-to-en",
                    100.0 + round as f64 * 1_000_000.0,
                    |_, _| false,
                )
                .unwrap();
            assert!(seen.insert(picked[0].entry.english.clone()));
            repo.complete_at(
                picked[0].review_id,
                0,
                0,
                false,
                100.0 + round as f64 * 1_000_000.0,
            )
            .unwrap();
        }
        assert_eq!(seen.len(), 12);
    }

    #[test]
    fn due_first_with_reserved_unseen_and_bounded_unique_rounds() {
        let dir = tempdir().unwrap();
        let repo = WordbookRepository::open(dir.path().join("db")).unwrap();
        let book = repo.replace("book", &words(8), false).unwrap();
        let first = repo
            .schedule_at("wordbook", Some(book.id), 2, "zh-to-en", 100.0, |_, _| {
                false
            })
            .unwrap();
        for item in &first {
            repo.complete_at(item.review_id, 0, 0, false, 100.0)
                .unwrap();
        }
        let second = repo
            .schedule_at(
                "wordbook",
                Some(book.id),
                3,
                "zh-to-en",
                1_000_000.0,
                |_, _| false,
            )
            .unwrap();
        assert_eq!(second.len(), 3);
        assert!(first.iter().any(|item| item.entry.id == second[0].entry.id));
        assert!(second
            .iter()
            .any(|item| first.iter().all(|prior| prior.entry.id != item.entry.id)));
        assert_eq!(
            second
                .iter()
                .map(|item| item.entry.id)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            3
        );
        assert_eq!(
            repo.schedule_at(
                "wordbook",
                Some(book.id),
                255,
                "zh-to-en",
                1_000_000.0,
                |_, _| false
            )
            .unwrap()
            .len(),
            8
        );
    }

    #[test]
    fn curve_retention_target_feedback_monotonicity_and_recovery() {
        assert_eq!(CURVE.retention(0.0, 100.0), 1.0);
        let interval = CURVE.interval(100.0);
        assert!((CURVE.retention(interval, 100.0) - 0.9).abs() < 1e-12);
        assert!(CURVE.retention(interval + 1.0, 100.0) < 0.9);
        let clean = CURVE.updated(1000.0, 0, 0, false);
        let hint = CURVE.updated(1000.0, 0, 1, false);
        let errors = CURVE.updated(1000.0, 1, 0, false);
        assert!(clean > hint && clean > errors);
        assert!(hint > CURVE.updated(1000.0, 0, 2, false));
        assert!(errors > CURVE.updated(1000.0, 2, 0, false));
        let skipped = CURVE.updated(1000.0, 0, 0, true);
        assert!(skipped < 1000.0 && skipped < errors);
        assert!(CURVE.updated(skipped, 0, 0, false) > skipped);
    }

    #[test]
    fn cross_source_identity_directions_reimport_and_completion_idempotency() {
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
        repo.add_favorite("apple", "苹果").unwrap();
        repo.add_favorite("apple", "水果").unwrap();
        let word = repo
            .schedule_at("wordbook", Some(book.id), 1, "zh-to-en", 100.0, |_, _| {
                false
            })
            .unwrap()
            .remove(0);
        repo.complete_at(word.review_id, 0, 0, false, 100.0)
            .unwrap();
        let state = memory(&repo, "apple", "苹果", "zh-to-en").unwrap();
        repo.complete_at(word.review_id, 9, 3, true, 900.0).unwrap();
        assert_eq!(memory(&repo, "apple", "苹果", "zh-to-en"), Some(state));
        assert!(memory(&repo, "apple", "苹果", "en-to-zh").is_none());
        let favorites = repo
            .schedule_at("favorites", None, 2, "zh-to-en", 101.0, |_, _| false)
            .unwrap();
        assert_eq!(favorites.len(), 2);
        assert!(memory(&repo, "apple", "水果", "zh-to-en").is_none());
        repo.record_mistake("APPLE", "苹果").unwrap();
        repo.record_mistake("apple", "苹果").unwrap();
        let mistake = repo
            .schedule_at("mistakes", None, 1, "en-to-zh", 101.0, |_, _| false)
            .unwrap()
            .remove(0);
        assert_eq!(mistake.direction, "en-to-zh");
        repo.complete_at(mistake.review_id, 0, 0, true, 102.0)
            .unwrap();
        assert_eq!(memory(&repo, "apple", "苹果", "zh-to-en"), Some(state));
        assert!(memory(&repo, "apple", "苹果", "en-to-zh").unwrap().0 < CURVE.initial_stability);
        repo.replace("book", &words(2), true).unwrap();
        drop(repo);
        let reopened = WordbookRepository::open(&path).unwrap();
        assert_eq!(
            reopened
                .schedule_at("favorites", None, 2, "mixed", 103.0, |english, _| english
                    == "apple")
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            reopened
                .schedule_at("mistakes", None, 1, "zh-to-en", 103.0, |_, _| false)
                .unwrap()[0]
                .entry
                .english
                .to_lowercase(),
            "apple"
        );
        assert_eq!(reopened.list_mistakes().unwrap()[0].error_count, 2);
        assert_eq!(memory(&reopened, "apple", "苹果", "zh-to-en"), Some(state));
    }

    #[test]
    fn separate_wordbooks_cover_the_same_pair_independently_and_active_rounds_still_fill() {
        let dir = tempdir().unwrap();
        let repo = WordbookRepository::open(dir.path().join("db")).unwrap();
        let first = repo.replace("first", &words(2), false).unwrap();
        let second = repo.replace("second", &words(2), false).unwrap();
        let picked = repo
            .schedule_at("wordbook", Some(first.id), 2, "zh-to-en", 100.0, |_, _| {
                false
            })
            .unwrap();
        for question in &picked {
            repo.complete_at(question.review_id, 0, 0, false, 100.0)
                .unwrap();
        }
        let other_source = repo
            .schedule_at("wordbook", Some(second.id), 1, "zh-to-en", 101.0, |_, _| {
                false
            })
            .unwrap();
        assert_eq!(other_source.len(), 1);
        let covered: i64 = repo
            .connect()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM review_coverage WHERE source = 'wordbook' AND source_id = ?1",
                [second.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(covered, 0);
        repo.complete_at(other_source[0].review_id, 0, 0, false, 101.0)
            .unwrap();
        assert_eq!(
            repo.connect()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM review_coverage WHERE source='wordbook' AND source_id=?1",
                    [second.id],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        // All words in the first source are already covered but not yet due.
        let active = repo
            .schedule_at(
                "wordbook",
                Some(first.id),
                255,
                "zh-to-en",
                101.0,
                |_, _| false,
            )
            .unwrap();
        assert_eq!(active.len(), 2);
        assert_eq!(
            active
                .iter()
                .map(|q| q.entry.id)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            2
        );
        assert!(repo
            .schedule_at("mistakes", None, 10, "mixed", 101.0, |_, _| false)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn rollback_clock_does_not_move_review_time_backwards() {
        let dir = tempdir().unwrap();
        let repo = WordbookRepository::open(dir.path().join("db")).unwrap();
        repo.add_favorite("a", "甲").unwrap();
        let first = repo
            .schedule_at("favorites", None, 1, "zh-to-en", 1000.0, |_, _| false)
            .unwrap();
        repo.complete_at(first[0].review_id, 0, 0, false, 1000.0)
            .unwrap();
        let second = repo
            .schedule_at("favorites", None, 1, "zh-to-en", 900.0, |_, _| false)
            .unwrap();
        repo.complete_at(second[0].review_id, 0, 0, false, 900.0)
            .unwrap();
        let last: f64 = repo
            .connect()
            .unwrap()
            .query_row(
                "SELECT last_reviewed FROM review_memory WHERE normalized_english = 'a'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(last, 1000.0);
    }

    #[test]
    fn replacement_and_favorite_readdition_reset_only_source_coverage() {
        let dir = tempdir().unwrap();
        let repo = WordbookRepository::open(dir.path().join("db")).unwrap();
        let original = repo.replace("book", &words(2), false).unwrap();
        repo.schedule_at(
            "wordbook",
            Some(original.id),
            1,
            "zh-to-en",
            100.0,
            |_, _| false,
        )
        .unwrap();
        let replacement = repo.replace("book", &words(2), true).unwrap();
        let coverage: i64 = repo
            .connect()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM review_coverage WHERE source = 'wordbook' AND source_id = ?1",
                [replacement.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(coverage, 0);
        let favorite = repo.add_favorite("word-0", "释义-0").unwrap();
        repo.schedule_at("favorites", None, 1, "zh-to-en", 100.0, |_, _| false)
            .unwrap();
        assert!(repo
            .remove_favorite(&favorite.english, &favorite.chinese)
            .unwrap());
        repo.add_favorite("word-0", "释义-0").unwrap();
        let coverage: i64 = repo
            .connect()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM review_coverage WHERE source = 'favorites'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(coverage, 0);
    }

    #[test]
    fn migration_from_legacy_schema_preserves_members_and_can_reopen() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("db");
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE wordbooks(id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
            CREATE TABLE entries(id INTEGER PRIMARY KEY, wordbook_id INTEGER NOT NULL, english TEXT NOT NULL, chinese TEXT NOT NULL, normalized_english TEXT NOT NULL, UNIQUE(wordbook_id, normalized_english, chinese));
            CREATE TABLE favorites(id INTEGER PRIMARY KEY, english TEXT NOT NULL, chinese TEXT NOT NULL, normalized_english TEXT NOT NULL, UNIQUE(normalized_english, chinese));
            CREATE TABLE mistakes(id INTEGER PRIMARY KEY, english TEXT NOT NULL, chinese TEXT NOT NULL, normalized_english TEXT NOT NULL, error_count INTEGER NOT NULL, UNIQUE(normalized_english, chinese));
            INSERT INTO wordbooks(id,name) VALUES (1,'legacy');
            INSERT INTO entries(wordbook_id, english, chinese, normalized_english) VALUES (1,'Apple','苹果','apple');
            INSERT INTO favorites(english,chinese,normalized_english) VALUES ('Apple','苹果','apple');
            INSERT INTO mistakes(english,chinese,normalized_english,error_count) VALUES ('Apple','苹果','apple',3);").unwrap();
        drop(db);
        for _ in 0..2 {
            let repo = WordbookRepository::open(&path).unwrap();
            assert_eq!(repo.list_mistakes().unwrap()[0].error_count, 3);
            assert_eq!(
                repo.schedule_at("wordbook", Some(1), 1, "zh-to-en", 100.0, |_, _| false)
                    .unwrap()
                    .len(),
                1
            );
            assert_eq!(
                repo.schedule_at("favorites", None, 1, "zh-to-en", 100.0, |_, _| false)
                    .unwrap()
                    .len(),
                1
            );
        }
    }

    #[test]
    fn scheduled_question_serializes_with_exact_contract() {
        let question = ScheduledQuestion {
            review_id: 42,
            entry: WordEntry {
                id: 7,
                english: "apple".into(),
                chinese: "苹果".into(),
            },
            direction: "zh-to-en".into(),
        };
        assert_eq!(
            serde_json::to_value(question).unwrap(),
            serde_json::json!({
                "reviewId": 42, "entry": {"id": 7, "english": "apple", "chinese": "苹果"}, "direction": "zh-to-en"
            })
        );
    }

    fn coverage(repo: &WordbookRepository, source: &str, id: i64) -> i64 {
        repo.connect()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM review_coverage WHERE source=?1 AND source_id=?2",
                params![source, id],
                |row| row.get(0),
            )
            .unwrap()
    }

    #[test]
    fn unfinished_round_precedes_due_and_new_even_after_reopen_and_capacity_reduction() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("db");
        let repo = WordbookRepository::open(&path).unwrap();
        let book = repo.replace("book", &words(6), false).unwrap();
        let first = repo
            .schedule_at("wordbook", Some(book.id), 3, "zh-to-en", 100.0, |_, _| {
                false
            })
            .unwrap();
        assert_eq!(
            coverage(&repo, "wordbook", book.id),
            0,
            "selection is not completion"
        );
        assert!(first.iter().all(|q| memory(
            &repo,
            &q.entry.english,
            &q.entry.chinese,
            "zh-to-en"
        )
        .is_none()));
        // The first answer is finished; the other two were abandoned, including an unseen question.
        repo.complete_at(first[0].review_id, 0, 0, false, 100.0)
            .unwrap();
        assert_eq!(coverage(&repo, "wordbook", book.id), 1);
        let repeat = repo
            .schedule_at("wordbook", Some(book.id), 3, "zh-to-en", 101.0, |_, _| {
                false
            })
            .unwrap();
        // An already covered repeat can itself be abandoned and must retain priority.
        assert_eq!(repeat[0].entry.id, first[1].entry.id);
        assert_eq!(repeat[1].entry.id, first[2].entry.id);
        let covered_repeat = repo
            .schedule_at(
                "wordbook",
                Some(book.id),
                6,
                "zh-to-en",
                1_000_000.0,
                |_, _| false,
            )
            .unwrap();
        assert!(covered_repeat
            .iter()
            .any(|q| q.entry.id == first[0].entry.id));
        drop(repo);
        let repo = WordbookRepository::open(&path).unwrap();
        let limited = repo
            .schedule_at(
                "wordbook",
                Some(book.id),
                2,
                "zh-to-en",
                1_000_001.0,
                |_, _| false,
            )
            .unwrap();
        assert_eq!(limited.len(), 2);
        assert_eq!(limited[0].entry.id, first[1].entry.id);
        assert_eq!(limited[1].entry.id, first[2].entry.id);
        assert_eq!(coverage(&repo, "wordbook", book.id), 1);
        let remaining = repo
            .schedule_at(
                "wordbook",
                Some(book.id),
                6,
                "zh-to-en",
                1_000_002.0,
                |_, _| false,
            )
            .unwrap();
        assert_eq!(remaining.len(), 6);
        assert_eq!(
            remaining
                .iter()
                .map(|q| q.entry.id)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            6
        );
        assert_eq!(coverage(&repo, "wordbook", book.id), 1);
        assert!(first[1..].iter().all(|q| memory(
            &repo,
            &q.entry.english,
            &q.entry.chinese,
            "zh-to-en"
        )
        .is_none()));
    }

    #[test]
    fn abandoned_covered_repeat_outranks_new_members_when_capacity_is_small() {
        let dir = tempdir().unwrap();
        let repo = WordbookRepository::open(dir.path().join("db")).unwrap();
        let book = repo.replace("book", &words(4), false).unwrap();
        let original = repo
            .schedule_at("wordbook", Some(book.id), 1, "zh-to-en", 100.0, |_, _| {
                false
            })
            .unwrap();
        repo.complete_at(original[0].review_id, 0, 0, false, 100.0)
            .unwrap();
        let round = repo
            .schedule_at(
                "wordbook",
                Some(book.id),
                4,
                "zh-to-en",
                1_000_000.0,
                |_, _| false,
            )
            .unwrap();
        let covered = round
            .iter()
            .find(|q| q.entry.id == original[0].entry.id)
            .unwrap();
        for question in round
            .iter()
            .filter(|q| q.entry.id != covered.entry.id)
            .take(2)
        {
            repo.complete_at(question.review_id, 0, 0, false, 1_000_000.0)
                .unwrap();
        }
        let unfinished_new = round
            .iter()
            .find(|q| {
                q.entry.id != covered.entry.id
                    && memory(&repo, &q.entry.english, &q.entry.chinese, "zh-to-en").is_none()
            })
            .unwrap();
        assert_eq!(coverage(&repo, "wordbook", book.id), 3);
        let limited = repo
            .schedule_at(
                "wordbook",
                Some(book.id),
                2,
                "zh-to-en",
                1_000_001.0,
                |_, _| false,
            )
            .unwrap();
        assert_eq!(limited.len(), 2);
        assert_eq!(limited.iter().map(|q| q.entry.id).collect::<std::collections::HashSet<_>>(),
            [unfinished_new.entry.id, covered.entry.id].into_iter().collect(),
            "unfinished covered repeat and unfinished unseen both precede non-outstanding candidates");
    }

    #[test]
    fn completion_alone_updates_coverage_memory_and_is_idempotent_per_source() {
        let dir = tempdir().unwrap();
        let repo = WordbookRepository::open(dir.path().join("db")).unwrap();
        let book = repo.replace("book", &words(1), false).unwrap();
        repo.add_favorite("word-0", "释义-0").unwrap();
        repo.record_mistake("word-0", "释义-0").unwrap();
        let word = repo
            .schedule_at("wordbook", Some(book.id), 1, "zh-to-en", 100.0, |_, _| {
                false
            })
            .unwrap();
        let favorite = repo
            .schedule_at("favorites", None, 1, "zh-to-en", 101.0, |_, _| false)
            .unwrap();
        let mistake = repo
            .schedule_at("mistakes", None, 1, "en-to-zh", 102.0, |_, _| false)
            .unwrap();
        for (source, id) in [("wordbook", book.id), ("favorites", 0), ("mistakes", 0)] {
            assert_eq!(
                coverage(&repo, source, id),
                0,
                "{source} scheduled without completion"
            );
        }
        assert!(memory(&repo, "word-0", "释义-0", "zh-to-en").is_none());
        repo.complete_at(word[0].review_id, 1, 0, false, 103.0)
            .unwrap();
        let state = memory(&repo, "word-0", "释义-0", "zh-to-en");
        repo.complete_at(word[0].review_id, 0, 0, false, 200.0)
            .unwrap();
        assert_eq!(memory(&repo, "word-0", "释义-0", "zh-to-en"), state);
        assert_eq!(coverage(&repo, "wordbook", book.id), 1);
        assert_eq!(coverage(&repo, "favorites", 0), 0);
        assert_eq!(coverage(&repo, "mistakes", 0), 0);
        repo.complete_at(mistake[0].review_id, 0, 3, true, 104.0)
            .unwrap();
        assert_eq!(coverage(&repo, "mistakes", 0), 1);
        assert!(memory(&repo, "word-0", "释义-0", "en-to-zh").is_some());
        assert_eq!(repo.list_mistakes().unwrap()[0].error_count, 1);
        repo.complete_at(favorite[0].review_id, 0, 0, false, 105.0)
            .unwrap();
        assert_eq!(coverage(&repo, "favorites", 0), 1);
    }

    #[test]
    fn replay_and_peer_transfer_keep_unfinished_outstanding_without_covering_it() {
        use std::collections::BTreeMap;
        let dir = tempdir().unwrap();
        let a = WordbookRepository::open(dir.path().join("a")).unwrap();
        let b = WordbookRepository::open(dir.path().join("b")).unwrap();
        for word in ["a", "b", "c"] {
            a.add_favorite(word, word).unwrap();
        }
        let selected = a
            .schedule_at("favorites", None, 2, "zh-to-en", 100.0, |_, _| false)
            .unwrap();
        a.complete_at(selected[0].review_id, 0, 0, false, 101.0)
            .unwrap();
        for change in a.changes_since(&BTreeMap::new(), 100).unwrap().iter().rev() {
            b.ingest(change, &super::super::SyncProjection).unwrap();
        }
        assert_eq!(coverage(&a, "favorites", 0), 1);
        assert_eq!(coverage(&b, "favorites", 0), 1);
        assert!(memory(
            &b,
            &selected[1].entry.english,
            &selected[1].entry.chinese,
            "zh-to-en"
        )
        .is_none());
        for repo in [&a, &b] {
            let next = repo
                .schedule_at("favorites", None, 1, "zh-to-en", 102.0, |_, _| false)
                .unwrap();
            assert_eq!(next[0].entry.english, selected[1].entry.english);
            assert_eq!(coverage(repo, "favorites", 0), 1);
        }
    }

    #[test]
    fn peers_share_one_pending_word_and_old_completion_keeps_newer_assignment() {
        use std::collections::BTreeMap;
        let dir = tempdir().unwrap();
        let a = WordbookRepository::open(dir.path().join("a")).unwrap();
        let b = WordbookRepository::open(dir.path().join("b")).unwrap();
        a.add_favorite("apple", "苹果").unwrap();
        for change in a.changes_since(&BTreeMap::new(), 100).unwrap() {
            b.ingest(&change, &super::super::SyncProjection).unwrap();
        }
        let older = a
            .schedule_at("favorites", None, 1, "zh-to-en", 100.0, |_, _| false)
            .unwrap()[0]
            .review_id;
        for change in a.changes_since(&BTreeMap::new(), 100).unwrap() {
            b.ingest(&change, &super::super::SyncProjection).unwrap();
        }
        let newer = b
            .schedule_at("favorites", None, 1, "zh-to-en", 101.0, |_, _| false)
            .unwrap()[0]
            .review_id;
        for change in b.changes_since(&BTreeMap::new(), 100).unwrap().iter().rev() {
            a.ingest(change, &super::super::SyncProjection).unwrap();
        }
        let outstanding = |repo: &WordbookRepository| -> i64 {
            repo.connect().unwrap().query_row(
                "SELECT COUNT(*) FROM outstanding_reviews WHERE source='favorites' AND normalized_english='apple' AND chinese='苹果'",
                [], |row| row.get(0),
            ).unwrap()
        };
        assert_eq!(outstanding(&a), 1);
        assert_eq!(outstanding(&b), 1);
        a.complete_at(older, 1, 0, false, 102.0).unwrap();
        for change in a.changes_since(&BTreeMap::new(), 100).unwrap().iter().rev() {
            b.ingest(change, &super::super::SyncProjection).unwrap();
        }
        for repo in [&a, &b] {
            assert_eq!(coverage(repo, "favorites", 0), 1);
            assert_eq!(
                outstanding(repo),
                1,
                "older completion must not clear newer pending word"
            );
        }
        b.complete_at(newer, 0, 0, false, 103.0).unwrap();
        for change in b.changes_since(&BTreeMap::new(), 100).unwrap().iter().rev() {
            a.ingest(change, &super::super::SyncProjection).unwrap();
        }
        for repo in [&a, &b] {
            assert_eq!(outstanding(repo), 0);
            assert_eq!(coverage(repo, "favorites", 0), 1);
            assert!(memory(repo, "apple", "苹果", "zh-to-en").is_some());
        }
    }

    #[test]
    fn mixed_direction_is_chosen_before_due_ranking_and_invalid_inputs_do_not_write() {
        let dir = tempdir().unwrap();
        let repo = WordbookRepository::open(dir.path().join("db")).unwrap();
        assert!(matches!(
            repo.schedule_practice("favorites", Some(1), 1, "mixed"),
            Err(RepositoryError::Validation(_))
        ));
        assert!(matches!(
            repo.schedule_practice("favorites", None, 0, "mixed"),
            Err(RepositoryError::Validation(_))
        ));
        assert!(matches!(
            repo.schedule_practice("favorites", None, 1, "invalid"),
            Err(RepositoryError::Validation(_))
        ));
        assert!(matches!(
            repo.complete_review(-1, 0, 0, false),
            Err(RepositoryError::Validation(_))
        ));
        assert!(matches!(
            repo.complete_review(1, 0, 4, false),
            Err(RepositoryError::Validation(_))
        ));
        repo.add_favorite("a", "甲").unwrap();
        repo.add_favorite("b", "乙").unwrap();
        let initial = repo
            .schedule_at("favorites", None, 2, "zh-to-en", 100.0, |_, _| false)
            .unwrap();
        for question in initial {
            repo.complete_at(question.review_id, 0, 0, false, 100.0)
                .unwrap();
        }
        let en = repo
            .schedule_at("favorites", None, 2, "en-to-zh", 101.0, |_, _| false)
            .unwrap();
        repo.complete_at(en[0].review_id, 0, 0, false, 101.0)
            .unwrap();
        repo.connect().unwrap().execute("UPDATE review_memory SET due_at = 101 WHERE normalized_english = 'b' AND direction = 'zh-to-en'", []).unwrap();
        // In mixed mode a is not due in en-to-zh, while b is due in zh-to-en.
        let picked = repo
            .schedule_at("favorites", None, 1, "mixed", 102.0, |english, _| {
                english == "a"
            })
            .unwrap();
        assert_eq!(picked[0].entry.english, "b");
        assert_eq!(picked[0].direction, "zh-to-en");
    }
}
