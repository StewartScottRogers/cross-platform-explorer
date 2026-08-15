---
id: CPE-1740
title: The S3 list path still accepts a partial or non-authoritative reply as a complete listing
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-14
closed: 2026-08-15
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

- [x] `list` refuses a `203` and a `206` reply rather than rendering it as the folder's complete contents,
      and the message names the status and says a listing must be `200`.
- [x] The belt's `203` message no longer claims reading the reply failed, and no longer reports "no
      `<Code>` element" about a body that is a listing.
- [x] Each guard broken **on its own** turns a **distinct** test red, real output pasted, per the Evidence
      Rules in `Ticketing/wiki.md`. Assert on the effect (bytes sent / entries rendered) **before** the
      `Result`, so the assertion carrying the harm is reachable.

## Work Log

- 2026-08-14 — Confirmed `S3Provider::probe_prefix` (the non-belt probe the ticket names for `stat` and
  `delete`'s first question) is a pure delegate to `probe_prefix_after` (`self.probe_prefix_after(key_prefix,
  None).map_err(...)`), which CPE-1727 already narrowed to `status == 200`. So probe_prefix's half of the
  ticket was already closed transitively by CPE-1727 — no separate narrowing needed there. Decision: leave
  `probe_prefix` untouched and spend the pass on `FileSystemProvider::list`'s pagination loop and the two
  message defects, which were the genuinely open items.
- 2026-08-14 — Narrowed `list_with_filtered_count`'s per-page status check from `!(200..300).contains(&status)`
  to `status != 200` (`crates/s3/src/provider.rs`), matching `probe_prefix_after`'s existing rule. A `203`
  or `206` now refuses the page instead of parsing and rendering it as the folder's contents.
- 2026-08-14 — Added `is_non_canonical_listing_status`/`non_canonical_listing_status_cause` (a status in
  `200..300` but not exactly `200`) and routed BOTH `list`'s not-200 branch and `probe_prefix_after`'s
  not-200 branch through it instead of `error::map_s3_error`/`map_response_error` for that one case — a
  203/206 body is a listing (or a legitimate fragment of one), never an S3 `<Error>` document, so hunting
  it for a `<Code>` element was always going to report a false "no <Code> element" absence.
- 2026-08-14 — Split `marker_confirmation_failure`'s `(200..300)` diagnosis arm into `Some(200)` (a real
  "reading the reply failed" story — UTF-8/XML parse failure downstream of an actual 200) and a new
  `is_non_canonical_listing_status` arm ("refused on its status before its body was ever read") — the old
  single arm asserted "what failed was reading the reply" even when nothing was ever read, because the
  status check now rejects 203/206 before the body is touched at all.
- 2026-08-14 — Added 4 tests (`crates/s3/src/provider.rs`, `provider::tests`):
  `list_refuses_a_203_and_a_206_reply_rather_than_rendering_it_as_the_folders_complete_contents`,
  `a_non_canonical_2xx_on_list_is_not_routed_through_the_s3_error_parser`,
  `the_belts_203_message_does_not_claim_a_read_failure_or_hunt_a_listing_for_an_error_code`, and (already
  present, re-verified as still-passing coverage for the belt's 206 case)
  `a_partial_content_listing_is_refused_rather_than_read_as_an_empty_one`. Verified each guard's own
  **assertion** reds on its own by manually reverting each of the three edits above one at a time and
  re-running: reverting `list`'s status check reproduced the exact round-5 measurement
  (`Ok([ProviderEntry { name: "a.jpg", .. }])`); reverting `list`'s cause selection reproduced the "no
  <Code> element" defect verbatim in `list`'s own message; reverting `probe_prefix_after`'s message
  selection reproduced the belt's "no <Code> element" defect verbatim; reverting
  `marker_confirmation_failure`'s new arm reproduced "what failed was reading the reply" for a 203. All
  reverted back before commit; `cargo test -p cpe-s3` is 192/192 green, `cargo clippy --all-targets -- -D
  warnings` clean.
  - **Correction (PR #911 review + UAT, 2026-08-15):** the claim "each guard turns a distinct test red"
    overstated it two ways. Reverting `list`'s status check ALSO reds
    `a_non_canonical_2xx_on_list_is_not_routed_through_the_s3_error_parser` (its own `.expect_err(...)`
    depends on the same refusal), so that mutation reds **two** tests, not one. And reverting
    `probe_prefix_after`'s message selection vs. reverting `marker_confirmation_failure`'s arm both red
    the **same** test (`the_belts_203_message_does_not_claim_a_read_failure_or_hunt_a_listing_for_an_error_code`)
    via two different assertions inside it (message half 2 vs. half 1 respectively), not two distinct
    tests. The honest claim is "each guard's own assertion reds when that guard alone is broken", not
    "one test per guard" — the mutation *evidence itself* was accurate (real output pasted, correctly
    isolated per guard), only the "distinct test" framing overstated the test-file shape.

## Round 2 (PR #911 review — CHANGES REQUESTED; UAT PASS)

- 2026-08-15 — Reviewer confirmed, with evidence (fetched the AWS `ListObjectsV2` contract, enumerated
  every 2xx, checked MinIO/Ceph/R2/B2), that narrowing to exactly `200` refuses no legitimate reply, and
  re-verified the `probe_prefix` transitive-fix claim and all four round-1 mutations as honest. Two items
  to fix, addressed below; kept unchanged: everything from round 1's Work Log above.
- 2026-08-15 — **BLOCKER (fixed).** `marker_confirmation_failure`'s new "refused on its status before its
  body was ever read" arm was selected on `status` alone, but `signed_exchange` treats every `2xx` as
  `ok`, so its OWN body-read failure or over-cap refusal under a non-canonical 2xx (e.g. a `203` whose
  body is over `MAX_RESPONSE_BODY_BYTES`) returns `Err((Some(203), …))` from `signed_get` — reaching
  `marker_confirmation_failure` before `probe_prefix_after`'s `status != 200` check ever runs. That
  produced the mirror image of the original defect: "refused … before its body was ever read" one
  sentence above an "Underlying error" naming the 8 MiB cap that had just stopped reading it.
  - **Fix chosen: reviewer's option 1** (a distinguishable signal, not the softened-wording fallback).
    Introduced `ProbeRefusal { status, refused_before_body_read, message }` as `probe_prefix_after`'s `Err`
    type (was a bare `(Option<u16>, String)`), replacing every anonymous-tuple `Err` construction in that
    function. `refused_before_body_read` is `true` ONLY for the function's own explicit `status != 200`
    short-circuit (the body is fully in hand there and simply never parsed); every error surfacing from
    `signed_get` — transport, body-read failure, over-cap — sets it `false`, because in every one of those
    the body was at least attempted. `marker_confirmation_failure` now takes `&ProbeRefusal` and matches
    on `(status, refused_before_body_read)`, giving the non-canonical-2xx case two distinct arms: one for
    "refused on status alone, body never inspected" (true) and a new one for "the body was engaged with
    and reading it is why this failed" (false), which states what actually happened without claiming
    either "reading failed" (may be over-cap, not merely unreadable) or "never read" (it was, to the cap).
    Chose option 1 over the softer-wording fallback because the codebase's standing style (every other
    diagnosis in this file) is to say the precise, evidenced thing rather than a phrase merely true in
    both sub-cases, and the plumbing turned out to be contained to one function + its one caller
    (`delete`'s belt) + `probe_prefix`'s one-line forwarder — not the invasive `signed_exchange`-wide
    change the "option 1 is more work" framing implied.
  - **New test:** `the_belts_203_message_does_not_claim_an_unread_body_when_it_was_read_to_the_cap_and_refused`
    — same over-cap-but-still-well-formed-XML shape CPE-1706 pins for `list` (a complete root element
    followed by megabytes of legal padding), sent under a `203` on the belt. Verified it catches the exact
    blocker by reverting the new arm's guard back to matching on status alone: reproduced the reviewer's
    quoted contradiction verbatim (`"...so it was refused on its status before its body was ever
    inspected... Underlying error: ...the response body exceeded the 8388608-byte cap..."`), reverted back
    before commit.
