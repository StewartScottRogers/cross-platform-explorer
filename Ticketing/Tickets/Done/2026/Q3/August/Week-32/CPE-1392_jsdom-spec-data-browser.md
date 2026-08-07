---
id: CPE-1392
title: "Test: jsdom render-spec for DataBrowser (SQLite data-grid viewer)"
type: Task
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-724
created: 2026-08-07
---

## Problem (QA-Architecture MVD burndown)
`src/lib/components/DataBrowser.svelte` (SQLite table/query viewer) has no jsdom coverage.

## Fix direction
Add `src/lib/components/DataBrowser.test.ts` (same recipe). Assert: `commands.dataBrowserSources` populates the
table picker; `commands.dataBrowserQuery` + `commands.dataBrowserPage` drive rows + pagination; empty-result,
query-error, and page-boundary states. Typed-call args + dispatched payloads. Report (don't fix) mis-wires.
Test-only; parallel-safe.
