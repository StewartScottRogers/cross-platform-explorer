---
title: "What remains to CLOSE epic CPE-730 (multi-agent conflict radar), and how to build it?"
date: 2026-07-26
tags: [conflict-radar, cpe-730, heat-map, rename-conflict, actor-tags, agent-watch, multi-session, off-means-off, cpe-1116, cpe-1117, cpe-1118]
status: current
---

## Question
CPE-730 is In Progress; the Radar tab, actor tags, and edit-edit/edit-delete detection shipped. What are the
remaining DoD items and how to build them so the epic can CLOSE?

## Ground truth (shipped vs claimed)
Shipped Radar = a **pure frontend fold**: `src-tauri` emits `ai-console://fs-activity` → `sidecar.ts:
normalizeFsActivity` → `agentActivity.ts` → `agentConflicts.ts:foldOverlaps` → `AgentTimeline.svelte` Radar tab.
Each activity item carries `actor` (sessionId / `"user"` / `"unknown"`, CPE-1101).

**Trap — five orphaned sidecar modules.** `sidecar/ai-console/src/{conflict,conflict_window,conflict_rename,
conflict_owner,conflict_region}.rs` are compiled + unit-tested but called nowhere outside their own tests, and
live in the WRONG process (sidecar, not the Tauri app that feeds the frontend radar). "The backend logic
exists" is misleading — it can't be consumed as-is. The architecture-consistent path is **frontend folds
mirroring that logic** (as `foldOverlaps` already does) + one genuine backend prerequisite (rename source→target
capture). Filed CPE-1119 (Deferred) to retire/document the orphans.

**Heat-map exists — coloured by KIND, not owner.** `FileList.svelte:464` uses `folderActivityKindNorm`
(`agentActivity.ts:231`) → left accent bar (write=`--accent`, read=`--text-muted`; active per-kind
`FileList.svelte:589-648`). The per-path `actor` is on the `fsActivity` map (`AgentActivity.actor`,
`agentActivity.ts:24`) but `normalizeActivityByKind` discards it.

**Rename source→target — NOT captured (prerequisite gap).** Watcher (`lib.rs:4561`) classifies
`Modify(ModifyKind::Name(_)) => "renamed"` on a SINGLE path; `fs_activity_pump` (:4609) flattens each path into
`HashMap<path,kind>`. Wire item `{kind,path,actor}` (`sidecar.ts:152`) has NO from→to. `conflict_rename` needs
`Vec<(from,to)>` — real backend slice.

## Tickets filed
- **CPE-1116 (Frontend, ready)** — owner-coloured heat-map + legend. Touch `agentActivity.ts` (carry actor +
  `folderOwnerNorm` mirroring `conflict_owner`/`conflict_region`), new `agentColors.ts` (stable theme-var per
  actor; user/unknown fixed), `FileList.svelte`. **Palette decided Opt A: fixed `--agent-1..6` theme vars**
  (N=1–4; theme-vars-only; reject HSL-from-hash). **Blocked on CPE-1112** (both edit FileList.svelte).
- **CPE-1117 (Backend, big-design, HIGH risk)** — capture/emit rename `from`/`to`. `lib.rs` classify_fs_event
  :4556 (`RenameMode::Both`) + fs_activity_pump :4590 (cookie-pair `From`+`To` within 200ms window; orphan →
  single-path fallback) + flush_fs_batch :4643 + `sidecar.ts` FsActivity + bindings regen. **Same lib.rs hot
  region as #413 actor-tag work — serialize; one bindings build at a time.** Cross-platform notify semantics →
  MUST pass 3-OS CI; graceful degradation where `Both` isn't emitted (decide-and-log, not a user-stop).
- **CPE-1118 (Frontend, ready, BLOCKED on 1117)** — `agentRenameConflicts.ts` fold mirroring `conflict_rename.rs`
  (divergence = same-from/diff-to/≥2 actors; collision = diff-from/same-to/≥2 actors; ignore same-agent + no-op)
  + `TimelineEntry.from/to` + a "Competing renames" section in the Radar tab. Sequences AFTER CPE-1116 (both
  edit agentActivity.ts) and after CPE-1114 (both edit AgentTimeline.svelte).

## Ordering & parallelism
A(1116) ∥ B(1117) disjoint crates. C(1118) blocks on B. A before C (shared agentActivity.ts type region). B must
not run concurrently with any other lib.rs pump/flush ticket. A blocked on CPE-1112 (FileList.svelte). C blocked
on CPE-1114 (AgentTimeline.svelte).

## DoD closure
After 1116 + 1117 + 1118 merge, all DoD bullets met (edits/deletes already; renames via B+C; heat-map by owner
via A; no cost single-session preserved — folds need ≥2 distinct actors, owner colour collapses to one/none,
off-means-off honoured) → **CPE-730 can CLOSE.** The epic's *rough-scope* "conflict banner" + per-file "who else
is here" indicator are NOT DoD bullets (Radar actor pills satisfy the DoD literally); if wanted, file separately
rather than blocking the close.

## Critical files
`src/lib/agentActivity.ts`, `src/lib/components/FileList.svelte`, `src/lib/components/AgentTimeline.svelte`,
`src/lib/sidecar.ts` (FsActivity:152); `src-tauri/src/lib.rs` (classify_fs_event:4556, fs_activity_pump:4590,
flush_fs_batch:4643); reference-only sidecar logic to mirror: `conflict_rename.rs` / `conflict_owner.rs` /
`conflict_region.rs`.
