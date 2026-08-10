---
id: CPE-1526
title: "Compact density: settings model + App wiring seam (foundation)"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1488
created: 2026-08-09
---
## Context
CPE-1488 ("Compact / dense view mode") is being activated for this sprint. The epic's own brief calls
this "the single most on-purpose candidate from the survey — it *is* fast/small/predictable made
visible": a `comfortable` (default, unchanged) / `compact` density toggle, persisted, that tightens row
pitch and chrome. Nothing today models density at all. This ticket is the **foundation slice**: the
persisted setting + the seam that threads it through the app, with **no visual change yet** — the value
is added but not yet consumed by the row/chrome rendering (that's CPE-1527/1528). It unblocks the other
three tickets, which all need this seam to exist before they can read a `density` value.

## Scope
- A new `density: "comfortable" | "compact"` setting in `src/lib/settings.ts`, following the file's
  existing pattern exactly (see `KEYS.view`/`loadView`/`saveView` and the `isView` validator for the
  shape to copy): add `KEYS.density = "cpe.density"`, an `isDensity` validator, `loadDensity()` (default
  `"comfortable"`) and `saveDensity(v)`.
- Thread it into `src/App.svelte`: a reactive `let density = settings.loadDensity()` (mirrors the
  existing `let dualPane = settings.loadDualPane()` pattern at `App.svelte:322`) and a `setDensity(d)`
  handler that calls `settings.saveDensity` and updates the variable. Pass `density` as a prop into the
  components that will consume it in follow-on tickets (`ExplorerPane`/`FileList`, `NavToolbar`,
  `TabBar`, `Sidebar`) — **additive prop plumbing only**; those components ignore the prop until
  CPE-1527/1528/1529 land, so this ticket produces zero visible/behavioral change.
- Do **not** touch any CSS, row rendering, or toolbar/tabbar/sidebar markup here — that's explicitly out
  of scope for this ticket (owned by CPE-1527/1528/1529) to keep this slice's conflict surface small.

## How
- Copy the existing settings.ts idiom (typed accessor + validator + `KEYS` entry + a short doc comment
  explaining default/off-cost, matching the style of every other entry in that file).
- `resetSettings()` already resets the whole `state` object, so a new key needs no special-case handling
  there.
- No new dependency. No backend involvement — pure frontend/settings.json, consistent with the epic's
  "Notes: pure frontend/CSS + a settings flag; no backend, no new deps."
- Delete-test: removing/ignoring the `density` value must degrade cleanly to `"comfortable"` (today's
  only behavior) — never crash, never change layout when the value is absent/corrupt.

## Verify
`npm run check`. Add/extend `src/lib/settings.test.ts` with unit tests: default is `"comfortable"`,
`saveDensity("compact")` round-trips through `loadDensity()`, and an invalid/corrupt stored value
degrades to the default (mirrors the existing `isView`-style tests already in that file). Fully headless
— no GUI verification needed for this slice since it has no visible effect yet.

## Notes
**Conflict surface:** `src/lib/settings.ts`, `src/lib/settings.test.ts`, `src/App.svelte` (additive
variable + prop-threading only — keep the diff small and mechanical so CPE-1527/1528/1529 don't have to
rebase around unrelated App.svelte churn). This is the **prerequisite** for all three sibling tickets
(CPE-1527, CPE-1528, CPE-1529) — land it first; the Foreman should not dispatch the others until this
merges, since they all import `loadDensity`/the `density` prop it introduces.
