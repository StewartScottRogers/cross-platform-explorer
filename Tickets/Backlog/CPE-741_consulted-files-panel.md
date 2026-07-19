---
id: CPE-741
title: Agent Watch — per-session "consulted files" panel (durable read-set)
type: feature
component: Frontend
priority: medium
status: Open
tags: ready
created: 2026-07-19
epic: CPE-726
estimate: 2-3h
---

## Summary
Child of CPE-726. CPE-405 already streams the agent's file **reads** into the `fs-activity` channel
(`kind:"read"`) and shows them as fading row annotations + mixed-kind timeline entries. What's missing is a
**dedicated, durable per-session "files the agent has consulted" panel** — the set of paths the agent has
read this session, so you can see *what context it gathered* before/while editing, separate from the
transient (6s TTL) annotations and the interleaved timeline.

## Scope
- Derive a per-session read-set from the existing `fs-activity` read events (dedupe by path, keep
  first-seen / last-seen + count). Pure, headless-testable, alongside `agentActivity.ts` (do NOT change the
  weakest-precedence fold — a path both read and edited still annotates as edited in the live view, but
  appears in the consulted set).
- A panel in the Agent Watch surface listing consulted files (newest first), click-to-reveal/navigate,
  reusing the timeline's row idiom + the CPE-405 dimmer/hollow "read" styling.
- Clears on stop-watching (reuse `clearActivity` lifecycle); empty + idle when not watching (off means off).

## Acceptance
- [ ] While watching, files the agent reads accumulate in a durable consulted-files panel (not fading).
- [ ] The set dedupes by path and survives longer than the 6s annotation TTL; clears on stop-watching.
- [ ] Clicking an entry reveals/navigates to it; styling matches the CPE-405 read treatment.
- [ ] Headless tests cover read-set derivation (dedupe, ordering, clear); no cost when not watching.

## Notes
Builds entirely on the CPE-405 read pipeline (shipped) — no backend/sidecar change needed.
