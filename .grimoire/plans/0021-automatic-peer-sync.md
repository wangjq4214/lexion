# Automatic paired-peer synchronization (T0005)

- **Input:** `.grimoire/ticket/0001-offline-lan-learning-data-sync/T0005-automatic-peer-sync.md`, Spec 0004 §§1–5, ADR 0009/0010; ticket relationship README.
- **Status:** Implementing
- **Risk:** High — authenticated network ingestion and durable learning data.

## Summary

Extend the existing Noise XX pairing transport to exchange bounded, versioned replay envelopes only after mutual direct trust. Reconcile contiguous per-origin cursors on reconnection; persist before acknowledging, report status/errors without stopping local study, and notify active views of committed changes. Do not forward operations from third-party origins or claim legacy data are synchronized.

## Source intent and acceptance

1. Two fresh paired devices automatically exchange offline content, favorites, mistakes, review and settings on LAN reunion: steps 1–3 and dual-repository network tests.
2. Reconnect, duplicate/partial/out-of-order arrival converge without duplicate effects or resurrection: steps 1–2 and retry/restart tests.
3. A–B/B–C do not imply A–C authorization, unpaired/mismatched identity gets no read/write: step 1–2 and three-device negative tests.
4. Offline/discovery failure leaves study available; remote persistence refreshes visible data but never local active selection or unfinished practice: steps 2–3 and UI tests/manual exercise.
5. Fresh-data rollout without clearing legacy data; existing tests pass; physical desktop pairing/outage manual validation is separately required and cannot be inferred from loopback tests.

## Design impact

- **Modify `src-tauri/src/lan.rs`:** Own the authenticated per-session wire protocol, bounded frames, automatic retry, and per-peer sync outcomes. Use authenticated Noise static keys as the authorization boundary, not discovery TXT or peer-supplied origin claims. Keep the transport state alive beyond pairing.
- **Modify `src-tauri/src/wordbooks/repository/replay.rs` and `repository.rs`:** expose a repository sync API for contiguous origin watermarks and validated ingestion, enforcing origin/dependency restrictions before persistence. Avoid directly manipulating SQL from the LAN module. No database-copy/snapshot merge.
- **Modify `src-tauri/src/lib.rs`, `src/data/lan.ts`, `src/routes/devices.tsx` and relevant view refresh seam:** wire repository to LAN service and surface progress/error plus committed-change notifications. Use existing Astryx components and tokens, no custom layout markup.

## Implementation steps

1. Add repository exchange methods to enumerate applied contiguous per-origin cursors and bounded operations, filter to self and directly paired peer origin, reject unauthorized origins/dependencies on ingress even when peer is trusted. Explicitly report an unshareable causal predecessor (e.g. B operation depending on C for A) rather than sending C or silently claiming B synchronized. Test origin spoofing, A–B/B–C and legacy rejection.
   - **Revision 2 security gate:** before LAN starts, derive the local replay origin from the durable Noise public key and bind it only if replay history is empty or already matches. On every authenticated session recompute the remote expected origin from its Noise public key; reject a different claimed origin before exporting or ingesting anything. Preserve nonempty incompatible history without deletion and surface a sync error. Add a malicious B-claims-C test.
2. Extend the Noise transport after mutual trust with a bounded request/response exchange (watermarks and batches). Send only authorized envelopes; validate every received message, persist via replay ingestion before next request/acknowledgement; retry from durable watermarks after failure. Bound batch/frame sizes and concurrent sessions, avoid deadlocks on simultaneous reconnect. Record success/failure timestamps and errors. Loopback dual/tri-peer tests exercise interruption, restart, duplicate delivery, unauthorized input and actual business projections.
3. Expose backend sync status to UI; notify on committed remote changes and refresh idle visible collections/target data through existing read paths, while leaving local selection and practice round state untouched. Test view notification behavior where feasible; show offline/error and last sync clearly.
4. Run Rust suite, TypeScript tests/typecheck/build, format within touched files, inspect diff, and document physical two-device manual validation results or outstanding limitation.


## Revision 3: Windows test loader and restart regression

Source: user request to declare Common Controls v6 for the Windows Rust test executable and fix the two runtime failures found after the loader starts (79/81 passed). Preserve the existing T0005 scope and unrelated uncommitted changes.

