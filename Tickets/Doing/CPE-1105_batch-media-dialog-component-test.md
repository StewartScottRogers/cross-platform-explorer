---
id: CPE-1105
title: "QA: BatchMediaDialog component test (burn down the dialog visual-verification debt)"
type: chore
component: Frontend
priority: medium
status: Doing
tags: ready
created: 2026-07-26
epic: CPE-396
---

## Summary
QA-Architect burndown. `src/lib/components/BatchMediaDialog.svelte` (CPE-1093) shipped with **no component
test** — its op-building / plan-preview / streamed-apply logic is only "human-verified looks-good" on the
installed build (`.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` CPE-1093 row). Add a jsdom component test
(like `AgentTimeline.test.ts` / other Svelte component tests in the repo) that exercises the dialog end-to-end
with the backend commands mocked, so the interaction logic is pinned by CI and can't silently regress. This is
robust (NOT WebView2/gui-smoke) — pure jsdom.

## Context (verified)
- `src/lib/components/BatchMediaDialog.svelte` — dumb dialog: `paths: string[]` prop in, `apply`/`cancel`
  events out. Builds an ordered `MediaOp[]` via an add-op dropdown + param field + pill list; debounced
  (~200ms) generation-tokened `commands.batchMediaPlan(job, paths)` preview; Apply streams
  `batch_media_execute_stream` over `createChannel<OpResult[]>()` with a progress bar; validate errors block
  Apply; non-destructive checkbox.
- Pure helpers already tested in `src/lib/batchMedia.test.ts` (`mediaOpLabel`, `opsToJob`, `partitionEligible`,
  `progressPercent`, `canBatchTransform`). This ticket covers the **component**, not those.
- Look at how the repo mocks `commands`/`createChannel`/`rawInvoke` in existing component tests (grep
  `vi.mock`, `@testing-library/svelte` usage). Mirror the established mocking pattern — do NOT invent a new one.

## Design (buildable)
Add `src/lib/components/BatchMediaDialog.test.ts` (jsdom + @testing-library/svelte + vitest fake timers where
needed) covering the dialog's user-facing logic with `commands.batchMediaPlan` + the execute-stream mocked:
1. **Op building** — adding a Resize/Convert op appends a pill with the right label; removing a pill works;
   ops preserve order; Add is disabled for an incomplete op (empty Convert ext / out-of-range compress).
2. **Plan preview** — after adding ops, the (debounced) `batchMediaPlan` mock is called with the built job +
   paths, and the returned `PlannedItem[]` render as `input → output — summary` rows; a stale/superseded plan
   doesn't overwrite a newer one (generation token) — assert with fake timers.
3. **Validation blocks Apply** — a `validate`-style `Err` (mock `batchMediaPlan` returning an error) surfaces
   and disables Apply; Apply is also disabled with 0 ops / 0 planned items.
4. **Apply + streamed progress** — clicking Apply calls the execute-stream mock with `{items, job, onResult}`;
   feeding `OpResult[]` batches into the mocked channel advances a `done/total` + `failed` counter / progress
   bar (NaN-safe at total 0); on completion an `apply` event is dispatched. Assert the channel subscription is
   torn down (no leak) on completion/cancel/destroy.
5. **Non-destructive toggle** flows into the built `BatchJob.non_destructive`.

## ⚠ Notes / guardrails
- Pure test addition + (if strictly needed) tiny testability tweaks to the component (e.g. a `data-testid` or
  exported const) — keep component changes minimal and behaviour-preserving. No new deps (reuse the repo's
  existing test stack). No backend change.
- Mirror the existing component-test mocking of `../invoke` / `commands` / `createChannel` — confirm the seam
  before writing.
- When green, flip the `MANUAL-TEST-BURNDOWN.md` CPE-1093 row's *logic* portion to automated (leave pure
  pixel/theme "feel" as residual human debt, since a jsdom test can't verify visual rendering).

## Acceptance Criteria
- [ ] `BatchMediaDialog.test.ts` exercises op-building, debounced+gen-tokened plan preview, validation-blocks-
      Apply, streamed-apply progress + completion event, channel teardown, and the non-destructive toggle —
      with the backend commands mocked; all assertions genuine (not smoke).
- [ ] `npm run check` clean; `npm test` green (report the added count); no new deps; component changes (if any)
      minimal + behaviour-preserving.
- [ ] `MANUAL-TEST-BURNDOWN.md` CPE-1093 logic portion flipped to automated; pixel/feel left as residual debt.

## Work Log
2026-07-26 (workshift, QA Architect) — Filed to burn down the batch-media dialog manual-verification debt
(the CPE-1093 UAT explicitly noted no component test exists). Pins the dialog interaction logic in CI without
flaky browser automation.
