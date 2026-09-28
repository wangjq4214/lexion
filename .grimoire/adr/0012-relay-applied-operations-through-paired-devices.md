# Relay Applied Operations Through Paired Devices

**Status:** Completed
**Date:** 2026-09-28
**Supersedes:** [ADR 0010](./0010-discover-lan-peers-with-mdns-and-display-pairing-code.md)

## Context

Direct-only operation exchange blocks multi-device synchronization when a device's later operation depends on changes it received from a third device. Requiring every pair of devices to meet and pair directly is not the desired model; an alternative is to trust a paired peer to relay its already-applied operation history.

## Decision

Keep mDNS discovery, first-connection pairing-code confirmation, and encrypted Noise connections between directly communicating peers on the same LAN. Synchronization is for devices belonging to one user, with pairing serving as the user's authorization. A device paired with B may synchronize **all operations already applied by B**, including operations B obtained from C, without separately pairing with C. Forwarded operations retain C as their original author rather than being rewritten as B's operations. A trusts the paired relay B's statement of C's authorship; no independent per-operation author signature is required. Pairing B authorizes this indirect exchange of B's applied history; it does not authorize an unpaired C to establish a direct connection to A.

## Consequences

- Devices need not all be online together: operations can travel through previously synchronized paired peers while preserving original provenance and causal ordering.
- Pairing a device can expose data acquired from other devices in its synchronization network; this transitive data-sharing scope must be understandable to the learner.
- The current direct-origin-only export/ingest restriction and pair-only cursors must change for this model to work. A receiver must not treat relayed operations as newly authored by the relay or apply an operation before its causal predecessors.
- Source attribution is based on the paired relay's assertion, not independent proof of C's authorship; a malicious or compromised paired device could misattribute an operation. This is an accepted trade-off for single-user LAN synchronization.
- Revocation and history compaction require separate policy decisions; they are not implied by a two-device synchronization cursor.
