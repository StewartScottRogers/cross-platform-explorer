---
id: CPE-1630
title: "Vault create's \"securely delete the original\" has no confirmed flag — the second door into irreversible shredding, still open after CPE-1611"
type: Task
status: Backlog
priority: Medium
component: Backend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Found by the independent Reviewer of CPE-1611 (PR #819), and independently by that PR's UAT tester. CPE-1611
gave `shred_paths` an explicit `confirmed: bool` so the engine refuses unless told — closing the same class
of gap CPE-1599 closed for Batch Media. But `shred_paths` turns out not to be the only door.

## The gap
`vault_manager::shred_tree` (`crates/server/src/vault_manager.rs:466-479`) calls
`secure_shred::shred_file` **directly**, bypassing `shred_paths` and therefore the new gate entirely.

Upstream, `vault_create` (`src-tauri/src/lib.rs:6912-6928`, `crates/server/src/vault_manager.rs:134-170`)
takes `shred_original: bool` straight from `VaultCreateDialog.svelte`'s checkbox
(`src/lib/components/VaultCreateDialog.svelte:40,75,208`) with **no separate confirm flag layered on top of
that intent bool**. A devtools or automation caller can invoke `vault_create(folder, dest, pass, true)` and
skip the dialog's warning panel completely.

This is architecturally identical to the pre-CPE-1599 batch-media gap and the pre-CPE-1611 `shred_paths`
gap: `shred_original`'s danger is **conditional** (only when true), which is exactly the shape CPE-1599
established needs a distinct `confirmed` parameter rather than one intent flag doing double duty.

## What already mitigates it (so scope this proportionately)
This is **not** the same severity as an unguarded `shred_paths`. `vault_create` already:
- defaults `shred_original` to `false`,
- refuses if the destination blob resolves **inside** the folder being shredded, and
- performs a full decrypt round-trip verification of the on-disk encrypted copy **before** shredding the
  plaintext original — with dedicated tests (`shred_original_refuses_to_shred_when_verify_fails`,
  `shred_original_refuses_when_dest_is_inside_the_folder_to_be_shredded`).

So data is not *lost* — a recoverable copy is proven to exist first. But the plaintext original is still
irreversibly destroyed with no trash fallback, which is the same blast-radius class as `shred_paths`.

## Goal
One consistent rule across every destructive engine entry point: **the engine refuses unless explicitly
told, separately from the caller's intent flag.**

## Scope
- Add a `confirmed: bool` to `vault_create` (and thread it into `vault_manager::create_vault`), required
  only when `shred_original` is true; refuse cleanly with a specific error otherwise — never a panic, never
  a partial shred, and never a vault created but a shred silently skipped without saying so.
- `VaultCreateDialog.svelte` is the one and only caller allowed to pass `true`.
- Match CPE-1599's and CPE-1611's error shape and naming so all three refusals read alike.
- Regenerate `bindings.gen.ts` (the command signature changes) or the typed-bindings drift guard fails.
- While here, **audit for any third door**: grep for every direct `secure_shred::` / `shred_file` /
  `shred_tree` caller and list them in the work log, so the next person doesn't have to rediscover this.

## Acceptance criteria
- `vault_create(..., shred_original: true, confirmed: false)` shreds nothing and returns a specific error;
  a test verifies the originals' **bytes are intact on disk**, not merely that an `Err` came back.
- The confirmed path still works end-to-end, including the existing verify-before-shred guarantee.
- The audit list of all direct shred callers appears in the work log.
- Tests must not assert exact filesystem byte counts (CI runs Linux + macOS + Windows).

**Conflict surface:** `crates/server/src/vault_manager.rs`, `src-tauri/src/lib.rs` (the `vault_create`
command + `generate_handler!`), `src/lib/bindings.gen.ts`, `src/lib/components/VaultCreateDialog.svelte`,
plus tests. Touches `lib.rs` and `bindings.gen.ts` — do not run in parallel with other command-signature work.
