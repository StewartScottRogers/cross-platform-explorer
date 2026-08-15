---
id: CPE-1749
title: read and stat accept any 2xx, so an unsolicited 206 truncates a download into a silent success
type: bug
priority: High
status: Backlog
tags: ready
estimate: S
created: 2026-08-14
closed:
---

## Problem

Found and **proven** by the PR #911 (CPE-1740) reviewer, 2026-08-14. Filed rather than folded in because
CPE-1740 scoped itself to the listing path — but its own opening line claims the sweep is crate-wide, and
shipping that claim while a proven data-loss path has no ticket behind it is the thing this repo does not
do.

CPE-1740 narrowed `list`'s per-page status check from "any 2xx" to `status != 200`. Two sibling sites in
`crates/s3/src/provider.rs` that also consume a document **where completeness is the whole point** still
accept any 2xx:

| Site | Purpose | Consequence of a non-200 2xx |
|---|---|---|
| `provider.rs:1997` | `read` — GET the object body | **a truncated file, reported as success** |
| `provider.rs:1924` | `stat` — HEAD, reads `Content-Length` | under a `206` the length is the *range* length, so the reported object size is wrong |

### Measured reproduction (`read`)

Fixture: the server answers an unsolicited `206` — the request carries no `Range` header, and the fixture
asserts that — with the 4-byte body `HALF`.

```
READ 206 >>> Ok([72, 65, 76, 70])
```

`FileSystemProvider::read`'s own doc comment (`provider.rs:1980-1982`) says:

> An over-cap read is a loud `Err`, never a truncated `Vec`… `cpe_server::transfer`'s download sink writes
> whatever comes back to disk as the finished file, so a silent truncation here would be data loss wearing
> a success.

A `206` walks straight past that guard. This is materially **worse** than the listing case CPE-1740 fixed:
a short folder is visible to the user, a truncated download is not.

## Why 200-only is the right answer here too

Established by the same review, from the AWS `ListObjectsV2`/GetObject contract: `206` is legitimate **only**
in answer to a `Range` header, which this client never sends (verified — `signed_request` passes no `Range`,
`provider.rs:1994` and `:1764`). So an unsolicited `206` is by definition not the whole document. `203` is
RFC 9110 §15.3.4's transforming-proxy answer — an explicit statement that something rewrote the payload.
`204`/`205` carry no body. MinIO, Ceph, R2 and B2 all implement the same documented contract.

## Acceptance criteria

- [ ] `read` refuses any status that is not exactly `200`, and refuses an unsolicited `206` with a message
      that names the status and says a complete body was required.
- [ ] `stat` refuses any status that is not exactly `200`, so a range `Content-Length` can never be reported
      as the object's size.
- [ ] The `206`-truncation reproduction above turns a **distinct** test red when the guard is removed, and
      the assertion names the truncated bytes (the harm), not merely a mismatched status — assert the effect
      **before** unwrapping the `Result`, since this defect fails by returning `Ok`.
- [ ] The fixture serving the `206` asserts that the request carried **no** `Range` header, so the test
      cannot pass against a legitimately-ranged reply.
- [ ] The remaining 2xx-accepting sites are re-checked and each is either narrowed or recorded as
      deliberately status-agnostic with the reason (`signed_exchange`'s `ok` flag at `:1361`,
      `head_proves_object` at `:1670`, and the PUT/DELETE sites at `:2051`/`:2082`/`:2237`/`:2311`, which
      consume no document).

## Notes

Related: CPE-1740 (PR #911, the listing half), CPE-1727 (which narrowed `probe_prefix_after` first).
