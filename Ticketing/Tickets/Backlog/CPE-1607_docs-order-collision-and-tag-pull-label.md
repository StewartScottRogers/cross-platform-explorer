---
id: CPE-1607
title: "Docs polish: duplicate `order: 38` inside Safety & Recovery, and the native tag Pull label nuance"
type: Task
status: Backlog
priority: Low
component: Docs
epic: CPE-1569
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Two small, non-blocking observations from the independent reviewer on CPE-1604 (PR #810). Neither justified
holding that PR; both are worth doing.

## 1. A real `order` collision (pre-existing)
`src/docs/38-trash.md` and `src/docs/safety-undo.md` both declare **`order: 38`** inside the same
**Safety & Recovery** category. `docs.ts` sorts by frontmatter `order`, so two pages sharing a value in one
category leaves their relative position down to whatever the glob happens to yield — stable in practice,
arbitrary in principle, and it will silently mis-order the day the glob order changes.

Pick the intended reading order, renumber, and — better — **add a guard test** asserting `order` is unique
within each category, so the next collision fails CI instead of lurking. The docs corpus is growing fast
(four more pages landed today alone); this class of problem gets worse with size.

## 2. The native tag **Pull** label nuance
`src/docs/explorer-tags.md` describes the native-metadata **Pull** as "merges… non-destructive; only adds,
never removes". True for tags — but incomplete for the **colour label**: per
`crates/server/src/native_tags.rs:93-97`, a pulled native label only wins when the internal label is
**empty**. The page never claims the label is unioned the way tags are, so this is an omission rather than
a misstatement — but the whole point of this epic's house style is that a user shouldn't be surprised.

Add a sentence stating the label rule explicitly. Note it is the mirror image of the **import** behaviour
the same page already documents (where a non-empty incoming label *overwrites*) — that asymmetry between
Pull and Import is exactly the kind of thing worth spelling out rather than leaving a reader to infer.

## Acceptance criteria
- No two pages share an `order` within a category, and a guard test enforces it.
- The Tags page states the Pull label rule and contrasts it with Import.
- `npm run check` and the docs guard tests green.

## Notes
Small. Conflict surface: `src/docs/38-trash.md`, `src/docs/safety-undo.md`, `src/docs/explorer-tags.md`,
`src/lib/docs.ts` + its test. Model: sonnet (or haiku — this is mechanical).
