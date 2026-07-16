---
id: CPE-501
title: "EPIC: Agent Grid — tiled split-pane grid of agent terminals"
type: Task
status: Done
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-16
closed: 2026-07-16
---

## Summary
Part of the **Agent Workspace** program (a sibling/evolution of the AI Console [[CPE-261]], proposed by
spike [[CPE-500]] after researching BridgeSpace). Evolve the AI Console's single-pane, tabbed sessions
into a **tiled, split-pane grid** so several agent terminals are visible and interactable at once. A
brief only — not decomposed until activated.

## Goal
See N running agents side by side (BridgeSpace shows up to 16), split any direction, without losing the
session model (daemon reattach CPE-309, session chips CPE-490) the AI Console already has.

## Rough scope (NOT decomposed)
- Split-pane layout engine (horizontal/vertical splits, resize, focus).
- Grid ↔ tabs toggle; per-pane the existing terminal + status.
- Carry the session-identity chip (CPE-490) + reattach (CPE-309) into grid slots.
- Keyboard navigation between panes; layout persistence.

## Open questions (resolve at activation)
- Max panes / performance ceiling? Virtualise off-screen terminals?
- Grid-only, or grid + tabs coexisting? Per-workspace layout persistence?
- Narrow-window / responsive behaviour.

## Decisions (activation 2026-07-16)
- **Layout model:** **Auto-reflowing grid** — uniform tiles that reflow to a best-fit rows×cols as
  sessions are added/closed (1→full, 2→side-by-side, 3-4→2×2, …). *Not* manual split panes (simpler,
  predictable; a manual-override could be a later follow-up).
- **Tabs vs grid:** **Toggle view** — a Tabs ⇄ Grid switch in the toolbar; **tabs stay the default**,
  grid is opt-in. Lowest-risk; nothing in the existing single-pane path breaks.
- **Pane ceiling:** **Up to 16 visible**, with **off-screen/non-focused panes throttled** so 16 live
  PTYs stay responsive (matches BridgeSpace). Further sessions remain reachable as tabs.
- **Persistence:** **Persist per session-set** — remember the active view + grid arrangement, restored
  on relaunch and on daemon reattach ([[CPE-309]]).

## Research (activation)
The AI Console UI is a single vanilla-JS file, `sidecar/ai-console/src/launcher.html` (~1273 lines): a
tab strip (`#tabs`, one agent per tab, CPE-336) over per-session `.term-pane` xterm terminals
(absolute `inset:0`, only the active one shown), with the CPE-490 session chip, CPE-311 usage, a custom
scrollbar, and CPE-309 reattach. The grid is therefore a **layout change over the existing term-panes**
(tile them instead of showing one) plus a `fit()` on each — not a rewrite of the terminal/session model.
A jsdom launcher test harness already exists ([[launcher-ui-has-a-jsdom-test-harness]] / CPE), so the
grid math + toggle/persistence logic is unit-testable headlessly.

## Child tickets (created at activation)
- [[CPE-506]] — Auto-reflow layout engine + Tabs⇄Grid toggle *(foundation; ready)*
- [[CPE-507]] — Per-pane session identity (CPE-490 chip) + keyboard navigation *(needs-prereq CPE-506)*
- [[CPE-508]] — Scale to 16 panes with off-screen throttling *(needs-prereq CPE-506; big-design)*
- [[CPE-509]] — Persist layout + active view across relaunch/reattach *(needs-prereq CPE-506; ties CPE-309)*
- [[CPE-510]] — Responsive narrow-window fallback *(needs-prereq CPE-506)*

Suggested order: **CPE-506** first (unblocks all), then CPE-507 / CPE-509 (independent), CPE-508
(performance), CPE-510 (polish).

## Resolution (closed 2026-07-16)
The Agent Grid shipped end-to-end over five children, all Done — the AI Console can now show many
running agents side by side, per the activation decisions (auto-reflow grid · Tabs⇄Grid toggle · 16
panes with off-screen throttling · persist per session-set):
- **[[CPE-506]]** — auto-reflow layout engine + Tabs⇄Grid toggle (foundation; `gridDims`, `applyView`).
- **[[CPE-507]]** — per-pane identity (CPE-490 chip/label/usage) + focus ring + Ctrl+Alt+Arrow keynav.
- **[[CPE-508]]** — scale to 16 tiles with off-screen output throttling (no output lost).
- **[[CPE-509]]** — persist the view + focus per workspace, restored on relaunch/reattach (CPE-309).
- **[[CPE-510]]** — responsive narrow-window fallback (width-aware columns, no horizontal scroll).

All built additively over the existing per-session term-panes — the **plain single-pane tabs view is
the untouched default** (PURPOSE tiebreaker). Fully headless-tested: the launcher jsdom harness grew
from 33 → 48 tests (gridDims/toggle/nextPaneId/paneWritePolicy/visibleGridIds/serialize/colsForWidth);
523 frontend tests pass overall; `npm run check` clean throughout.

**Carve-out / deferred (recorded):** the *live* 16-PTY smoothness measurement in [[CPE-508]] needs real
WebView2 + PTYs and is deferred to GUI QA — the throttle mechanism + policy are unit-verified. Also, a
future **manual split-pane override** (vs. pure auto-grid) was consciously left out at activation and
could be a follow-up. Neither blocks this epic.

The [[CPE-511]] Herdr spike (filed during this epic) recommends adding **semantic agent-state
awareness** (idle/working/blocked/done) to these grid tiles — a strong candidate follow-up child/sibling.

## Notes
Successor/sibling to [[CPE-261]]; from [[CPE-500]]. Highest-value + closest-to-existing of the five —
recommended to build first.

## Work Log (close)
2026-07-16 — **Closed.** All 5 children (CPE-506…510) Done; DoD met (grid usable end-to-end, identity +
keynav, 16-pane throttling, persistence, responsive). Built additively; plain tabs view unchanged.
Deferred: live 16-PTY perf measurement (GUI QA) + optional manual splits. Moved Epics/ → Done/.

## Work Log
2026-07-16 — Filed as a dormant `Proposed` brief (from spike CPE-500).
2026-07-16 — **Activated.** Researched the AI Console launcher (single-file vanilla JS: tabs over
per-session term-panes; jsdom test harness exists). Resolved the 4 open questions with the user (see
Decisions: auto grid · Tabs⇄Grid toggle · 16 panes w/ off-screen throttling · persist per session-set)
and decomposed into 5 child tickets (CPE-506…510) in Backlog, each `epic: CPE-501`. Status → In Progress.
