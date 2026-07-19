---
id: CPE-729
title: "EPIC: Intervene & approve — gate high-impact agent actions"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic, big-design]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
A visibility mode that can also *gate*: optionally pause an agent before high-impact filesystem actions
(bulk deletes, writes outside an allow-scoped subtree, touching ignored/secret paths) and surface an
approve / reject / edit-scope prompt in the explorer, backed by rules the user sets.

## Why
Watching is safer when you can also stop. This deliberately crosses Agent Watch's current "observe, not
drive" boundary — a scoped, opt-in exception — so it's its own epic and its own boundary decision.

## Rough scope (areas, not child tickets)
- A policy engine (rules: path scopes, action classes, secret/ignored patterns).
- A console-side hook to hold an action pending a decision.
- An approval surface in the explorer (approve / reject / edit-scope) with an audit of every decision.
- Safe defaults and a clear off switch.

## Open questions (resolve at activation)
- Feasibility of holding an agent action pending approval via the sidecar contract.
- Which action classes are gateable and how to define scopes.
- The explicit boundary decision: this is the one mode that drives, not just observes.

## Definition of Done
- Users can define rules that pause an agent before high-impact actions and approve/reject/edit-scope.
- Every gate decision is audited; defaults are safe and the feature is fully opt-in.
- With the feature off, Agent Watch observes only (no gating), per its stated boundary.
