---
id: CPE-490
title: "Make AI Console tabs and left-pane Agent leaves clear + correlatable"
type: Feature
status: Open
priority: Medium
component: Multiple
tags: [ready]
estimate: 2-3h
created: 2026-07-16
epic: CPE-261
---

## Summary
With several sessions of the same agent running, both surfaces are hard to read and impossible to
correlate: the **AI Console tab** caption is `agent · provider · model` (long, and every same-agent
tab leads with the identical `claude · openrouter · …`, so the distinguishing tail is what gets
truncated), and the **left-pane Agents leaf** shows just `agentName` + folder (so multiple sessions all
read `claude`). Nothing links a specific tab to its left-pane leaf. Make them clear and correlatable.

## Design (agreed 2026-07-16)
The console assigns session ids `s1`, `s2`, … and that **same `sessionId` flows to the left pane**, so
both surfaces can derive an **identical chip with zero cross-window coordination**:
- **Shared identity chip** — a colour (deterministic hash of `sessionId` → small palette) + a number
  (parsed from the id, `s2` → `2`), rendered identically on the console tab AND the left-pane leaf.
  Colour is the fastest correlation ("the blue ②"); the number is the durable label. Duplicate the
  tiny colour/number helper in `launcher.html` (JS) and the Svelte side (TS) with a "keep in sync" note.
- **Better labels** — lead with the distinguishing part and keep the tail readable: show a short model
  (last `/` segment) and the folder; middle-ellipsis so the model isn't the bit that's cut.
- **Tooltip parity** — both surfaces show the same full detail on hover: `agent · provider · model ·
  <folder> · started …`.

## Acceptance Criteria
- [ ] Each running session shows the **same colour + number chip** on its AI Console tab and its
      left-pane Agents leaf, so a user can match them at a glance.
- [ ] AI Console tab caption is readable when several same-agent sessions run (chip + shortened,
      distinguishing label; full detail in the tooltip).
- [ ] Left-pane Agents leaf shows the chip + agent + short model + folder, with a full tooltip.
- [ ] Colour/number derivation is identical on both surfaces (same `sessionId` → same chip), verified
      by a test on the shared helper.
- [ ] Existing behaviour (click leaf → navigate to cwd; tabs switch sessions) is preserved.

## Notes
Filed from a direct user request ("captions … too short and not easily correlated"). Surfaces:
`sidecar/ai-console/src/launcher.html` (tab render in `addSession`), `src/lib/components/Sidebar.svelte`
(agent leaves). Possible follow-up (out of scope here): **two-way linking** — clicking a left-pane leaf
focuses the AI Console window and selects that session's tab (needs cross-window messaging). Pairs with
[[CPE-489]] (same Agents leaves).
