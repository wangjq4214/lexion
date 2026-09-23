# Preserve Mistake Records Independently of Wordbooks

**Status:** Completed
**Date:** 2026-09-23

## Context
The learner wants incorrect word records and their accumulated error counts to remain available after replacing a source wordbook. Reimporting a same-named wordbook deletes and recreates its entries, so tying mistake history solely to entry row IDs would lose that history; the alternative is to retain independent word-and-meaning records.

## Decision
Persist mistake records, their English-word/Chinese-meaning pairs and cumulative error counts independently of source wordbook entry IDs in the application's local store, so the mistake collection remains browsable and usable for practice after replacement.

## Consequences
- Replacing a source wordbook does not remove its mistake records or reset their counts.
- Mistake collection practice and per-pair count updates must not rely on stable imported entry IDs.
- Mistake records have an independent lifecycle from imported entries.
