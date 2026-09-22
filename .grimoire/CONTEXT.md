# Project Context

## Concepts

### wordbook
- **Definition:** A named, independently selectable collection of vocabulary entries imported from an Excel file. The application supports multiple wordbooks.
- **Relationships:**
  - contains vocabulary-entry

### vocabulary-entry
- **Definition:** A paired English word and Chinese meaning used to generate spelling practice questions. Entries are imported from the first worksheet of an Excel file; duplicates within one import are removed.
- **Relationships:**
  - belongs to wordbook

### active-wordbook
- **Definition:** The wordbook selected on the main screen as the source from which the application randomly draws up to ten vocabulary entries for the next practice round.
- **Relationships:**
  - references wordbook

### practice-round
- **Definition:** One complete vocabulary practice session created after the user starts practice, covering all sampled questions. Its elapsed time starts when the first question is shown, accumulates across every question including retries and hint use, stops when the final question is answered correctly, and is displayed during practice and in the completion summary in minutes and seconds.
- **Relationships:**
  - depends on active-wordbook
  - contains vocabulary-entry

### wordbook-import
- **Definition:** An atomic operation that reads `english` and `chinese` columns from the first worksheet of an `.xlsx` or `.xls` file, defaults the wordbook name from the file name while allowing edits, and persists the resulting wordbook in SQLite. Duplicate pairs are detected after trimming both values and comparing English case-insensitively; the same English word with a different Chinese meaning remains a separate entry. A duplicate wordbook name requires confirmation before replacement.
- **Relationships:**
  - implements wordbook
  - contains vocabulary-entry
