# Learning history and settings sync

- **Input:** `.grimoire/ticket/0001-offline-lan-learning-data-sync/T0003-learning-history-and-settings.md`; Spec 0004 §§1–3; ADRs 0003, 0004, 0006, 0009.
- **Status:** Completed
- **Risk:** High (durable replay, cumulative learning data, multi-device convergence).

## Source intent and acceptance

Sync raw mistake submissions, scheduling/coverage, review completion and target changes, without aggregating remote summaries. Preserve pair/source/direction semantics, exam/practice separation, event occurrence times, local-only selection and unfinished rounds; retries and late canonical reorder must converge. T0001 replay is available; T0002 content projection currently owns content only. Existing legacy data remains untouched and unsynchronized.

## Design impact

- Add `repository/learning.rs`: validated version-1 learning operations and their SQL projection, with stable submission identity scoped by originating device, and stable schedule identity `(origin, sequence)` rather than local review row ID. Owns mistakes, submissions, coverage, review memory and target on rebuild; local pending tokens are mapped to stable schedule identities and retained across replay when possible. Apply completion only to a scheduled token and guard against duplicate completion.
- Compose content and learning projections in `repository.rs`, dispatching tagged operations; reset learning before content so old coverage is not restored over reconstructed coverage. Use this composite for all production content writes and future network ingress. No database-copy or state-snapshot merge.
- Route `record_mistake`, `record_mistake_once`, `schedule_at`, `complete_at`, `set_review_target` through the atomic append boundary on fresh databases, retaining legacy write paths for existing data. Scheduling returns local IDs while logging selected stable pairs/directions and stable source name; completion uses the persisted schedule origin and occurrence time. Local tokens remain local IDs and must not collide with remote tokens.

## Implementation steps

1. Define learning operation validation, replay and composite projection; migrate learning tables safely for fresh data and preserve legacy mode. Verify malformed operations cannot commit, reorders rebuild consistently.
2. Hook mistake and settings writes to log with occurrence time; preserve submission retry/conflict rules and exam-only mistake semantics. Verify count, de-duplication and target rescheduling.
3. Hook scheduling and completion, mapping local review IDs to stable change IDs; preserve original selection algorithm and local session tokens. Verify coverage per source, memory per pair/direction, independent meanings, deletion/replacement and clock skew.
4. Run targeted Rust tests, existing scheduling/exam suite, formatting and integrated checks; assess replay reconstruction, compatibility and error paths.

## Edge cases

| Condition | Expected | Check |
| --- | --- | --- |
| Two origins use identical submission string | Both count, each retry once; same origin conflicting pair rejected | dual-repo tests |
| Local token after remote reorder or same numeric token on peer | No wrong completion; local unfinished round survives | replay tests |
| Completion re-received or reordered before its schedule | Applied once after causal parent; no phantom result | replay tests |
| Content removal/reimport | Coverage resets only affected source; mistakes and memory survive | integration tests |
| Legacy database | No destructive reset or fabricated history | existing migration tests |


## Revision log

| Revision | Changed approach | Reason and evidence |
| --- | --- | --- |
| 1 | Permanent origin-scoped submission claim → origin-scoped claim with deterministic 30-day practice reuse (exam claims permanent) | Review found the prior implementation broke the repository's existing practice retry-window contract; occurrence timestamps allow identical replay without relying on a peer's current clock. |
| 2 | Accept any referenced completion → require same origin as schedule; reject non-normalized schedule pairs; detect pending-only legacy databases | Review identified concrete foreign-token completion, split pair identity and legacy migration failure; targeted regression tests now cover each. |
## Verification strategy

Rust SQLite dual-end ingestion with reversed delivery, retries and reopen; assertions on mistakes/submission count, coverage, memory and target. Existing scheduling and exam tests; `cargo test` and `cargo fmt --check` in `src-tauri`. No LAN network implementation (T0005), no active session transfer.
