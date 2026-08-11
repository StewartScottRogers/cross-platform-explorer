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
