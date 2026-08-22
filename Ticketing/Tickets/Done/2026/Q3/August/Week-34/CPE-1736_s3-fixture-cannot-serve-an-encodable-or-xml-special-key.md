---
id: CPE-1736
title: The cpe-s3 test fixture cannot serve an encodable key or an XML-special one, so no end-to-end test covers them
type: chore
priority: Low
status: Done
tags: ready
estimate: S
created: 2026-08-14
closed: 2026-08-21
---

Found by the CPE-1727 (PR #903) UAT while trying to drive keys with spaces, non-ASCII and `&` through
`delete`'s new `start-after` belt end to end. Neither limit is a product defect — both are limits of the
in-process `tiny_http` harness in `crates/s3/src/provider.rs` — but together they mean **no test in this
crate exercises a key needing percent-encoding, or one containing `&`/`<`, through the fixture**.

## 1. `handle` never percent-decodes the object path

`let real = root.join(path.trim_start_matches('/'));` uses the raw request path. The client correctly
percent-encodes (`my photos/x` → `my%20photos/x`), so the fixture stores and serves the object under its
**encoded** spelling and the key never round-trips as itself. Any test asserting on the filesystem for
such a key is asserting on the wrong name.

The encoding itself *is* covered, at the `sigv4`/`RequestTarget` level (`keys_needing_percent_encoding_are_encoded_the_same_way_in_both_styles`
and the signing vectors) — what is missing is the end-to-end leg.

**Fix:** decode `path` in `handle` the way `parse_query` already decodes query values (`percent_decode`
exists in that test module), then assert a space/non-ASCII key round-trips through
`write` → `list` → `stat` → `delete`.

## 2. `list_page_xml` interpolates key text into XML unescaped

A key containing `&` or `<` produces a document `roxmltree` rejects, so the listing path cannot carry
those two bytes at all in a test. Both are perfectly legal S3 key bytes, and a real server escapes them.

**Fix:** escape `&`, `<`, `>` when building `<Key>`/`<Prefix>` text, then add a listing test for a key
containing `&`.

## Acceptance criteria

- [x] `handle` percent-decodes the object path; a key with a space and a non-ASCII key round-trip
      end to end (write → list → stat → delete) with the filesystem asserted on the **decoded** name.
- [x] `list_page_xml` XML-escapes key text; a key containing `&` lists without a parse error.
- [x] Each new escape/decode is broken on its own and shown to red a distinct test, per the Evidence
      Rules in `Ticketing/wiki.md`.
- [x] The two limits currently recorded in `handle`'s doc are removed when they stop being true.

## Notes

Pre-existing; predates CPE-1727 and blocks nothing. Recorded in `handle`'s doc comment so a future test
author does not conclude the coverage exists. Related: **CPE-1727**, **CPE-1683** (which built the
fixture), **CPE-1704** (leaf-safety rules for legal-but-awkward keys).

## Work Log

**2026-08-21** — Fixed both fixture limits in `crates/s3/src/provider.rs`'s `handle`/`list_page_xml` and
added the end-to-end tests they were blocking.

- `handle` now runs the request path through the existing `percent_decode` (the same helper `parse_query`
  already used for query values) before it becomes `real`/`leaf`, so an object key needing encoding is
  stored and served under its decoded spelling instead of its `%XX` one.
- `list_page_xml` now escapes `&`/`<`/`>` (new `xml_escape` helper) when building `<Key>`/`<Prefix>` text,
  so a key containing those bytes produces a document `roxmltree` can parse; `roxmltree`'s own entity
  decoding on `.text()` is what un-escapes it back to the original byte on the way into the caller.
- Removed the "Two limits of this harness" doc section on `handle` (AC4) and replaced it with a short
  note that both are now fixed, naming the new tests.
- New tests, all in `crates/s3/src/provider.rs`'s `tests` module:
  - `handle_percent_decodes_the_object_path_so_a_key_with_a_space_round_trips_end_to_end` — write → list →
    stat → delete of `/my photos/beach party.jpg` through the real `std::fs`-backed fixture, asserting the
    file lands on disk under the **decoded** name and that no `my%20photos` encoded-spelling directory
    exists.
  - `handle_percent_decodes_the_object_path_so_a_non_ascii_key_round_trips_end_to_end` — same shape for
    `/café/münchen.txt`.
  - `list_page_xml_escapes_an_ampersand_so_a_key_containing_one_lists_without_a_parse_error` — write →
    list of `/AT&T/report.txt` through the real fixture (`&` is a legal Windows/POSIX filename byte, so
    this one *can* go through `std::fs`).
  - `xml_escape_lets_a_key_containing_angle_brackets_or_an_ampersand_list_without_a_parse_error` — `<` and
    `>` are reserved Windows filename characters, so they cannot reach `list_page_xml` through a real
    on-disk key the way `:` already couldn't for `a_key_containing_a_colon_reaches_the_caller_end_to_end`.
    This drives the actual `xml_escape` function (not a re-typed copy) against a hand-rolled server, the
    same technique that existing colon test uses, so a regression in the real helper still reds it.

**Red-proofs (Evidence Rule 1):**
- Broke the decode guard: `let path = percent_decode(raw_path);` → `let path = raw_path.to_string();`.
  Reds exactly the two round-trip tests, distinctly, both on the same line:
  `write returned Ok but the object is not on the server under its DECODED name — the fixture must be
  storing it under the encoded request path instead: Os { code: 3, kind: NotFound, ... }` (space test) and
  the equivalent message for the non-ASCII test. Restored via a follow-up `Edit` back to the original line
  (not `git checkout --`, since the ticket's own changes were uncommitted at the time — verified `cargo
  test` green afterward with a fresh `Compiling` line).
- Broke the escape guard: `xml_escape` body → `s.to_string()` (identity). Reds both XML tests: `a key
  containing '&' must list without the XML failing to parse: "s3: bad ListObjectsV2 XML: malformed entity
  reference at 1:137"` and `a key containing '<', '&' and '>' must list without the XML failing to parse:
  "s3: bad ListObjectsV2 XML: malformed entity reference at 1:96"`. Restored the same way; `cargo test`
  green again (209 passed).

**Gates:** `cargo clippy --all-targets -- -D warnings` (in `crates/s3`) clean, no warnings. `cargo test` (in
`crates/s3`) — 209 passed, 0 failed, 0 ignored (205 before this ticket's 4 new tests). `crates/server` was
not touched, so its gate was not re-run.

**Provenance:** every assertion above comes from the in-process `tiny_http`/`std::fs` fixture (or, for the
`<`/`>` case, a hand-rolled in-process server), never a real S3 gateway — this proves the fixture and the
client-side XML parser agree on the shape, not that a real S3-compatible server percent-decodes or escapes
identically. No QNAP-NAS-equivalent live S3 endpoint exists to cross-check against (the QNAP box on the
LAN speaks SFTP/WebDAV/FTP/SMB/NFS, not S3). Did not touch CPE-1735 (delete asymmetries needing a real
gateway) or CPE-1800 (ureq 2 dot-segment rewriting) — out of scope by the brief; no CPE-1800 unreachable-key
case was hit while writing these tests, since none of the new keys are dot segments.
