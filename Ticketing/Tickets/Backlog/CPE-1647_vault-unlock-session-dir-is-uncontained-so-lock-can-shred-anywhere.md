---
id: CPE-1647
title: "vault_unlock takes session_dir straight off the IPC boundary with no containment check — so vault_lock can shred an arbitrary directory"
type: Bug
status: Backlog
priority: High
component: Backend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Found by the independent Reviewer/Security lens on CPE-1630 (PR #836), while verifying that PR's claim that
`wipe_session_dir` was safe to leave ungated. The claim rests on an assumption the backend does not enforce.

**Pre-existing** — it predates CPE-1611 and CPE-1630 and was neither caused nor worsened by them.

## The gap
`vault_unlock` (`src-tauri/src/lib.rs:7003-7018`) takes `session_dir: String` **directly off the IPC
boundary with zero backend validation** that it resolves under the app's own `vault-sessions` cache root.
That containment exists only by frontend convention — `vaultStore.ts:114` builds
`join(base, "vault-sessions", crypto.randomUUID())`. Nothing in `unlock_to_session` or
`VaultRegistry::unlock_with_wiper` (`vault_manager.rs:249-256, 364-390`) checks the path at all.

The contrast is instructive: `create_vault` in the very same file **does** have exactly this kind of guard —
`resolves_inside` (`vault_manager.rs:222-240`) — to stop the destination blob landing inside the folder being
shredded. The pattern is established; it just isn't applied here.

**Consequence.** A devtools or automation caller holding a valid `.cpevault` blob and its passphrase can:

    vaultUnlock(blob, passphrase, "C:\Users\<you>\Documents")   // decrypts the vault's plaintext INTO Documents
    vaultLock(blob)                                             // -> wipe_session_dir -> shred_tree on Documents

and **every file under that directory is securely shredded** — including files that predate the vault and
have nothing to do with it — then the directory is removed. No confirmation of any kind, unlike `shred_paths`
(CPE-1611) and `vault_create` (CPE-1630) after this sprint.

So the blast radius is far larger than "wipes app-owned scratch data", which is how the exemption was
justified.

## What is NOT the fix
Adding a `confirmed` flag. Locking genuinely *should* always wipe its session directory — that is the
feature. The problem is that the directory being wiped is caller-chosen and unvalidated.

## Fix
Canonicalize `session_dir` in `unlock_to_session` and assert it resolves **under the app's `vault-sessions`
cache root**, mirroring `create_vault`'s existing `resolves_inside` guard. Refuse cleanly otherwise, with the
same error shape CPE-1599/1611/1630 established so all four refusals read alike.

Points to get right:
- **Canonicalize before comparing** — a path-prefix string test is defeated by `..`, by a symlink or junction
  in the session path, and by the `Photos` vs `Photos2` boundary problem. CPE-1613/CPE-1623 fought exactly
  this; reuse `path_key`-style identity comparison rather than inventing a third approach, and **fail closed**
  on any resolution failure.
- The session root is app-owned, so a strict containment check should have no legitimate false positives —
  verify that by running the real unlock/lock round-trip.

Also worth doing here: the orphan sweep (`sweep_orphan_sessions`, `vault_manager.rs:289-291`) was checked and
is **safe** — it only reads immediate children of a hardcoded `sessions_root` and does not walk above it. Say
so in the work log so the next auditor doesn't re-derive it.

## Acceptance criteria
- `vault_unlock` refuses a `session_dir` outside the app's session root; a test proves an arbitrary directory
  (with pre-existing files in it) is neither unlocked into nor shredded, verified by reading bytes off disk.
- The refusal is cleanly distinct and matches the CPE-1599/1611/1630 message shape.
- Symlink/junction/`..` variants of an out-of-root path are refused; every resolution-failure branch fails
  closed. Negative control per case.
- The normal unlock → edit → lock round-trip is unaffected.

**Conflict surface:** `crates/server/src/vault_manager.rs` (`unlock_to_session`, `unlock_with_wiper`),
`src-tauri/src/lib.rs` (`vault_unlock`), plus tests. Overlaps **CPE-1645** (locking silently destroys edits
made while unlocked) — both concern what `lock` does to the session directory, so design them together.
