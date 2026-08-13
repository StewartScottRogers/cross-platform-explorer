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

## Round-2 additions (from the PR #892 review + UAT, 2026-08-13)

Both independent checks returned blocking findings, and the Reviewer reached the main one from code
reading before it saw the UAT result -- so it has two independent confirmations.

### Item 1 is NOT closed by per-read timeouts alone

A server that accepts, sends valid `200 OK` headers, then dribbles **one byte every 5 s** is unbounded at
the shipped values. Both `S3Provider::list` and `WebdavProvider` failed to return within 100 s. The
`MAX_LIST_WALL_CLOCK` deadline is evaluated at the **top of the page loop, before `signed_get`**, so it
cannot fire mid-body; `timeout_read` is per-read and restarts on every byte. Nothing bounds one page.

Worst case against the 8 MiB body cap at one byte per 29 s is roughly **7.7 years** on one held blocking
thread. **WebDAV is live today** (S3 is latent until CPE-1685): `crates/vfs/src/lib.rs:56` ->
`remote_dir_entries` -> `remote_list_dir_impl` <- the `list_dir` command, on `spawn_blocking` with
tokio's default 512-thread pool and no `max_blocking_threads` set.

**The fix, verified both by source reading and measurement:** `ureq` 2.12.1's `Request::timeout(Duration)`
is a *per-request* deadline -- `self.timeout.or(agent.config.timeout)` (`request.rs:122`) -- and
`DeadlineStream::fill_buf` (`stream.rs:85-89`) recomputes the remaining budget on **every** read, so it
bounds a body continuously. Applied to the listing GET only: a legitimate slow server returned `Ok` in
9.51 s, the dribbler was cut at 30.01 s. Apply to the ListObjectsV2 GET and the PROPFIND, **not** the
large-object GET where per-read semantics are correct.

**Do not set the agent-level `.timeout()`** -- `stream.rs:433-441` is a genuine either/or, so it would
replace `timeout_read` rather than add to it. That call was correct and independently reconfirmed.

### The code currently records a FALSE safety argument

- `crates/s3/src/provider.rs:685` -- "an in-flight page always completes or fails on its own socket
  timeout first". Disproven by measurement.
- `crates/webdav/src/lib.rs:50` -- "the per-request bounds already bound the whole operation". Disproven.

Both must change with the fix. A wrong comment at a safety boundary is worse than none.

### `WebdavProvider::read` has no byte cap at all

`crates/webdav/src/lib.rs:192` is `read_to_end` with no `.take()` -- no equivalent of s3's
`MAX_RESPONSE_BODY_BYTES`. Unbounded in memory as well as time, on the live provider.

### Item 6's guarantee is narrower than "never"

An over-cap body **can** be sold as a complete listing when the truncation lands after a well-formed root
element followed by legal post-root whitespace: an 8392422-byte body returned `Ok` in 241 ms with one
entry. It holds only when truncation lands mid-element. Soften the claim, or compare the read length
against the declared `Content-Length`.

### Stale doc inherited from CPE-1704, to fix here

`is_safe_s3_leaf`'s doc (`provider.rs:418`) and the test-section comment (`:1274`) claim it refuses "a
literal `..` segment (including its percent-encoded form, e.g. `..%2f`)". **It does not**, deliberately --
a test ~80 lines below asserts `report%2ffinal.txt` and `https%3A%2F%2Fexample.com%2Findex.html` are
accepted. Behaviour is correct; the doc was left behind. It describes a security guard, so it matters.

## Work Log

**2026-08-13** — Worked on branch `cpe-1706-http-timeouts`. All six items done; nothing deferred.

### Item 1 — the values chosen, and why

Confirmed against the vendored source rather than from memory: `ureq` 2.12.1 `AgentBuilder::timeout_read`
and `timeout_write` both default to `None` ("requests may block forever on reads by default", `agent.rs`),
while `timeout_connect` already defaults to 30 s (`agent.rs:256`) — so **connect was the one phase that was
never unbounded**, and read/write were the whole exposure.