1. Add an MSVC-only linker manifest dependency for Common Controls v6 in `src-tauri/build.rs` so the `lib` unit-test executable loads `TaskDialogIndirect` without manual modification of `target/`. Check the generated executable actually embeds the dependency and executes after a clean rebuild; do not depend on `rustc-link-arg-tests` alone, which does not target the library unit-test harness.
2. Correct `load_trust` in `src-tauri/src/lan.rs`: `sync_ids` keys are Noise key hashes while `peers` keys are raw public-key hex. Validate each hash against its peer key and retain duplicate/format rejection. Cover valid persisted trust after restart and malformed/untrusted pinned identities.
3. Run `cargo test --lib` without post-processing the executable, `cargo test`, `cargo fmt --check`, and relevant frontend checks if affected. Report other failures separately.
4. Parallel Rust execution exposed an independent Confirm/Commit race: `finish_pairing` can receive Commit while waiting for another Confirm already received on an earlier iteration. Skip the extra receive when both confirmations are present; repeat the default parallel suite to catch regressions.
## Edge cases

| Condition | Expected behavior | Check |
| --- | --- | --- |
| Non-mutual trust, forged discovery ID, origin spoof, unauthorized dependency | No data read/write or third-party forwarding | network negative tests |
| Large content/frame, malformed or unsupported operation | bounded failure with status; no false acknowledgement | protocol tests |
| Interruption between receive and response, restart, duplicate concurrent connection | persisted cursor drives retry, idempotent projection | loopback tests |
| Peer offline or mDNS unavailable | local operations continue and UI says retry/unavailable | status and manual test |
| Existing legacy aggregates | never deleted or presented as replayable history | repository rejection tests |
| Remote replay while local practice is in flight | current UI selection/round retained; durable data refreshed after commit | view test/manual exercise |

## Verification strategy

Rust repository and TCP loopback tests compare actual business state on both peers and deny third-party and unpaired access; full `cargo test`, `cargo fmt --check`, `bun run test`, `bun run typecheck`, `bun run build`. Physical desktop two/three-device mDNS, pairing, outage and recovery acceptance remains required; report honestly if unavailable. Review confidentiality, resource limits, causal dependencies and UI stale-state risks independently from intent check.

## Risks and assumptions

- Replay currently records all applied operations as causal predecessors. A device with B/C history can produce B-origin operations depending on C; A must not receive C. This may prevent exchange of those B operations with A until the causal model is revised; do not silently flatten dependencies or leak C. A material change to the replay causal contract requires a plan revision and tests.
- An applied change may be too large for existing 4 KiB pairing frames; sync needs a separate explicitly bounded data-frame limit or chunks and validation, never unbounded allocation.
- Physical LAN testing may not be available in this environment; absence prevents declaring the desktop manual acceptance clean.
- **Verification limitation:** Git Bash `cargo test --lib --no-run` succeeds, but the Windows test binary exits before running any tests with `STATUS_ENTRYPOINT_NOT_FOUND` (also when launched with a clean Windows PATH). This is not passing runtime evidence. Physical multi-device LAN acceptance is likewise unverified.
- **Integrated static evidence:** Git Bash `cargo check`, `cargo test --lib --no-run`, `cargo fmt --check`, frontend `bun run check`/`bun run build` and 131 Vitest tests pass on the current combined tree. Runtime Rust and physical LAN evidence still missing.
- **Identity boundary:** Revision 2 derives replay IDs from authenticated Noise public keys on fresh databases; mismatched historic operation logs are retained but cannot sync, rather than silently rewritten or cleared. Verify upgrade messaging and actual cross-device operation on a working Rust host.

## Revision log

| Revision | Changed approach | Reason and evidence |
| --- | --- | --- |
| 1 | Single-frame operation exchange → bounded multi-frame operation transfer; normalize status ID and offline state | Review found 60 KiB frame silently prevents ordinary full-book imports (>300 KiB), trusted status IDs differ from sync status IDs, and mDNS removal leaves stale “synced” UI. Preserve existing unbounded local import semantics while bounding network transfer. |
| 2 | Trust-on-first-use replay ID pin → deterministic replay ID derived from authenticated Noise static public key for fresh syncable databases, reject mismatched pre-existing identities | Independent security/intent audits showed a trusted B can claim C’s replay ID before the first pin and forward C-origin changes to A. An ID must be verifiably bound to its Noise key; preserve existing data and fail closed for incompatible historical databases rather than rewriting history. |
| 3 | Declare Common Controls v6 for Windows Rust test linking; validate saved sync pin keys against hashed trusted keys | The generated lib-test executable loaded comctl32 v5 and failed before tests; after manual manifest embedding, 2 restart tests failed on raw-key/hash mismatch. |
| 3a | Share one Common Controls manifest through MSVC linker input and disable Tauri's duplicate embedded manifest resource on Windows | `cargo test` also links the Tauri bin target; adding the manifest input on top of its default resource caused LNK1123. The shared input now preserves the app's v6 manifest and enables lib-test execution. Parallel test runs additionally exposed a Confirm/Commit receive race, fixed in `finish_pairing`. |
