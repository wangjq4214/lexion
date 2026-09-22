use std::{collections::HashSet, path::Path};

use calamine::{open_workbook_auto, Data, Reader};

use super::model::ImportedEntry;

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("只支持 .xlsx 或 .xls 文件")]
    UnsupportedFormat,
    #[error("无法读取 Excel 文件：{0}")]
    Open(String),
    #[error("工作簿不包含工作表")]
    MissingWorksheet,
    #[error("无法读取第一个工作表：{0}")]
    ReadWorksheet(String),
    #[error("第一张工作表必须包含 english 和 chinese 列")]
    MissingColumns,
    #[error("第 {0} 行的 english 和 chinese 都必须填写")]
    IncompleteRow(usize),
    #[error("第一张工作表没有可导入的词条")]
    Empty,
}

pub fn parse_workbook(path: impl AsRef<Path>) -> Result<Vec<ImportedEntry>, ImportError> {
    let path = path.as_ref();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_lowercase);
    if !matches!(extension.as_deref(), Some("xlsx" | "xls")) {
        return Err(ImportError::UnsupportedFormat);
    }
    let mut workbook =
        open_workbook_auto(path).map_err(|error| ImportError::Open(error.to_string()))?;
    let first_sheet = workbook
        .sheet_names()
        .first()
        .cloned()
        .ok_or(ImportError::MissingWorksheet)?;
    let range = workbook
        .worksheet_range(&first_sheet)
        .map_err(|error| ImportError::ReadWorksheet(error.to_string()))?;
    parse_rows(range.rows())
}

fn parse_rows<'a>(
    rows: impl Iterator<Item = &'a [Data]>,
) -> Result<Vec<ImportedEntry>, ImportError> {
    let mut rows = rows;
    let headers = rows.next().ok_or(ImportError::MissingColumns)?;
    let english_index = headers
        .iter()
        .position(|cell| matches!(cell, Data::String(value) if value == "english"));
    let chinese_index = headers
        .iter()
        .position(|cell| matches!(cell, Data::String(value) if value == "chinese"));
    let (Some(english_index), Some(chinese_index)) = (english_index, chinese_index) else {
        return Err(ImportError::MissingColumns);
    };

    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    for (index, row) in rows.enumerate() {
        let english = row.get(english_index).map(cell_text).unwrap_or_default();
        let chinese = row.get(chinese_index).map(cell_text).unwrap_or_default();
        if row.iter().all(|cell| cell_text(cell).is_empty()) {
            continue;
        }
        if english.is_empty() || chinese.is_empty() {
            return Err(ImportError::IncompleteRow(index + 2));
        }
        let key = (english.to_lowercase(), chinese.clone());
        if seen.insert(key) {
            entries.push(ImportedEntry { english, chinese });
        }
    }

    if entries.is_empty() {
        return Err(ImportError::Empty);
    }
    Ok(entries)
}

fn cell_text(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        _ => cell.to_string().trim().to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(values: &[&str]) -> Vec<Data> {
        values
            .iter()
            .map(|value| Data::String((*value).to_owned()))
            .collect()
    }

    #[test]
    fn trims_deduplicates_pairs_and_preserves_distinct_meanings() {
        let rows = [
            row(&["english", "chinese"]),
            row(&[" Apple ", " 苹果 "]),
            row(&["apple", "苹果"]),
            row(&["APPLE", "水果"]),
            row(&["", ""]),
        ];
        let entries = parse_rows(rows.iter().map(Vec::as_slice)).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].english, "Apple");
        assert_eq!(entries[0].chinese, "苹果");
        assert_eq!(entries[1].chinese, "水果");
    }

    #[test]
    fn rejects_missing_columns_incomplete_rows_and_empty_data() {
        let missing = [row(&["word", "chinese"]), row(&["apple", "苹果"])];
        assert!(matches!(
            parse_rows(missing.iter().map(Vec::as_slice)),
            Err(ImportError::MissingColumns)
        ));

        let incomplete = [row(&["english", "chinese"]), row(&["apple", ""])];
        assert!(matches!(
            parse_rows(incomplete.iter().map(Vec::as_slice)),
            Err(ImportError::IncompleteRow(2))
        ));

        let unrelated_value = [
            row(&["english", "chinese", "notes"]),
            row(&["", "", "review"]),
        ];
        assert!(matches!(
            parse_rows(unrelated_value.iter().map(Vec::as_slice)),
            Err(ImportError::IncompleteRow(2))
        ));

        let empty = [row(&["english", "chinese"]), row(&["", ""])];
        assert!(matches!(
            parse_rows(empty.iter().map(Vec::as_slice)),
            Err(ImportError::Empty)
        ));
    }

    #[test]
    fn parses_real_xlsx_and_xls_files_from_the_first_sheet() {
        for file_name in ["valid.xlsx", "valid.xls"] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("fixtures")
                .join(file_name);
            let entries = parse_workbook(path).unwrap();
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].english, "Apple");
            assert_eq!(entries[0].chinese, "苹果");
            assert_eq!(entries[1].chinese, "水果");
        }
    }

    #[test]
    fn rejects_non_excel_extensions() {
        assert!(matches!(
            parse_workbook("words.csv"),
            Err(ImportError::UnsupportedFormat)
        ));
    }
}
