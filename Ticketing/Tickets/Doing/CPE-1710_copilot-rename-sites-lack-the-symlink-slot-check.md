---
id: CPE-1710
title: copilot's rename and transfer sites destroy a dangling symlink at the destination
type: bug
priority: Medium
status: Doing
tags: ready
estimate: S
created: 2026-08-13
closed:
---

## Problem

Found by the PR #893 (CPE-1705) reviewer, 2026-08-13, while enumerating that ticket's sites rather than
spot-checking them.

`copilot::apply_op` (the `Rename` arm) and `copilot::transfer_entry` are both **`fs::rename`-destructive**
at the destination. Both received the `clobber_refusal` guard in CPE-1705 — but **neither got
`symlink_slot_refusal`**, which `rename_entry_impl` and `move_exact_impl` both have.

The consequence: **a dangling symlink sitting at the destination is silently destroyed.** `clobber_refusal`
answers "is something already here?" using a stat that follows the link; for a link whose target does not
exist, that answers *no*, the slot reads as free, and the rename replaces the link itself.

The PR's own helper doc comment states that a `rename`-destructive site needs the extra symlink check.
These two sites are the exceptions to a rule the same PR wrote down.

## Why it is Medium and not High

A dangling symlink is a less common thing to lose than a file with contents, and the loss is of the link
rather than of data — the link's target was already absent. It is still a silent destruction of something
the user created, at a site whose two siblings guard against exactly this.

## Scope

`copilot::apply_op`'s `Rename` arm and `copilot::transfer_entry`. Compare against `rename_entry_impl` and
`move_exact_impl`, which are the correct shape.

## Acceptance criteria

- [ ] Both sites apply `symlink_slot_refusal` alongside `clobber_refusal`, matching `rename_entry_impl`.
- [ ] A test proves a **dangling** symlink at the destination survives, for each of the two sites, and that
      removing the check turns a **distinct** test red. Assert on the slot still being a symlink after the
      call — not on the returned `Result`, which was `ok: true` in the reviewer's reproduction.
- [ ] Check whether any **other** `fs::rename`-destructive site is missing the pairing. The reviewer found
      these two by enumeration; enumerate again rather than fixing only the two reported. If the pairing is
      always required, consider making it structurally impossible to apply one without the other rather
      than relying on every future author remembering.
- [ ] Platform-gate correctly. Symlink creation on Windows needs either Developer Mode or elevation, so a
      test that silently no-ops on an unprivileged runner proves nothing — detect and skip **loudly** with a
      `writeln!(stderr)` notice, and make sure the Linux and macOS legs assert something real. CI runs a
      3-OS matrix.
- [ ] Each guard broken **on its own** turns a **distinct** test red, real output pasted in the PR, per the
      Evidence Rules in `Ticketing/wiki.md`.

## Notes

Filed by the Foreman from the PR #893 review, 2026-08-13, on the reviewer's recommendation to handle it as
a follow-up rather than widening that PR.

