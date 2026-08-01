---
id: CPE-1226
title: "QA: gui-smoke pin + Visual Critic screenshot for archive rows in the operations/transfer panel"
type: Task
priority: Low
component: gui-smoke
tags: [ready]
estimate: 1h
created: 2026-08-01
closed:
---

## Context
CPE-1184 (PR #523) added archive compress/extract rows to the operations/transfer panel
(`TransferPanel.svelte`) — a new visual state (archive icon + "compressed/extracted" wording +
progress). The panel itself is already validated and copy/move rows are pinned, but the archive-op
row variant has no gui-smoke screenshot for the Visual Critic yet (a compress/extract-in-progress row
is timing-sensitive to capture). Reviewer + UAT covered the code + behaviour; this pins the visual.

## Acceptance criteria
- A gui-smoke spec drives a real compress (or extract) through the panel and `snap`s a frame showing
  the archive row (icon + wording + progress). Deterministic capture (e.g. a fixture large enough that
  the row is visible, or assert the completed "N items compressed" row if in-progress is too flaky).
- Visual Critic judges the screenshot (icon legible, wording correct, reflows, on-theme).
