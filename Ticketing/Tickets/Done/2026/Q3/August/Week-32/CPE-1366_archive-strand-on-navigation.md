---
id: CPE-1366
title: "Back/Forward and tab-switch strand the archive browse-view (archive contents bleed onto real folders/tabs)"
type: Bug
status: Done
priority: High
component: Frontend
tags: [ready]
epic: CPE-059
created: 2026-08-06
closed: 2026-08-06
---

## Problem (found by adversarial audit of the archive drill-down the user is actively using)

When browsing INSIDE a `.zip`/`.rar` in place, pressing **Back/Forward** or **switching tabs** left the
archive view stranded: the zip's inner entries bled onto the real folder you navigated to (or an unrelated
tab), breadcrumb read `Home › archive.zip` while `isHome` was true, and file mutations stayed blocked with
"read-only view inside an archive" even though the toolbar claimed a real location. Recovery needed a crumb
click or Alt+Up.

## Root cause

`loadPath()` (App.svelte, real-filesystem navigation) reset the other two virtual view-modes —
`smartFolder` (CPE-667) and `structuredSearch` (CPE-1229) — but **not `archive`**. The manual nav paths
(sidebar, crumbs, home, browse-for-folder, workspace restore, palette) all guard with
`if (archive) exitArchive()` before navigating, but the **history** (`goBack`/`goForward`, Alt+←/→) and
**tab** paths (`selectTab`/`newTab`/`cycleTab`/`closeTab`/`reopenClosedTab`/duplicate) call `loadPath`
directly with no such guard. `archive` is a single top-level variable (not per-tab), so a stranded archive
bled across every tab until something called `exitArchive`. `goUp` handled `archive` explicitly — the
asymmetry was the tell.

## Fix

Clear `archive` inside `loadPath` alongside `smartFolder`/`structuredSearch` — the **single chokepoint**
that every real-fs navigation (history + all tab ops included) flows through. `enterArchive` and all
in-archive navigation (`openInArchive`, crumb-descent, `goUp`) mutate only the `archive` object and never
call `loadPath`, so this can't clear an archive the user is legitimately entering/browsing.

## Tests

NEW `src/App.archiveNav.test.ts` — drives the real App: descend Home → C:\d → C:\d\photos, enter a zip in
place (inner entry renders), press Back to the real folder C:\d, assert the inner entry is no longer
stranded. Verified it FAILS without the fix and PASSES with it (Home was the one target that masks the bug
— it unmounts the FileList — so the test Backs into a real folder). Full App suite + `npm run check` green.

## Work Log

- 2026-08-06 — Adversarial audit of the archive drill-down flow (the surface the user is actively using)
  surfaced this HIGH-sev strand. Fixed at the loadPath chokepoint; wrote a real-App regression test and
  validated it guards the fix (fail-without / pass-with). Board epics regression check in the same audit
  came back clean. Shipped in v0.57.56.
