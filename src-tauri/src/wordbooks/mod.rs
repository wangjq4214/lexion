mod import;
mod model;
mod repository;

use serde::Serialize;
use tauri::State;

pub use model::{ImportResult, MistakeEntry, ScheduledQuestion, WordEntry, WordbookSummary};
pub use repository::WordbookRepository;

#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "message", rename_all = "camelCase")]
pub enum CommandError {
    Conflict,
    Validation(String),
    Database(String),
}

#[tauri::command]
pub fn list_wordbooks(
    repository: State<'_, WordbookRepository>,
) -> Result<Vec<WordbookSummary>, CommandError> {
    repository
        .list()
        .map_err(|error| CommandError::Database(error.to_string()))
}

#[tauri::command]
pub fn import_wordbook(
    path: String,
    name: String,
    replace_existing: bool,
    repository: State<'_, WordbookRepository>,
) -> Result<ImportResult, CommandError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CommandError::Validation("单词本名称不能为空".to_owned()));
    }

    let entries = import::parse_workbook(path)
        .map_err(|error| CommandError::Validation(error.to_string()))?;
    let wordbook = repository
        .replace(name, &entries, replace_existing)
        .map_err(map_repository_error)?;
    Ok(ImportResult { wordbook })
}

fn map_repository_error(error: repository::RepositoryError) -> CommandError {
    match error {
        repository::RepositoryError::Conflict => CommandError::Conflict,
        repository::RepositoryError::Validation(message) => {
            CommandError::Validation(message.to_owned())
        }
        repository::RepositoryError::Database(error) => CommandError::Database(error.to_string()),
    }
}

#[tauri::command]
pub fn delete_wordbook_entry(
    wordbook_id: i64,
    entry_id: i64,
    repository: State<'_, WordbookRepository>,
) -> Result<bool, CommandError> {
    repository
        .delete_wordbook_entry(wordbook_id, entry_id)
        .map_err(map_repository_error)
}

#[tauri::command]
pub fn add_favorite(
    english: String,
    chinese: String,
    repository: State<'_, WordbookRepository>,
) -> Result<WordEntry, CommandError> {
    repository
        .add_favorite(&english, &chinese)
        .map_err(map_repository_error)
}

#[tauri::command]
pub fn remove_favorite(
    english: String,
    chinese: String,
    repository: State<'_, WordbookRepository>,
) -> Result<bool, CommandError> {
    repository
        .remove_favorite(&english, &chinese)
        .map_err(map_repository_error)
}

#[tauri::command]
pub fn is_favorite(
    english: String,
    chinese: String,
    repository: State<'_, WordbookRepository>,
) -> Result<bool, CommandError> {
    repository
        .is_favorite(&english, &chinese)
        .map_err(map_repository_error)
}

#[tauri::command]
pub fn list_favorites(
    repository: State<'_, WordbookRepository>,
) -> Result<Vec<WordEntry>, CommandError> {
    repository
        .list_favorites()
        .map_err(|error| CommandError::Database(error.to_string()))
}

#[tauri::command]
pub fn sample_favorites(
    limit: u8,
    repository: State<'_, WordbookRepository>,
) -> Result<Vec<WordEntry>, CommandError> {
    repository
        .sample_favorites(limit)
        .map_err(map_repository_error)
}

#[tauri::command]
pub fn record_mistake(
    english: String,
    chinese: String,
    repository: State<'_, WordbookRepository>,
) -> Result<MistakeEntry, CommandError> {
    repository
        .record_mistake(&english, &chinese)
        .map_err(map_repository_error)
}

#[tauri::command]
pub fn record_mistake_once(
    english: String,
    chinese: String,
    submission_id: String,
    repository: State<'_, WordbookRepository>,
) -> Result<MistakeEntry, CommandError> {
    repository
        .record_mistake_once(&english, &chinese, &submission_id)
        .map_err(map_repository_error)
}

#[tauri::command]
pub fn list_mistakes(
    repository: State<'_, WordbookRepository>,
) -> Result<Vec<MistakeEntry>, CommandError> {
    repository
        .list_mistakes()
        .map_err(|error| CommandError::Database(error.to_string()))
}

#[tauri::command]
pub fn sample_mistakes(
    limit: u8,
    repository: State<'_, WordbookRepository>,
) -> Result<Vec<WordEntry>, CommandError> {
    repository
        .sample_mistakes(limit)
        .map_err(map_repository_error)
}

#[tauri::command]
pub fn review_target(repository: State<'_, WordbookRepository>) -> Result<f64, CommandError> {
    repository.review_target().map_err(map_repository_error)
}

#[tauri::command]
pub fn set_review_target(
    target: f64,
    repository: State<'_, WordbookRepository>,
) -> Result<(), CommandError> {
    repository
        .set_review_target(target)
        .map_err(map_repository_error)
}

#[tauri::command]
pub fn schedule_practice(
    source: String,
    wordbook_id: Option<i64>,
    limit: u8,
    mode: String,
    repository: State<'_, WordbookRepository>,
) -> Result<Vec<ScheduledQuestion>, CommandError> {
    repository
        .schedule_practice(&source, wordbook_id, limit, &mode)
        .map_err(map_repository_error)
}

#[tauri::command]
pub fn complete_review(
    review_id: i64,
    error_count: u32,
    hint_count: u8,
    skipped: bool,
    repository: State<'_, WordbookRepository>,
) -> Result<(), CommandError> {
    repository
        .complete_review(review_id, error_count, hint_count, skipped)
        .map_err(map_repository_error)
}

#[tauri::command]
pub fn sample_wordbook(
    wordbook_id: i64,
    limit: u8,
    repository: State<'_, WordbookRepository>,
) -> Result<Vec<WordEntry>, CommandError> {
    repository
        .sample(wordbook_id, limit)
        .map_err(|error| CommandError::Database(error.to_string()))
}
