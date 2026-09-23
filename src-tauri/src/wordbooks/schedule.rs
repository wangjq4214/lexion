use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, OptionalExtension};

use super::{RepositoryError, WordbookRepository};
use crate::wordbooks::model::{ScheduledQuestion, WordEntry};

// Calibration knobs: seconds throughout. These are predictions, not measured recall probabilities.
// A review can be retried after an ambiguous response, but abandoned round tokens need not live forever.
const REVIEW_TOKEN_RETENTION_SECONDS: f64 = 30.0 * 24.0 * 60.0 * 60.0;
#[derive(Clone, Copy)]
struct Curve {
    target: f64,
    initial_stability: f64,
    success_growth: f64,
    error_weight: f64,
    hint_weight: f64,
    skip_factor: f64,
    minimum_stability: f64,
    maximum_stability: f64,
}

const CURVE: Curve = Curve {
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
    fn interval(self, stability: f64) -> f64 {
        -stability * self.target.ln()
    }

    #[cfg(test)]
    fn retention(self, elapsed: f64, stability: f64) -> f64 {
        (-elapsed.max(0.0) / stability).exp()
    }

    fn updated(self, previous: f64, errors: u32, hints: u8, skipped: bool) -> f64 {
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
    due_at: Option<f64>,
    direction: &'static str,
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

    pub fn set_review_target(&self, target: f64) -> Result<(), RepositoryError> {
        if !target.is_finite() || !(0.0..1.0).contains(&target) || target == 0.0 {
            return Err(RepositoryError::Validation("目标保持率必须在 0 和 1 之间"));
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
        // Fixed table names from the validated source, never user-provided SQL.
        let sql = format!(
            "SELECT e.id, e.english, e.chinese, e.normalized_english,
                    c.source IS NOT NULL, zh.due_at, en.due_at
             FROM {table} e
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
            let rows = statement.query_map(params![source, source_id], |row| {
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
                    due_at: row.get(if direction == "zh-to-en" { 5 } else { 6 })?,
                    direction,
                })
            })?;
            rows.collect::<Result<_, _>>()?
        };
        let mut unseen = Vec::new();
        let mut due = Vec::new();
        let mut other = Vec::new();
        for candidate in candidates.drain(..) {
            if !candidate.covered {
                unseen.push(candidate);
            } else if candidate.due_at.is_some_and(|due_at| due_at <= now) {
                due.push(candidate);
            } else {
                other.push(candidate);
            }
        }
        due.sort_by(|a, b| {
            a.due_at
                .unwrap()
                .total_cmp(&b.due_at.unwrap())
                .then(a.entry.id.cmp(&b.entry.id))
        });
        let capacity = usize::from(limit);
        let reserve = usize::from(!unseen.is_empty());
        // With one slot, first exposure takes precedence; otherwise due gets all but one slot.
        let mut selected = Vec::with_capacity(capacity);
        selected.extend(due.drain(..due.len().min(capacity - reserve)));
        selected.extend(unseen.drain(..unseen.len().min(capacity - selected.len())));
        selected.extend(due.drain(..due.len().min(capacity - selected.len())));
        selected.extend(other.drain(..other.len().min(capacity - selected.len())));

        let mut result = Vec::with_capacity(selected.len());
        for candidate in selected {
            transaction.execute(
                "INSERT OR IGNORE INTO review_coverage(source, source_id, normalized_english, chinese)
                 VALUES (?1, ?2, ?3, ?4)",
                params![source, source_id, candidate.normalized, candidate.entry.chinese],
            )?;
            transaction.execute(
                "INSERT INTO pending_reviews(normalized_english, chinese, direction, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    candidate.normalized,
                    candidate.entry.chinese,
                    candidate.direction,
                    now
                ],
            )?;
            result.push(ScheduledQuestion {
                review_id: transaction.last_insert_rowid(),
                entry: candidate.entry,
                direction: candidate.direction.to_owned(),
            });
        }
        transaction.commit()?;
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
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        let pending: Option<(String, String, String, bool)> = transaction.query_row(
            "SELECT normalized_english, chinese, direction, completed FROM pending_reviews WHERE id = ?1",
            [review_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        ).optional()?;
        let Some((english, chinese, direction, completed)) = pending else {
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
        assert_eq!(covered, 1);
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
