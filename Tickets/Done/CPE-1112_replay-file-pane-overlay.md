---
id: CPE-1112
title: "Activity replay: read-only file-pane overlay while scrubbing (optional)"
type: feature
component: Frontend
priority: low
status: Done
tags: ready
created: 2026-07-26
closed: 2026-07-26
epic: CPE-728
---

## Summary
OPTIONAL enhancement (CPE-728 slice e — NOT required by the closed epic's DoD, which is satisfied by the
in-drawer reconstruction from CPE-1111). Graduate the reconstructed listing from the Replay-tab drawer to a
**read-only overlay of the MAIN explorer file pane** while scrubbing, so the whole browser shows the folder as
it looked at time T. Highest-risk slice — coexists with a live session still emitting events. Design:
`.claude/research-library/entries/activity-replay-event-reconstruction-plan.md` (§2 "graduate", §4 slice e).

## Design (buildable)
Drive `ExplorerPane`/`FileList` to render `childrenAt(stateAtFrom(baseline, events, t), currentPath)` (the same
`replayFold.ts` fold CPE-1111 already uses) as a **read-only, ephemeral** overlay, gated behind an explicit
**Replay mode** toggle so the live listing and the reconstruction never render simultaneously. Restore the live
listing on exit. Must NOT mutate the live navigation/listing store.

## ⚠ Guardrails / risks
- Strictly read-only + ephemeral; explicit Replay-mode gate; guaranteed restore-on-exit; never mutate live
  `entries`/navigation. Off-means-off; no new deps. This is the risky coexistence slice — de-risk by keeping
  the in-drawer view (CPE-1111) as the always-available fallback.

## Acceptance Criteria
- [x] A Replay-mode toggle shows the reconstructed listing in the main file pane while scrubbing; exiting
      restores the live listing; live store/navigation never mutated; read-only.
- [x] `npm run check` clean; vitest green; no new deps; off-means-off.

## Work Log
2026-07-26 (workshift) — Filed as the optional CPE-728 graduate (file-pane overlay). The epic closed on the
in-drawer reconstruction (CPE-1111); this is a nice-to-have, pickable anytime.

2026-07-26 — DONE. Built a new pure module `src/lib/replayOverlay.ts` (mode-gating is ONE function,
`resolveOverlay(active, source, tMs, dir) -> DirEntry[] | null`, so "off-means-off" and "restore-on-exit" are
true by construction — no imperative cleanup to forget) plus `toDirEntries`/`overlayEntriesAt`/`dirSetOf` that
convert the CPE-1111 `replayFold.ts` fold's output into `DirEntry`-shaped rows so the overlay reuses `FileList`
with zero changes to it. Wired a new "Show in file pane" checkbox (off by default) into `AgentTimeline`'s
Replay tab, which dispatches a `replayOverlay` event (`DirEntry[] | null`) on every change to the toggle, the
scrub position, the reconstructed folder, or the loaded session; `onDestroy` sends an explicit final `null` so
closing the drawer mid-overlay can't leave a stale reconstruction on screen. `App.svelte` forwards that event
straight into a new `ExplorerPane` prop, `replayOverlay: DirEntry[] | null` (default `null`). `ExplorerPane`
never assigns to it — `$: paneEntries = replayOverlay ?? visible` is the ONLY thing that changes: the live
`entries`/`shown`/`visible`/`selectedEntries` pipeline keeps deriving from the real listing the entire time,
untouched, so the live store is provably never mutated (falls back to it automatically the instant
`replayOverlay` goes back to `null`). While active: a "Replay mode" banner replaces the Agent-Watch strip
(never rendered simultaneously), `canDrag`/`showFolderSizes` are forced off, and `open`/`context`/
`contextEmpty`/`commitRename`/`drop` are no-ops on FileList's dispatches — enforcing read-only. The in-drawer
reconstruction (CPE-1111) is completely untouched and stays the always-available fallback.
Tests: `src/lib/replayOverlay.test.ts` (16 new, pure-fn) covers dirSetOf/toDirEntries/overlayEntriesAt
(dir-vs-file detection, extension/hidden/modified derivation, no-mutation-of-inputs) and `resolveOverlay`'s
gating contract (off returns null even with a loaded source; active-with-no-source returns null, never throws;
restore-on-exit by re-evaluation; source-loss on session change also restores). Extended
`src/lib/components/AgentTimeline.test.ts` with 6 component tests (28 total, up from 22): off-by-default,
checking dispatches the same reconstruction the drawer shows, unchecking restores, leaving the Replay tab
restores without touching the toggle, unmounting mid-overlay restores, and the `entries` prop the component was
given is never mutated across a toggle-on/scrub/toggle-off cycle. Self-verified: `npm run check` 0 errors/0
warnings; full suite 1238/1238 green (was 1216 before CPE-1115 + this ticket's 22 new tests); `git diff --stat`
on package.json/package-lock.json/src-tauri/Cargo.{toml,lock} is empty (no new deps); no backend/bindings files
touched. Files: `src/lib/replayOverlay.ts` (new), `src/lib/replayOverlay.test.ts` (new),
`src/lib/components/ExplorerPane.svelte`, `src/lib/components/AgentTimeline.svelte`,
`src/lib/components/AgentTimeline.test.ts`, `src/App.svelte`.
