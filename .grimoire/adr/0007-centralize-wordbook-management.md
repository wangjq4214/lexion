# Centralize Wordbook Management on One Page

**Status:** Superseded
**Superseded by:** [0008-keep-practice-entry-deletion-alongside-central-management](./0008-keep-practice-entry-deletion-alongside-central-management.md)
**Date:** 2026-09-25

## Context

Wordbook import currently has its own home entry and route, whole-book deletion is in practice setup, and deletion of a wordbook entry is available during practice. Keeping these actions scattered conflicts with the requested single place for ordinary wordbook management; practice and exam still need wordbook selection as a learning-source choice.

## Decision

Make a dedicated wordbook management page the home entry for importing and deleting wordbooks, browsing each book's words, and removing individual words. Remove wordbook-management actions from other pages, including deletion during practice, while retaining wordbook selection in practice and exam because it chooses the source of learning rather than managing the collection.

## Consequences

- The home wordbook button leads to management instead of the standalone import action; import is accessible from management, including when there are no wordbooks.
- Practice setup no longer deletes whole wordbooks, and practice questions no longer delete entries; users perform those actions from management.
- Browsing all entries in a particular wordbook needs a full-entry retrieval path rather than the existing random, limited practice sample.
- Favorites and mistake collections remain separate from wordbook management and retain their independent lifetimes.
