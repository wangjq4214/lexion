# Remove Only the Selected Direct Device Pairing

**Status:** Completed
**Date:** 2026-09-28

## Context

The learner needs to remove a paired device without disrupting synchronization between other paired devices. Under [ADR 0012](./0012-relay-applied-operations-through-paired-devices.md), synchronization can also depend on a paired device acting as the only relay; the alternative of automatically reconnecting the remaining devices would establish a new trust relationship without their existing pairing-code confirmation.

## Decision

Removing a device revokes only the direct pairing with that selected device on the initiating device. Other direct pairings remain intact and continue to synchronize; previously synchronized data is not rolled back. If removal breaks the only relay path between two remaining devices, they must pair manually to restore communication rather than automatically trusting one another.

## Consequences

- Removing one pairing must not delete or reset the trust records or synchronization state of the other paired devices.
- A device relying solely on the removed pairing for a relay path cannot continue synchronizing across that path until a new manual pairing creates another path.
- This is a local direct-pairing removal, not a network-wide revocation of the selected device or a purge of previously shared data. The removed device's new operations may still arrive indirectly through other still-paired devices under ADR 0012; the learner explicitly accepts this boundary.
