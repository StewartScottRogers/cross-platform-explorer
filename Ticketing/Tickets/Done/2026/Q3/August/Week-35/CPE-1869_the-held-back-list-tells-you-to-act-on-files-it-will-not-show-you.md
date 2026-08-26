---
id: CPE-1869
title: the held-back list tells you to delete files it will not show you
type: task
priority: Medium
status: Done
tags: ready
estimate: M
created: 2026-08-23
closed:
---

## Problem

CPE-1845's revert panel names up to **8** held-back paths, then prints "and N more". Its next step, in
the permanent cases, says **"delete these files yourself if you want them gone"**.

At 200 held-back deletions the user is told to act on a set they can see 4% of.

Whether that matters depends entirely on which hold-back fired:

- **Empty checkpoint** — survivable. The held-back set is literally everything in the folder, so the user
  can see it in the file pane.
- **Unrestorable key** — **not** survivable. The set is "everything added since the checkpoint", which is
  not derivable from anything on screen and appears nowhere else in the app.

So the advice is actionable in one case and a dead end in the other, with identical wording and an
identical 8-name preview.

## What it needs

Not a bigger cap. Both the UAT and the worker landed on the same answer independently: **8 is a fine
preview provided the full list is retrievable.** What is missing is an affordance —

- copy the full list to the clipboard, or
- reveal/select the held-back paths in the file pane, or
- write them to a file the user can open.

The file-pane route is the strongest, because the user's next action is deleting them.

## Acceptance criteria

- [ ] The full held-back set is obtainable without re-running the revert. Say which affordance you chose
      and why.
- [ ] The permanent-case next step points at that affordance rather than at a list the user cannot see.
- [ ] The 8-name preview stays. Do not replace it with a scrolling list of 200 — the count and the reason
      are what the user needs first, and CPE-1845 measured what 200 repeated lines cost.
- [ ] Check the alias/collision case does **not** get a delete affordance. Those files **are** the
      checkpoint's own content under another spelling; deleting them destroys it. CPE-1845's docs carry
      the fourth bullet for exactly this reason.
- [ ] Red-proof each new test with the minimal realistic change, observe red, revert, record the line.
- [ ] Assert the fixture is live before asserting the harm. Fold the check into helpers rather than per
      test — that is what fixed CPE-1844 after its liveness claim inverted, and CPE-1845's own first draft
      of a test passed with the fix disabled because the fixture armed a different branch.

## Notes

Recorded by CPE-1845's worker and independently by its UAT, both concluding it is a new UI surface rather
than a wording change. Written into `MAX_LISTED`'s doc comment so the next person to touch the cap finds
the reasoning before changing the number.

One limit that will apply here too: jsdom applies no component CSS under this project's vitest config, so
nothing you write can check layout, ordering on screen, or visibility. CPE-1859 built a real-render
harness (`scripts/dev-harness/statusbar-notice`) for exactly this gap — reuse it rather than asserting on
markup and calling it verified.

Related: CPE-1845 (the panel and the typed outcome), CPE-1823 (the stand-down that produces the
hold-backs), CPE-1847 (the empty-checkpoint case).

## Work Log (2026-08-25)

### Design decision: copy-to-clipboard, not reveal-in-pane or write-to-file

Chose **copy-to-clipboard** as the affordance, over the other two options the ticket named:

- **Reveal/select in the file pane** is the strongest affordance in principle (the user's next action IS
  deleting them), but `RevertOutcomePanel.svelte` is one component shared across three hosts with three
  different relationships to "a pane": `CheckpointDialog` is palette-driven and has no file pane at all;
  `AgentTimeline` and `CopilotDialog` each have their own, unrelated to each other. Worse, the held-back
  set (everything added since the checkpoint) can span many subdirectories under the revert root, which
  no single directory-pane view shows flatly — there is no one screen to "reveal into" for the case that
  actually needs it.
- **Write to a file** adds a save-dialog round trip and a throwaway file to clean up, for a list most
  users want to read once and act on immediately.
- **Copy to clipboard** needs no navigation, no new window, and behaves identically from all three hosts.
  The paths land wherever the user actually wants to work through them — a search box, a terminal, a text
  editor — rather than only inside this app. Reused the existing `formatPathsForClipboard` helper
  (`src/lib/format.ts`) so the format matches Explorer's own "Copy as path" (quoted, one per line), the
  same convention already used elsewhere in the app.

Kept the 8-name preview exactly as CPE-1845 shipped it — this ticket is explicit that the cap itself was
never the bug.

