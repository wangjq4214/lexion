# Practice Round Elapsed Time

- **Input:** Refined conversation and `.grimoire/CONTEXT.md#practice-round`
- **Date:** 2026-09-22
- **Status:** Completed

## Summary

Add a total elapsed-time indicator for each practice round. Timing begins when the first question is shown, continues across question changes, incorrect submissions, and hint use, and stops when the final answer completes the round. The practice screen shows the live cumulative duration and the completion summary preserves the final duration in minutes and seconds. This is display-only; it does not introduce a deadline or timeout.

The smallest coherent implementation keeps the timer lifecycle in `App`, where the existing setup → practice → summary transition is owned, and adds only focused formatting/timing helpers if needed for deterministic tests.

## Design impact

### Practice and summary state

- **Action:** Modify
- **Location:** `src/App.tsx`, `PracticeState`, `SummaryState`, `appReducer`
- **Responsibility:** Preserve the finalized round duration when practice transitions to the summary.
- **Relationships:** Uses the existing phase transition and answer submission flow.
- **Rationale:** The final duration belongs to the completed practice round and must remain stable after the live timer stops. Extending the existing state machine is more cohesive than introducing a separate persistence boundary.

### Round timer lifecycle

- **Action:** Modify
- **Location:** `src/App.tsx`, `App`
- **Responsibility:** Start timing when the practice phase first renders, refresh the displayed elapsed duration while practice remains active, compute elapsed time from the round start rather than counting interval callbacks, and clean up timing work when practice ends or the component unmounts.
- **Relationships:** Observes `AppState.phase`; supplies the current/final duration to practice rendering and the final submission transition.
- **Rationale:** The app component already owns the complete practice lifecycle. Deriving elapsed time from a start timestamp avoids drift when interval callbacks are delayed and ensures question changes, retries, and hints do not reset the timer.

### Duration presentation

- **Action:** Modify
- **Location:** `src/App.tsx`, practice and summary rendering
- **Responsibility:** Format and display cumulative elapsed time in minutes and seconds.
- **Relationships:** Uses the existing Astryx `Text` and layout components.
- **Rationale:** The indicator is compact supporting information inside existing screens; no new page or standalone widget is required.

### Application behavior tests

- **Action:** Modify
- **Location:** `src/App.test.tsx`
- **Responsibility:** Verify timer start, cumulative behavior, stop/finalization, formatting, and cleanup through the user-visible practice flow.
- **Relationships:** Uses Vitest fake timers and the existing mocked `WordbookService`.
- **Rationale:** The timer is coupled to visible phase transitions, so component-level tests provide the strongest regression boundary without duplicating reducer internals.

### Design decisions

- Track one timer per practice round, not one timer per question.
- Compute elapsed duration from the round start timestamp on refresh so delayed callbacks do not lose elapsed real time.
- Finalize the elapsed value as part of the last correct-answer transition so the summary cannot continue changing.
- Keep the feature in memory only; the settled requirement does not call for persistence or historical statistics.

## Implementation steps

### 1. Add deterministic duration formatting and round timing

- **Files or discovery point:** `src/App.tsx`, `App`, nearby helper declarations
- **Change:** Add a duration formatter that renders accumulated whole seconds as minutes and zero-padded seconds. Add round-start tracking and a periodic refresh active only during the practice phase. Calculate each refresh from the start timestamp, and clean up the scheduled callback whenever practice ends or the component unmounts.
- **Outcome:** The elapsed value starts at the first question, advances for the whole round, and never resets on question index, answer, error, or hint changes.
- **Verification:** Use fake time to show that the visible value advances by elapsed wall-clock time and remains cumulative after an incorrect answer, a hint, and a correct non-final answer.

### 2. Finalize elapsed time on round completion

- **Files or discovery point:** `src/App.tsx`, `AppAction`, `SummaryState`, `appReducer`, final-answer submission handlers
- **Change:** Pass the current elapsed duration through the final submission path and store it in `SummaryState`. Ensure button and Enter submission use the same duration-aware handler so both paths finalize consistently.
- **Outcome:** The completion summary owns an immutable final total and no timer remains active after the last correct answer.
- **Verification:** Complete a round by both supported submission paths, advance fake time afterward, and assert that the summary duration does not change.

### 3. Render the live and final duration

- **Files or discovery point:** `src/App.tsx`, practice progress area and completion result section
- **Change:** Add an Astryx `Text` indicator for the current cumulative duration on the practice screen and a corresponding total-duration row in the existing completion summary. Use the same formatter for both locations.
- **Outcome:** During practice, the user can see how long the current round has taken; after completion, the same final value remains visible in `分:秒` form.
- **Verification:** Assert the timer labels and formatted values through accessible text queries in `src/App.test.tsx`.

### 4. Run repository verification

- **Files or discovery point:** `package.json` scripts
- **Change:** Run the focused component tests, then the full test suite and static checks.
- **Outcome:** The timer behavior is covered without regressions to practice, imports, linting, or types.
- **Verification:** `bun test src/App.test.tsx`, `bun run test`, and `bun run check` all pass.

## Edge cases

| Condition | Expected behavior | Owning step or verification |
| --- | --- | --- |
| An incorrect answer is submitted | Error handling proceeds and the same round timer continues without resetting | Step 1 component test |
| A spelling hint is revealed | Hint count/display changes while the same timer continues | Step 1 component test |
| A correct non-final answer advances to the next question | The question changes but elapsed time remains cumulative | Step 1 component test |
| The final answer is submitted between periodic refreshes | The final duration is computed from the round start at submission rather than relying only on the last rendered tick | Step 2 component test |
| The round completes or the app component unmounts | Scheduled timer work is cleaned up; the summary value remains fixed | Step 1 and Step 2 tests |
| Elapsed time crosses a minute boundary | Formatting changes from `0:59` to `1:00` and seconds remain zero-padded | Step 1 formatter/component test |
| A new round starts from the summary | A fresh timer starts at `0:00`; the previous round duration is not reused | Step 1 and Step 2 component test |

## Verification strategy

| Behavior or risk | Method | Evidence |
| --- | --- | --- |
| Timer starts with the first rendered question | Component test with fake timers | Practice screen initially shows the elapsed-time label and `0:00` |
| Timing is total for the round | Component test across retries, hints, and question advancement | Value continues increasing and never returns to zero until a new round |
| Delayed callbacks do not cause drift | Component test that advances fake wall-clock time | Display reflects timestamp difference, not callback count |
| Final duration stops changing | Component test after final correct answer | Advancing fake time leaves summary text unchanged |
| Button and Enter submission agree | Component tests through both interactions | Either path reaches a summary with a finalized duration |
| Minute/second formatting is stable | Focused helper or component assertions | Boundary values render as `0:00`, `0:59`, and `1:00` |
| Existing behavior remains valid | Full automated suite and static checks | Existing practice/import tests plus `bun run test` and `bun run check` pass |

## Affected files

| File | Action | Purpose |
| --- | --- | --- |
| `src/App.tsx` | Modify | Own the round timer lifecycle, finalize elapsed state, and render live/final duration |
| `src/App.test.tsx` | Modify | Cover cumulative timing, transitions, formatting, and cleanup |

## Risks and open questions

- **Risk:** Interval callbacks can be throttled by the runtime. Mitigate by deriving elapsed duration from the stored start timestamp rather than incrementing a counter per callback.
- **Risk:** Separate button and Enter handlers could finalize different values. Mitigate by routing both through one submission function.
- **Risk:** Tests using fake timers can accidentally leave scheduled work active. Verify cleanup and restore real timers after each timing test.
