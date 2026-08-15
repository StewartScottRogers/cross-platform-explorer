---
id: CPE-1748
title: remote_stat and remote_read collapse a directory onto its same-named file, reopening CPE-1737 the moment they are wired up
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-14
closed:
---

## Problem

Found by the PR #908 (CPE-1737) UAT, 2026-08-14, and reproduced. Filed rather than folded in because it is
in **currently-dead code** — the defect cannot bite a user today, only the moment that code is wired up.

CPE-1737 fixed the listing collision: an S3 object `photos` and a prefix `photos/` are independent rows, and
their `DirEntry.path` values are now distinguished by a trailing `/` on the directory. That fix is in
`crates/vfs/src/connect.rs`'s `child_uri` / `join_remote`, and it holds for **listing**.

`provider_path_to_object_key` trims slashes off **both ends**. So on the same colliding keyspace:

```
S3Provider::stat("/photos/")   ->  is_dir = false   // the DIRECTORY's own path
S3Provider::stat("/photos")    ->  is_dir = false   // the FILE
```

Both resolve to the **file**. The trailing slash the listing fix just added — the only thing distinguishing
the two rows — is discarded on the way in. A user who clicks the folder row gets the file.

## Why it is not live

The shipped command surface (`src-tauri/src/lib.rs`) routes only remote **listing** through
`cpe_vfs::connect` (`remote_list_dir_impl` / `_stream`). `open_external` (double-click) is a raw OS-shell
launch that never touches a remote provider, and `cpe_vfs::connect::remote_stat` / `remote_read` are dead
code — grepped tree-wide, zero call sites outside their own tests. Remote file *open* is not implemented at
all.

## Why it must be fixed before CPE-1499

CPE-1499 is the ticket that wires remote file operations onto exactly these functions. Wiring them up as
they stand reopens CPE-1737's ambiguity on the operation where it does real harm: listing merely *showed*
two rows, but `stat`/`read` would silently *act* on the wrong one. Same family as the already-filed
CPE-1735 gap on `delete`.

## Acceptance criteria

- [ ] A path ending in `/` resolves to the **prefix**, and the same path without the slash resolves to the
      **object**, for `stat` and `read` alike, on the colliding keyspace CPE-1737 uses
      (`["photos", "photos/a.jpg", "photos/b.jpg"]`).
- [ ] The distinction survives the whole way from a `DirEntry.path` produced by `child_uri` to the key sent
      to the provider — no intermediate normalisation discards it. Trace and test the full path, not just
      the endpoints.
- [ ] `delete` is checked for the same defect (see CPE-1735 item 2) and either fixed here or its state
      recorded.
- [ ] Breaking the fix turns a **distinct** test red, and the assertion names the wrong-target damage (which
      object was acted on), not merely a mismatched string.
- [ ] The other remote backends (SFTP/FTP/WebDAV) are checked for the same both-ends trim, since the fix
      that motivated this lives in the shared `cpe-vfs` layer.

## Notes

Related: CPE-1737 (PR #908, the listing half), CPE-1735 (the `delete` sibling), CPE-1499 (the consumer that
makes this live).
