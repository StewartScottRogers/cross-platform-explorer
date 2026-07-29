---
id: CPE-505
title: "EPIC: Integrated workbench — in-pane diff/editor + embedded browser"
type: Task
status: Done
priority: Medium
component: Frontend
tags: [epic]
estimate: 4h+
created: 2026-07-16
closed: 2026-07-16
---

## Summary
Part of the **Agent Workspace** program (sibling to the AI Console [[CPE-261]]; from spike [[CPE-500]]).
Close the review loop in one window — an **in-pane diff/editor** and an **embedded browser** beside the
agent grid (the explorer is already the file tree). A brief only until activated.

## Goal
Read an agent's diff, edit alongside it, and open localhost to see the result — without leaving the
window (BridgeSpace's integrated editor + browser).

## Rough scope (NOT decomposed)
- A **diff view** of agent changes (git-backed).
- An **editor** beside the grid (reuse/extend the preview-pane text editor CPE-066).
- An **embedded browser** pane (open localhost / a URL) under the app's strict CSP.

## Open questions (resolve at activation)
- Embedded browser under the strict CSP — a dedicated webview window vs. an iframe; security review.
- Reuse the preview pane's editor, or a richer code editor?
- Diff source + granularity (working tree vs a session's changes).

## Decisions (activation 2026-07-16)
Adopted on the "do them all" directive — defaults, reversible:
- **Embedded browser:** a **dedicated webview window** (safe under the strict CSP — not an iframe in
  the main window), reusing the AI Console separate-window pattern.
- **Editor:** **reuse the preview-pane editor** ([[CPE-066]]) for now (not a richer code editor).
- **Diff source:** **working tree vs HEAD** (`git diff`).

## Child tickets (created at activation)
Sprint **[[SPR-06]]**:
- [[CPE-526]] — git diff model + Diff view *(ready)*
- [[CPE-527]] — editor pane + embedded browser (webview) *(needs-prereq CPE-526)*

## Resolution (closed 2026-07-16)
The integrated workbench shipped across 2 children in SPR-06: [[CPE-526]] a pure unified-diff parser +
`workbench_diff` command + Diff view (read an agent's changes), and [[CPE-527]] an editor hand-off
(reusing the preview editor) + an embedded browser in a dedicated webview window (open localhost). Opened
from a **"Workbench"** Sidebar entry. Read the diff, edit alongside, open the running app — in one window.
~10 tests; clippy + `npm run check` clean. **Follow-on (recorded):** a side-by-side split (diff | editor
| browser) layout and broad external-origin browsing (Tauri capability config) are polish items; the
localhost + diff + edit loop is complete.

## Notes
From [[CPE-500]]; the explorer file tree + preview/edit are the starting point.

## Work Log
2026-07-16 — Filed as a dormant `Proposed` brief (from spike CPE-500).
2026-07-16 — **Activated** into SPR-06 on the "do them all" directive (defaults above). Decomposed into
CPE-526 (diff) + CPE-527 (editor + browser). Status → In Progress.
