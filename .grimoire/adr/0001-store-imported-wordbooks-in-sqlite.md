# Store Imported Wordbooks in SQLite

**Status:** Completed
**Date:** 2026-09-22

## Context

The application currently bundles a fixed in-memory vocabulary list. It must persist multiple user-imported Excel wordbooks across launches and query the selected collection for practice.

## Decision

Store imported wordbooks and their vocabulary entries in a local SQLite database. Import the first worksheet of `.xlsx` and `.xls` files atomically using `english` and `chinese` columns. Remove duplicate pairs after trimming values and comparing English case-insensitively; preserve entries whose English matches but Chinese meaning differs. The file name supplies an editable default wordbook name; replacing an existing same-named wordbook requires confirmation. Practice reads up to ten randomly selected entries from the active wordbook rather than from the bundled fixed list.

## Consequences

- Imported wordbooks remain available after the application restarts.
- Wordbook and vocabulary-entry persistence becomes part of the Tauri application boundary.
- Database initialization, schema evolution, import transactions, and persistence error handling are required.
