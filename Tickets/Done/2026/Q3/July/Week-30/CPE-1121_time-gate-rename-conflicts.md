---
id: CPE-1121
title: "Conflict radar: (optionally) time-gate the competing-rename fold so stale renames don't read as live"
type: enhancement
component: Frontend
priority: low
status: Done
tags: ready
created: 2026-07-26
epic: CPE-730
---

## Summary
Fast-follow to CPE-1118. `foldRenameConflicts` (like the Rust `conflict_rename.rs` it mirrors) has **no time
component** — so in a long-lived live Radar view, a rename from an agent session that ended hours/days ago can
still fold into a "competing rename" with a fresh rename onto/from the same path, visually indistinguishable
from a genuinely concurrent race. Both the CPE-1118 Reviewer and UAT flagged this (a UAT probe folded two
same-`from` renames ~11.5 days apart into one divergence). It is **not a bug** per CPE-1118 (time-gating was
explicitly optional there), but it's a real "logically-correct vs reads-as-current" gap.

## Design (buildable, needs one product call)
Gate `foldRenameConflicts` (and consider the same for `foldOverlaps` if it has the same property) by a temporal
window before flagging — only fold renames whose contributing entries fall within `W` ms of each other.
**Recommended default:** reuse the existing `OVERLAP_WINDOW_MS` const the overlap detector already uses (keeps
the two radar folds consistent). **Alternative:** a distinct, larger rename window (renames may legitimately
race over a longer span than edits). Pick one; if unsure, default to `OVERLAP_WINDOW_MS` and log the choice.
Keep `lastAt` for the label. Off-means-off + no new deps unchanged.

## Acceptance Criteria
- [ ] Competing-rename detection only flags renames within the chosen window; two same-`from` renames far apart
      in time no longer fold into one conflict.
- [ ] Consistent with the overlap detector's windowing; `npm run check` clean; `npm test` green (extend
      `agentRenameConflicts.test.ts` with an in-window vs out-of-window case); no new deps.

## Work Log
2026-07-26 (workshift) — Filed as the CPE-1118 fast-follow (both gates flagged the un-gated fold). Low-pri;
needs a one-line product decision on the window size (recommended: reuse OVERLAP_WINDOW_MS).

2026-07-26 (workshift) — Built (PR #437, merged c3410a58). Reviewer APPROVE + UAT PASS: sliding trailing-window mirrors foldOverlaps exactly (same `>` boundary eviction), reuses OVERLAP_WINDOW_MS (5000ms); in-window renames still fold, out-of-window (incl the 11.5-day case CPE-1118 flagged) no longer fold; divergence/collision keying + same-actor/no-op exclusions unchanged; 20/20 tests. Resolves the un-time-gated UX quirk from CPE-1118.
