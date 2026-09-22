# Excel Wordbook Import and Selection

**Spec ID:** 0001
**Status:** Implemented
**Date:** 2026-09-22

## Requirements

1. The application must import vocabulary entries from the first worksheet of an `.xlsx` or `.xls` file and persist them in a local SQLite database.
2. The worksheet must contain `english` and `chinese` columns. Fully blank rows are ignored; a populated row missing either value makes the import fail without persisting partial data.
3. Import must trim both values. Rows with the same trimmed Chinese value and case-insensitively equal trimmed English value are duplicates and must collapse to one entry. The same English value paired with a different Chinese value must remain a distinct entry.
4. Each import creates a separately selectable wordbook. The Excel file name supplies an editable default name. Reusing an existing wordbook name requires confirmation before the existing wordbook is atomically replaced.
5. The application must support multiple imported wordbooks and provide a main-screen dropdown for choosing the active wordbook.
6. A practice round must randomly draw up to ten distinct entries from the active wordbook. If it contains fewer than ten entries, the round uses all of them in random order. Existing practice modes, answer checking, hints, progress, and summary behavior continue to operate on that round.
7. When no wordbook has been imported, the main screen must show only the primary import action. Once at least one wordbook exists, the practice setup is shown and a smaller import action remains at the top so another wordbook can be added.
8. Imported wordbooks must remain available after the application restarts.

## Solution

Replace the bundled fixed vocabulary source with a persistence-backed wordbook source. The Tauri backend owns Excel parsing, import validation and deduplication, transactional SQLite writes, wordbook listing, and retrieval of a random practice sample. The React application owns the empty state, import interaction, replacement confirmation, wordbook selector, and the existing practice flow.

An import is committed only after the workbook, required columns, rows, and target name have been validated. If the target name already exists, no replacement occurs until the user confirms it; confirmed replacement and insertion of its entries happen in one transaction.

### Seams

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Native import interaction | React UI ↔ desktop file selection | User selects one `.xlsx` or `.xls` file and can edit its default wordbook name | Selected path/name or cancellation without changing stored data |
| Wordbook commands | React UI ↔ Tauri backend | Import request, optional replacement confirmation, list request, and active wordbook identifier | Structured success results, wordbook metadata, entries/sample, or actionable errors |
| Wordbook persistence | Tauri backend ↔ SQLite | Transactional schema operations for wordbooks and entries | Durable wordbooks, atomic replacement, and random distinct sampling scoped to one wordbook |
| Practice source | Wordbook data ↔ practice domain | Up to ten entries from the selected wordbook | Questions compatible with the existing three practice modes |

## End-to-End Tests

### E2E: First valid import leaves the empty state

- **Given:** The database contains no wordbooks and the main screen shows only the import button.
- **When:** The user selects a valid Excel file, accepts or edits its file-name-derived wordbook name, and imports it.
- **Then:** The wordbook is persisted, the setup screen appears, the wordbook is available in the dropdown, and the smaller top import button is visible.

### E2E: Multiple wordbooks remain independently selectable

- **Given:** One wordbook has already been imported.
- **When:** The user imports a second valid file and selects either wordbook from the dropdown.
- **Then:** Starting practice draws questions only from the selected wordbook.

### E2E: Practice draws at most ten distinct entries

- **Given:** The active wordbook contains more than ten entries.
- **When:** The user starts a practice round.
- **Then:** The round contains ten distinct randomly drawn entries and preserves the selected practice mode.

### E2E: Small wordbook uses every entry

- **Given:** The active wordbook contains fewer than ten entries.
- **When:** The user starts a practice round.
- **Then:** Every entry appears once in random order and progress and summary totals equal that wordbook's entry count.

### E2E: Duplicate rows are collapsed

- **Given:** A workbook contains repeated rows whose trimmed English values differ only by case and whose trimmed Chinese values match.
- **When:** The workbook is imported.
- **Then:** Only one vocabulary entry is stored for those rows, while a row with the same English value and a different Chinese value remains separate.

### E2E: Invalid workbook is rolled back

- **Given:** A workbook is missing a required column or contains a populated row with only one required value.
- **When:** The user attempts to import it.
- **Then:** The UI reports the import problem and no wordbook or partial entries from that attempt are persisted.

### E2E: Same-name replacement requires confirmation

- **Given:** A stored wordbook has the same name as a pending import.
- **When:** The user first cancels replacement and later confirms it.
- **Then:** Cancellation preserves the original wordbook; confirmation atomically replaces its entries with the newly imported entries.

### E2E: Imported wordbooks survive restart

- **Given:** One or more wordbooks were imported successfully.
- **When:** The desktop application is closed and reopened.
- **Then:** The wordbooks are still listed and can be selected for practice.

## Decisions

### Persist imported wordbooks in SQLite

- **Choice:** SQLite is the durable source for wordbooks and entries, and practice no longer depends on the bundled fixed list.
- **Reason:** The feature requires multiple local collections to survive application restarts and be queryable by selected wordbook.
- **ADR reference:** [0001-store-imported-wordbooks-in-sqlite](../adr/0001-store-imported-wordbooks-in-sqlite.md)

### Treat an import as one transaction

- **Choice:** Validation, same-name replacement, and entry insertion either complete together or leave existing data unchanged.
- **Reason:** This preserves the accepted requirement that any import error must not create a partial wordbook.
- **ADR reference:** [0001-store-imported-wordbooks-in-sqlite](../adr/0001-store-imported-wordbooks-in-sqlite.md)

## Test Plan

- **Backend tests:** Parse `.xlsx` and `.xls` fixtures; verify required-column lookup, trimming, blank-row handling, invalid-row failure, pair deduplication, distinct-meaning preservation, transaction rollback, replacement behavior, persistence across reopened database connections, and random sample size/scope.
- **Frontend tests:** Verify the import-only empty state, populated setup state, top import action, editable default name, replacement confirmation, selector changes, loading/error states, and practice startup from backend data.
- **Domain tests:** Verify question generation for lists below, at, and above ten entries and ensure progress/summary totals use the generated round length instead of the old fixed list length.
- **Manual tests:** Import representative `.xlsx` and `.xls` files through the packaged desktop file picker, restart the application, switch between multiple wordbooks, and complete rounds in all three modes.
- **Edge cases:** Cancelled file selection, fully blank rows, empty worksheet, duplicate-only input, Unicode Chinese values, case variants in English, duplicate wordbook names, and database/import failures.

## Out of Scope

- Importing more than the first worksheet.
- Header aliases such as `英文` and `中文`; the required headers are `english` and `chinese`.
- CSV or other non-Excel formats.
- Editing individual entries after import.
- Deleting or renaming an existing wordbook outside the confirmed same-name replacement flow.
- Changing the per-round limit from ten in the UI.

## Future Evolution

- Add configurable column mapping or localized header aliases if users need other workbook layouts.
- Add wordbook management and configurable round size if later requested.
