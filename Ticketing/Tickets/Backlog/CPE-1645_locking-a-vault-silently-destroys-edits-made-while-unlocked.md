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
