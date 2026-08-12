---
id: CPE-1645
title: "Locking a vault silently destroys everything you edited while it was unlocked — nothing is ever re-sealed, despite the docs promising it"
type: Bug
status: Backlog
priority: High
component: Backend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Found by the independent UAT of CPE-1630 (PR #836) while sanity-checking the one shred caller deliberately
left ungated. It is **pre-existing** — CPE-1630 neither caused nor worsened it — but it is a live data-loss
path on a feature whose entire purpose is protecting the user's files.

## The bug, demonstrated
The tester followed exactly what the shipped docs tell a user to do (`src/docs/20-vaults.md`):

> "While unlocked, the vault behaves like an ordinary folder — you can browse, open, and edit its
> contents... Lock... to **re-seal** the vault"

Sequence run against the real code:
1. Sealed a vault.
2. Unlocked it (contents extracted to a session directory).
3. Wrote a **new file** into the session directory — the documented "edit its contents" affordance.
4. Locked it.
5. Unlocked again into a fresh session directory.

**The new file was gone.** Only the original sealed content remained. It was destroyed silently — no
confirmation, no verify-before-shred, no warning of any kind.

## The mechanism
`encrypt_tree` — the only thing that ever writes into a `.cpevault` blob — is called **only** from
`create_vault`. `VaultRegistry::lock` never calls it. Lock purely runs `wipe_session_dir`, which shreds the
extracted session directory.

So "lock" does not re-seal anything. It destroys the working copy and leaves the blob exactly as it was at
creation. **The word "re-seal" is wrong in both the code comment and the user documentation** — and a user
who believes the documentation will lose work.

## Why this is High
Every other destructive path in this app has been given a gate this sprint — Batch Media's in-place
overwrite (CPE-1590/1599), `shred_paths` (CPE-1611), `vault_create`'s shred-original (CPE-1630), and failed
checkpoints now leave a durable record (CPE-1600). This one destroys **user-authored content the app itself
invited them to create**, with no gate, no confirmation, and a documentation promise pointing the other way.

## Fix — decide the product behaviour first, then implement
This is a design decision, not just a code fix. The options, roughly:
- **Re-encrypt on lock** (what the docs already promise): diff or wholesale re-seal the session directory
  back into the blob before wiping. Must adopt `create_vault`'s existing safety discipline — **verify the
  new blob decrypts correctly before destroying the working copy** — otherwise the fix becomes a bigger
  version of the same bug.
- **Refuse to lock with unsaved changes**, telling the user plainly and offering to re-seal or discard.
- **Make the vault genuinely read-only while unlocked**, and correct the docs — the honest version of the
  current behaviour, but it removes a capability the docs advertise.

Re-encrypting is the most faithful to what users have been told, and is probably right — but whichever is
chosen, **the documentation and the code comment must stop saying "re-seal" unless it is true.**

## Acceptance criteria
- Editing a file inside an unlocked vault and locking it either preserves the edit or refuses with a clear
  explanation — never silently discards it. A test performs the exact five-step sequence above and asserts
  the edit survives (or that lock refuses), failing against today's code as a negative control.
- If re-encryption is implemented, the new blob is verified to decrypt correctly **before** the working copy
  is destroyed, matching `create_vault`'s existing guarantee.
- `src/docs/20-vaults.md` and the `lock` code comment describe what actually happens.
- Existing vault create/unlock/lock round-trips still pass unchanged.

**Conflict surface:** `crates/server/src/vault_manager.rs` (`lock`, `wipe_session_dir`, `encrypt_tree`),
possibly `vault_crypto`, `src/docs/20-vaults.md`. Related to the pre-existing CPE-1248/CPE-1249 gap.

## Work Log

- 2026-08-11 — **Product decision: re-encrypt on lock** (option 1), implemented with `create_vault`'s
  verify-before-destroy discipline. Shipped in one PR with [[CPE-1653]] and [[CPE-1654]] (same files).

  **Why re-seal and not refuse.** Three options were on the table.
  - *Refuse to lock with unsaved changes* preserves data but is a dead end as a product: the user edits a
    file, and now the vault can never be locked again without discarding the edit. "Refuse" only avoids
    data loss by leaving the decrypted plaintext on disk indefinitely — which is the security hole vaults
    exist to close — and every escape hatch it offers ("discard and lock") is the original bug with a
    dialog in front of it. It also needs change *detection*, and any cheap detector (size+mtime) can
    return a false "unchanged" and silently discard the edit anyway — reintroducing the exact bug.
  - *Make the vault read-only while unlocked* is honest but removes an advertised capability, and there is
    no cross-platform way to enforce it: the session dir is a real directory that any other program can
    write to, so we would be documenting a promise we cannot keep.
  - *Re-seal* is what the docs already promise, what users expect of "lock", and — done in the right
    order — is the only option that neither loses an edit nor leaves plaintext behind. Locking a vault you
    changed does the obvious thing; locking one you didn't is a no-op you cannot tell apart.

  It also satisfies the "cannot lose user data" test most strictly, because the ordering makes the
  guarantee, not a heuristic: encrypt → write a **staging file beside the blob** → re-read it **from disk**
  and decrypt it in full (`verify_blob`, in memory, no plaintext written) → rename over the vault → *only
  then* wipe the working copy. Any failure returns `Err` having wiped nothing, removed the staging file
  and left the old blob byte-for-byte intact, with the mapping kept so the lock is retryable and the
  user's edits are still reachable in the session folder. There is deliberately no "lock anyway" path: we
  never destroy a working copy we have not first proven we can reproduce.

  **Decide-and-log calls made along the way.**
  - *The passphrase is retained in memory while unlocked* (in the `Session`, as a zeroize-on-drop
    `SecretString`), because sealing needs it and prompting again at lock time would be a UX regression
    that also can't help a lock triggered by anything but a button. This is a strictly smaller exposure
    than the mount tradeoff v1 already accepts — the whole decrypted tree is on disk for that same window
    — and it still never persists. Documented in the module docs, VAULT-SECURITY.md §5, and the user docs.
  - *Always re-seal, never diff.* Skipping the re-seal when "nothing changed" would make the common case
    faster, but every cheap change-detector can be wrong in the direction that destroys data. Locking now
    costs about what creating the vault did; that is stated in the user docs.
  - *A vanished session dir locks cleanly* rather than erroring — there is nothing left to preserve, and
    erroring would wedge the vault "unlocked" forever with no user-reachable way to clear it.
  - *Two new refusals guard the re-seal itself*: a session path that is a link (following it would seal a
    stranger's files INTO the vault, replacing the real contents, as well as shredding them), and a vault
    file living inside the session dir (re-sealing there would write a good vault and then shred it with
    the working copy — the same hazard `create_vault`'s `resolves_inside` already guards).
  - *Not done, stated plainly:* re-sealing happens at lock, not continuously, so killing the app while
    unlocked still loses that session's edits; and `VaultRegistry::unlock` on an already-unlocked vault
    still supersedes + wipes the prior session dir (CPE-1249's deliberate no-orphaned-plaintext
    behaviour), discarding any edits in it. That path is unreachable from the UI — `App.tryUnlockVault`
    navigates to the live session instead of re-unlocking — and changing it would undo a reviewed
    decision, so it is documented in VAULT-SECURITY.md §5 as a residual rather than changed here.

  **The docs and the code comment now describe what actually happens** — `src/docs/20-vaults.md` gained a
  "What locking guarantees about your changes" section (and the memory-held passphrase in *Honest
  limits*), and the `lock` / `vault_lock` doc comments were rewritten.

  **Red→green.** `locking_re_seals_edits_made_while_unlocked_into_the_blob` performs the reporter's exact
  five-step sequence and fails on the unfixed code with *"a file CREATED while the vault was unlocked was
  DESTROYED by locking"*; `locking_re_seals_deletions_made_while_unlocked` fails likewise. Both pass after
  the change, alongside falsifiable tests for the injected verifier (the old blob survives byte-for-byte),
  the re-seal→wipe ordering (with a positive control), the two new refusals, and the vanished-session-dir
  case. All 32 pre-existing `vault_manager` tests still pass unchanged.

- 2026-08-12 — **Round 3 of the PR #847 review: the alias guard was check-then-USE.** The re-audit
  demonstrated a victim file zero-filled while `lock` returned `Ok(())` and the UI said "Locked".
  `ensure_no_aliased_files` walked the session tree once, at the top of the re-seal, and the link counts
  were never consulted again; `wipe_session_dir` → `shred_tree` → `collect_files` re-walked at the END and
  overwrote every regular file it found, hard links included, with no check of its own — the one
  destructive step in the module that leaned on a caller's earlier check. No race had to be won: the
  staging file appearing beside the `.cpevault` is a publicly observable starting gun proving the guard
  has already passed, so the attacker polls for it and then calls `create_hard_link(victim,
  "<session>/loot.xlsx")` — a registered IPC command, unprivileged on NTFS.

  **Fixed in two places.** (a) The session wipe re-reads each file's link count immediately before
  overwriting *that* file, and **unlinks** rather than overwrites anything not provably single-named —
  a name that has another name is not ours to destroy, and unlinking one of an inode's names destroys
  nothing. Fail-closed against destruction: an unreadable count disposes exactly like a known alias.
  `create_vault`'s optional shred-original keeps its old behaviour (`AliasPolicy::ShredEveryFile`) — that
  folder is the user's own pick, not an app-owned session tree. (b) `ensure_no_aliased_files` runs a
  second time after `encrypt_tree` and before the staging file exists, shrinking the confidentiality half
  to the encrypt walk alone and refusing before the blob is replaced — and before the starting gun fires.

  **Red→green.** The auditor's exploit test reproduced the destruction verbatim on this branch (victim
  read back as 32 zero bytes, `lock` = `Ok(())`); it is kept as
  `an_alias_planted_after_the_alias_guards_is_unlinked_not_shredded_through` with the assertion flipped,
  joined by the deterministic `the_session_wipe_unlinks_an_alias_instead_of_overwriting_it` (no thread),
  `an_alias_appearing_during_the_encrypt_walk_is_caught_before_the_blob_is_replaced` (via a new
  `after_encrypt` seam on `reseal_session_with_hooks`, the same falsifiable-injection shape the verifier
  already used) and the pure `the_wipe_never_overwrites_a_file_it_cannot_prove_is_ours`.

  **Three previously-unpinned guards, each deletable with the whole suite green, now pinned.**
  `sweep_stale_staging`'s "never delete an object we cannot prove we created" was `#[cfg(unix)]` — i.e.
  unenforced on Windows, the platform where the unprivileged hard-link primitive exists — and now goes
  through the same platform-independent `hard_link_count`
  (`the_sweep_leaves_a_hard_link_planted_at_a_staging_name`). `hard_link_count`'s fail-closed-on-`Unknown`
  is pinned by `a_link_count_that_cannot_be_read_is_refused_not_assumed_to_be_one` (a missing path, which
  exercises the error arm of both platform implementations). The `sync_all`-before-verify ordering is
  pinned by `the_staging_blob_is_fsynced_before_it_is_verified`: there is no portable way to ask the OS
  after the fact, so `sync_durably` counts its calls in test builds and the injected verifier reads the
  counter at the moment it runs. Every one of the six guards was neutralised on its own and confirmed to
  turn a distinct test red.

  **Two nits.** `App.lockActiveVault` now takes the `lockInFlightFor` claim inside the `try`, so a
  rejected `navigate` can no longer latch it forever and disable *every* vault's Lock button for the
  session (the banner binds `locking={lockInFlightFor !== null}`, not `=== blobPath`). The blocker-B
  cross-language guard enumerates `LockFailureCode` through an exhaustive `match`
  (`every_lock_failure_code`) instead of a hand-written list of four, so a fifth variant cannot be added
  silently. Also renumbered this branch's follow-up ticket CPE-1667 → **CPE-1669**: `main` had already
  merged a different CPE-1667 (batch-media) in #848, and VAULT-SECURITY.md pointed readers at the wrong bug.