- `TIMEOUT_READ` / `TIMEOUT_WRITE` = **30 s**, in both crates. These are *per read/write*, not per request:
  the clock restarts on every byte, so they bound a **stall** and never a slow-but-progressing transfer.
  That is the property a large listing (or a large `read`) over a poor link needs — it may take as long as
  it takes provided it keeps moving. 30 s is a wide margin over any real gateway's time-to-first-byte
  (AWS's own SDKs use 30–60 s for the same knob).
- `TIMEOUT_CONNECT` = **30 s**, set explicitly in both crates. Identical to `ureq`'s current default, so it
  changes nothing today; it pins the value against a future `ureq` default change.
- **`ureq`'s overall `.timeout()` is deliberately NOT set**, in either crate. Two reasons, both checked in
  the source: (a) it caps a whole request regardless of progress, so it would kill a legitimate
  multi-minute GET of a large object over a bad connection — a real user, not a hypothetical; (b) it
  *takes precedence over* `timeout_read`/`timeout_write` (`agent.rs:476-477`) rather than adding to them,
  so setting it trades a good bound for a worse one. It also would not solve the actual problem, which is
  per-**listing**, not per-**request** — see next.
- `MAX_LIST_WALL_CLOCK` = **10 min**, `cpe-s3` only. This is the bound a per-read timeout *cannot* give and
  the reason the ticket's threat model needed more than the agent knobs: a server dribbling one byte per
  29 s never trips `timeout_read`, and `MAX_LIST_PAGES` then multiplies that — 1000 pages × a 30 s stall is
  **~8 hours of held `spawn_blocking` thread**, not meaningfully better than unbounded. A deadline over the
  whole `list` call is the only knob whose units match the risk. 10 min is set against the *legitimate*
  worst case, not the median: a listing cannot legitimately exceed `MAX_LIST_ENTRIES` (200 000), which at
  `max-keys=1000` is **200 pages, not 1000**, and even a punishing 2 s per page finishes those in ~400 s.
  **What this knowingly tolerates:** a hostile endpoint can still hold one blocking thread for ten minutes.
  That is the price of not breaking the real user on the bad link, and it is bounded, which is the point.
- `cpe-webdav` gets **no** listing deadline, deliberately: its `list` is a single `PROPFIND` with `Depth: 1`
  and no pagination loop, so there is no page count for a per-request stall to be multiplied by.

Testing note: the stall tests inject a short `Duration` through `connect_with_timeouts`, which is the same
`build_agent` → `list`/`read` → `req.call()` path `connect` drives — only the `Duration`s differ — because
waiting out the shipped 30 s would cost 30 s of CI wall clock on three OSes. The shipped values themselves
are pinned by a separate assertion test, so removing the knob reds one test and gutting the value reds the
other. The error *text* is not asserted beyond the URL/path prefix: a socket read timeout surfaces through
`std::io` differently per platform (`WouldBlock` on Unix, `TimedOut` on Windows) and CI is a 3-OS matrix.

### Items 2–6

- **2.** Page-level `IsTruncated`/`NextContinuationToken` now read from `root_element().children()`, and
  `Key`/`Size`/`Prefix` from their own container's `children()` — plus `Contents`/`CommonPrefixes`
  themselves, for consistency (a `<Contents>` nested inside a `<Contents>` was previously a duplicate
  entry). Two tests, one per level, so the two changes red separately.
- **3.** `MAX_KEY_LEAF_BYTES = 1024`, matching real S3's own key limit; an over-long leaf is dropped exactly
  like any other unsafe name, on both the `Contents` and `CommonPrefixes` paths.
- **4.** `ci.yml`'s s3 comment now says the real reason (every server the tests talk to is spawned
  in-process on loopback), keeping the unchanged conclusion.
