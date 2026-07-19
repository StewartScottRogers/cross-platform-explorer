---
id: CPE-726
title: "EPIC: Agent read-trace — surface the reads the watcher can't see"
type: Task
status: In Progress
priority: High
component: Frontend
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
Surface the file *reads* the filesystem watcher can't observe (the acknowledged CPE-405 gap) as a
first-class Agent Watch capability: a distinct "consulted" annotation kind, a per-session "files the agent
has looked at" panel, and read-vs-write heat contrast so you can see what context the agent gathered before
it started editing.

## Why
Agent Watch's tiebreaker is visibility, and reads are currently invisible (a Windows FS watcher can't see
them). Ingesting the agent's own tool-output stream closes the single biggest blind spot in the mode.

## Rough scope (areas, not child tickets)
- Console/sidecar tool-output plumbing to stream `read` events over the Status channel.
- A read-event source alongside the FS watcher, feeding the same activity model.
- A "consulted" annotation kind + a per-session read-set panel.
- Read-vs-write heat contrast in the live view + heat-map.

## Open questions (resolve at activation)
- Reliability/coverage of the agent tool-output stream vs. actual reads.
- Attribution of reads to a session and dedup with writes.
- Retention of the read set (transient vs. durable — see [[CPE-733]]).

## Definition of Done
- File reads reported by the agent appear as "consulted" activity in the live view and a read-set panel.
- Read vs. write is visually distinguishable in annotations and the heat-map.
- With Agent Watch off, no read tracking runs and the plain explorer is unchanged.

## Decisions (activated 2026-07-19 — best-guess logged per user "don't ask, use your recommendation")

**Critical finding at activation: most of this epic already shipped in CPE-405 (Done, 2026-07-14).** The
brief was written on stale grounding — AGENT-WATCH.md still calls reads "the one remaining piece (CPE-405),"
but that ticket is closed. Already delivered by CPE-405:
- Sidecar `ReadScanner` parsing the agent's tool-output stream for reads (`sidecar/ai-console/src/agent_reads.rs`).
- Host arm re-emitting `fs-read:<json>` onto the SAME `ai-console://fs-activity` channel as `kind:"read"`
  (`src-tauri/src/lib.rs`).
- Frontend `read` kind + `normalizeFsActivity` allow + **weakest-precedence fold** (`agentActivity.ts`,
  `sidecar.ts`) + `affectsListing` exclusion.
- A **distinct dimmer/hollow "consulted" badge + row accent** in `AgentTimeline.svelte` and `FileList.svelte`.

So scope items "tool-output plumbing", "a read-event source alongside the watcher", and the inline
"consulted" annotation are **DONE**. This epic is therefore **re-scoped** to the two read-visibility
surfaces CPE-405 did NOT build, and is no longer a large epic (2 frontend-only children, no backend change):
1. A **durable per-session consulted-files panel** (the read-set), distinct from the 6s-TTL fading
   annotations and the interleaved mixed-kind timeline.
2. **Read-vs-write contrast in the folder heat-map** — `folderHasActivity` ignores kind today, so a
   read-only subtree looks identical to a written one.

Everything builds on the shipped CPE-405 pipeline; **no sidecar/host change is required.** If even this
narrow remnant isn't wanted, close CPE-726 as superseded-by-CPE-405 instead.

## Child tickets
1. **CPE-741** — Per-session "consulted files" panel (durable read-set derived from the existing read
   events; dedupe/reveal/clear-on-stop). Frontend-only. *(ready)*
2. **CPE-742** — Read-vs-write contrast in the folder heat-map (kind-aware folder-activity check; cooler
   heat for read-only subtrees; preserves the CPE-698 normalize-once optimization). Frontend-only. *(ready)*

## Work Log
2026-07-19 — Activated. Research revealed CPE-405 already delivered the read pipeline + inline "consulted"
annotation (see Decisions). Re-scoped from the original 4-area epic to the 2 genuinely-unbuilt read-visibility
surfaces; filed children CPE-741 (consulted-files panel) and CPE-742 (heat-map read/write contrast), both
frontend-only on the shipped CPE-405 base. Set status In Progress.
