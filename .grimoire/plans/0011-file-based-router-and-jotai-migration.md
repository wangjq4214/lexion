# Migrate Page Navigation to File-Based TanStack Router and Shared State to Jotai

- **Input:** Conversation request and `.grimoire/adr/0005-use-file-based-tanstack-router-and-jotai-for-frontend-navigation-and-state.md`; existing behavior in `.grimoire/CONTEXT.md` and `src/App.tsx`
- **Date:** 2026-09-24
- **Status:** Proposed
- **Endpoint:** Implementation plan only; no code migration in this refine route.

## Summary

Replace `App.tsx`'s conditional page selection with TanStack Router file-based routes for setup, active practice, summary, favorites, and mistakes. Move state shared by those pages into Jotai while keeping the pure `appReducer` transition rules. Preserve the current wordbook, practice, favorites, mistakes, and error/retry flows. Block attempts to navigate away from an active practice round and point the learner to the existing exit-and-settle action. Practice/summary URLs without the corresponding in-memory round state return to setup; no round persistence is added.

Route names such as `/`, `/practice`, `/summary`, `/favorites`, and `/mistakes` below are implementation candidates, not newly established external URL requirements. Select the exact file conventions and API from the installed TanStack Router version during implementation.

## Design impact

### Router bootstrap and generated route tree

- **Action:** Modify `package.json`, `vite.config.ts`, `src/main.tsx`; create `src/routes/` files and generated route-tree output as required by the router tooling.
- **Responsibility:** Install/configure TanStack Router file-based route generation, mount a router inside the existing theme setup, and share the Astryx page frame through the root route/layout.
- **Relationships:** Route files compose the existing `PracticeSetup`, `PracticeQuestion`, `PracticeSummary`, `FavoritesList`, and `MistakesList`; Vite and TypeScript must recognize the generated tree.
- **Rationale:** A generated file route tree fulfills the requested file-based routing; handwritten route registration would not. Reuse the existing components and UI layout instead of redesigning screens. Check whether the router plugin must precede the existing StyleX/React plugins and ensure generation also works in tests/typecheck/build.

### Shared application and practice state

- **Action:** Create a focused Jotai state module (e.g. `src/state/`), modify `src/App.tsx` and `src/features/practice/practiceReducer.ts` only as needed for integration.
- **Responsibility:** Own state that crosses route boundaries (selected wordbook/source/mode/count, round phase and questions, summary, shared feedback or writes that must survive route transitions). Preserve existing reducer invariants through a reducer-backed atom or an equivalent atom action dispatch, not an untested rewrite of the state machine.
- **Relationships:** The existing `WordbookService` remains the data boundary; existing practice reducer tests remain the transition contract. Keep request identities, pending writes, and synchronous guards tied to the appropriate lifecycle; do not turn every local rendering flag or ref into global state solely for uniformity.
- **Rationale:** Jotai supplies explicit cross-page ownership while the reducer retains tested review/score semantics. Avoid one monolithic atom for all unrelated UI details or duplicate sources of truth for route versus round phase.

### Route-aware feature coordination

- **Action:** Move or extract page-specific orchestration out of `src/App.tsx` into route-facing controllers/hooks as warranted, without replacing `src/data/wordbooks.ts` or the Rust repository.
- **Responsibility:** Preserve loading/refresh/import, practice start and settling, favorite and mistake writes, delete confirmation, error/retry feedback, and timer behavior across the new route mounts.
- **Relationships:** Setup imports and selection feed practice start; practice completion feeds summary; favorites/mistakes lists query the same service. Route transitions happen only after state updates and requisite writes succeed.
- **Rationale:** Splitting by lifecycle reduces the present giant component's page coupling without expanding the domain model. The existing pure UI components can remain prop-driven.

### Active-round navigation safety

- **Action:** Add a route-level navigation blocker/guard and a visible explanation or action using existing UI components.
- **Responsibility:** Reject browser back and other router navigation that would leave an active practice round directly. The current exit-and-settle operation remains the way to leave; its normal summary transition must be permitted once settlement succeeds.
- **Relationships:** Guard state derives from the round and pending-write state; `completeQuestion("exit-and-settle")` and the reducer still own settlement. Failure keeps the learner on practice with retry rather than skipping review persistence.
- **Rationale:** URL navigation must not silently bypass the already-established review and mistake write invariants. Avoid treating history navigation as a second implementation of settlement.

## Implementation steps

### 1. Establish route generation and isolated bootstrap

- **Files or discovery point:** `package.json`, `bun.lock`, `vite.config.ts`, `src/main.tsx`, new `src/routes/__root.tsx` and index/feature route files (filenames to follow installed router conventions), `tsconfig.json` if generated files require it.
- **Change:** Add compatible TanStack Router runtime/plugin and Jotai packages; wire the file-route generator into Vite, create root layout with current `AppShell`/`Layout`/`LayoutContent`, preserve theme and Astryx CSS initialization, and instantiate a router for the app. Establish a per-test router/store factory so parallel tests do not share mutable state or browser history.
- **Outcome:** Generated tree and route modules build; landing page has the current frame and styling.
- **Verification:** `bun run typecheck`, `bun run build`; targeted router smoke test. Verify generation in a clean checkout/build, not only after dev mode has created the tree.

### 2. Extract shared state without changing practice transitions

