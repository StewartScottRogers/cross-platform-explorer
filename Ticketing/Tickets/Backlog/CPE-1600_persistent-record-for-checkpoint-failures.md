---
id: CPE-1600
title: "Give a failed pre-write checkpoint a persistent home, not just a 5-second toast"
type: Task
status: Backlog
priority: Low
component: Frontend
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Raised by the independent reviewer across three rounds of CPE-1590 (PR #805), and explicitly left out of
scope there because it is a property of the app's notification model as a whole rather than a defect that
ticket introduced.

When Batch Media is about to overwrite a user's originals in place, it takes a best-effort checkpoint of
each affected folder first. CPE-1590 made a *failed* checkpoint impossible to miss inside the dialog, and
guaranteed the warning now fires on all three dismissal paths (Done, Cancel, Escape/backdrop) via
`showNotice`. But `showNotice` is a **single global banner that auto-dismisses after ~5 seconds** — a user
who dismisses the dialog and looks away still misses it.

That is now exactly on par with how the app reports every other outcome (delete, rename, batch-media
success and skips all use the same ephemeral banner), so CPE-1590 brought this warning **up to** the
app-wide standard rather than leaving it below. The question this ticket asks is whether that standard is
good enough for *this particular* message: "the safety net for your irreplaceable originals did not
exist."

## Goal
A failed pre-write checkpoint leaves a durable trace the user can find later, not only a banner they had
to be watching for.

## Fix (suggested shape — the reviewer's)
The **Checkpoints panel is the natural home**: it already exists, already has a persistent store
(`crates/server/src/checkpoint_store.rs`'s index), and is exactly where a user goes when they want to
recover something. Record the attempted-but-failed checkpoint there — folder, timestamp, the operation
that prompted it, and the reason it failed — so "I tried to protect this folder and couldn't" is visible
alongside the checkpoints that did succeed.

Consider whether this should generalise: every other caller of the "checkpoint before an irreversible
batch" pattern (Metadata Studio, Declutter, Similar Images) has the same silent-failure shape, and would
benefit from the same record. Prefer one shared mechanism over a per-dialog one.

## Explicitly NOT in scope
A general notification-history/inbox feature. If that is what this really wants, file it as its own thing —
do not grow it out of this ticket.

## Notes
Conflict surface: the Checkpoints panel component, `crates/server/src/checkpoint_store.rs`, and the
callers listed above. Model: sonnet.
