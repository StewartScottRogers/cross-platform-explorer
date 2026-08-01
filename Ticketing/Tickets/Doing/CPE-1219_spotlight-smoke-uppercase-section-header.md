---
id: CPE-1219
title: "Fix spotlight gui-smoke: section-header assertion is case-sensitive but the label is CSS-uppercased"
type: Bug
status: In Progress
priority: Medium
component: gui-smoke
tags: [ready]
estimate: 15m
created: 2026-08-01
closed:
---

## Problem
`gui-smoke/specs/spotlight.smoke.ts:113` asserts the section header list `.to.include("Files")`,
but `.sp-section-label` renders with `text-transform: uppercase` (`Spotlight.svelte:244`), so
WebDriver's `getText()` returns the *rendered* text `"FILES"`. The assertion fails
(`expected [ 'FILES' ] to include 'Files'`), the spec errors before `snap("spotlight")` (line 135),
and no overlay screenshot is captured for the Visual Critic. The product is correct — uppercase
section headers are intentional design; only the spec's assertion is wrong.

## Fix
Compare case-insensitively: lower-case the collected label texts and assert `.to.include("files")`
(mirrors the existing `markText.toLowerCase()` pattern two lines down). Spec-only change.

## Acceptance criteria
- The spotlight smoke spec passes against the real built app and captures `spotlight.png`.
- The assertion still genuinely verifies a Files section header renders (not weakened to a no-op).

## Work Log
2026-08-01 (workshift) — Foreman fix: case-insensitive section-header check. Caught by the live
epic-704 Visual Critic screenshot run (the assertion blocked screenshot capture).
2026-08-01 (workshift) — Second assertion also fixed in the same spec: the matched-run highlight
check assumed the FIRST <mark class="sp-hl"> spelled the whole query, but fuzzy matches split into
multiple non-contiguous runs, so the first run was just 'm'. Now collects ALL runs in the row and
asserts their concatenation includes "marker". Both are spec-only assertion corrections.
