---
id: CPE-1736
title: The cpe-s3 test fixture cannot serve an encodable key or an XML-special one, so no end-to-end test covers them
type: chore
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-14
closed:
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

- [ ] `handle` percent-decodes the object path; a key with a space and a non-ASCII key round-trip
      end to end (write → list → stat → delete) with the filesystem asserted on the **decoded** name.
- [ ] `list_page_xml` XML-escapes key text; a key containing `&` lists without a parse error.
- [ ] Each new escape/decode is broken on its own and shown to red a distinct test, per the Evidence
      Rules in `Ticketing/wiki.md`.
- [ ] The two limits currently recorded in `handle`'s doc are removed when they stop being true.

## Notes

Pre-existing; predates CPE-1727 and blocks nothing. Recorded in `handle`'s doc comment so a future test
author does not conclude the coverage exists. Related: **CPE-1727**, **CPE-1683** (which built the
fixture), **CPE-1704** (leaf-safety rules for legal-but-awkward keys).
