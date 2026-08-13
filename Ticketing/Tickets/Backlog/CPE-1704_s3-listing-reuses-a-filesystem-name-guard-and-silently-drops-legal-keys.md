---
id: CPE-1704
title: S3 listing reuses a filesystem name guard, so a legal S3 key silently vanishes from the explorer
type: bug
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-13
closed:
---

## Problem

Found by the independent UAT on PR #888 (CPE-1683), which drove `S3Provider::list` with keys that are legal
in S3 but awkward for a filesystem.

`S3Provider::list` reuses `crates/server`'s `is_safe_name` — the traversal guard written for local paths,
SFTP and WebDAV (CPE-1461). It is correctly conservative about escaping a prefix, and **the security
property holds: no key can produce an entry outside the listed prefix.** That is verified and not in
question here.

The problem is that it imports filesystem semantics into a keyspace that does not have them, and the
failure mode is silent.

### 1. A key containing `:` disappears with no error

`is_safe_name` rejects any leaf containing `:` — a Windows drive-letter / NTFS alternate-data-stream
hardening rule. **S3 has no ADS concept and `:` is a completely legal key character.**

So an object named `colon:name.txt`, sitting in the bucket, is **absent from the listing** with no error, no
warning, and no indication anything was filtered. From the user's side that is indistinguishable from data
loss: the file is there, they cannot see it, and nothing says why.

### 2. A key containing a literal `../` segment becomes a phantom empty folder

`a/../b.txt` is a **real, distinct key** in S3 — not a path to normalise. Today it produces a
seemingly-empty virtual directory `a/` at the root; descending into it shows nothing, and `b.txt` is
unreachable anywhere in the tree.

The guard is doing its job (the deeper leaf is literally `..`, correctly refused, so nothing escapes) — but
the user-facing result is a legitimate object vanishing behind what looks like an ordinary empty folder.

Both are the failure the CPE-1683 UAT brief named explicitly as the one to flag: **a legitimate key that
vanishes is worse than an ugly one that shows up.**

## Why this is High but not yet user-visible

`crates/s3` is not wired into the app yet — **CPE-1685** is the ticket that routes `s3` through
`cpe_vfs::open`. So no user can hit this today, which is why PR #888 merged with it open rather than being
blocked.

**It must be fixed before CPE-1685 lands.** A note has been added to that ticket making this a prerequisite.
Shipping a file explorer that silently hides files in a connected bucket is not an acceptable first
impression of S3 support.

## Scope

`crates/s3/src/provider.rs`, and a new S3-appropriate name check. **Do not loosen `crates/server`'s
`is_safe_name`** — it guards local paths, SFTP and WebDAV, where the `:` rule is correct and load-bearing.
This needs a sibling that encodes S3's rules, not a weakening of the shared one.

## Acceptance criteria

- [ ] A key containing `:` appears in the listing. A test pins it, naming S3's key rules as the reason so
      nobody "re-hardens" it later.
- [ ] The security property is **unchanged**: no key can produce an entry that escapes the listed prefix.
      Re-run PR #888's own traversal test (`a_content_key_with_a_traversal_segment_or_embedded_slash_is_dropped`)
      plus the UAT's set — `..%2f`, `%2e%2e/`, a key that is exactly `..`, a leading `/`, a backslash key,
      an embedded NUL, an embedded newline. **Breaking the guard must still turn a distinct test red.**
- [ ] Decide what happens to a key the guard genuinely must refuse, and make it **not silent**. Options:
      surface it under a visibly-escaped display name, or report that N entries were filtered. Either is
      acceptable; dropping it invisibly is not. Record the choice.
- [ ] Decide what `a/../b.txt` should look like to a user, and write the reasoning down. It is a real key
      and it has to be *reachable* or *visibly explained* — a phantom empty folder is neither.
- [ ] `crates/server`'s `is_safe_name` is untouched, or if it is touched, SFTP and WebDAV are re-verified
      against their own traversal tests.
- [ ] Each guard broken **on its own** turns a **distinct** test red, real output pasted in the PR, per the
      Evidence Rules in `Ticketing/wiki.md`.

## Notes

Filed by the Foreman from the PR #888 UAT, 2026-08-13. The UAT correctly judged it non-blocking for
CPE-1683 — that ticket's AC5 only requires that no entry escapes the listed prefix, which is true — and
recommended exactly this follow-up.

Related: **CPE-1683** (which introduced the reuse), **CPE-1685** (which would make it user-visible —
blocked on this), **CPE-1461** (the traversal guard being reused), **CPE-1684** (the sibling object-ops
ticket, which will hit the same key-shape questions for stat/read/write).
