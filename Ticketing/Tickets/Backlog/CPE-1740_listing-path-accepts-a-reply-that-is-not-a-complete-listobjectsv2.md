---
id: CPE-1740
title: The S3 list path still accepts a partial or non-authoritative reply as a complete listing
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-14
closed:
---

Found by the CPE-1727 (PR #903) UAT, rounds 3–5. **Pre-existing and crate-wide** — not introduced by
CPE-1727.

`crates/s3/src/provider.rs` decides "this reply is a listing" on two things: the status, and the body
parsing. A reply that satisfies both while being neither complete nor a listing reads as **an empty
listing**, which is the one answer that licenses a DELETE.

## What CPE-1727 already closed (do not redo)

Two of the three conditions were fixed inside PR #903, because on `delete`'s `start-after` belt the
consequence is a removed key:

1. **Root element.** `parse_list_bucket_result` now requires a `<ListBucketResult>` root. Before that, a
   proxy's `200 text/html` error page — well-formed XML with no `<Contents>` — read as "this prefix holds
   nothing", and the belt deleted the marker on it. Measured by this UAT at round 4:
   `delete = Ok(()); DELETEs sent = ["/test-bucket/photos/"]`.
2. **`206` on the belt.** `probe_prefix_after` narrowed from `200..300` to `status != 200`. Measured at
   round 4 before the fix: a `206 Partial Content` carrying a well-formed empty listing produced
   `*** DELETE WENT THROUGH (Ok) ***`.

## What remains, and is this ticket

**`FileSystemProvider::list` was not narrowed.** Its pagination loop still uses
`if !(200..300).contains(&status)`, so a listing whose status is not `200` is still read as complete and
authoritative. Measured at round 5, against a server answering `203` with an otherwise valid listing:

```
UAT5-3  list under a 203 = Ok(["a.jpg"])
```

The same is true of `206`: `list` will render a partial listing as the folder's complete contents. That
is the hazard `signed_exchange`'s over-cap guard already refuses in its own words — *"refusing rather
than parsing a truncated body, which can look like a complete but much shorter listing"* — arriving by
status code instead of byte count. `delete` is now protected from it; the pane the user actually reads
is not, and a folder that silently renders short is the same class of confident wrong answer.

`probe_prefix` (the non-belt probe, used by `stat` and `delete`'s first question) also still uses
`200..300` and should be considered in the same pass.

## A message defect on the path CPE-1727 did narrow

Also measured at round 5, and left here rather than reopened in PR #903 because the *behaviour* is right
(refusing is correct) and only the wording is wrong. A `203` on the belt now produces:

```
The server answered HTTP 203 — it did reply, and successfully; what failed was reading the reply. ...
Underlying error: s3: "scratch/": s3: HTTP 203 and the response body could not be read as an S3 error
(no <Code> element was found in it); refusing to guess which cause applies
```

Two things are wrong with that. Reading the reply did **not** fail — the reply was perfectly readable and
was rejected on its *status*. And `map_response_error` then hunted for an S3 `<Error>` document inside
what is a valid **listing**. Nothing tells the user that this client requires exactly `200` and something
in the path answered `203`. RFC 9110 §15.3.4 makes `203` the standards-legitimate answer from a
transforming proxy, so this is reachable behind a corporate MITM proxy or a CDN.

## Suggested fix

1. Narrow `list`'s and `probe_prefix`'s success check to `status == 200`, as `probe_prefix_after` already
   is. `ListObjectsV2` has exactly one success status.
2. Give the not-200 case its own message, on every path, saying that a listing must be `200` and naming
   what arrived — rather than routing a readable non-error body through the S3 *error* parser.

## Acceptance criteria

- [ ] `list` refuses a `203` and a `206` reply rather than rendering it as the folder's complete contents,
      and the message names the status and says a listing must be `200`.
- [ ] The belt's `203` message no longer claims reading the reply failed, and no longer reports "no
      `<Code>` element" about a body that is a listing.
- [ ] Each guard broken **on its own** turns a **distinct** test red, real output pasted, per the Evidence
      Rules in `Ticketing/wiki.md`. Assert on the effect (bytes sent / entries rendered) **before** the
      `Result`, so the assertion carrying the harm is reachable.

## Notes

Filed by the PR #903 UAT, 2026-08-14, and re-scoped at round 5 after the Foreman closed the two deletion
conditions inside that PR. Measurements above are from throwaway probes on the CPE-1727 branch; they were
not committed.

Related: **CPE-1727** (which closed the belt's half), **CPE-1684** (the delete decision), **CPE-1683** (the
listing path), **CPE-1736** (the sibling fixture limits), **CPE-1518** (the QNAP — the first real endpoint,
where a real gateway's statuses can finally be seen).
