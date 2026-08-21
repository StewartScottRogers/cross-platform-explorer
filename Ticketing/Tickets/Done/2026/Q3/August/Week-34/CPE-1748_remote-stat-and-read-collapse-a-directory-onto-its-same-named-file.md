---
id: CPE-1748
title: remote_stat and remote_read collapse a directory onto its same-named file, reopening CPE-1737 the moment they are wired up
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-14
closed: 2026-08-20
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

## Work Log

**2026-08-20 — fixed, still latent, not yet reachable from the shipped app.**

Confirmed the "not live" framing before touching anything, and it goes one layer further than the ticket
already said. Not only do `crates/vfs::connect::remote_stat`/`remote_read` have zero call sites outside
their own tests (re-confirmed by re-grepping `src-tauri/src/lib.rs`: only `remote_list_dir_impl` /
`remote_dir_entries` route through `cpe_vfs::connect`, nothing calls `remote_stat`/`remote_read`) — the
crate that actually holds the bug, `cpe-s3`, is not even a dependency of `cpe-vfs` or `src-tauri` yet.
`cpe_vfs::open` explicitly refuses `s3://` as an unsupported scheme
(`crates/vfs/src/lib.rs` test `an_unsupported_scheme_is_a_clear_error`), and `crates/vfs/Cargo.toml`
carries no `cpe-s3` dependency line at all. So this PR changes a crate that is not compiled into the
shipped binary today. The fix is real and worth landing now (CPE-1499 wires it up later and this closes
the hole before that happens), but nothing in it changes today's running app's behaviour.

**Root cause, matched to CPE-1737's approach.** CPE-1737 fixed *listing* by threading `is_dir` through
`child_uri`/`join_remote` so a directory child's URI carries a trailing `/`. That distinguishing bit
reaches `crates/vfs::connect::remote_stat`/`remote_read` intact — `location::parse` preserves a path's
trailing slash verbatim, and neither function normalises it away before calling
`provider.stat`/`provider.read`. The bug is one layer further in, inside `cpe-s3::provider::S3Provider`:
`provider_path_to_object_key` computes the SAME object key for `/photos` and `/photos/` (it strips at
most one trailing slash as a matter of course, unconditionally), so `stat`/`read`/`delete` had no way to
tell "the object" from "the same-named prefix" once that key was in hand — the trailing slash was
discarded by the object-key derivation itself, not lost anywhere in `cpe-vfs`.

**Fix**: added `path_addresses_a_directory(path, key)` in `crates/s3/src/provider.rs` — true when `path`
carries a trailing `/` that the key derivation actually consumed (i.e. `path.ends_with('/') &&
!key.ends_with('/')`; the `!key.ends_with('/')` half matters because CPE-1722 gave a *double* trailing
slash a real, different meaning — `/a.txt//` addresses the literal object key `a.txt/`, not a directory,
and must keep working). Wired it into:
- `stat`: an explicitly directory-addressed path now resolves the prefix directly, never falling through
  to the object HEAD that used to make `/photos/` answer `is_dir: false`.
- `read`: an explicitly directory-addressed path is refused outright (a directory has no bytes to return)
  instead of silently GETting the same-keyed object.
- `delete` (AC3 — checked and partially fixed, rest recorded below): the HEAD-proves-object fallback
  (used when the `s3:ListBucket` probe itself fails) is now gated on the SAME check, so an explicit
  directory delete can no longer silently delete the unrelated same-named object just because the probe
  that would confirm the directory's contents happened to fail.

**Delete — what stayed CPE-1735's, per this ticket's instruction not to take that ticket on.** The
narrow slice above is fixed. The rest of CPE-1735 item 2 — when the `s3:ListBucket` probe SUCCEEDS and
finds real content under the colliding prefix, `delete("/photos")` (the bare, object-addressed path) is
still refused as "directory with content", because the probe-first arbitration doesn't yet consult
`path`'s own trailing-slash intent in that branch. That is CPE-1735's own documented, harder half
("granting a permission makes an operation impossible"), explicitly gated on a real gateway
confirming the collision is reachable in practice, and explicitly out of scope for this ticket per the
Foreman's brief. Recorded, not fixed, here.

