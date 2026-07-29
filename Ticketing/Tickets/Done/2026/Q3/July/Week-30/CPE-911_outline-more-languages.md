---
id: CPE-911
title: Code outline — more languages (Ruby, PHP, C-family)
type: feature
component: Backend
priority: low
tags: ready
epic: CPE-724
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
Extends the symbol outline (CPE-910) to more languages, same dependency-free heuristic:
- **Ruby** — def (method by indent) / class / module.
- **PHP** — function / class / interface / trait / enum (visibility modifiers stripped).
- **C-family** (C / C++ / C# / Java / Kotlin / Swift) — keyword-declared **types**: class / struct /
  interface / enum / union / namespace / record. Functions/methods in these are keyword-less and too
  ambiguous to detect heuristically, so they're intentionally skipped (no false positives).

## Acceptance Criteria
- [x] Ruby / PHP / C-family outlines with correct kinds + 1-based lines.
- [x] 9 outline tests (3 new); clippy `-D warnings` clean; no new deps; 3-OS.

## Work Log
- 2026-07-22 — Follow-up to CPE-910 on epic CPE-724 — broadens language coverage for the jump-to-symbol
  outline.
