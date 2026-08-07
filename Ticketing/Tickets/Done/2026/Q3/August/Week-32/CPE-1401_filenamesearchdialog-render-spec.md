---
id: CPE-1401
title: "Test: jsdom render-spec for FileNameSearchDialog (streaming + generation-token supersede)"
type: Task
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-704
created: 2026-08-07
---

## Problem (hardening scout, Vein B — real streaming state machine)
`src/lib/components/FileNameSearchDialog.svelte` has untested: recents localStorage load/save, streaming-channel
batch accumulation, and a generation-token supersede guard (the CPE-666 class of bug this pattern protects
against). Zero coverage.

## Fix direction
Add `src/lib/components/FileNameSearchDialog.test.ts`. READ the component first for the real `rawInvoke`/channel
API. Mock `rawInvoke`/`createChannel`; assert: a stale batch from a SUPERSEDED search is dropped (gen-token
guard — the key regression risk); recents persist to/from localStorage; `onMount` pre-fill + auto-run works;
batches accumulate into the visible list. Non-hollow. Report any real mis-wire (don't fix). Test-only.
