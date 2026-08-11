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

## Work Log
2026-08-11 (sprint worker) — Implemented the reviewer's suggested shape:
- `crates/server/src/checkpoint_store.rs`: new `CheckpointFailure { operation, reason, ts }` type — no
  `manifest_id`, so it structurally cannot be passed to any preview/revert call. New JSON-lines index
  `checkpoint_failures.json`, kept a SEPARATE file from `checkpoints.json` (a torn line in one can never
  affect the other; `checkpoint_list`, the restore surface, never has to filter failures out — it never
  sees them). `record_checkpoint_failure`/`checkpoint_failures_list` entry points. Rotates at
  `MAX_CHECKPOINT_FAILURES = 50` (oldest out first, same crash-safe temp+rename rewrite as
  `audit_journal::trim`) — a checkpoint attempt happens at most once per user-initiated batch, never a
  retry loop, so this comfortably bounds even a persistently broken root (e.g. a read-only drive hit
  daily). 5 new backend tests.
- `src-tauri/src/lib.rs`: two thin dispatchers, `checkpoint_record_failure` / `checkpoint_failures_list`,
  registered in both `generate_handler!` and `specta_commands!`. Regenerated `src/lib/bindings.gen.ts`
  (`cargo run --bin export_bindings --features "specta-bindings sidecar-platform"`).
- `src/lib/checkpointFailures.ts` (new): one shared `recordCheckpointFailure(root, operation, error)`
  helper — the "prefer one shared mechanism over a per-dialog one" the ticket asked to consider. Wired
  into all four "checkpoint before an irreversible batch" callers' existing failure `catch` blocks
  (alongside their `console.error`, never instead of it): `BatchMediaDialog.svelte`,
  `MetadataStudioDialog.svelte`, `DeclutterDialog.svelte`, `SimilarImagesDialog.svelte`.
- `src/lib/components/CheckpointDialog.svelte` (the Checkpoints panel): loads both `checkpointList` and
  the new `checkpointFailuresList` for the current root, interleaves them newest-first by timestamp, and
  renders a failed attempt as a **structurally distinct** row (danger-tinted left border/background, a
  "ban" icon, no manifest id, no Preview/Revert buttons at all — not disabled, absent) so it can never be
  mistaken for a restore point. New `$t("ckpt.failedTitle")` key added to all 12 complete locale catalogs.
- `src/docs/16-checkpoints.md`: new "When a pre-write checkpoint fails" section.
- Did NOT touch `crates/server/src/batch_media.rs` / `batch_execute.rs` (CPE-1613, PR #818, is in flight
  there) — the whole feature routes through the checkpoint store + the dialogs' existing frontend catch
  blocks instead.
- Verified: `cargo build`/`cargo test` (crates/server: 1863 passed), `cargo test --lib` (src-tauri: 144
  passed), `cargo clippy --all-targets -- -D warnings` clean in both crates x both feature modes
  (default and `specta`/`specta-bindings sidecar-platform`), `npm run check` (0 errors), `npx vitest run`
  (274 files / 3334 tests passed, including 14 new/updated CheckpointDialog tests + 4 new
  checkpointFailures.ts tests + updated Declutter/MetadataStudio/SimilarImages/BatchMedia dialog tests).
