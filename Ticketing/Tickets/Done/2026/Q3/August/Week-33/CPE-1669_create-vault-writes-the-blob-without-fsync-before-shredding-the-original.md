---
id: CPE-1669
title: "create_vault writes the blob without fsync, then shreds the original — a power loss in that window loses both copies"
type: bug
priority: Medium
status: Backlog
component: Backend
tags: ready
estimate: S
created: 2026-08-12
closed:
---

## Problem

Found by the independent reviewer of PR #847 (CPE-1645), blocker C, while checking the **re-seal's**
durability. The re-seal was fixed in that PR; `create_vault` has the identical gap and was deliberately
left alone rather than widening a security fix.

`create_vault` (`crates/server/src/vault_manager.rs`) does:

```rust
let blob = vault_crypto::encrypt_tree(folder, passphrase)?;
std::fs::write(dest_blob_path, &blob)?;   // no sync_all
...
verify(dest_blob_path, passphrase)?;      // reads it straight back — from the page cache
shred_tree(folder, opts.shred_scheme)?;   // destroys the plaintext, with flush + sync_all per pass
```

So the *destruction* is durable and the *replacement* is not. `verify_recoverable` re-reads the file it
just wrote, which on every mainstream OS is served from the page cache — it proves the bytes **parse**,
not that they reached the disk. If the machine loses power between the write and the shred completing,
the user can be left with a `.cpevault` whose directory entry points at unwritten data and no plaintext
original, having been told the copy was verified first.

This is the same shape as the re-seal gap fixed in PR #847, and the same fix applies.

## Fix

- `File::create` + `write_all` + `sync_all` on the blob **before** `verify` runs.
- On Unix, fsync the destination's parent directory after the file is created, so the directory entry is
  durable too.
- Only then allow `shred_tree` to run.

Consider factoring the re-seal's `write_new_exclusive` (already does write + `sync_all`) so both paths
share one durable-write helper.

## Acceptance criteria

- [ ] `create_vault` fsyncs the blob (and, on Unix, its parent directory) before the verify step, and
      therefore before any shred.
- [ ] A test proves the ordering — e.g. an injected writer/verifier seam asserting `sync_all` happens
      before `verify`, in the falsifiable style `create_vault_with_verifier` already uses.
- [ ] `cargo clippy --all-targets -D warnings` clean in both feature modes; crates/server suite green.

## Notes

- Source: independent review of PR #847, blocker C, 2026-08-12. Filed by the PR author at the reviewer's
  request rather than widening that PR.
- Related: [[CPE-1645]] locking a vault re-seals (where the same gap was closed for the re-seal),
  [[CPE-1248]] the verify-before-shred invariant this strengthens.
- Not urgent (it needs a power loss inside a sub-second window, and only bites the opt-in
  `shred_original` path), but it is the one remaining place where this module destroys data against an
  unproven copy.

## Work Log

- 2026-08-12 — Filed from the PR #847 review.