- **5.** `call_with_deadline` added; `a_server_that_never_stops_truncating_is_capped_by_max_list_pages`
  routed through it at 60 s (~235× the green path's measured 255 ms). Proven: with `MAX_LIST_PAGES` raised
  to `usize::MAX` the test reds at exactly 60.01 s instead of running to the CI job limit.
- **6.** An over-cap body is pinned to the honest parse error `s3: bad ListObjectsV2 XML: the root node was
  opened but never closed` — exactly the string the ticket predicted. The fixture keeps its keys under
  `MAX_KEY_LEAF_BYTES` and its count under `MAX_LIST_ENTRIES` so that removing the body cap makes the call
  return **`Ok` with 9882 entries**, an unambiguous break rather than a different cap's error.

### Evidence

Eleven guards broken one at a time, each producing a distinct red, restored with `git checkout --` and
re-confirmed green with a real `Compiling` line. Full pasted output in the PR body.

Suite cost is unchanged: `cpe-s3` 117 tests in 1.18 s (was 110 in 1.30 s), `cpe-webdav` 16 in 0.63 s.
`cargo clippy --all-targets -- -D warnings` clean in both, plus downstream `crates/vfs`.

**2026-08-13, round 2** — a Reviewer (from the code) and an independent UAT (by measurement) each found
the same blocking defect, separately. **The above was wrong about the thing that mattered most.**

### What was actually broken

A server that accepts, sends valid `200 OK` headers, then dribbles **one byte every 5 s** was not bounded
at all — measured holding a `list` thread past 100 s at shipped values, in both crates. `timeout_read` is
per-*read*, so its clock restarts on every byte and such a peer is never "stalled" at any instant.
`MAX_LIST_WALL_CLOCK` is checked between pages, so it cannot fire while a body is in flight. Nothing
bounded a single page's body. Worst case at the old values was ~7.7 years, not the "10 minutes" the code
and PR body both claimed. WebDAV was the live half — it is routed through `crates/vfs` today, inside
`spawn_blocking` on tokio's default 512-thread pool, so each attempt drained a pool thread permanently.

**The false comments were the worse half of the defect.** `provider.rs` said *"an in-flight page always
completes or fails on its own socket timeout first"* and `lib.rs` said *"the per-request bounds already
bound the whole operation"*. Both are disproven by measurement. A wrong comment at a safety boundary is
worse than no comment: the next reader believes the case is handled and stops checking. Both are gone.

### The fix

`ureq::Request::timeout` — **per request**, distinct from the agent-level `.timeout()` this ticket
correctly declined. `DeadlineStream::fill_buf` recomputes the remaining budget on every read and the
deadline propagates into the response body reader, so it bounds a whole exchange continuously. Applied to
the `ListObjectsV2` GET and to WebDAV `PROPFIND`/`MKCOL`/`DELETE`/`MOVE`; deliberately **not** to the
large-object `GET` or `PUT`, where per-read semantics are what a legitimate slow transfer needs. The
earlier error was flattening "which request deserves which bound" into one all-or-nothing choice.

**60 s, not the 120 s first tried.** A per-request deadline *replaces* `timeout_read` for that request
(`stream.rs:433-441` is a genuine either/or), so the value must also keep a dead share failing promptly —
120 s bought no real listing any safety and made a dead endpoint take two minutes to report.

Corrected worst case for one `list`: `MAX_LIST_WALL_CLOCK + TIMEOUT_LIST_REQUEST` ≈ **12 minutes**, since
a final page may start just under the budget and take its own full deadline. The 10-minute claim was
wrong twice over.

### Other round-2 corrections

- **Item 6 was overclaimed.** An over-cap body whose truncation lands after a complete root element is
  still well-formed and parsed as a short, *complete-looking* listing — the UAT reproduced
  `Ok in 241 ms with 1 entries: ["decoy.txt"]`. Truncation is now detected by **length**, which document
  shape cannot fool, and the counter-example shape is pinned by a test.
- **The 200-page arithmetic assumed something S3 does not guarantee** (a gateway may return fewer keys
  than `max-keys`). Doc corrected, including the case the budget would legitimately abandon.
- **Eight guard arms reddened nothing.** Now guarded — and two of my first attempts at guarding them were
  themselves ineffective, caught by probing rather than assumed: `timeout_connect` asserted `Some(30s)`,
  which is *also* `ureq`'s default, so it passed with the line deleted; and the body cap's `.take()` is
  invisible to a length check, so removing it left the over-cap tests green. Both fixed.
- `WebdavProvider::read` gains a byte cap that **errors rather than truncating** — `download_tree` writes
  what comes back to disk as the finished file.
- Fixed CPE-1704's stale `is_safe_s3_leaf` doc claiming it refuses `..%2f` while a test below asserts that
  key is accepted, and gave both fixtures a nanosecond stamp (223 stale `(pid, n)` roots had accumulated;
  Windows pid reuse made two tests share a dirty root and fail).

Final: `cpe-s3` **141 passed**, `cpe-webdav` **20**, `crates/vfs` **21**; clippy clean in all three. The
suites now take ~60 s each, deliberately: the shipped-values dribble tests wait out the real deadline,
because round 1 proved a mechanism through a seam and shipped a configuration that did not bound it.