- 2026-08-15 — **REQUIRED (done).** Cited CPE-1749 (filed by the Foreman: `read`/`stat` accept any 2xx too,
  and an unsolicited `206` is real data loss — `Ok([72, 65, 76, 70])`, a truncated file written to disk) in
  the PR body, so this ticket's "crate-wide" opening line is backed by a scheduled follow-up. Not
  implemented here — out of this ticket's scope, per the Foreman.
- 2026-08-15 — **Non-blocking (fixed).** `non_canonical_listing_status_cause` unconditionally appended both
  the 203 sentence and the 206 sentence regardless of which status actually arrived. Now `match`es on
  `status` and includes only the sentence that applies (203 → transforming-proxy; 206 → partial/range,
  explicitly noting this client never sends `Range`; any other non-canonical 2xx → no over-claimed cause).
- 2026-08-15 — Re-verified: `cargo test -p cpe-s3` 193/193 green (192 + the new blocker test), `cargo
  clippy --all-targets -- -D warnings` (crates/s3) clean, `cargo test -p cpe-vfs` 21/21 green.

## Notes

Filed by the PR #903 UAT, 2026-08-14, and re-scoped at round 5 after the Foreman closed the two deletion
conditions inside that PR. Measurements above are from throwaway probes on the CPE-1727 branch; they were
not committed.

Related: **CPE-1727** (which closed the belt's half), **CPE-1684** (the delete decision), **CPE-1683** (the
listing path), **CPE-1736** (the sibling fixture limits), **CPE-1518** (the QNAP — the first real endpoint,
where a real gateway's statuses can finally be seen).
