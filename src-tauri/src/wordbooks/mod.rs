mod import;
mod model;
mod repository;

use serde::Serialize;
use tauri::State;

pub use model::{ImportResult, WordEntry, WordbookSummary};
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
        .map_err(|error| match error {
            repository::RepositoryError::Conflict => CommandError::Conflict,
            repository::RepositoryError::Database(error) => {
                CommandError::Database(error.to_string())
            }
        })?;
    Ok(ImportResult { wordbook })
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
