---
id: CPE-1148
title: "Workshift visual self-check: gui-smoke screenshot capture + a Visual-Critic role that judges them"
type: feature
component: Testing
priority: high
status: Backlog
tags: ready
created: 2026-07-30
epic: CPE-579
---

## Summary
User-requested (2026-07-30). GUI-verify keeps bouncing to the user for *visual* judgments (button placement,
icon choice, clipping, alignment) because the workshift has no way to *see* its own GUI work. Give it eyes:
**(A)** the `gui-smoke` harness captures **screenshots** of the real built app at each surface/state, and
**(B)** a new **Visual Critic** workshift role reads those screenshots and judges the work against the design
conventions + good taste — the gauntlet's **visual leg** (Reviewer = code, UAT = behaviour, **Critic =
looks/feel**) — so the user is asked **minimally** (only genuine subjective taste, or things a screenshot
can't show).

## Part A — screenshot capture (app-repo, this ticket's build)
- Add a screenshot helper to `gui-smoke` (WebdriverIO's `browser.saveScreenshot(path)`): a small
  `snap(name)` util that writes a PNG to a known artifacts dir (e.g. `gui-smoke/.screenshots/<name>.png`,
  gitignored — artifacts, not committed).
- Have each existing smoke spec `snap` the surface it drives (open-dir, organize, instant-search, batch-media,
  replay, cost-history, and the column view) at its key state, so one `npm test` run leaves a **gallery of the
  app's main screens** on disk. Keep the specs' existing assertions + non-blocking (`continue-on-error`).
- Add a `snap`-on-demand convention + a short doc (a `gui-smoke/README` note or `docs/design/` addition):
  how a worker captures the surface it changed, and where the Critic finds the PNGs.
- `npm run check` green; running the real gui-smoke produces the PNGs; the `.screenshots/` dir is gitignored.

## Part B — the Visual Critic role (process; Foreman authors in the workshift skill + a memory)
*(Not app code — a change to `.claude/commands/workshift.md` + a memory. Done by the Foreman alongside Part A;
listed here so the whole feature is one tracked unit.)*
- Add **Visual Critic** to the crew (per-ticket, for GUI-affecting tickets): an independent sub-agent that
  reads the gui-smoke screenshots + the design standards (MENUS.md, TABS.md, pill/tick-tack reflow rules,
  light-theme palette, alignment/spacing) + the ticket's visual intent, and returns **`VISUAL PASS`** or
  **`VISUAL CHANGES`** with concrete defects (clipped / misaligned / misplaced / wrong-or-ambiguous glyph /
  off-theme).
- Wire it into the per-ticket gauntlet: for a GUI change, the worker's gui-smoke run captures screenshots →
  the Critic judges them → defects route back to the worker (bounded, like the failure circuit breaker) →
  escalate to the **user only** for (a) a genuinely subjective taste/preference call (and then as a concrete
  pick-list, not an open question), or (b) something a screenshot can't reveal (interaction feel / animation
  cadence / real-hardware behaviour).
- Update the escalation policy: the Critic + screenshots become the routine catch-obvious-visual-defects loop;
  the build→deploy→run-with-user step becomes a **final confirmation / interaction-feel** check, not the
  every-iteration eyes-on it is today.
- Write a memory (feedback) capturing this as the standing way.

## Acceptance Criteria
- [x] `gui-smoke` has a `snap(name)` helper; a full run writes PNGs of the key surfaces to a gitignored
      `.screenshots/` dir; existing specs still pass; `npm run check` green.
- [x] A short doc explains how a worker/critic captures + finds screenshots.
- [ ] `.claude/commands/workshift.md` gains the **Visual Critic** role in the crew table, the per-ticket
      gauntlet (3rd visual leg), and the minimal-escalation policy.
- [ ] A memory records the screenshots + Visual-Critic standard.
- [ ] (Nice-to-have, if cheap) a first real exercise: a Critic sub-agent reads one captured screenshot and
      returns a PASS/defects verdict, proving the loop end-to-end.

## Notes
- Filed under epic CPE-579 (self-maintaining quality infra) — it's the visual rung of the manual-test
  burndown (row #1 headless GUI / row #3 visual-theme regression).
- Directly targets the pain from the CPE-1147 button saga: the clip + right-vs-left placement are
  screenshot-visible (Critic catches them with zero user involvement); only the pure icon *preference*
  legitimately reached the user.

## Work Log
2026-07-30 (workshift, Worker) — Implemented Part A only (screenshot-capture infra); Part B (the Visual
Critic role in `.claude/commands/workshift.md`) is the Foreman's, still pending — ticket left in Backlog.

- Added `gui-smoke/lib/snap.ts`: `snap(name)` creates `gui-smoke/.screenshots/` (if missing) and calls
  `browser.saveScreenshot(...)` to write `<name>.png` there. Deliberately swallows its own errors — a
  screenshot is an observability artifact, not an assertion, so a capture failure never fails or masks a
  spec's real checks.
- Wired `snap(...)` into all 6 existing smoke specs, each called AFTER that spec's own assertions (so a
  failed assertion still leaves a shot of the failing state) and before any cleanup click: `open-dir.png`
  (`open-dir.smoke.ts`), `organize-dialog.png` (`organize.smoke.ts`), `instant-search.png`
  (`instant-search.smoke.ts`), `batch-media-dialog.png` (`batch-media.smoke.ts`), `replay-tab.png`
  (`replay.smoke.ts`), `cost-history.png` (`cost-history.smoke.ts`). No existing assertion or the
  non-blocking (`continue-on-error`) CI behaviour was touched. No standalone "column view" spec exists yet
  in `gui-smoke/specs/` (grepped — not present), so no 7th snap was added; the doc below explains how to
  add one when/if that spec lands.
- `gui-smoke/tsconfig.json`: added `lib/**/*.ts` to `include` so the new helper type-checks.
- `gui-smoke/.gitignore`: added `.screenshots/` (run artifacts, never committed).
- `gui-smoke/README.md`: new "Screenshots for the Visual Critic (CPE-1148 Part A)" section — a table of
  every PNG name → spec → surface, the gitignore/non-fatal-capture rationale, and how a worker captures
  the surface it changed (reuse an existing spec's `snap()` call, or add a new spec following the existing
  pattern if none exists yet).

Verified:
- `npm run check` (root, svelte-check): 0 errors, 0 warnings.
- `cd gui-smoke && npm ci && npm run typecheck` (`tsc --noEmit`): clean.
- **Ran the real gui-smoke harness locally** (tauri-driver + msedgedriver already installed in
  `~/.cargo/bin` on this machine): `npm run build` (frontend) → `npm run tauri build -- --no-bundle`
  (release binary, ~2m25s compile) → `cd gui-smoke && npm test`. All 6 spec files passed (9 `it`s total,
  0 failures, ~46s). Confirmed `gui-smoke/.screenshots/` was populated with all 6 expected, non-empty
  PNGs: `open-dir.png` (78,252 B), `organize-dialog.png` (76,382 B), `instant-search.png` (82,773 B),
  `batch-media-dialog.png` (81,225 B), `replay-tab.png` (109,929 B), `cost-history.png` (119,521 B).
  Visually opened two of them (`open-dir.png`, `organize-dialog.png`) — both show the real rendered app
  state (directory listing with the seeded fixtures; the auto-organize dialog's by-extension grouped
  preview), not blank/garbage frames.
- Reverted incidental `package-lock.json`/`src-tauri/Cargo.toml` diffs picked up by `npm install`/the
  release build (a stale lockfile version field) before committing — out of scope for this ticket.

Landed as branch `cpe-1148a-gui-screenshots`, PR opened against `main`. Part B (Visual Critic role +
memory) is the Foreman's separate change; this ticket stays in Backlog until both parts land.
