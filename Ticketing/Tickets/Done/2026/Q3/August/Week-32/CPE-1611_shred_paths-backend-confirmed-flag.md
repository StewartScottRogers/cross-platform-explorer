---
id: CPE-1611
title: "Defence in depth for secure delete: add an explicit confirmed flag to shred_paths"
type: Task
status: Doing
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

## Work Log

**2026-08-11 — implemented, tested, PR opened (sprint worker).**

Matched CPE-1599's shape exactly:

- `crates/server/src/secure_shred.rs`: `shred_paths` now takes `confirmed: bool` and returns
  `Result<Vec<ShredResult>, String>` instead of `Vec<ShredResult>`. When `confirmed` is `false` it
  returns `Err` immediately — nothing is touched, no path in the batch is opened or removed — with a
  specific message ("refusing to shred: `confirmed` was not set on this shred_paths call...", contains
  "confirm" like CPE-1599's refusal). When `true`, behavior is unchanged (skip-and-report per path).
- `src-tauri/src/lib.rs`: the `shred_paths` Tauri command gained a `confirmed: bool` parameter and now
  returns `Result<Vec<ShredResult>, String>`; it's still a thin dispatcher — `note_app_op` (Agent Watch
  attribution) is now only called when `confirmed` is true, since nothing happens otherwise. Still
  registered in both `generate_handler!` call sites (there are two in this file — verified both).
- `src/lib/components/ShredConfirmDialog.svelte`: `confirmShred()` (fired only by the "Shred
  permanently" button) is the one and only call site that passes `confirmed: true`. Updated to handle
  the command's new `Result<T,E>`-shaped return (`res.status === "ok" | "error"`, same pattern as
  `NewLinkDialog.svelte`/`RepairLinkDialog.svelte`), since the codegen wraps any Rust command returning
  `Result<T,E>` that way.
- `src/lib/bindings.gen.ts`: regenerated via `cargo run --bin export_bindings --features
  "specta-bindings sidecar-platform"` from `src-tauri/` — not hand-edited. Only the `shredPaths` entry
  changed (new `confirmed` param, new `Result<ShredResult[], string>` return shape + doc comment).
- `src/lib/components/ShredConfirmDialog.test.ts`: the one existing invoke-args assertion for
  `shred_paths` now expects `confirmed: true` too.
- Tests added in `crates/server/src/secure_shred.rs`: `shred_paths_refuses_the_whole_batch_when_not_confirmed`
  (two files, `confirmed: false` → `Err` containing "confirm", both files verified untouched with
  original bytes — no exact-byte-count assumptions beyond the literal fixture) and
  `shred_paths_proceeds_once_confirmed_is_true` (identical call with `confirmed: true` succeeds and the
  file is actually removed). The three pre-existing `shred_paths` tests were updated to pass
  `confirmed: true` and `.unwrap()` the new `Result`.

Verification (all run synchronously in the worktree, from `Z:\repos\cross-platform-explorer\.claude\worktrees\agent-a9213426e72e2369c`):
- `cargo build --features "specta-bindings sidecar-platform" --bin export_bindings` (src-tauri) — pass.
- `cargo run --bin export_bindings --features "specta-bindings sidecar-platform"` (src-tauri) — pass,
  regenerated `bindings.gen.ts`.
- `cargo test secure_shred` (crates/server) — 13 passed, 0 failed.
- `cargo test` (crates/server, full suite) — 1859 passed, 0 failed, 1 ignored.
- `cargo build` (src-tauri, default features) — pass, no warnings.
- `cargo clippy --all-targets -- -D warnings` (src-tauri) — clean.
- `cargo clippy --all-targets --features sidecar-platform -- -D warnings` (src-tauri) — clean.
- `npm run check` — 0 errors, 0 warnings.
- `npx vitest run src/lib/components/ShredConfirmDialog.test.ts src/App.paneBArchiveVault.test.ts` —
  16/16 passed.
- `npx vitest run src/docs.coverage.test.ts src/lib/components/ContextMenu.test.ts
  src/App.paneBBulkOps.test.ts` — 68/68 passed (checked these because they reference "shred" elsewhere
  in the app — menu wiring / vault-create's separate shred-original path — to confirm no incidental
  breakage).

No new dependencies added. Diff kept to the conflict surface the ticket named:
`crates/server/src/secure_shred.rs`, `src-tauri/src/lib.rs`, `src/lib/bindings.gen.ts`,
`src/lib/components/ShredConfirmDialog.svelte`, `src/lib/components/ShredConfirmDialog.test.ts`. No
docs update needed — `ShredConfirmDialog`'s user-visible behavior is unchanged, per the ticket's own
Scope note.
