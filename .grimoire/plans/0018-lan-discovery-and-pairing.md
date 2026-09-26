# LAN discovery and explicit pairing

- **Input:** `.grimoire/ticket/0001-offline-lan-learning-data-sync/T0004-lan-discovery-and-pairing.md`, Spec 0004 §4, ADR 0010
- **Status:** Implementing

## Summary

Provide an independent Tauri LAN pairing service without exchanging learning data. Advertise and browse mDNS; bind a displayed short authentication string to a Noise XX handshake and persist only identities confirmed by both endpoints. Keep pairing state and authorization in Rust, not the frontend.

## Source intent and acceptance

1. Discovery and encrypted first connection, both endpoints verify session code, persistent direct trust: steps 1–3, tests and two-instance manual check.
2. Mismatch, cancellation, unconfirmed peers and impersonation never grant authority: steps 2–3, adversarial tests.
3. A–B and B–C do not authorize A–C; unavailable LAN leaves study working: steps 2–4, tests and UI check.
4. Learning-data exchange and automatic sync are explicitly deferred to T0005.

## Design impact

- Create `src-tauri/src/lan.rs`: own discovery daemon, listener, Noise transport, pending pairing lifecycle, trust storage and Tauri commands. Isolate this security boundary from SQLite/replay; persist a local static key and explicit peer public keys under app data using atomic file replacement.
- Modify `src-tauri/src/lib.rs`: start LAN service without failing ordinary local app startup on network errors; expose commands. No learning data commands are added to the network protocol.
- Create `src/data/lan.ts` and `src/routes/devices.tsx`; modify home navigation: polling discovery/pending/trusted status and explicit confirm/cancel UX. Retain existing app frame.
- Add Rust tests for SAS binding, persistence, non-transitive trust, rejection paths and a loopback handshake; typecheck/build frontend.

## Implementation steps

1. Persist stable Noise static key and direct-trust set; use restrictive local file permissions where supported, validate stored bytes, atomic writes, and no transitive import. Test restart and corruption rejection.
2. Advertise/browse a unique mDNS instance, accept TCP, run Noise XX (both directions); derive code from handshake hash and identify peer from authenticated Noise static key, not mDNS TXT metadata. Bind each pending request to one live session with timeout and failure/cancel handling. Test transport and mismatch paths.
3. Require explicit confirmation at both ends before persisting the remote key. Recognized keys may reconnect without first-pairing confirmation; no application data exchange exists in T0004. Test unconfirmed, mismatch, cancellation, restart and A–B/B–C isolation.
4. Expose discovered peers, pending codes and trusted identities to a simple devices screen with clear offline/error messaging. Confirm requires typing the other screen's code; allow cancel. Verify TypeScript, tests and manual two-instance instructions.

## Edge cases

| Condition | Behavior |
| --- | --- |
| mDNS unavailable or no peers | Local app starts; screen explains state and retry |
| handshake error, stale request, cancel, timeout | no authorization; clear pending session |
| forged mDNS identity or changed Noise key | never treated as existing trusted peer |
| simultaneously present A–B/B–C | only directly saved static keys authorized |
| malformed state file | fail closed; do not silently replace identity or trust |

## Verification strategy

Run `cargo test`, `cargo check`, `bun run typecheck`, `bun run test`, `bun run build` as available; inspect the full diff and test boundary failures. Manual two-device LAN test remains necessary for mDNS across different hosts; no claim of full physical-LAN validation from loopback tests.

## Risks and assumptions

- Noise XX with session-hash-derived 8-digit SAS and user-entered comparison provides MITM detection only if users actually compare on both screens; avoid claiming cryptographic identity from mDNS names.
- At-rest identity storage is app-private but not OS keychain protected; device compromise is outside this ticket.
- No revocation UX is required by the ticket; future sync must consult direct trust before transmitting data.

## Revision log

| Revision | Changed approach | Reason and evidence |
| --- | --- | --- |
| 1 | Encrypted Hello conveys each side's stored trust state; asymmetric records re-enter mutual confirmation instead of auto-closing. Retry remaining discovered addresses after pre-decision handshake failure; allow consent withdrawal while pending. | Security review found a one-sided trust write can strand subsequent pairings, a stale first TCP address can block a valid second address, and cancellation after initial confirmation was rejected. Added loopback and decision tests. |
