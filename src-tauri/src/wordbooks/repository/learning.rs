//! Durable learning events, keyed by originating change rather than local row IDs.
use rusqlite::{params, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use super::replay::{ChangeId, Envelope, Projection, ReplayError};
use super::schedule::{Curve, CURVE};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Selection {
    pub english: String,
    pub chinese: String,
    pub direction: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum LearningChange {
    Mistake {
        english: String,
        chinese: String,
        submission: Option<String>,
    },
    Schedule {
        source: String,
        book: Option<String>,
        selected: Vec<Selection>,
    },
    Complete {
        schedule: ChangeId,
        index: usize,
        errors: u32,
        hints: u8,
        skipped: bool,
    },
    Target {
        value: f64,
    },
}

impl LearningChange {
    pub(super) fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("learning events serialize")
    }

    pub(super) fn decode(change: &Envelope) -> Result<Self, ReplayError> {
        if change.version != 1 {
            return Err(ReplayError::Invalid("unsupported learning version"));
        }
        let value: Self = serde_json::from_slice(&change.content)
            .map_err(|_| ReplayError::Invalid("malformed learning event"))?;
        match &value {
            Self::Mistake {
                english,
                chinese,
                submission,
            } => {
                pair(english, chinese)?;
                if submission.as_ref().is_some_and(|s| s.trim().is_empty()) {
                    return Err(ReplayError::Invalid("empty submission"));
                }
            }
            Self::Schedule {
                source,
                book,
                selected,
            } => {
                if !matches!((source.as_str(), book), ("wordbook", Some(name)) if !name.is_empty())
                    && !matches!((source.as_str(), book), ("favorites" | "mistakes", None))
                {
                    return Err(ReplayError::Invalid("invalid learning source"));
                }
                for item in selected {
                    pair(&item.english, &item.chinese)?;
                    if item.english.to_lowercase() != item.english {
                        return Err(ReplayError::Invalid("schedule pair must be normalized"));
                    }
                    if !matches!(item.direction.as_str(), "zh-to-en" | "en-to-zh") {
                        return Err(ReplayError::Invalid("invalid direction"));
                    }
                }
            }
            Self::Complete {
                schedule,
                index,
                hints,
                ..
            } => {
                if schedule.device != change.id.device {
                    return Err(ReplayError::Invalid(
                        "completion must originate with its schedule",
                    ));
                }
                if schedule.sequence == 0
                    || schedule.sequence > i64::MAX as u64
                    || schedule.device.0.len() != 32
                    || !schedule
                        .device
                        .0
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                    || *index > i64::MAX as usize
                    || *hints > 3
                {
                    return Err(ReplayError::Invalid("invalid completion"));
                }
            }
            Self::Target { value } if !value.is_finite() || *value <= 0.0 || *value >= 1.0 => {
                return Err(ReplayError::Invalid("invalid target"));
            }
            _ => {}
        }
        if (matches!(value, Self::Schedule { .. } | Self::Complete { .. })
            || matches!(
                value,
                Self::Mistake {
                    submission: Some(_),
                    ..
                }
            ))
            && change.occurred_at.is_none_or(|time| time < 0)
        {
            return Err(ReplayError::Invalid("missing learning occurrence time"));
        }
        Ok(value)
    }
}

fn pair(english: &str, chinese: &str) -> Result<(), ReplayError> {
    if english.is_empty()
        || chinese.is_empty()
        || english.trim() != english
        || chinese.trim() != chinese
    {
        return Err(ReplayError::Invalid("invalid learning pair"));
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub(super) struct LearningProjection;

impl Projection for LearningProjection {
    fn validate(&self, change: &Envelope) -> Result<(), ReplayError> {
        LearningChange::decode(change).map(|_| ())
    }

    fn reset(&self, tx: &Transaction<'_>) -> Result<(), ReplayError> {
        tx.execute_batch(
            "DELETE FROM sync_submissions; DELETE FROM sync_completions;
            DELETE FROM mistakes; DELETE FROM mistake_submissions;
            DELETE FROM review_coverage; DELETE FROM review_memory;
            UPDATE review_settings SET target_retention=0.9 WHERE id=1;
            UPDATE pending_reviews SET completed=0 WHERE origin_device IS NOT NULL;",
        )?;
        Ok(())
    }

    fn apply(&self, tx: &Transaction<'_>, change: &Envelope) -> Result<(), ReplayError> {
        match LearningChange::decode(change)? {
            LearningChange::Mistake {
                english,
                chinese,
                submission,
            } => {
                let normalized = english.to_lowercase();
                if let Some(token) = submission {
                    let occurred_at = change
                        .occurred_at
                        .ok_or(ReplayError::Invalid("missing mistake occurrence time"))?;
                    let existing: Option<(String, String, i64)> = tx.query_row(
                        "SELECT normalized_english,chinese,created_at FROM sync_submissions WHERE device=?1 AND token=?2",
                        params![change.id.device.0, token],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    ).optional()?;
                    if let Some((old_english, old_chinese, old_time)) = existing {
                        if token.starts_with("exam:")
                            || occurred_at.saturating_sub(old_time) <= 2_592_000
                        {
                            if (old_english, old_chinese) != (normalized, chinese) {
                                return Err(ReplayError::BusinessConflict);
                            }
                            return Ok(());
                        }
                    }
                    tx.execute(
                        "INSERT INTO sync_submissions(device,token,normalized_english,chinese,created_at) VALUES (?1,?2,?3,?4,?5)
                         ON CONFLICT(device,token) DO UPDATE SET normalized_english=excluded.normalized_english,
                         chinese=excluded.chinese,created_at=excluded.created_at",
                        params![change.id.device.0, token, normalized, chinese, occurred_at],
                    )?;
                    if change.id.device.0 == local_device(tx)? {
                        tx.execute("INSERT INTO mistake_submissions(submission_id,normalized_english,chinese,created_at) VALUES (?1,?2,?3,?4)
                            ON CONFLICT(submission_id) DO UPDATE SET normalized_english=excluded.normalized_english,
                            chinese=excluded.chinese,created_at=excluded.created_at",
                            params![token, normalized, chinese, occurred_at])?;
                    }
                }
                tx.execute("INSERT INTO mistakes(english,chinese,normalized_english,error_count) VALUES (?1,?2,?3,1)
                    ON CONFLICT(normalized_english,chinese) DO UPDATE SET error_count=error_count+1",
                    params![english,chinese,normalized])?;
            }
            LearningChange::Schedule {
                source,
                book,
                selected,
            } => {
                let source_id = if let Some(book) = book {
                    tx.query_row("SELECT id FROM sync_book_ids WHERE name=?1", [book], |r| {
                        r.get::<_, i64>(0)
                    })
                    .optional()?
                    .unwrap_or(-1)
                } else {
                    0
                };
                // A concurrent content deletion may precede this schedule in canonical order.
                // Only coverage requires present membership; the original learning outcome
                // remains valid for its normalized pair even if the source is now gone.
                for (index, item) in selected.iter().enumerate() {
                    // Coverage belongs only to a currently present source.
                    let present: bool = match source.as_str() {
                        "wordbook" => tx.query_row("SELECT EXISTS(SELECT 1 FROM entries WHERE wordbook_id=?1 AND normalized_english=?2 AND chinese=?3)",
                            params![source_id,item.english,item.chinese], |r| r.get(0))?,
                        "favorites" => tx.query_row("SELECT EXISTS(SELECT 1 FROM favorites WHERE normalized_english=?1 AND chinese=?2)",
                            params![item.english,item.chinese], |r| r.get(0))?,
                        _ => tx.query_row("SELECT EXISTS(SELECT 1 FROM mistakes WHERE normalized_english=?1 AND chinese=?2)",
                            params![item.english,item.chinese], |r| r.get(0))?,
                    };
                    if present {
                        tx.execute(
                            "INSERT OR IGNORE INTO review_coverage VALUES (?1,?2,?3,?4)",
                            params![source, source_id, item.english, item.chinese],
                        )?;
                    }
                    // Remote tokens are never exported as local review IDs.
                    if change.id.device.0 == local_device(tx)? && !tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM sync_retired_reviews WHERE device=?1 AND sequence=?2 AND item_index=?3)",
                        params![change.id.device.0,change.id.sequence as i64,index as i64], |r| r.get::<_, bool>(0))? {
                        tx.execute("INSERT OR IGNORE INTO pending_reviews(normalized_english,chinese,direction,created_at,origin_device,origin_sequence,origin_index)
                            VALUES (?1,?2,?3,?4,?5,?6,?7)",
                            params![item.english,item.chinese,item.direction,change.occurred_at.unwrap() as f64 / 1000.0,
                                change.id.device.0, change.id.sequence as i64,index as i64])?;
                    }
                }
            }
            LearningChange::Complete {
                schedule,
                index,
                errors,
                hints,
                skipped,
            } => {
                let parent: Option<Envelope> = tx.query_row("SELECT content,occurred_at FROM replay_changes WHERE device=?1 AND sequence=?2",
                    params![schedule.device.0,schedule.sequence as i64], |r| Ok(Envelope {
                        id: schedule.clone(), version: 1, content: r.get(0)?, occurred_at: r.get(1)?, dependencies: Default::default()
                    })).optional()?;
                let Some(parent) = parent else {
                    return Err(ReplayError::BusinessConflict);
                };
                let LearningChange::Schedule { selected, .. } = LearningChange::decode(&parent)?
                else {
                    return Err(ReplayError::BusinessConflict);
                };
                let Some(item) = selected.get(index) else {
                    return Err(ReplayError::BusinessConflict);
                };
                if tx.execute(
                    "INSERT OR IGNORE INTO sync_completions VALUES (?1,?2,?3)",
                    params![schedule.device.0, schedule.sequence as i64, index as i64],
                )? == 0
                {
                    return Ok(());
                }
                let previous: Option<(f64,f64)> = tx.query_row("SELECT stability,last_reviewed FROM review_memory WHERE normalized_english=?1 AND chinese=?2 AND direction=?3",
                    params![item.english,item.chinese,item.direction], |r| Ok((r.get(0)?,r.get(1)?))).optional()?;
                let target: f64 = tx.query_row(
                    "SELECT target_retention FROM review_settings WHERE id=1",
                    [],
                    |r| r.get(0),
                )?;
                let curve = Curve { target, ..CURVE };
                let stability = curve.updated(
                    previous.map_or(CURVE.initial_stability, |p| p.0),
                    errors,
                    hints,
                    skipped,
                );
                let now = (change.occurred_at.unwrap() as f64 / 1000.0)
                    .max(previous.map_or(0.0, |p| p.1));
                tx.execute("INSERT INTO review_memory VALUES (?1,?2,?3,?4,?5,?6)
                    ON CONFLICT(normalized_english,chinese,direction) DO UPDATE SET stability=excluded.stability,last_reviewed=excluded.last_reviewed,due_at=excluded.due_at",
                    params![item.english,item.chinese,item.direction,stability,now,now+curve.interval(stability)])?;
                tx.execute("UPDATE pending_reviews SET completed=1 WHERE origin_device=?1 AND origin_sequence=?2 AND origin_index=?3",
                    params![schedule.device.0,schedule.sequence as i64,index as i64])?;
            }
            LearningChange::Target { value } => {
                tx.execute(
                    "UPDATE review_settings SET target_retention=?1 WHERE id=1",
                    [value],
                )?;
                tx.execute(
                    "UPDATE review_memory SET due_at=last_reviewed+stability*?1",
                    [-value.ln()],
                )?;
            }
        }
        Ok(())
    }
}

fn local_device(tx: &Transaction<'_>) -> Result<String, ReplayError> {
    Ok(tx.query_row(
        "SELECT device FROM replay_identity WHERE singleton=1",
        [],
        |r| r.get(0),
    )?)
}
