---
id: CPE-1741
title: upload_tree fails when the remote base directory already exists
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-14
closed:
---

## Problem

Found by **CPE-1731**, 2026-08-14, as a direct consequence of making a test double honest.

`cpe_server::transfer::upload_tree` (`crates/server/src/transfer.rs:~744`) opens with an
unconditional

```rust
provider.mkdir(&base)?; // ensure the remote root exists
```

`mkdir` maps to `SSH_FXP_MKDIR` / FTP `MKD`, both of which are `mkdir(2)`: a real server answers
`SSH_FX_FAILURE` / `550` when the name **already exists**. So the `?` aborts the whole upload, and
**uploading into a remote folder that already exists fails** — which is the ordinary case for a user
syncing into an existing directory.

This is not a new defect. It has always been broken against a real OpenSSH or vsftpd server; the reason
nobody saw it is that `cpe-sftp`'s and `cpe-ftp`'s `#[cfg(test)]` rigs implemented `mkdir` with
`create_dir_all`, which is idempotent — the double was more forgiving than the wire, so the client test
passed against behaviour no real server has. CPE-1731 changed both rigs to `create_dir`, which is what
made it measurable:

```text
mkdir onto the seeded /sub    = Err("/sub: Failure: Failure")
upload_tree -> /up (new)      = Ok(1)
upload_tree -> /up (existing) = Err("/up: Failure: Failure")
```

The suite stays green because its one `upload_tree` test uploads to a fresh base.

## Scope

`crates/server/src/transfer.rs` — `upload_tree`'s `mkdir(&base)` and the per-directory `mkdir(&remote)`
inside the walk, which has the same shape for any directory that already exists remotely.

The decision to take is which shape "ensure it exists" should have against a wire verb that has no
idempotent form:

1. tolerate an already-exists error from `mkdir` and continue (needs the providers to classify it —
   `SftpProvider` gets `SSH_FX_FAILURE`, which is *also* the generic failure, so this may not be
   cleanly distinguishable); or
2. `stat` first and only `mkdir` when absent (an extra round-trip per directory, and racy, though
   harmlessly so here); or
3. give the provider trait an explicit `mkdir_all`/`ensure_dir` so each backend can express it in the
   way its protocol allows.

Prefer whichever keeps the *provider* honest to its verb — CPE-1731's whole thesis is that a double
which is more permissive than the wire hides client bugs, and (1) risks re-introducing that at the
client layer instead.

## Acceptance criteria

- [ ] `upload_tree` into a remote directory that **already exists** succeeds and lands every file,
      asserted on the remote bytes.
- [ ] The same for a nested directory inside the tree that already exists remotely (partial re-upload).
- [ ] The rigs are **not** loosened back to `create_dir_all` to make this pass — the fix is in the
      client. A guard-neutralisation showing the new test red against the current `upload_tree`.
- [ ] Whatever classification the fix relies on is measured, not assumed: if it distinguishes
      already-exists from a generic failure, paste what each actually returns.

## Notes

Filed from CPE-1731 (PR #905). Related: **CPE-1742** (FTP `STOR`'s parent invention, the other half of
the same "the double is more permissive than the wire" family).
