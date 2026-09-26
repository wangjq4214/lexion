# Support Offline Multi-Device LAN Synchronization

**Status:** Implementing
**Date:** 2026-09-26

## Context

Learners want wordbooks, favorites, mistake records, and review progress shared across devices. A single always-online host would not allow each device to modify its local data while disconnected; alternatively, devices can retain independent local changes and merge after reconnecting.

## Decision

Support offline edits on each device and automatically merge their changes when the devices synchronize on the local network. Synchronize wordbooks, favorites, mistake records, review progress, and review-target settings together, rather than only imported wordbooks; keep the current selected wordbook and an in-progress practice round local to each device. Merge by replaying changes rather than overwriting a peer's whole database: preserve per-device operation order and causal order across devices, then deterministically order concurrent operations. A later replayed edit or deletion takes effect, including concurrent changes to the same entry. Preserve the actual occurrence time of learning events for review-interval calculations.

## Consequences

- Each device needs a durable record of changes made while disconnected, and synchronization must tolerate retries and reconnection.
- The current local SQLite data model and mutation paths require a merge boundary; directly copying one database over another would lose offline edits.
- Replay must be deterministic and idempotent across devices; deletion and content changes require stable identity so later operations cannot accidentally target a different entry.
- Learning events need their occurrence times as distinct data from the replay ordering; review state must remain reproducible after reconciliation.

## Clarification (2026-09-26): Ordered replay

The learner chose time-ordered replay for merging offline changes. To avoid treating unsynchronized device clocks as authoritative, the learner confirmed preserving per-device and causal order and using deterministic order for concurrent operations; later replayed modifications or deletions take effect. Learning events retain occurrence time for review timing. Review-target settings synchronize; current wordbook selection and unfinished practice remain local.

## Clarification (2026-09-26): Fresh-data rollout

The learner will clear existing local data after the sync feature is ready, before using synchronization. The initial sync scenario therefore starts with fresh data on participating devices; merging independent pre-feature learning histories or reconstructing past operations from current aggregates is not required. The application must not delete existing data automatically.
