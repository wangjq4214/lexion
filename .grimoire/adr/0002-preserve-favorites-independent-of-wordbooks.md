# Preserve Favorites Independently of Wordbooks

**Status:** Completed
**Date:** 2026-09-23

## Context

A learner wants one favorites collection whose saved words remain available for practice after a source wordbook is replaced. Reimporting a same-named wordbook currently deletes and recreates its entries, changing their row IDs. A collection tied only to entry IDs would lose those favorites on replacement.

## Decision

Keep saved English-word/Chinese-meaning pairs independently of source wordbook entries in the application's local persistent store, rather than relying solely on references to source entry IDs. The collection is singular; it can serve as a practice source after wordbook replacement.

## Consequences

- A replaced source wordbook does not delete its previously saved favorite pairs.
- Favorites have a separate lifecycle from imported wordbook entries, so removal must be explicit.
- Membership and practice queries cannot depend on source entry IDs remaining stable across reimports.
