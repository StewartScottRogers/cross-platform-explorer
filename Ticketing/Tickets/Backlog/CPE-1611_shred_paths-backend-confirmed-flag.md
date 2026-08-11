---
id: CPE-1611
title: "Defence in depth for secure delete: add an explicit confirmed flag to shred_paths"
type: Task
status: Backlog
priority: Medium
component: Backend
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Follow-up from CPE-1599 (backend confirm flag for Batch Media's in-place overwrite). That ticket's review
also flagged `shred_paths` / `secure_shred::shred_paths` (`crates/server/src/secure_shred.rs`) as having
the same architectural gap: the frontend `ShredConfirmDialog.svelte` is the only thing standing between a
caller and an irreversible secure-delete — the backend command takes no "confirmed" flag of any kind, so a
devtools call or a future automation surface could invoke `shred_paths` directly and skip the dialog
entirely, exactly like the pre-CPE-1590 batch-media gap.

CPE-1599 deliberately did NOT fold this in — see its Work Log for the full rationale. Short version:
`shred_paths` is unconditionally destructive on every call (no "safe plan" a caller could mistake for a
dangerous one, unlike `batch_media::plan`'s conditional `output == input`), so the fix is smaller
(thread a `confirmed: bool` straight through, no plan-inspection needed) but still deserves its own
reviewed, bounded diff rather than riding along in an already-large batch-media PR.

## Goal
Give `shred_paths` the same "the engine refuses unless told explicitly" treatment CPE-1599 gave
`batch_media_execute_stream` — proportionate to how much simpler the shred case is.

## Scope
- Add a `confirmed: bool` parameter to `secure_shred::shred_paths` (and the `shred_paths` Tauri command in
  `src-tauri/src/lib.rs`); refuse the whole batch (nothing shredded) with a clean, specific error when it's
  not set — never a panic, never a partial/silent shred.
- Thread it through from `ShredConfirmDialog.svelte` — the existing confirm dialog — as the one and only
  place in the codebase allowed to set it `true`. Regenerate `src/lib/bindings.gen.ts`.
- Tests: an unconfirmed call is refused by the engine and shreds nothing; a confirmed call proceeds exactly
  as today; `ShredConfirmDialog`'s existing frontend tests still pass (mock arguments will need the new
  parameter).
- Update `docs/design` / in-app docs only if `ShredConfirmDialog`'s user-visible behaviour changes (it
  shouldn't — this is purely a backend backstop for a flow that already always confirms today).

## Notes
Conflict surface: `crates/server/src/secure_shred.rs`, `src-tauri/src/lib.rs` (the `shred_paths` command),
`src/lib/bindings.gen.ts`, `src/lib/components/ShredConfirmDialog.svelte` +
`ShredConfirmDialog.test.ts`. Small, self-contained — good pickup for any model.

## Priority raised to Medium — the deferral rationale was backwards (Foreman, 2026-08-10)
CPE-1599's author deferred this arguing that Batch Media's danger is *conditional* (a plan only sometimes
resolves in-place, so the engine genuinely needs telling) while `shred_paths` is *unconditionally*
destructive, so there is no "looks safe but isn't" case to guard.

The independent reviewer on PR #812 judged that argument to answer a different question than the one
CPE-1599 exists to answer, and I agree. The threat model is **"a caller reaches the engine without ever
passing the confirmation UI"** — a devtools call, a future automation or agent surface, a new UI entry
point. That threat is identical for both, conditional danger or not. On the merits `shred_paths` is the
*stronger* case:

- **Worse blast radius.** Per `ShredConfirmDialog.svelte`'s own doc comment, secure delete has **no trash
  fallback at all**. A confirmed batch-media overwrite at least gets a best-effort checkpoint; a shred gets
  nothing.
- **Smaller fix.** A bare `confirmed: bool` on the command — no plan inspection needed, since every call is
  destructive by definition.

So this is cheaper to build and protects something less recoverable. Do not re-derive the deferral
reasoning when picking this up.
