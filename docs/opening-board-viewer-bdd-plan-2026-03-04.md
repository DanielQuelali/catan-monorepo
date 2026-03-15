# Opening Board Viewer BDD Specification Plan (2026-03-04)

Status: Planned  
Owner: Engineering  
Quality target: Behavior-first scenarios executed primarily against exported module contracts.

## 1. Product Intent (Outside-In)

Primary user:
- Analyst evaluating opening boards and white holdout recommendations.

User goals:
- Find the right sample quickly.
- Understand board context and opening placements.
- Compare top recommendations confidently.
- Trust results when data updates or artifacts are missing.

Non-goals for this BDD step:
- Choosing the future UI framework (`TBD`).
- Rewriting game logic or changing holdout semantics.

## 2. Code Reality Check (Current State)

Current implementation facts:
1. `apps/opening-board-viewer/app.js` is a monolith (~1402 lines).
2. It currently exposes no `export`ed module API.
3. Domain logic, parsing, state transitions, and DOM orchestration are tightly coupled.

Implication:
- We cannot satisfy Ian Cooper's rule ("test the public API, i.e., module exports") without first introducing explicit exported contracts.

## 3. Testing Policy (Ian Cooper Rule, Applied)

Mandatory policy:
1. Every behavior scenario must execute against a public module export first.
2. Browser/UI tests are secondary integration confidence checks, not the primary behavior-spec vehicle.
3. No scenario may be validated exclusively through DOM traversal if an exported contract can express it.

Consequence:
- First implementation step is architectural: extract and publish module exports for behavior boundaries.

## 4. Framework Decision (Revised for API-First BDD)

Primary BDD runner:
- `@cucumber/cucumber` for feature execution against module exports in Node.

Secondary runner:
- Playwright for thin UI smoke and wiring checks.

Reason for revision:
- Applying Ian Cooper's guidance strictly means we should execute BDD scenarios against module contracts, not browser internals.

## 5. Public Module API to Introduce

Target module: `apps/opening-board-viewer/src/public-api.ts`

Planned exported contracts:
1. `parseSampleCatalog(indexText: string): SampleCatalogResult`
2. `resolveSelection(previous: SelectionState, catalog: SampleCatalog): SelectionState`
3. `deriveBoardViewModel(board: BoardData, placements: Placements): BoardViewModel`
4. `resolvePlacements(sample: SampleRecord, state: StateData | null): Placements`
5. `summarizeHoldout(csvText: string): HoldoutSummaryResult`
6. `selectHoldoutPreview(base: Placements, preview: HoldoutBranch | null): PreviewOverlay`
7. `shouldReload(previous: SnapshotFingerprint, next: SnapshotFingerprint): boolean`
8. `resolveRuntimeRoots(locationHref: string): RuntimeRoots`
9. `toUserFacingFailure(error: unknown, context: FailureContext): ViewerFailure`

Contract rules:
1. Exports are pure or effect-light orchestrators with explicit inputs/outputs.
2. Exports never require DOM to compute behavior outcomes.
3. Browser adapter code must call these exports; tests call the same exports directly.

## 6. Artifact Strategy and Portability

Long-lived artifacts:
- `.feature` files (domain language).
- Module export signatures and return contracts.
- Contract tests tied to exports.

Swap-cost artifacts:
- Step glue implementation.
- Runner/hook configuration.
- Reporter output wiring.

Switching expectation:
- Fast for scenarios and domain vocabulary.
- Moderate for step/hook/report plumbing.

## 7. Planned Folder Structure and File Count

Target root: `apps/opening-board-viewer`

