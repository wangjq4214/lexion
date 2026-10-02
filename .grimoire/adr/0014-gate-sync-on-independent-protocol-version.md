# Gate Synchronization on an Independent Protocol Version

**Status:** Completed
**Date:** 2026-09-28

## Context

The user requires peers to exchange synchronization protocol versions before synchronizing and reject synchronization when those versions differ. The synchronization protocol version is not the application's software release version; release-version equality is not the requested compatibility criterion.

## Decision

Use an independent synchronization protocol version as the compatibility gate: peers exchange it before synchronization and may synchronize only when their protocol versions match. Do not substitute the software release version for this check.

## Consequences

- Different synchronization protocol versions must not exchange or apply learning-data changes.
- Software release versions do not determine protocol compatibility; different releases with the same synchronization protocol version are not rejected solely for their release-version difference.
- The existing synchronization introduction in `src-tauri/src/lan.rs` carries a hard-coded `version: 2` and rejects unexpected introductions generically; implementation must be checked against the explicit exchange and compatibility requirement rather than assuming a new gate is wholly absent.
