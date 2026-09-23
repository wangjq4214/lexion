# Schedule Practice With Source Coverage and Due Review

**Status:** Implementing
**Date:** 2026-09-23

## Context

Wordbook, favorites, and mistake practice currently sample randomly on each round. This can repeat previously seen words while leaving others unseen; per-round summary counts do not provide durable per-word review progress. Alternatives include keeping independent random sampling or introducing persistent, source-aware review scheduling.

## Decision

Schedule each practice source to eventually cover all its eligible words: prioritize due reviews while reserving capacity for words not yet drawn from that source, with no duplicate word within a round. Coverage is source-specific, while an identical English-and-Chinese pair shares memory stability and due time across sources for the same question direction; different meanings and different directions remain distinct. In mixed mode choose the direction before checking due status. Only a first-attempt correct answer without hints is an unassisted success; hints, incorrect submissions, and skips bring a word's review forward, with additional errors or hints further shortening its review interval. Initially predict retention using R(t) = exp(-t / S) with per-pair, per-direction memory stability S updated by outcomes; schedule the next review when R reaches an adjustable target initially set at 90%, instead of using a fixed day ladder. Error submissions and hint levels reduce the improvement to S; skipping reduces S. Calibrate update coefficients through testing and feedback.

## Consequences

- Multiple rounds of wordbook, favorites, and mistake practice can cover each source instead of relying on chance.
- The same pair may have different source coverage progress, but shares one memory state per direction across sources; Chinese-to-English and English-to-Chinese progress remain independent.
- Durable per-word history and review timing, plus corresponding selection and outcome-recording boundaries, are required; existing one-round statistics alone are insufficient.
- A fair allocation between due and unseen words is necessary so due reviews do not indefinitely crowd out first exposure.

## Correction (2026-09-23)

The original time-based wording was underspecified. The learner explicitly chose a predicted forgetting curve over a fixed 1/3/7/14/30-day interval ladder; the decision above now makes that distinction explicit.

## Clarification (2026-09-23)

The learner accepted the exponential retention model and an adjustable initial 90% target. The stability update coefficients are not fixed by this decision and must be calibrated rather than presented as established learning constants.

## Clarification (2026-09-23): Cross-source identity

The learner confirmed independent coverage for each source but a shared memory state for the same English-and-Chinese pair and direction across wordbooks, favorites, and mistakes; mixed-mode direction is fixed before due-status selection. The decision and consequences above now explicitly include this distinction.
