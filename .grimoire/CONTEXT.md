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
- **Definition:** The wordbook selected on the main screen as the source from which the application randomly draws entries for the next practice round, up to the learner's selected practice count and the number of available entries. The default selected count is ten.
- **Relationships:**
  - references wordbook

### practice-round
- **Definition:** One complete vocabulary practice session created after the user starts practice, covering all sampled questions. A question is completed by answering correctly or skipping it. In Chinese-to-English questions, hints have three sequential levels: the first reveals about one third of the English letters, the second about two thirds while retaining earlier reveals, and the third reveals the full word. Each level counts as one hint use; the third is equivalent to skipping: it reveals the English word and Chinese meaning, counts as skipped rather than correct, and advances only after the learner has seen the answer. Direct skipping has the same reveal and continuation behavior. Each incorrect answer submission increments the error count, including repeated incorrect submissions for one question; skipping without an incorrect submission does not increment it. Elapsed time starts when the first question is shown, includes retries, hints, and skips, and stops when the last question is completed. The completion summary displays minutes and seconds, correct answers, error count, hint count, and skip count, but does not list skipped words.
- **Relationships:**
  - depends on practice-source
  - contains vocabulary-entry
  - contains favorite-entry
  - contains mistake-entry

### wordbook-import
- **Definition:** An atomic operation that reads `english` and `chinese` columns from the first worksheet of an `.xlsx` or `.xls` file, defaults the wordbook name from the file name while allowing edits, and persists the resulting wordbook in SQLite. Duplicate pairs are detected after trimming both values and comparing English case-insensitively; the same English word with a different Chinese meaning remains a separate entry. A duplicate wordbook name requires confirmation before replacement.
- **Relationships:**
  - implements wordbook
  - contains vocabulary-entry

### favorites-collection
- **Definition:** The application's single collection of saved word-and-meaning pairs for later review and practice. Its contents remain available when the source wordbook is replaced by another import.
- **Relationships:**
  - contains favorite-entry

### favorite-entry
- **Definition:** A saved copy of an English word and Chinese meaning originally taken from a vocabulary-entry. Its lifetime is independent of the source wordbook and its entries.
- **Relationships:**
  - belongs to favorites-collection

### practice-source
- **Definition:** The selected source for drawing a practice round: the active wordbook, the single favorites collection, or the mistake collection.
- **Relationships:**
  - references active-wordbook
  - references favorites-collection
  - references mistake-collection

### mistake-collection
- **Definition:** The application's persistent collection of word-and-meaning pairs on which the learner has submitted incorrect answers. It can be browsed and selected as a source of practice; its records survive replacement of the originating wordbook.
- **Synonyms:** 错题本
- **Relationships:**
  - contains mistake-entry

### mistake-entry
- **Definition:** An English word and Chinese meaning pair with a cumulative error count. Each incorrect submission for this pair increments its count, including repeated incorrect submissions on the same question. A skip alone does not increment it. The pair remains available for review and practice after its source wordbook is replaced.
- **Relationships:**
  - belongs to mistake-collection