- **Files or discovery point:** `src/App.tsx`, `src/features/practice/practiceReducer.ts`, new `src/state/*`.
- **Change:** Map every App state/ref to its owning lifecycle. Put cross-page selection and round state into Jotai and dispatch the existing reducer through an atom. Keep service/time/random test injection possible; retain guards against duplicate submissions, writes, and stale async completions. Make route and phase transitions agree when starting, settling, restarting, and returning to setup.
- **Outcome:** Only one authoritative practice phase exists; navigating between pages cannot reset live round state inadvertently.
- **Verification:** Existing `src/features/practice/practiceReducer.test.ts` and focused atom transition tests; check default selection and 1–255 count validation.

### 3. Route setup, favorites, and mistakes

- **Files or discovery point:** `src/features/practice/PracticeSetup.tsx`, `src/features/favorites/FavoritesList.tsx`, `src/features/mistakes/MistakesList.tsx`, setup/list route modules, `src/App.test.tsx`.
- **Change:** Replace `showFavorites`/`showMistakes` view flags with route navigation. Keep initial wordbook load, empty-book import, import refresh/selection, list loading/error/retry, removal and return behavior. Invalidate or discard stale list responses on route exit/re-entry; preserve any established gating from initial wordbook load instead of inventing new empty-state behavior.
- **Outcome:** Setup and lists use URL-backed pages and retain current visible interactions.
- **Verification:** Convert existing integration tests to a fresh router/store per case; assert route locations, list re-entry and stale-request behavior, empty and error states.

### 4. Route the round and summary with safe settlement

- **Files or discovery point:** practice/summary route modules, `src/features/practice/PracticeQuestion.tsx`, `src/features/practice/PracticeSummary.tsx`, extracted practice coordinator, `src/App.test.tsx`.
- **Change:** Start the round only after scheduling supplies questions, then navigate to practice; transition to summary on completed or early-settled reducer state. Block back/link/programmatic route departure while round remains active and show how to use the existing exit control. Do not navigate until required mistake/review writes resolve; on failure keep the current question and retry path. Once settled, allow summary, restart, and return-to-setup navigation. Reject direct/reloaded practice or summary URLs lacking appropriate in-memory phase by returning to setup.
- **Outcome:** No route action skips a pending review write or produces a summary from absent round state; existing timer and scoring semantics stay intact.
- **Verification:** Router integration tests for back attempt during active round, pending write and failed write, explicit exit and summary, reload/direct URL fallback, restart, and ordinary completion.

### 5. Retire conditional page switch and run regression checks

- **Files or discovery point:** `src/App.tsx`, `src/App.test.tsx`, route/store tests, `src/main.tsx`.
- **Change:** Remove superseded conditional page rendering and duplicate page flags only after each behavior has an owning route. Keep any remaining global document style/feedback/dialog host at the root if it must survive navigation. Update tests to inject `WordbookService`, `now`, and `random` without a singleton store leak.
- **Outcome:** The app renders pages through the file route tree and shared state through Jotai, without legacy navigation booleans.
- **Verification:** `bun run test`, `bun run check`, `bun run build`; manual Tauri/browser check of navigation and page styling.

## Edge cases

| Condition | Expected behavior | Owning step or verification |
| --- | --- | --- |
| Back/link navigation while practice is active, including a revealed wrong/skip answer | Remain on practice; direct learner to explicit exit-and-settle, without losing or double-writing the current review | Step 4, router integration |
| Review or mistake write pending/fails when exit is requested | Preserve existing blocked/retry path; no premature summary or duplicate submission | Steps 2 and 4, deferred-promise tests |
| Direct entry or reload on `/practice` or `/summary` with no matching round in memory | Return to setup; never construct an empty question or fabricated summary | Step 4, fresh-store tests |
| Slow favorites/mistakes/wordbook request resolves after navigation | Stale result does not overwrite newer page data or introduce an obsolete error | Steps 2 and 3, deferred-request tests |
| Start returns no entries, or no wordbook has been imported | Retain current empty/import or source-specific error flow; do not open an empty practice route | Steps 3 and 4, integration tests |
| Normal completion, early exit, restart, or back from summary | Route and reducer phase remain consistent; elapsed time and completed-question counts remain as before | Steps 2 and 4, existing regression suite |

## Verification strategy

| Behavior or risk | Method | Evidence |
| --- | --- | --- |
| Generated routes/config integration | Clean build and typecheck | Generated route imports resolve without prior dev session |
| Page navigation/state ownership | Router/store integration tests | URL and rendered page agree; no test-to-test state leak |
| Practice transitions and write safety | Existing reducer and App regression tests plus browser-back/pending-write cases | Scores/timer unchanged, write failure stays retryable, navigation cannot skip settlement |
| Wordbook and collection behavior | Converted App integration suite | Import, favorites, mistakes, refresh, empty and error paths match existing assertions |
| Layout and desktop-shell behavior | Manual browser/Tauri smoke | Existing Astryx styles and page frame still render at startup and after route changes |
| Quality gate | `bun run test && bun run check && bun run build` | All commands exit successfully |

## Risks and open questions

- **Risk:** Vite route generation/plugin ordering with StyleX and React/compiler setup; validate with a clean build before migrating all screens.
- **Risk:** Router blockers may cover in-app/history navigation differently from a full browser reload; the agreed reload fallback does not imply persisting or restoring an active round. Verify supported blocker behavior against the installed version and test both paths.
- **Risk:** Existing refs and async callbacks guard duplicate review/mistake writes; moving them into short-lived route components could drop retry state or permit duplicate actions. Preserve ownership until settlement is complete.
- **Implementation discovery:** Choose exact route filenames, generated-file policy, router APIs, and test history utilities against the installed TanStack Router version; these are tooling details, not new product decisions.
