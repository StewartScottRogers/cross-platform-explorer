---
id: CPE-1397
title: "DataBrowser: 'rows X–Yof Z' page-range readout is missing a space before 'of'"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-724
created: 2026-08-07
---

## Problem (CPE-1392 / PR #670 spec observation — cosmetic)
In `src/lib/components/DataBrowser.svelte`, the page-range readout renders as e.g. `rows 1–100of 105` (no space
before "of"). Svelte trims the leading space of the `{#if}`-block's first text node, so
`{offset+page.rows.length}{#if page.total !== undefined} of {page.total}{/if}` concatenates without a separating
space. Data is correct; purely cosmetic. Pinned as observed behavior in `DataBrowser.test.ts` (CPE-1392).

## Fix direction
Add a non-breaking or explicit space so it reads `rows 1–100 of 105` — e.g. move the space outside the `{#if}`
(`… {offset+page.rows.length}{#if page.total !== undefined}&nbsp;of {page.total}{/if}`) or use `{" of "}`.
Update the `DataBrowser.test.ts` assertion to the corrected string.
