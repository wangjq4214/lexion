# Count Practice Coverage on Completion

**Status:** Completed
**Date:** 2026-09-28
**Supersedes:** [ADR 0004](./0004-schedule-practice-with-coverage-and-due-review.md)

## Context

[ADR 0004](./0004-schedule-practice-with-coverage-and-due-review.md) reserved slots for words not yet drawn, but the current scheduler marks coverage when it assigns a round, even if the learner exits before submitting or ever seeing the question. The alternative is to distinguish assignment from completion so unfinished words are not silently treated as covered.

## Decision

Assigning a word to a round does not count as source coverage. A question that is not completed before exit remains pending for that source, whether shown without submission or not yet shown. In the next round from that source, pending words take the first available slots up to the requested count, ahead of due reviews and newly encountered words; fill remaining slots by the existing due-review/new-word rules. Completion, including a wrong submission or a skip under the existing practice-round semantics, counts toward source coverage and updates review memory as already specified. Preserve ADR 0004's other decisions on independent source coverage, shared pair-and-direction memory, due scheduling, mixed-mode direction and outcome-sensitive stability.

## Consequences

- An early exit cannot consume a word's first-exposure opportunity; a smaller next round can resume as many pending words as fit without duplicates.
- Assignment and completed-question feedback need distinct, durable state transitions so an unfinished or never-shown question can be rescheduled, including after restart.
- Previously assigned-but-unfinished questions must not update memory or be treated as completed; the review schedule's existing due and new-word priorities apply after pending words.

## Clarification (2026-09-28): Pending word identity and old data

“待练”按来源与英文—释义词对记录一条状态，而不是按每次排题记录多条待完成题。并行设备或重开新轮给同一词再次排题时，以同步重放的确定顺序最后生效的排题身份代表该词的待练状态；较旧身份的完成不能清掉最新待练状态，最新身份完成则解除待练。旧版本已经排题但未提交的记录不追溯恢复；保留已有历史学习数据，新规则从后续排题开始生效。
