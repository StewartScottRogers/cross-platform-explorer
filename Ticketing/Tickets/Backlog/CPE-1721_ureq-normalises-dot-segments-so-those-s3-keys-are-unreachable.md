---
id: CPE-1721
title: "ureq normalises dot segments out of a URL, so an S3 key containing `.`/`..` is unreachable"
type: Bug
status: Backlog
priority: Low
component: Backend
tags: [ready]
epic: CPE-1503
estimate: M
created: 2026-08-13
closed:
---

## What

`crates/s3` signs the canonical request path **unnormalised**, on purpose (CPE-1689): an S3 key is an
opaque byte string, so `a/../b.txt` is a real object with nothing to do with `b.txt`, and collapsing it
would silently address the wrong object.

`ureq` 2.12.1 parses every URL through the `url` crate, which implements WHATWG URL parsing, which
**resolves dot segments as part of parsing** — before `ureq` has any say in it. So the request is signed
for one path and sent for another.

Consequence: `stat`/`read`/`write`/`delete`/`mkdir` on a key containing a `.` or `..` path segment are
refused by `cpe_s3::provider::guard_path_survives_the_client` (CPE-1684). The refusal is the right answer
given the transport — the alternatives are silently addressing a different object, or an opaque
`SignatureDoesNotMatch` — but the key is genuinely unreachable, which is a real gap rather than a
non-issue.

## Measured, not assumed

The CPE-1684 worker measured it against the real send path, with a raw-socket recorder reading the
request line off the wire and comparing it to `RequestTarget::encoded_path` (the exact string that was
signed):

```text
SIGNED "/test-bucket/a/../b//c%252Fd.txt", SENT "/test-bucket/b//c%252Fd.txt"
```

This confirms the hypothesis the PR #868 reviewer raised and explicitly labelled unverified (they were
offline). **Only dot segments are affected, and that is measured too**: in the same request an empty
path segment (`//`) and a percent-encoded `%` (`%252F`) both survived byte for byte. See
`crates/s3/src/provider.rs`'s `tests::a_key_with_a_double_slash_and_a_percent_encoded_slash_reaches_the_wire_intact`
and `tests::a_key_with_a_dot_segment_is_refused_because_ureq_resolves_it_away_before_sending`.

## Why this is not urgent

Keys with a `.`/`..` segment are rare and usually accidental (a tool that joined paths carelessly). The
failure today is a loud, specific refusal naming the cause — not data loss and not an opaque 403. So this
is a completeness gap, not a correctness one.

## Options (none chosen — this needs a decision, not just a fix)

1. **Send the request without `ureq`'s URL parsing.** `ureq` 2 offers no way to hand it a pre-parsed
   target, so this means a different client, or building the request line directly over a TLS stream.
   Both are large; the second re-implements an HTTP client, which is exactly what the crate's "no new
   dependency family" decision was avoiding.
2. **`ureq` 3** — check whether its URL handling differs. `ureq-3.3.0` and `ureq-proto` are already in
   the local registry (pulled by something else in the tree). This is the cheapest thing to measure
   first, and it should be measured before anything is designed.
3. **Accept the limitation permanently** and make the refusal the documented, final answer. Defensible:
   the shape is rare, and it is already documented honestly in `src/docs/31-network.md`.

## Acceptance criteria

- [ ] Whichever option is chosen, the decision is recorded in `crates/s3/src/provider.rs`'s module doc
      alongside the existing measurement, not just in this ticket.
- [ ] If a client change lands: `tests::a_key_with_a_dot_segment_is_refused_...` is replaced by a test
      that asserts the dot-segment path reaches the wire **byte-identical to what was signed**, through
      the same raw-socket recorder — never by a test that merely asserts the call returns `Ok`.
- [ ] If the limitation is accepted: `src/docs/31-network.md`'s bullet loses its "tracked separately"
      hedge and says so plainly.

## Notes

Filed 2026-08-13 by the CPE-1684 worker, from its own measurement. The ticket it came from
(`Ticketing/Tickets/Doing/CPE-1684_*.md`) carries the full write-up under "Test this first"; that
section was a hypothesis and this ticket is its resolution.