### The two cases had to be told apart structurally, not by wording

The frontend (`revertHoldBack.ts`) already reads `outcome` discriminants only, never `error`/`reason`/
`next_step` text (that's CPE-1845's own rule) — but both "delete these yourself" (empty checkpoint /
unrestorable key / permanent write refusal) and "nothing needs doing" (alias/collision) share the exact
same `HeldBackOutcome::HeldBackByCheckpoint` discriminant. There was no existing field to gate a new
affordance on without violating that rule. Added one: `HeldBack::advises_manual_delete` (`revert_engine.rs`)
→ `HeldBackSummary::advises_manual_delete` (`checkpoint_store.rs`) → `RevertSummary.advisesManualDelete`
(`revertHoldBack.ts`) → `RevertOutcomePanel.svelte`'s `showCopyAffordance`. `true` for the three "go delete
these" branches, `false` for the alias/collision hold-back (paths ARE the checkpoint's own content — a
delete affordance there would be the bug) and for the retryable hold-back (nothing needs deleting yet,
re-running is the real next step).

### What was verified

- **Rust**: `cargo test` on `cpe-server` — full suite green (2383 passed). `cargo clippy --all-targets
  -D warnings` clean in both feature modes (plain and `--features specta`).
- **Red-proofed** three new/extended assertions the minimal realistic way (flipped one boolean literal at
  the exact construction site, ran, observed the new assertion fail with the real `HeldBack` debug output
  in the panic message, reverted):
  - Alias/collision branch flipped `false`→`true`: `cpe_1823_a_delete_that_resolves_onto_a_checkpoint_entry_is_held_back`'s new assertion failed.
  - Permanent write-refusal branch flipped `true`→`false`: `cpe_1845_a_permanent_write_refusal_is_never_reported_as_retryable`'s new assertion failed.
  - Frontend: dropped the `advisesManualDelete` gate from `showCopyAffordance` (`summary.heldBack > 0`
    only) — both new "does NOT offer the copy affordance" tests in `CheckpointDialog.test.ts` failed
    (button appeared where it must not).
  All three reverted after observing red; final state is green.
- **`npm run check`**: 0 errors, 0 warnings.
- **Frontend suite**: `npx vitest run` — full suite green, 331 files / 4457 tests, including the
  `bidiEscape.guard.test.ts` raw-render registry (updated for the new button-label line — it interpolates
  only a count and literals, the same provably-safe class as the adjacent `summary.more`, not a new
  filename/path surface).
- **Guard-test liveness**: `revertHoldBack.test.ts`'s 200-hold-back case now also asserts
  `allHeldBackPaths` has length 200 (not just the 8-preview) — the fixture is proved to actually carry
  all 200 before asserting anything about them, per this ticket's own acceptance criterion about fixture
  liveness.
- **Real-browser GUI verification** (jsdom applies no component CSS, per this ticket's own note): built
  `scripts/dev-harness/revert-heldback-copy/` (mounts the real `RevertOutcomePanel.svelte` directly, no
  Tauri imports to mock) + `vite.harness.revert-heldback.config.ts` (`npm run harness:revert-heldback-copy`,
  port 4329 — 4327/4328 were already in use on this shared machine). Drove plain installed
  `chrome.exe --headless=new` (never WebdriverIO/tauri-driver — msedgedriver here is version-mismatched
  and hangs), no CDP, just `--screenshot=`. Three fixtures side by side (unrestorable-key /
  alias-collision / retryable), both themes, plus an `?autoclick=1` run that dispatches a real click on
  the button and captures the post-click "Copied" confirmation state with the readout proving the
  clipboard write actually contains all 23 absolute paths (`/work/proj/assets/added-00.png` …
  `added-22.png`), not just the 8 shown on screen. Confirms: button present + correctly labelled on the
  unrestorable-key case; absent on both the alias/collision and retryable cases, in both themes.

### Assumption

`root` (the checkpoint's revert root) is passed down from each of the three hosts
(`CheckpointDialog`'s `path`, `AgentTimeline`'s `currentPath`, `CopilotDialog`'s `root`) so the copied
paths are absolute rather than the bare `/`-relative paths the wire carries — assumed the user wants
paths they can act on directly (paste into Explorer, a terminal, a search) rather than paths relative to
a root they'd have to supply themselves. If `root` is ever empty, the affordance still works and falls
back to the bare relative paths (no assumption is load-bearing for correctness, only for convenience).
