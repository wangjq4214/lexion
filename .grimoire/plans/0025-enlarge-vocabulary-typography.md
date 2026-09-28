# Enlarge vocabulary typography

- **Input:** Conversation: enlarge English words and Chinese meanings in practice, exam, mistakes, favorites, and wordbook entry lists.
- **Date:** 2026-09-27
- **Status:** Completed

## Summary

Increase vocabulary content typography in the five requested surfaces using Astryx component props and theme tokens, leaving navigation, metadata, and unrelated UI unchanged.

## Source intent and acceptance

- Practice prompt and revealed word/meaning (including incorrect-answer comparison) are easier to read.
- Exam prompts and revealed correct answers are easier to read.
- Word and meaning rows in mistakes, favorites, and wordbook management are larger; long text stays readable and row actions remain available.
- No change to question logic, score, stored entries, or non-vocabulary pages.

## Implementation steps

1. Update `src/features/practice/PracticeQuestion.tsx` to emphasize the question prompt and revealed answers using supported Astryx typography; preserve diff highlights and answer flow. Verify both directions and reveal cases.
2. Update `src/routes/exam.tsx` to enlarge question labels and correct-answer text without changing grading or entry controls. Verify question and graded states.
3. Update `src/features/mistakes/MistakesList.tsx`, `src/features/favorites/FavoritesList.tsx`, and `src/routes/wordbooks.tsx` so row labels and meanings use larger token-backed typography with wrapping where necessary, while keeping metadata and row actions usable. Verify all three lists.
4. Run targeted rendering checks and type/lint/build checks; inspect the final diff for design-system compliance and unrelated changes.

## Design impact and risk

- Modify existing view components only. Preserve their existing List/Section layout and data dependencies; no new abstraction or schema changes.
- Risk: long English words or Chinese meanings may clash with trailing controls; ensure wrapping and avoid truncating the meanings.
- Risk: enlarged exam rows may become taller; allow natural list growth instead of fixed heights.

## Verification strategy

- Rendering assertions for vocabulary content and relevant typographic styling on all requested surfaces; static type/lint/build checks.
- Visual verification of narrow-width wrapping when a browser is available; otherwise disclose this evidence limit.
