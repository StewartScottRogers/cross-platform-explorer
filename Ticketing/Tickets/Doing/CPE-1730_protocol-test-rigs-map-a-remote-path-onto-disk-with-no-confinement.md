---
id: CPE-1730
title: The FTP, SFTP and WebDAV test rigs map a remote path onto disk with no confinement to their root
type: bug
priority: Low
status: In Progress
tags: ready
estimate: S
created: 2026-08-14
closed:
---

## Problem

Found by CPE-1726 while checking the sibling destructive primitives in the three protocol crates, and
deliberately **left out of that ticket's scope** — CPE-1726 is about what a rename does to a *link* at the
destination; this is about the destination being outside the served tree at all.

Each crate's in-process fake server maps a client-supplied path onto disk with a bare join and no
containment check:

- `crates/ftp/src/lib.rs` — `real_path(root, ftp_path)` is `root.join(ftp_path.trim_start_matches('/'))`
- `crates/sftp/src/lib.rs` — `FsSftp::real` is the same shape
- `crates/webdav/src/lib.rs` — `root.join(url.trim_start_matches('/'))`, and `MOVE`'s destination takes a
  second, separate path from the `Destination` header

So a request naming `../../something` resolves **outside** the rig's temp root. Everything under that root
is then a live destination for the rigs' `fs::write`, `remove_file`, `remove_dir_all` and `fs::rename`.

## Why it is worth a ticket even though it is test-only code

Nothing sends such a path today: the shipped providers never emit `..`, and the rigs are `#[cfg(test)]`
(measured in CPE-1726 — that measurement is what let those renames stay unguarded). So this is **latent**,
not live, which is why it is Low and why it was not folded into CPE-1726.

But the blast radius is a developer's working tree, not a temp dir. These rigs run under `cargo test` on
this repo's own checkout, and `remove_dir_all` on a path that escaped its root is exactly how the janitor
incident destroyed a live worktree. The rigs are *also* used as deliberately hostile-server models
(`list_filters_out_hostile_traversal_names`, the `evil\..\..\escape` fixture in `cpe-sftp`), so a future
test that drives a `..`-shaped path through the rig rather than seeding it with `std::fs` is a plausible
next step, not a contrived one.

## Scope

`cpe_server::fsutil::contained_under(joined, root)` already exists and already returns the right shape,
and all three crates already depend on `cpe-server` (CPE-1726 measured that too — the CPE-1719
reimplement-rather-than-depend precedent does **not** apply here). So this is likely a small change: run
each resolver's result through it and answer the protocol's own "no such file" on refusal.

Check whether any existing test depends on the resolver being naive before changing it — at least one
hostile-server test seeds its fixture outside the rig, which is unaffected, but that must be confirmed
rather than assumed.

## Acceptance criteria

- [ ] Each of the three rigs refuses a path that resolves outside its root, answering the protocol's
      not-found/forbidden rather than touching the escaped path.
- [ ] A test per crate drives a `..`-shaped path over the real wire and asserts **a file outside the root
      is still there with its bytes**, never on the returned status alone.
- [ ] The guard broken on its own turns a distinct test red, real output pasted (Evidence Rules,
      `Ticketing/wiki.md`).

## Notes

Filed by CPE-1726 (PR for that ticket), 2026-08-14. Related: **CPE-1461** (the traversal guard the
*clients* already carry, on server-supplied listing names — this is the mirror image, on rig-supplied
request paths), **CPE-1726** (the `#[cfg(test)]` confinement measurement and the guard that pins it),
**CPE-1719** (`fs::write` follows a link).
