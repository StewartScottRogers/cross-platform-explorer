---
id: CPE-409
title: Session output dialog must fetch fresh output each open (cache made it stale)
type: bug
priority: medium
estimate: XS
status: Done
created: 2026-07-14
closed: 2026-07-14
tags: [ui, ai-console, bug]
---

## Problem
Opening "⇕ Full output" re-requests /api/session/{id}/output, but it's a GET on a stable URL, so
WebView2 can serve a cached (stale) response — the panel doesn't reflect output produced since the
first open, so it drifts out of sync with the live console.

## Fix
- Fetch the output with `cache: "no-store"` (+ a cache-busting timestamp) so every open pulls the
  current ring from the sidecar.

## Acceptance
- [x] Reopening the panel after new output shows the new output (no stale cache)
