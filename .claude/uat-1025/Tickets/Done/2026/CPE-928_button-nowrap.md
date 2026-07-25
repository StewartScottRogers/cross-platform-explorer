---
id: CPE-928
title: Buttons must not wrap their text
type: bug
component: Frontend
priority: medium
tags: ready
created: 2026-07-23
closed: 2026-07-23
status: Done
---

## Summary
Button labels could wrap/clip mid-word in narrow containers. Added `white-space: nowrap` to the global
`button` rule so every button keeps its label on one line, across all three UIs:
- main app (`src/app.css` global `button {}`),
- Agent Deck launcher (`sidecar/ai-console/src/launcher.html` `button {}`),
- Agent Board sidecar (`sidecar/agent-board/src/ui.rs` — added a `button {}` rule).

`nowrap` only affects inline text, so intentional multi-line (block-child) buttons are unaffected; a button
that genuinely needs to wrap can opt back in with `white-space: normal`.

## Acceptance Criteria
- [x] Global button rule sets `white-space: nowrap` in all three UIs.
- [x] `npm run check` + full frontend suite (921 tests) green.

## Work Log
- 2026-07-23 — Global CSS fix; verified with type-check + full vitest suite.
