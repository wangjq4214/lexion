# Discover LAN Peers with mDNS and Display a Pairing Code

**Status:** Implementing
**Date:** 2026-09-26

## Context

Local-network synchronization needs a way for nearby devices to find each other and authorize a new peer. Manual address entry was an alternative for discovery, and unauthenticated connections would not establish which devices may exchange learning data.

## Decision

Automatically discover devices using mDNS. For the first connection, manually confirm a displayed pairing code and use the Noise Protocol Framework to establish an encrypted peer connection; thereafter, synchronize automatically when paired devices meet on the same LAN. Every participating device must be explicitly paired; pairing A with B and B with C does not implicitly authorize A to trust or synchronize with C.

## Consequences

- Devices must advertise and discover peers on the LAN and handle networks where mDNS discovery is unavailable.
- Pairing must bind the confirmed code to the connected device, remember authorized peers, and reject unpaired peers; code verification and revocation details still need definition.
- Paired peers must reconnect and resume synchronization without repeating the first-pairing confirmation.

## Clarification (2026-09-26): Protocol and reconnect behavior

The learner confirmed that “noisy” means Noise Protocol Framework, and accepted first-pairing confirmation followed by automatic synchronization on subsequent LAN encounters.

## Clarification (2026-09-26): No transitive trust

The learner confirmed that each participating device needs explicit pairing: a peer trusted by another peer is not automatically trusted or synchronized.
