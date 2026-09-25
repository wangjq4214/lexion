use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordbookSummary {
    pub id: i64,
    pub name: String,
    pub entry_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordEntry {
    pub id: i64,
    pub english: String,
    pub chinese: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MistakeEntry {
    pub id: i64,
    pub english: String,
    pub chinese: String,
    pub error_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExamQuestion {
    pub entry: WordEntry,
    pub direction: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledQuestion {
    pub review_id: i64,
    pub entry: WordEntry,
    pub direction: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub wordbook: WordbookSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedEntry {
    pub english: String,
    pub chinese: String,
}
