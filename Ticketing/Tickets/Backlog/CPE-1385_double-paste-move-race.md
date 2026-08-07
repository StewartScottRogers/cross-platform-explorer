---
id: CPE-1385
title: "Clipboard: two rapid Ctrl+V pastes can double-fire a cut-move before the clipboard clears"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-661
created: 2026-08-06
---

## Problem (CPE-1380 / PR #662 reviewer observation — PRE-EXISTING, not introduced by #662)

`doPaste` clears the clipboard (`clipboard = emptyClipboard()`) only AFTER `moveEntries` resolves. So two
paste invocations fired within the async window (two rapid Ctrl+V before the first `await moveEntries`
settles) both read the same non-empty cut clipboard and both call `moveEntries` with the same source paths —
a double-move race. A single paste, or a paste after the first resolves, is fine (clipboard already empty).
This exists identically in the pre-#662 code; #662's pane-routing neither introduced nor worsened it.

## Fix direction

Guard against re-entrancy: snapshot + clear the clipboard SYNCHRONOUSLY at the start of `doPaste` (before
the `await`), operating on the local snapshot — so a second paste sees an empty clipboard and no-ops. Or set
a `pasting` in-flight flag that short-circuits a second paste until the first settles. Add a test firing two
Ctrl+V within the async window and asserting `moveEntries` is called exactly once. Touches `src/App.svelte`
`doPaste`. Low priority (requires two keypresses inside a short async window) but it's a data-movement
double-fire, so worth closing.
