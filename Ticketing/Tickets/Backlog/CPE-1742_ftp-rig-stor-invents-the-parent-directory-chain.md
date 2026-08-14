---
id: CPE-1742
title: The FTP rig's STOR invents the parent directory chain, which no real daemon does
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-14
closed:
---

## Problem

Found by **CPE-1731**, 2026-08-14, while fixing the sibling verbs.

`crates/ftp/src/lib.rs:~526`, inside the `#[cfg(test)]` rig's `STOR` handler:

```rust
let path = real_path(&root, &arg);
if let Some(p) = path.parent() {
    let _ = std::fs::create_dir_all(p);
}
```

RFC 959 `STOR` has **no** directory-creating semantics. A real daemon answers `550` when the parent
directory does not exist; it does not create it. So a client test can upload to `/a/b/c.txt` against
this rig with no `MKD` at all and pass, against behaviour no real server has — the same family as
CPE-1731's `RMD`/`MKD` fixes, one verb over.

## Scope

`crates/ftp/src/lib.rs`, the `STOR` arm.

**This is a client-visible change, which is why CPE-1731 did not do it as a drive-by.** Unlike
`RMD`/`MKD` — where the primitive simply did not match the verb — removing this changes what a *client*
must do before uploading, so the fix is "make `STOR` refuse a missing parent" **plus** whatever the FTP
provider needs to create directories first. Check `cpe_server::transfer::upload_tree` and CPE-1741
before starting; the two interact.

Measured while filing and **re-measured at the current suite size**: removing the `create_dir_all`
leaves `cpe-ftp` **14/14 green**, and this crate has no `upload_tree` test at all — so nothing *in this crate* depends on it. That is a statement about
this crate's coverage, not about the client being correct.

## Acceptance criteria

- [ ] `STOR` to a path whose parent does not exist is answered `550`, and the parent chain is **not**
      created — asserted on the filesystem.
- [ ] `STOR` to a path whose parent exists still works, with the bytes asserted (no over-rejection).
- [ ] Any client path that relied on the invention is fixed rather than the rig being loosened back.
- [ ] The guard broken on its own turns a distinct test red, real output pasted.

## Notes

Filed from CPE-1731 (PR #905), where the deliberate non-change is recorded **at the `STOR` arm itself**
(`crates/ftp/src/lib.rs:~525`), with the full reasoning in the `MKD` arm below it. An earlier draft of
this line claimed the note was at the site when it was only in `MKD`, thirty lines away. Related:
**CPE-1741** (`upload_tree`'s unconditional `mkdir(&base)`).