```text
apps/opening-board-viewer/
  package.json
  package-lock.json
  tsconfig.json
  cucumber.mjs
  vitest.config.ts
  playwright.config.ts
  src/
    index.ts
    public-api.ts
    domain/
      sample-catalog.ts
      navigation.ts
      board-view-model.ts
      placement-provenance.ts
      holdout-summary.ts
      holdout-preview.ts
      freshness-policy.ts
      runtime-roots.ts
      failures.ts
    adapters/
      browser-ui.ts
      fetch-gateway.ts
      clock.ts
  bdd/
    README.md
    features/
      sample-discovery.feature
      sample-navigation.feature
      board-legibility.feature
      placement-provenance.feature
      holdout-ranking.feature
      holdout-comparison.feature
      freshness.feature
      configuration.feature
      failure-communication.feature
      operability.feature
    steps/
      sample.steps.ts
      navigation.steps.ts
      board.steps.ts
      placement.steps.ts
      holdout.steps.ts
      freshness.steps.ts
      configuration.steps.ts
      failure.steps.ts
      operability.steps.ts
    support/
      world.ts
      api-harness.ts
      fixtures.ts
  tests/
    contract/
      sample-catalog.contract.spec.ts
      navigation.contract.spec.ts
      board-view-model.contract.spec.ts
      placement-provenance.contract.spec.ts
      holdout-summary.contract.spec.ts
      holdout-preview.contract.spec.ts
      freshness-policy.contract.spec.ts
      runtime-roots.contract.spec.ts
      failures.contract.spec.ts
    smoke/
      viewer-smoke.spec.ts
      recommendation-smoke.spec.ts
```

Planned file count: **54 files**

## 8. Scenario Catalog (Titles Only, No Full Specs)

### 8.1 Feature: Sample Discovery

1. Viewer opens on the first available sample.
2. Viewer displays effective data and analysis roots for verification.
3. Viewer communicates when no samples are available.
4. Viewer communicates when sample index data is unreadable.
5. Viewer keeps context when refreshed data still contains current sample id.
6. Viewer keeps context by board identity when sample id is unavailable.

### 8.2 Feature: Sample Navigation

7. Analyst can move to previous sample when one exists.
8. Analyst can move to next sample when one exists.
9. Analyst cannot navigate before the first sample.
10. Analyst cannot navigate past the last sample.
11. Analyst can jump directly to any sample from the sample picker.
12. Keyboard left navigation moves to previous sample.
13. Keyboard right navigation moves to next sample.
14. Navigation feedback reports current sample position.

### 8.3 Feature: Board Legibility

15. Board view shows all land tiles with readable resource identity.
16. Number tokens appear on all non-desert tiles.
17. Desert tile is shown without a production number.
18. Token lettering is stable for a given board.
19. Port opportunities are visible at coastline positions.
20. Board remains fully legible after dynamic content changes.

### 8.4 Feature: Placement Provenance

21. Opening placements prefer state-file placements when valid.
22. Opening placements fall back to sample-index placements when state is absent.
23. Opening placements fall back to sample-index placements when state is invalid.
24. Placement panel matches rendered placements.
25. Metadata panel identifies sample and seed provenance.
26. Port summary ordering matches board port ordering.

### 8.5 Feature: Holdout Ranking

27. Holdout panel communicates loading state while recommendations are prepared.
28. Holdout panel communicates unavailable analysis for a sample.
29. Holdout panel communicates when no usable recommendation rows exist.
30. Recommendation ranking ignores rows outside holdout scope.
31. Recommendation strength favors recommendations backed by larger evidence sets.
32. Recommendation strength still computes when evidence weights are absent.
33. Recommendations are grouped by settlement pair regardless of order.
34. Group ranking reflects strongest expected white outcome.
35. Group ranking is deterministic for identical input data.
36. Recommendation list is limited to top five groups.
37. Branches inside each group are shown strongest to weakest.
38. Recommendation cards use enhanced metric view when enhanced data exists.
39. Recommendation cards use legacy view when enhanced data is absent.

### 8.6 Feature: Holdout Comparison

40. Previewing a recommendation shows that recommendation on the board.
41. Ending recommendation preview restores baseline board view.
42. Keyboard focus provides preview parity with pointer interactions.
43. Branch preview overrides group-level preview.
44. Branch preview exits cleanly when branch preview intent ends.
45. Active emphasis matches currently previewed recommendation.
46. Analyst can reveal additional branch alternatives.
47. Preview includes follower placements for selected branch.

