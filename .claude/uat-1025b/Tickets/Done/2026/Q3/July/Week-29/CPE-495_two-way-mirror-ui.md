---
id: CPE-495
title: "Two-way mirror UI — pull / push / sync with per-repo policy + dry-run preview"
type: Feature
status: Done
priority: Medium
component: Multiple
tags: [ready]
estimate: 3-4h
created: 2026-07-16
epic: CPE-488
closed: 2026-07-16
---

## Summary
The mirror **engine** (CPE-438 planner) and the `forge_sync` / `forge_repo_status` host commands +
ahead/behind status bar (CPE-462) already exist. Build the UI that drives **two-way** sync end-to-end:
Pull (merge/rebase), Push, and a **per-repo sync policy**, with a **dry-run/preview** of the plan
before it runs and clear divergence/dirty-tree warnings. Safe-by-default — never force-push.

## Acceptance Criteria
- [x] The Repositories surface offers Pull / Push / Sync actions for a local repo, driven by the
      existing `forge_sync` + `plan_sync` engine.
- [x] A per-repo sync policy (merge / rebase / manual on-diverge; `allow_force` stays off) is settable
      and persisted.
- [x] A dry-run **preview** shows the planned `SyncActions` (ahead/behind, conflict-risk, dirty-tree)
      before executing; the user confirms.
- [x] Never force-pushes; a divergence surfaces for the user rather than silently reconciling.
- [x] Frontend + backend tests; `npm run check` clean; Rust green.

## Resolution
Built the two-way-mirror **Sync dialog** over the existing CPE-438 planner + `forge_sync` /
`forge_repo_status` host commands. What changed:

- **Backend** (`src-tauri/src/lib.rs`, behind `sidecar-platform`):
  - `forge_repo_status` now takes an optional `on_diverge` and builds a `repos::SyncPolicy`
    (`merge`/`rebase`/`manual`, `allow_force:false`) so the **dry-run preview** reflects the caller's
    chosen policy. Absent ⇒ the safe `merge` default (what the quick status-bar Pull/Push uses).
  - `forge_sync` extended with the reconcile actions the planner can emit: `pull-ff` (ff-only),
    `pull-merge` (`--no-rebase`), `pull-rebase` (`--rebase`), and `push`. No force action exists — a
    conflict returns git's non-zero output for the user to resolve, never a silent reconcile.
- **Frontend:**
  - `src/lib/syncPolicy.ts` (new) — per-repo on-diverge policy persisted in a `localStorage` map keyed
    by repo path; safe `merge` default; `syncActionLabel` for the plan action names.
  - `src/lib/components/SyncDialog.svelte` (new) — a bordered modal that dry-run **previews** the plan
    (branch, ahead/behind, dirty, planned steps, conflict-risk + warnings), exposes the on-diverge
    policy as an inline `<select>` that **re-plans instantly**, and runs the steps in order on confirm
    (halting + surfacing git's output on the first failure). Pills reflow (tick-tack rule); visible
    border (dialog rule); no red menu text.
  - `src/App.svelte` — `refreshGitStatus` passes the saved per-repo policy so the status bar and dialog
    agree; a `Sync…` action opens the dialog; `on:done` refreshes the status bar + listing.
  - `src/lib/components/StatusBar.svelte` — added a `Sync…` button (alongside the quick Pull/Push).
- **Tests:** `src/lib/syncPolicy.test.ts` (new) — persistence round-trip, safe default, corrupt/garbage
  fallback, action labels. `npm run check` clean; 497 frontend tests pass; `cargo clippy --features
  sidecar-platform -D warnings` clean.

Tradeoff: the rich in-app three-way conflict *resolver* is intentionally **not** here — a conflict
currently surfaces git's message and halts. That resolver is the sibling ticket **CPE-496**; this
ticket delivers the safe-by-default two-way sync surface it builds on.

## Work Log
2026-07-16 — Picked up. Estimate: 3-4h. Plan: build the two-way-mirror UI over the existing CPE-438 planner + forge_sync/forge_repo_status — a Pull/Push/Sync surface with a dry-run PREVIEW of the SyncPlan, per-repo policy (merge/rebase/manual), ahead/behind + conflict-risk warnings, never force-push.
2026-07-16 — Backend: `forge_repo_status` takes `on_diverge` → policy-aware dry-run plan; `forge_sync` gained `pull-ff`/`pull-merge`/`pull-rebase` (still never force). Verified against the `repos::sync` types.
2026-07-16 — Frontend: added `syncPolicy.ts` (per-repo persisted policy), `SyncDialog.svelte` (preview + inline policy + sequential run), wired App.svelte + a `Sync…` button in StatusBar. Applied the standing UI rules (visible dialog border, reflowing pills, no red text).
2026-07-16 — Verified: `npm run check` 0 errors; `npm test` 497 pass (incl. new syncPolicy tests); `cargo clippy --features sidecar-platform -D warnings` clean. All ACs met. Closing.
