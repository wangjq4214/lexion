# Keep Practice Entry Deletion Alongside Central Management

**Status:** Completed
**Date:** 2026-09-25
**Supersedes:** [0007-centralize-wordbook-management](./0007-centralize-wordbook-management.md)

## Context

The management page is intended to be the normal place to import and delete wordbooks and browse their words. The earlier decision to remove deletion of the current word during practice would disrupt an existing useful quick action; the user clarified that it must remain. Alternatives were to confine every deletion action to management or to retain the in-practice exception.

## Decision

Provide a dedicated wordbook management page for import, whole-book deletion, browsing every book's words, and deleting individual words while browsing. Remove other wordbook-management entry points, including whole-book deletion from practice setup, but retain the existing action for deleting the current word during practice. Keep wordbook selection in practice and exam as a learning-source choice rather than a management action.

## Consequences

- The home wordbook button opens management instead of a standalone import action; import remains available there even with no books.
- Both the management word list and the practice question can delete a word; their views must reflect persistent changes without deleting unrelated favorites or mistakes.
- Browsing all entries requires a full-entry retrieval path rather than the random, limited practice sample.
- Deleting entire wordbooks happens in management, not practice setup; practice and exam retain source selection.