**Useful technique, measured on this sprint:** a slot whose stat is genuinely refused can be staged locally
on Windows two independent ways — deny `(R)` on the target **plus `RD` on its parent** (which kills
`fs::metadata`'s `FindFirstFileW` fallback), or a **symlink whose resolution target is denied**. The second
exercises the reparse path and is the more natural fit here. See CPE-1705's "CORRECTION 4" section; that
ticket's guidance was wrong four times before this was understood, so read it before writing an ACL test.

Related: **CPE-1705** (which added `clobber_refusal` to these sites), **CPE-1687** (the honest-refusal
wording pattern), **CPE-1696** (the sibling stat-collapse round).

## Work Log

**2026-08-13 — worked to a pushed PR (branch `cpe-1710-copilot-symlink-slot`).**

### What the fix is

`fsutil::rename_slot_refusal(target, occupied)` — `clobber_refusal` then `symlink_slot_refusal`, in the
order the two correct sites already used, as **one call that cannot be half-applied**. The reported two
sites were fixed by converting to it, and so were the other two the enumeration turned up.

### Enumeration — every `fs::rename` in the tree, and its guard status

Scope of the search (stated, per Evidence Rule 2): `grep -rn "fs::rename"` over `crates/`, `src-tauri/src/`
and `sidecar/`, then each hit read in context.

**Rename-destructive at a user-named slot — the class this ticket is about (6 sites):**

| Site | Before | After |
|---|---|---|
| `copilot::apply_op` `Rename` arm (`copilot.rs:233`) | `clobber_refusal` only | `rename_slot_refusal` |
| `copilot::transfer_entry` (`copilot.rs:267`) | `clobber_refusal` only | `rename_slot_refusal` |
| `organize_apply::apply_proposals` (`organize_apply.rs:99`) | **`clobber_refusal` only — same bug, not reported** | `rename_slot_refusal` |
| `ticket_move` board move (`src-tauri/src/lib.rs:167`) | **`clobber_refusal` only — same bug, not reported** | `rename_slot_refusal` |
| `rename_entry_impl` (`src-tauri/src/lib.rs:1842`) | both, open-coded | `rename_slot_refusal` |
| `move_exact_impl` (`src-tauri/src/lib.rs:3408`) | both, open-coded | `rename_slot_refusal` |

So it was **four of six** missing the pairing, not two of six. That is the argument for making it
structural: CPE-1705 wrote the rule into a doc comment and two thirds of its own sites did not follow it.

**Not in this class, checked and left alone (with the reason):**

- **Atomic tmp → final replaces** — `audit_journal`, `checkpoint_store`, `metrics_journal`,
  `replay_baseline`, `known_hosts`, `index`, `semantic_index`, `vector_index`, `vault_manager` (×2),
  `vault_crypto`, `src-tauri/src/lib.rs:3545`. The destination is a file **we** own and are deliberately
  replacing; refusing on a link there would break the write, not protect a user's file.
- **Name-picking probes rather than refusals** — `unique_target` → `do_move_into`, and `resolve_conflict`
  (`src-tauri`). These *advance past* an occupied slot instead of refusing at it, so `rename_slot_refusal`
  is the wrong shape: a dangling link there reads as a free name and the auto-rename picks it. **This is a
  real residual instance of the same hazard** and is filed separately rather than smuggled into this
  ticket — see "Follow-up" below.
- **Protocol server rigs** — `crates/ftp`, `crates/sftp`, `crates/webdav` implement a wire protocol's own
  rename semantics against a sandbox root; not app-side destination guards.
- **`clobber_refusal` sites that are not renames** — `split_join` (×3), `folder_template`,
  `src-tauri` trash-restore (×2). Those precede `File::create`/`fs::write` or an OS restore, and the
  helper's doc already scopes them.

### Structural: the pairing is now enforced, not remembered

`fsutil::tests::guards_are_paired_at_every_rename_destructive_site` scans `crates/server/src/**.rs` plus
`src-tauri/src/lib.rs` and fails if (a) a bare `clobber_refusal` call sits within 25 lines of an
`fs::rename` — the exact half-guarded shape — or (b) `symlink_slot_refusal` is called anywhere outside
`fsutil`. It asserts its own inputs too (file count, and that it can still see the combined helper being
called), so it cannot silently scan nothing.

### Evidence (Evidence Rules, `Ticketing/wiki.md`)

Committed **before** probing. Each guard broken **on its own**, restored with `git checkout --`, real
recompiles observed (`Compiling cpe-server`). Full output pasted in the PR body. Five breaks, each redding
a **distinct** test:

1. `copilot` `Rename` arm → only `cpe_1710_execute_never_renames_over_a_dangling_link_at_the_new_name`
   (+ the structural scan, by design). The other two site tests stayed green.
2. `copilot::transfer_entry` → only `cpe_1710_execute_never_moves_over_a_dangling_link_at_the_destination`.
3. `organize_apply` → only `cpe_1710_organize_never_renames_over_a_dangling_link_in_the_destination_folder`.
4. `ticket_move` in `src-tauri` → the structural scan names `src-tauri/src/lib.rs:160`, proving the scan
   reaches the app adapter.
5. `rename_entry_impl` re-separated into two calls → the scan fires **both** rules at once.

Each test asserts on the **slot** (`symlink_metadata(...).is_symlink()`), never on the returned `Result` —
the reviewer's reproduction returned `ok: true` while destroying the link.

### Platform gating

No ACLs are needed here at all: a *dangling* link is an ordinary object, and `try_exists` answers
`Ok(false)` for one on every platform. Only **creating** the link can be refused. `fsutil::make_dangling_link`
tries `symlink_file` first (needs Developer Mode / elevation on Windows) and falls back to an NTFS
**junction** (no privilege — created against a real directory that is then removed), so the Windows leg
asserts for real on an unprivileged runner too. If both fail it is a loud `writeln!(stderr)` skip that says
nothing was covered. Unix creates the link unconditionally. Verified locally on Windows with `--nocapture`:
no skip notice printed, so the links were really created.

### Checks

`cargo test` (2107 passed / 0 failed) and `cargo clippy --all-targets -- -D warnings` clean in
`crates/server` for **both** CI feature modes (default and `--features index`); `src-tauri` clippy clean in
both modes (default and `--features sidecar-platform`) and `cargo test` green.

### Follow-up filed — CPE-1713

The `unique_target` / `resolve_conflict` name-picking probes treat a dangling link as a free name, so a
bulk move auto-renames *onto* the link and destroys it. Same hazard, different shape (the fix is "treat a
link slot as occupied and pick the next name", not "refuse"), so it is its own ticket rather than a
widening of this one.
