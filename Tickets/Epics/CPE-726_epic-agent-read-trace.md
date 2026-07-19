---
id: CPE-726
title: "EPIC: Agent read-trace — surface the reads the watcher can't see"
type: Task
status: Proposed
priority: High
component: Multiple
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
