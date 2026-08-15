---
id: CPE-1730
title: The FTP, SFTP and WebDAV test rigs map a remote path onto disk with no confinement to their root
type: bug
priority: Low
status: Done
tags: ready
estimate: S
created: 2026-08-14
closed: 2026-08-14
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

- [x] Each of the three rigs refuses a path that resolves outside its root, answering the protocol's
      not-found/forbidden rather than touching the escaped path.
- [x] A test per crate drives a `..`-shaped path over the real wire and asserts **a file outside the root
      is still there with its bytes**, never on the returned status alone.
- [x] The guard broken on its own turns a distinct test red, real output pasted (Evidence Rules,
      `Ticketing/wiki.md`).

## Work Log

### The scope note was wrong about `contained_under`, and it matters

The Scope section above says `cpe_server::fsutil::contained_under` "already returns the right shape". It
does not, **for these callers**, and the reason is written on `contained_under` itself: its documented
precondition is that the target is an *existing* path about to be removed, so it returns `Ok` when the
path does not canonicalise. Every call site here is the opposite case — a `STOR`/`PUT` target, an
`MKD`/`MKCOL` name, a rename destination — where *not existing yet* is the ordinary state. Reused as-is it
would have answered "contained" for `root/../evil.txt` (which does not exist, because nothing has written
it yet) and the rig would then have written it. Its own doc says so in as many words: *"Do not reuse this
to validate a create/copy destination."*

So this ticket adds a sibling, `cpe_server::fsutil::confined_to(path, root)`, which **fails closed** on
everything it cannot resolve.

### Containment is not equality — and lexical `..` popping is unsound in this direction

`same_place` (CPE-1731) may fall back to a lexical comparison when `canonicalize` fails, because for
*equality* the lexical `..` pop errs safe: popping only shortens a path, hence makes it *more* likely to
equal the root, hence more likely to refuse. **That proof does not transfer.** In the containment
direction the same pop errs unsafe — `root/link/..` pops lexically to `root` (contained, allow) while the
filesystem resolves it to `link`'s parent, which may be anywhere. `confined_to` therefore never pops
`..`: it asks the filesystem (canonicalise the deepest existing ancestor, follow dangling links by hand)
and refuses when the filesystem cannot answer.

### Three escape shapes, all measured

| # | shape | needs `..`? | needs an absolute path? |
|---|-------|-------------|-------------------------|
| a | `/../<sibling>/victim.txt` | yes | no |
| b | an **absolute** destination — `Path::join` discards its base | no | yes |
| c | through a **symlinked intermediate directory** (`/outlink/victim.txt`) | no | no |

(c) is the one no textual filter can see. (b) is Windows-only through these rigs, because all three trim
the leading `/` first, so a POSIX absolute path becomes relative and lands *inside* the root.

**(b) was already live in an existing regression row.** `cpe-webdav`'s CPE-1726 table has a row sending
`Destination: http://{host}/p://evil.example/ok.txt`, pinned at `404`. On Windows `p:` is a *Disk prefix*,
so `root.join("p://evil.example/ok.txt")` returns `"p://evil.example/ok.txt"` — measured — and the row's
`404` was an **errno** (drive `P:` does not exist), standing in front of a real escape. That row now
expects `403` on Windows and `404` on POSIX, with the measurement written down beside it.

### What each rig answers now

| rig | escapes the root | rename/MOVE source is the root |
|-----|------------------|-------------------------------|
| `cpe-ftp` | `550 Path escapes the served root` | `550 Rename source is the served root` |
| `cpe-sftp` | `Failure: path escapes the served root` | `Failure: rename source is the served root` |
| `cpe-webdav` | `403` | `409` |

Every one of those is unreachable from an errno: the FTP rig never puts `io::Error` text on the control
channel, `io_err` in the SFTP rig discards the error's text and returns a bare `StatusCode`, and the
WebDAV rig sends `403`/`409` from these two guards and nowhere else. Each is also deliberately **not**
CPE-1731/CPE-1726's code (`553` / `400`), so those tickets' tests keep reddening for their own reasons.

The source-root guard closes the gap CPE-1731 recorded at its own call sites (`RNFR /` + `RNTO /elsewhere`
moved the served root away and answered `250 Renamed`). The interesting finding is that **containment is
not what closes it** — the root is contained in itself by design, so `confined_to` allows `RNFR /`; it is
`same_place` on the *source* that refuses it.

### Evidence — the guards neutralised, one at a time

**A. `confined_to` forced to `true`.** All six confinement tests red, and in every case the *first*
failure is the damage assertion, not the status:

```text
cpe-ftp    ... assertion `left == right` failed: STOR to /../cpe-ftp-srv-…-cpe-1730-victim/victim.txt
               must NOT reach a file outside the served root (outcome was Ok(()))
                 left: Some([80, 87, 78, 69, 68])            <- "PWNED"
                right: Some([98, 121, 116, 101, 115, …])     <- the victim's bytes
cpe-sftp   ... assertion `left == right` failed: a write to /../cpe-sftp-srvroot-…-cpe-1730-victim/victim.txt
               must NOT reach a file outside the served root (outcome was Ok(()))
cpe-webdav ... assertion `left == right` failed: a PUT to /../cpe-webdav-…-cpe-1730-victim/victim.txt
               must NOT reach a file outside the served root (response was "HTTP/1.1 201 Created…")
```

`Ok(())` and `201 Created` — the bug reports success while the escape happens, which is exactly why every
leg asserts the filesystem *before* it looks at the outcome.

**B. the source-root `same_place` guards removed.** All three source tests red, and each shows the
"saved by an errno" trap in its purest form — with the guard gone the refusal is still a refusal, but it
is the OS's, not the server's:

```text
cpe-ftp    ... left: "550 Rename failed"          right: "550 Rename source is the served root"
cpe-sftp   ... Got Err("/ -> /moved-root: Failure: Failure")
cpe-webdav ... Got Err("/: HTTP 404")
```

### `..` never reached the WebDAV rig through the provider — a trap this test fell into first

`ureq` parses the request URL, so `provider.write("/../<sibling>/victim.txt", …)` is **normalised
client-side** and arrives at the rig as `/<sibling>/victim.txt`. The first draft of the WebDAV test drove
its legs through `WebdavProvider` and passed the victim-bytes assertion for the wrong reason — the escape
was never sent. Every path-bearing leg there is now a hand-written request on a raw socket (the rig reads
the request line with no URL parsing at all). The FTP and SFTP clients pass the path through verbatim, so
they drive their providers directly.

### Deliberately out of scope

- **`DELETE /` in `cpe-webdav` still removes the served root** (`204`). Containment cannot speak to it —
  the root is contained in itself — and it destroys only the rig's own temp root, never anything outside
  it. Recorded at the call site rather than changed.
- **TOCTOU.** `confined_to` is not atomic with the caller's primitive; a component could be swapped for a
  symlink in between. Closing that needs `openat2(RESOLVE_BENEATH)` or an `O_NOFOLLOW` walk, neither of
  which `std` offers. These are single-threaded in-process rigs, so it is recorded rather than solved —
  and `confined_to`'s doc says a real server must not treat it as sufficient.

## Notes

Filed by CPE-1726 (PR for that ticket), 2026-08-14. Related: **CPE-1461** (the traversal guard the
*clients* already carry, on server-supplied listing names — this is the mirror image, on rig-supplied
request paths), **CPE-1726** (the `#[cfg(test)]` confinement measurement and the guard that pins it),
**CPE-1719** (`fs::write` follows a link).