### 8.7 Feature: Freshness

48. Viewer updates when available sample set changes.
49. Viewer updates when active sample board changes.
50. Viewer updates when active sample placement state changes.
51. Transient refresh failures preserve current analyst view.
52. Refresh does not unexpectedly reset current sample context.

### 8.8 Feature: Configuration

53. Custom data root changes sample asset resolution.
54. Custom analysis root changes holdout asset resolution.
55. Mixed default/custom roots resolve sample and analysis assets correctly.
56. Effective runtime roots are visible to analyst.

### 8.9 Feature: Failure Communication

57. Unreadable board data surfaces a user-visible error.
58. Unreadable state data degrades gracefully without blocking board view.
59. Malformed holdout rows are ignored without breaking recommendation display.
60. Recommendations remain correct when source fields contain quoted commas.
61. Recommendations remain correct when source fields contain escaped quotes.
62. Invalid follower placement tokens are safely ignored.

### 8.10 Feature: Operability

63. Recommendation cards are keyboard reachable.
64. Branch entries are keyboard reachable.
65. Focus indication is visible for recommendation interactions.
66. Loading and steady-state status messages are visible.
67. Keyboard navigation remains bounded by available sample range.

## 9. Scenario-to-Export Mapping (Mandatory)

Mapping rules:
1. Every scenario in Section 8 references one primary export from Section 5.
2. No scenario can be marked complete without a passing contract test in `tests/contract/*`.
3. UI smoke tests validate wiring only for selected high-risk paths.

Primary mapping:
- 8.1 + 8.2 -> `parseSampleCatalog`, `resolveSelection`, `resolveRuntimeRoots`
- 8.3 + 8.4 -> `deriveBoardViewModel`, `resolvePlacements`
- 8.5 + 8.6 -> `summarizeHoldout`, `selectHoldoutPreview`
- 8.7 -> `shouldReload`
- 8.8 -> `resolveRuntimeRoots`
- 8.9 -> `toUserFacingFailure`, `summarizeHoldout`
- 8.10 -> `resolveSelection`, `selectHoldoutPreview`

## 10. One Example Scenario (Only Example)

```gherkin
Scenario: Analyst sees stable top recommendations for the same holdout data
  Given an analyst has holdout data for a sample
  When recommendations are summarized for that sample
  Then the top recommendation ordering is consistent for the same input
  And only the top five recommendation groups are shown
```

## 11. Delivery Sequence

1. Extract module boundaries from `app.js` into `src/domain/*`.
2. Publish `src/public-api.ts` exports listed in Section 5.
3. Add contract tests for each export in `tests/contract/*`.
4. Implement `.feature` files and step bindings against exported API.
5. Add minimal Playwright smoke checks for browser wiring.
6. Enable CI gates: contract tests + feature tests + smoke tests.
7. Freeze feature vocabulary before UI framework migration work.

## 12. BDD Quality Gates

A scenario is merge-ready only if:
1. It names an analyst outcome, not implementation mechanics.
2. It is executable against a module export.
3. It asserts externally visible behavior.
4. It remains valid if DOM structure changes.
5. It has contract-level coverage before smoke-level coverage.

## 13. Authoring and Review Workflow (Three Amigos)

For each feature:
1. Product, QA, and engineering run example mapping.
2. Rules, examples, and open questions are captured explicitly.
3. Only validated examples become scenario titles.
4. Each approved scenario is linked to one export contract.
5. Deferred examples are logged as backlog, not hidden in steps.

## 14. Assumptions and Defaults

1. Browser target for smoke checks: Chromium.
2. Test data source: `data/opening_states` and `data/analysis/opening_states`.
3. CI profile: headless by default.
4. UI framework migration remains out of scope for this BDD phase.
5. Existing behavior is baseline truth unless explicitly re-specified.