**SFTP/FTP/WebDAV (AC5) — checked, no both-ends trim found, no collision possible.** Grepped all three
backends' `stat`/`read`/`delete` for a `trim_matches`-shaped (both-ends) slash strip: none exists. Each
uses at most a one-sided trim, and only for computing a *display name* in `stat`
(`crates/ftp/src/lib.rs:301`, `crates/webdav/src/lib.rs:292` trim a trailing slash;
`crates/sftp/src/lib.rs:334` takes the last non-empty segment) — never before the byte reaches the wire.
More fundamentally, the CPE-1737/CPE-1748 collision needs a backend where an object and a prefix of the
same name can coexist; SFTP/FTP/WebDAV all address a REAL filesystem, where a file and a directory can
never share one name in one parent. The collision is structurally S3-specific (a flat key/value store
where `photos` and `photos/a.jpg` are just two independent strings). Nothing to fix on the other three.

**Tests added** (`crates/s3/src/provider.rs`, `crates/vfs/src/connect.rs`), all over the exact colliding
keyspace `["photos", "photos/a.jpg", "photos/b.jpg"]` CPE-1737 uses:
- `stat_on_the_colliding_keyspace_resolves_the_trailing_slash_to_the_prefix_and_the_bare_path_to_the_object`
- `read_on_the_colliding_keyspace_refuses_the_trailing_slash_and_reads_the_bare_path_as_the_object`
- `delete_of_an_explicit_directory_path_never_falls_back_to_deleting_the_same_named_object_on_a_failed_probe`
- `crates/vfs::connect::the_trailing_slash_that_marks_a_directory_reaches_the_provider_unchanged_for_stat_and_read`
  (traces the FULL path per AC2: builds the two child URIs through the real `child_uri`, feeds them back
  into `remote_stat`/`remote_read`, and asserts on the literal string a recording fake provider received)

**Red-proof for each (minimal one-line break, observed red, reverted — exact lines):**
- `stat` guard: `crates/s3/src/provider.rs` line (the `if path_addresses_a_directory(path, &key) {` that
  opens the new branch in `stat`) → changed to `if false && path_addresses_a_directory(path, &key) {`.
  Red: `stat("/photos/") must resolve to the DIRECTORY — got a FILE result (is_dir: false)...`.
- `read` guard: the matching line in `read` → same `if false &&` change. Red: `reading a path that
  explicitly addresses a directory must be refused... : [100, 97, 116, 97]` — literally `b"data"`, the
  file's own bytes, returned for what was asked as a directory.
- `delete` guard: `if !path_addresses_a_directory(path, &key) && self.head_proves_object(&key)...` →
  removed the `!path_addresses_a_directory(path, &key) &&` clause. Red: assertion named the actual
  damage — `outcome was Ok(())`, and the surviving key set dropped from
  `["photos", "photos/a.jpg", "photos/b.jpg"]` to `["photos/a.jpg", "photos/b.jpg"]`: the wrong,
  unselected object silently deleted.
- Full-path trace: `crates/vfs/src/connect.rs`'s `remote_read` → `provider.read(&loc.path)` changed to
  `provider.read(loc.path.trim_end_matches('/'))`. Red: the fourth recorded path flipped from
  `"/srv/photos/"` to `"/srv/photos"` — the exact byte lost.

**Live-rig hazard (per the Foreman's brief) — none of this evidence comes from a rig; all of it is a
custom in-process fixture.** The S3 tests use `spawn_a_keyspace_server_with_listbucket` /
`_without_listbucket`, an in-process `tiny_http` server this crate's OWN test module hand-writes and
hand-maintains (already used by the pre-existing CPE-1727/CPE-1737 collision tests) — not a third-party
rig, and not a real S3-compatible endpoint. It is honest about what it models (HEAD/GET/DELETE/
`ListObjectsV2` over a flat key set) and, being written by the same hand as the code under test, carries
the same-author-fixture risk CPE-1659's Work Log already flagged for WebDAV: it cannot by construction
catch a bug in an assumption the fixture and the code share. No S3-compatible endpoint (real or MinIO/
Ceph) was reachable in this environment, and the QNAP NAS on the LAN speaks SFTP/WebDAV/FTP/SMB/NFS, not
S3, so it cannot exercise this path either — this fix carries **no live-endpoint coverage**, consistent
with CPE-1735's own note that a real gateway is needed to confirm the collision is reachable in practice.

**Gates run** (only the two crates touched — `crates/net`/`crates/server`/`src-tauri` were not touched
and do not depend on either `cpe-vfs` or `cpe-s3`, confirmed by grepping their `Cargo.toml`s):
- `cargo clippy --all-targets -- -D warnings` — clean, exit 0, both `cpe-s3` and `cpe-vfs`.
- `cargo test` (`--lib`) — `cpe-s3`: 205 passed, 0 failed. `cpe-vfs`: 24 passed, 0 failed.

Left in Backlog (not moved) per the Foreman's instructions — this PR closes the ticket's code scope but
folder placement is the Foreman's call at merge time.
