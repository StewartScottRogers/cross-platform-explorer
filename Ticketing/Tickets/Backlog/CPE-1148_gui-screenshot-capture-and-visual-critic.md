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
- [ ] `gui-smoke` has a `snap(name)` helper; a full run writes PNGs of the key surfaces to a gitignored
      `.screenshots/` dir; existing specs still pass; `npm run check` green.
- [ ] A short doc explains how a worker/critic captures + finds screenshots.
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
