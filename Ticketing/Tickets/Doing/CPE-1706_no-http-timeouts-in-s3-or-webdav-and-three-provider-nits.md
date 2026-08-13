---
id: CPE-1706
title: S3/WebDAV have no HTTP timeouts, one test can hang CI forever, and the body cap is untested
type: bug
priority: Medium
status: In Progress
tags: ready
estimate: S
created: 2026-08-13
closed:
---

## Problem

Six items from the PR #888 (CPE-1683) review, rounds 1 and 2, judged non-blocking there and deliberately not fixed in
that PR. The first is the substantive one; the rest are grouped because they live in the same file.

### 1. No HTTP timeouts — wall-clock is completely unbounded *(the real one)*

`crates/s3/src/provider.rs:299` builds its agent as `AgentBuilder::new().redirects(0).build()` — no
`timeout_read`, no `timeout_write`, no `timeout`. **`ureq` 2.x defaults all three to `None`.**

`crates/webdav/src/lib.rs:61` does the same. This is inherited, not introduced by CPE-1683 — but S3 is
now the **second** crate carrying it, which is the point at which it stops being a one-off.

Every *other* bound in the S3 listing path is in place and was verified by the review: an 8 MiB body cap,
`MAX_LIST_PAGES = 1000`, `MAX_LIST_ENTRIES = 200_000`, a nesting-depth guard, DTD refusal. **Bytes and
memory are bounded; time is not.** A slowloris endpoint dribbling a byte at a time holds a `list` thread
indefinitely, and the caps compound it: a hostile server gets up to 1000 × RTT of held thread with no
cancellation. The reviewer measured 1000 round trips locally in 255 ms; at 100 ms RTT that is ~100
seconds, and a deliberately slow server can make it unbounded. Worst case across the page cap is **8 GiB
transferred with no time limit at all**.

Both crates run this on `spawn_blocking` threads, so a handful of hostile connections can occupy the
blocking pool.

### 2. Page-level fields are found by whole-document search

`provider.rs:227-238, 243, 264-265` locate page-level fields via `doc.descendants()`, which searches the
entire document — so an `<IsTruncated>` or `<NextContinuationToken>` nested inside a `<Contents>` element
becomes eligible when the top-level one is absent. Same for `Key`/`Prefix` inside their containers.

Not exploitable (the server already controls every key, and the caps bound the loop), but it contradicts
the module's own stated principle: *"never assume a network-controlled response honours its own
protocol"*. `doc.root_element().children()` is the tighter read for the two page-level fields.

### 3. No per-key length bound

A hostile server can return a single key of ~8 MiB — the body cap is the only limit. **Real S3 caps keys
at 1024 bytes**, which is the protocol's own answer. These names flow straight to the UI.

Cap the leaf length and drop over-long keys the way any other unsafe name is dropped.

### 4. A CI comment that is now untrue

`.github/workflows/ci.yml:419-422` still says the s3 slice *"is pure computation … there is nothing to run
against and no fixture server."* CPE-1683 added a `tiny_http` fixture binding `127.0.0.1:0`. The
*conclusion* is still right — it needs no network, services or Docker — but the stated reason is wrong.
Say the fixture is in-process on loopback.

### 5. A test whose regression mode is a six-hour CI timeout

`provider.rs`, `a_server_that_never_stops_truncating_is_capped_by_max_list_pages`. It is the one test in
the crate whose failure mode is an **unbounded hang** rather than a red: against a zero-growth
endlessly-truncating server, `MAX_LIST_ENTRIES` can never trip, so removing the page cap loops forever
making loopback requests. **libtest has no per-test timeout**, so CI would run to the job limit.

The green path is fast and deterministic (1001 sequential loopback round trips; whole suite 1.18 s), so
there is no flake risk today — this is purely about what happens the day someone breaks it.

Prescribed, ~10 lines: run `provider.list("/")` on a spawned thread and `recv_timeout` a channel, asserting
completion within ~60 s. That turns a six-hour timeout into a deterministic red with a message saying what
happened. **This is the only test with that property** — `MAX_LIST_ENTRIES` and the truncation guard both
red cleanly in ~10 s, and the depth guard's break crashes fast and loud — so it is one test, not a pattern.

### 6. `MAX_RESPONSE_BODY_BYTES` is the last uncovered runtime defence

Removing the 8 MiB body cap leaves all 105 tests passing. It is the fifth of the crate's five runtime
defences and the only one with no test; the other four were covered in PR #888's round 2.

The behaviour is already verified correct at runtime: an over-cap body surfaces as the honest parse error
`Err("the root node was opened but never closed")`, **never a partial listing sold as complete**. So this
is transcription — a fixture returning an over-cap body, asserting that error.

## Scope

`crates/s3/src/provider.rs`, `crates/webdav/src/lib.rs` (item 1 only), `.github/workflows/ci.yml`.

## Acceptance criteria

- [ ] Both agents set `timeout_read` and `timeout_write` (and consider an overall `timeout`). **Pick the
      values deliberately and record why** — too short breaks a legitimately slow large listing over a
      poor link, which is a real user on a real connection, not a hypothetical.
- [ ] A test proves the timeout fires: a fixture that accepts the connection and then stalls must produce
      a timeout error rather than hanging the test. **Make sure the test itself cannot hang CI** if the
      timeout is later removed — a test whose failure mode is a six-hour CI timeout is not a good test.
- [ ] Item 2: page-level fields read from `root_element().children()`, with a test proving a nested
      `<IsTruncated>` inside `<Contents>` is not mistaken for the page's own.
- [ ] Item 3: a key longer than the cap is dropped like any other unsafe name, pinned by a test.
- [ ] Item 4: the `ci.yml` comment states the real reason.
- [ ] Item 5: the page-cap test cannot hang. Prove it — break the page cap and show the test produce a
      **red within its timeout** rather than running away.
- [ ] Item 6: an over-cap body is pinned to the honest parse error, and breaking the cap reds it.
- [ ] Each guard broken **on its own** turns a **distinct** test red, real output pasted in the PR, per
      the Evidence Rules in `Ticketing/wiki.md`.

## Notes

Filed by the Foreman from the PR #888 review, 2026-08-13; items 5 and 6 added from its round-2 pass. The
reviewer marked every item non-blocking, and recommended the timeout work be split across both crates
rather than fixed only in the one that happened to be under review.

Related: **CPE-1683** (which surfaced them), **CPE-1684** / **CPE-1685** (the next consumers of this
transport), **CPE-1398** (the WebDAV parser hardening whose crate carries the same missing timeouts).
