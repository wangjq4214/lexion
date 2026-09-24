# Use File-Based TanStack Router and Jotai for Frontend Navigation and State

**Status:** Testing
**Date:** 2026-09-24

## Context

The React frontend currently selects setup, practice, summary, favorites, and mistakes views through conditional rendering in `src/App.tsx`, which also owns their state via `useReducer`, `useState`, and refs. The user requested replacing the current page and state management rather than retaining this centralized arrangement.

## Decision

Use TanStack Router with file-based routes for setup, practice, summary, favorites, and mistakes navigation and Jotai for state shared across those pages. Keep the pure practice reducer for its transition rules, rather than requiring a rewrite of those rules as individual atoms. While a practice round is active, prevent browser-back or other route navigation from leaving it directly; direct the learner to the existing exit-and-settle flow so review writes are not bypassed. Reloading a practice or summary route without its in-memory round data returns to setup rather than attempting to restore a round.

## Consequences

- Page selection moves from the conditional view switch toward a file-based route tree.
- Shared state currently held by `App` moves to Jotai where needed across routes; local transient state may remain local, and the existing pure practice reducer continues to define practice transitions.
- Navigation away from an active round needs a guard and a clear path to the existing exit-and-settle action. A direct reload does not restore in-memory practice progress.
- Routing and state migration must preserve established practice, favorites, mistakes, and wordbook behavior.
