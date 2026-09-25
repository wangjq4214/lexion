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
- **Definition:** One vocabulary practice session created after the user starts practice. It normally covers all sampled questions, but the learner may exit early and settle immediately. Early settlement counts only completed questions; an unsubmitted current question and all remaining questions are excluded from scores (including hints used on the unfinished question) and review records. If the current question has already been submitted incorrectly or skipped and its answer is showing, save that question's review outcome before settlement; if saving fails, remain in practice for retry. The early-settlement summary shows the actual completed question count and elapsed time through exit. A question is completed by answering correctly, submitting an incorrect answer, or skipping it. An incorrect submission ends the question without requiring a retry: show the learner's answer alongside the correct answer with character-level differences highlighted in either question direction, then wait for the learner to continue manually. This is a distinct wrong-completion outcome: increment the error count once but not the correct or skip counts. In Chinese-to-English questions, hints have three sequential levels: the first reveals about one third of the English letters, the second about two thirds while retaining earlier reveals, and the third reveals the full word. Each level counts as one hint use; the third is equivalent to skipping: it reveals the English word and Chinese meaning, counts as skipped rather than correct, and advances only after the learner has seen the answer. Direct skipping has the same reveal and continuation behavior. Skipping without an incorrect submission does not increment the error count. Elapsed time starts when the first question is shown, includes hints and revealed-answer viewing time, and stops when the last question is completed or the learner exits. The completion summary displays minutes and seconds, correct answers, error count, hint count, and skip count, but does not list skipped words.
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
- **Definition:** The application's single collection of saved word-and-meaning pairs for later review and practice. Its contents remain available for browsing and practice when the source wordbook is replaced or deleted, including when no wordbook remains.
- **Relationships:**
  - contains favorite-entry

### favorite-entry
- **Definition:** A saved copy of an English word and Chinese meaning originally taken from a vocabulary-entry. Its lifetime is independent of the source wordbook and its entries.
- **Relationships:**
  - belongs to favorites-collection

### practice-source
- **Definition:** The selected source for drawing a practice round: the active wordbook, the single favorites collection, or the mistake collection. With no wordbook present, a populated favorites or mistake collection remains available for browsing and practice. The entry screen offers navigation to practice, exam, mistakes, favorites, and wordbook import; practice setup contains source and mode settings but not collection browsing or import actions. When all sources are empty, learners return home to import a wordbook, and the wordbook source remains unavailable until a wordbook is imported.
- **Relationships:**
  - references active-wordbook
  - references favorites-collection
  - references mistake-collection

### mistake-collection
- **Definition:** The application's persistent collection of word-and-meaning pairs on which the learner has submitted incorrect answers. It can be browsed and selected as a source of practice; its records remain available when the originating wordbook is replaced or deleted, including when no wordbook remains.
- **Synonyms:** 错题本
- **Relationships:**
  - contains mistake-entry

### mistake-entry
- **Definition:** An English word and Chinese meaning pair with a cumulative error count. Each incorrect practice answer submission for this pair increments its count; in the agreed wrong-completion flow, a wrong submission ends the attempt, so at most one such increment occurs per question. Incorrect exam questions (including unanswered ones at final checking) also increment the count once per question. A practice skip alone does not increment it. The pair remains available for review and practice after its source wordbook is replaced.
- **Relationships:**
  - belongs to mistake-collection

### review-schedule
- **Definition:** A per-practice-source coverage schedule that selects words across rounds so eligible words are eventually drawn, while prioritizing due reviews and reserving places for words not yet drawn from that source. Each wordbook maintains its own coverage progress; the favorites collection and mistake collection each maintain separate coverage progress. An identical English-and-Chinese pair in these sources shares its memory stability and due time for the same question direction. A word is not repeated within a round. Mixed-mode scheduling fixes the question direction before assessing due status.
- **Synonyms:** 复习调度
- **Relationships:**
  - depends on practice-source
  - references vocabulary-entry
  - references favorite-entry
  - references mistake-entry

### exam-round
- **Definition:** A vocabulary assessment drawing distinct word-and-meaning entries from one selected wordbook, the mistake collection, or the favorites collection, with separately specified counts of English-to-Chinese and Chinese-to-English questions; if the requested total exceeds available entries, the learner must reduce the count rather than repeat entries. Answers are checked together after the learner finishes; blank answers are incorrect, each question has equal weight, and the score is the percentage of correct answers on a 100-point scale. Answer matching follows existing practice rules: English ignores case after trimming, while Chinese must match the stored meaning after trimming. Incorrect exam answers enter the mistake collection, but exam results do not change practice review progress.
- **Relationships:**
  - depends on practice-source
  - contains vocabulary-entry
  - contains favorite-entry
  - contains mistake-entry

### review-outcome
- **Definition:** A completed practice question's result used to adjust that word's later review: only a first-attempt correct answer without hints counts as an unassisted success; a hint, a wrong-completion outcome, or a skip prompts earlier review. More hint levels shorten the review interval. Mastery is tracked separately for Chinese-to-English and English-to-Chinese questions, but shared for an identical English-and-Chinese pair across practice sources in the same direction; distinct meanings remain distinct pairs. Initial scheduling predicts retention as R(t) = exp(-t / S), with per-pair, per-direction memory stability S updated by outcomes, and schedules review when retention reaches an adjustable target initially set at 90%. Update coefficients remain subject to testing and feedback; this is not a fixed day ladder.
- **Relationships:**
  - belongs to practice-round
  - depends on review-schedule
