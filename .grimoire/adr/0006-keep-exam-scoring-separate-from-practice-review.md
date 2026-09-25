# Keep Exam Scoring Separate from Practice Review

**Status:** Completed
**Date:** 2026-09-24

## Context

Practice outcomes update the per-direction review schedule, while an exam checks all answers together and assigns a score. Exam outcomes could update both mistake counts and the practice review schedule, or only the mistake counts.

## Decision

Incorrect exam questions increment the independent mistake collection, including unanswered questions at final checking, but exam results do not change practice review progress. This retains a record of mistakes without treating an exam attempt as a scheduled practice review.

## Consequences

- Exam scoring and answer checking remain separate from practice review-outcome writes.
- Final checking must record each incorrect question only once, even if results are viewed again.
- A later practice round may still draw the same word from the mistake collection without inheriting an exam review outcome.
