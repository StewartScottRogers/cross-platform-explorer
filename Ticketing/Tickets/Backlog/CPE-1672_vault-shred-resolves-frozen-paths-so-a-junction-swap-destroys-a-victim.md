---
id: CPE-1672
title: The vault session shredder re-resolves frozen paths, so a junction swapped at a parent directory gets a victim file securely destroyed
type: bug
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-12
closed:
---

## Problem

Found by the independent Security Auditor re-auditing PR #847 (CPE-1645), and **reproduced 3/3 end-to-end
through the public `VaultRegistry::lock` API** on that PR's head.

`shred_tree` (`crates/server/src/vault_manager.rs`) is **collect-then-shred**: `collect_files(root, &mut
files)` walks the whole tree and freezes absolute paths, then the loop calls `hard_link_count(file)` and
`secure_shred::shred_file(&p, …)` — and **both of those re-resolve the whole path from scratch**.

CPE-1645 added a per-file link-count check immediately before each overwrite, which closed the hard-link
variant it was filed for. But that check has no-follow semantics on the **final component only**; every
*parent* component is resolved by the OS. `wipe_session_dir`'s own root-is-not-a-link check is likewise a
single pass, before `collect_files`.

### The reproduction (no elevation, no Developer Mode, no race precision)

1. Before locking, the attacker creates a real directory `<session>/zsub/` holding a real file
   `taxes.xlsx`. Every alias guard correctly finds this innocuous — link count 1, not a reparse point — so
   both `ensure_no_aliased_files` walks pass and it is sealed into the blob.
2. **Starting gun:** poll the session dir. The first shredded file disappearing proves `collect_files` has
   already run and the path list is frozen.
3. `remove_dir_all(<session>/zsub)`, then `create_junction(C:\…\Documents, <session>/zsub)` — a registered
   IPC command (`src-tauri/src/lib.rs:3570`, in `generate_handler!` at 11565). NTFS junctions need neither
   elevation nor Developer Mode.
4. The loop reaches the frozen path `<session>/zsub/taxes.xlsx`, which now resolves through the junction to
   `Documents\taxes.xlsx`. `hard_link_count` reads **`One`** → `WipeDisposition::Shred` → `shred_file`
   overwrites and unlinks the victim.

Measured on `591a06db`:

```
AUDIT PROBE 2: swapped=true lock=Ok(()) victim_exists=false
               victim_dir_exists=true bystander_exists=true
```

`Documents/` and an un-named bystander file survive, which proves this is the **shredder writing through
the junctioned parent** rather than `remove_dir_all` recursing. `lock` returned `Ok(())` and the UI says
"Locked".

This is **strictly worse than the hard-link variant** CPE-1645 closed: there the victim's inode kept its
other name, so nothing was lost. Here the victim's only name is securely overwritten and removed.

## Why this was not blocked on

The collect-then-shred structure is **pre-existing on `main`** — it is not a regression introduced by
CPE-1645, and the same attacker with the same access could do this before that ticket. CPE-1645 delivered
its own scope (locking re-seals your edits instead of destroying them) and had already been through three
rounds, so it landed with this filed rather than growing a fourth time. The auditor explicitly endorsed
that call, on condition the overclaiming comment was corrected — which it was, in the same PR.

## Scope

**Proportionate fix (preferred, small):** stop collecting first. Walk and shred **inline**, and immediately
before descending into a subdirectory check `symlink_metadata(dir).file_type().is_symlink()` and
refuse/skip. That removes the frozen path list *and* the observable starting gun, shrinking the window to a
few syscalls with no signal an attacker can wait for.

**Complete fix:** handle-based. Open each file once with no-follow, read `nNumberOfLinks` from **that**
handle, and write through the same handle — no window, no parent-component resolution. This is the shape
PR #848 adopted for the Batch Media write path after the same class of finding, and it worked there.

Also re-check `wipe_session_dir`'s root check under whichever shape is chosen.

## Acceptance criteria

- [ ] The reproduction above is refused: the victim survives byte-for-byte, verified by reading it back off
      disk, and the bystander is untouched.
- [ ] A test plants the junction at a *parent* directory mid-wipe and proves the shred never writes through
      it — not merely that the final component is checked.
- [ ] The hard-link variant CPE-1645 closed stays closed (its tests must remain green).
- [ ] Neutralise the new guard on its own and confirm a distinct test goes red.
- [ ] `shred_tree`'s doc comment is updated to describe what the new shape actually guarantees — and, if a
      window remains, says so with its real size. Four comments in this codebase were corrected in one
      night for claiming more than the code held; do not add a fifth.

## Notes

Filed by the Foreman from the PR #847 round-3 security re-audit, 2026-08-12.

Everything else in that audit passed: the exclusive staging open, the per-attempt nonce, the structured
error codes and their cross-language guard, the concurrency serialisation, the durability fix, and the
three newly-pinned guards were each neutralised individually and each turned a distinct test red. The
auditor also confirmed the hard-link variant is genuinely dead, and that making the link count read fail
disposes safely (`Unknown` → unlink, never overwrite).

Related: **CPE-1669** (`create_vault` writes the blob without fsync before shredding the original) and
**CPE-1670** (locking replaces a symlinked vault path instead of writing through it), both filed on the
same branch.
