---
id: CPE-570
title: "Workbench — intra-line (word-level) diff highlighting"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-505
estimate: 1h
created: 2026-07-17
closed: 2026-07-17
---

## Summary
The Workbench highlights whole changed lines, but on a small edit you still hunt for what actually
changed. Add GitHub-style **intra-line highlighting**: for a modified line (a `del` immediately followed
by an `add`), highlight the exact characters that differ within each line.

## Acceptance Criteria
- [x] Pure `inlineDiff(old, new)` returns unchanged/changed segments (common prefix + suffix; middle
      differs). Unit-tested (identical, fully-different, insertion).
- [x] Pure `annotateInline(lines)` pairs each `del→add` and attaches segments; other lines pass through.
      Unit-tested against the sample diff.
- [x] The Workbench renders the changed span with a stronger add/del highlight; unmodified lines are plain.
- [x] `npm run check` clean; component test asserts the `.chg` highlight; full suite green.

## Resolution
`diff.ts`: `inlineDiff` (prefix/suffix scan → `InlineSeg[]` for old + new) and `annotateInline` (attaches
`.segs` to each `del→add` pair → `RenderLine[]`). `WorkbenchView` renders `annotateInline(h.lines)`; a
line with `.segs` renders its segments wrapping `changed` ones in `<span class="chg">` (stronger
add-green / del-red), else plain text. Updated the WorkbenchView tests to match the now-split line text via
a `.code`-textContent matcher (anticipating the CPE-563-style pitfall) + a `.chg` assertion. `diff.test.ts`
+3, `WorkbenchView.test.ts` +1; full suite **609 pass / 63 files**; `npm run check` 0/0.

## Notes
Cheap prefix/suffix approximation (not full LCS) — ideal for the common single-line edit; multi-line
del/add blocks just don't pair (fall back to whole-line highlight). Non-i18n. Epic CPE-505.
