---
id: CPE-1727
title: Restore delete for a GetObject-holding credential, add the start-after belt, and fix list's bare 403
type: bug
priority: Medium
status: Done
tags: ready
estimate: M
created: 2026-08-14
closed: 2026-08-14
---

All four found by the PR #900 (CPE-1723) UAT, which passed the PR and then went past its brief.

## 1. HEAD-proof delete — restores the entitled operation *(the substantive one)*

CPE-1723 made a denied `ListObjectsV2` probe **refuse** rather than fall back, and that was right: S3's
`DELETE` answers 204 for a prefix as readily as for an object, so an un-probed fallback would report
`/photos/2024` deleted while every object under it stayed put. Both the reviewer and the UAT reproduced
exactly that.

**But the UAT found a third option neither the author nor the reviewer considered:** delete only when a
`HEAD` proves the key names an object — a question `s3:GetObject` permits. Measured:

```
CASE 1 virtual directory (the catastrophic case): Err — a.jpg survives, b.jpg survives
CASE 2 a real object:                             Ok(()) — single.jpg gone from disk
CASE 3 a key that does not exist:                 Err
```

The safety property is **fully preserved** — a pure prefix has no object at its own key, so it HEADs
404/403 and never 200 — at the same two round trips. And it **restores delete for the exact user this
family of tickets is about**: CPE-1723 currently leaves a credential holding `s3:DeleteObject` unable to
delete anything, and its own test asserts that as correct.

Note `probe_prefix`'s doc argues against HEAD-before-DELETE, but for a **different purpose** — proving a
deletion *happened*. It does not address using HEAD to prove *object-ness*, which is a question with a
trustworthy answer.

**Not measured:** an object and a prefix sharing a name (`photos` the object, `photos/` the prefix). The
filesystem-backed fixture cannot represent both. **Measure that before shipping** — it is the one case
where "HEAD says object" and "prefix has contents" are simultaneously true.

## 2. The `start-after` third belt

CPE-1723 closed item 6 as "no proportionate fix", and I authorised it. **The UAT disproved the reasoning.**

Re-list the same prefix with **`start-after`** set to the marker key, **only** on the marker-only verdict.
That is not the same question re-asked: under-filling a page is *legal* S3 latitude, so the first lie costs
nothing, while returning zero keys with `IsTruncated=false` when keys exist beyond the marker is a **flat
protocol violation**. The belt forces a strictly stronger lie.

```
CASE H  honest server, genuinely empty directory:   delete ok = true,  DELETEs sent: ["/test-bucket/photos/"]
CASE L  under-fills AND denies truncation:          delete ok = false, DELETEs sent: []
```

**Full existing suite green with the belt in place (176 passed).**

Two things to carry over so they are not re-derived:

- Count **`entries.len() + filtered_count`**, not `raw_entries` — a naive `raw_entries` belt gets a false
  positive from the re-returned marker and reds the honest server's empty-directory delete. That false
  positive is why an earlier review concluded no belt was possible.
- The shipped fixture **ignores `start-after`** (its `handle` reads only `max-keys` and
  `continuation-token`), so it cannot exhibit the difference. Teach the fixture `start-after` first, or the
  new test proves nothing.

**Scoped:** the belt does not catch a server that ignores `start-after`, and no real gateway's behaviour has
been measured.

## 3. `list`'s bare 403 is the least actionable message in the provider

`provider.rs:1354`, from CPE-1683, untouched since:

```
s3: HTTP 403 AccessDenied: Access Denied. — the credentials are valid but the bucket policy or IAM policy denies this request
```

No path, no `s3:ListBucket`, no operation name. And `list` is **the one operation that genuinely always
needs `s3:ListBucket`** — the first thing a user hits browsing a bucket. An odd gap immediately after a
ticket whose whole thesis was making a denied `ListBucket` actionable. Give it the `probe_failure`
treatment.

## 4. Two smaller items

- The item-6 **characterisation test asserts on `is_ok()`** — the one place in that PR asserting on the
  `Result` rather than on what the user gets, and its fixture is not filesystem-backed so it *cannot* show
  effect. Give it a filesystem-backed fixture and assert on the keys actually deleted.
- On the marker-only verdict `doomed_key` is `key/` (measured: `/test-bucket/photos/`), so item 6's residual
  harm is **a lost marker plus a false success**, not subtree orphaning. The doc overstates it.

## Acceptance criteria

- [x] A credential with `s3:GetObject` + `s3:DeleteObject` but no `s3:ListBucket` can delete a real object,
      and still **cannot** delete a virtual directory. Both pinned.
- [x] The object-and-prefix name collision is measured and its behaviour recorded.
- [x] The `start-after` belt catches the double-liar and leaves every conforming case untouched — with the
      fixture taught `start-after` first, so the test is not vacuous.
- [x] `list`'s denied-`ListBucket` message names the operation, the path and the permission.
- [x] Each guard broken **on its own** turns a **distinct** test red, real output pasted in the PR, per the
      Evidence Rules in `Ticketing/wiki.md`.
- [ ] **Nothing here has been measured against a real gateway.** If a real S3/MinIO/Ceph endpoint or the
      QNAP becomes available, item 1's 404-vs-403 premise and item 2's `start-after` reliance are the two
      things to check first.

## Notes

Filed by the Foreman from the PR #900 UAT, 2026-08-14. The UAT passed the PR and produced all four of these
anyway — including a working counter-example to a closure **I** had authorised.

Related: **CPE-1723** (which refused the fallback and closed item 6), **CPE-1684** (the delete decision and
`probe_prefix`), **CPE-1683** (`list`'s 403 message), **CPE-1685** (which makes all of this user-reachable),
**CPE-1518** (the QNAP, the first real endpoint).

## Work Log

**2026-08-14 — implemented in `crates/s3` (branch `cpe-1727-getobject-delete-startafter`).**

1. **HEAD-proof delete.** `S3Provider::head_proves_object` (2xx only — a 403 is not proof, since AWS
   answers 403 for a missing key without `s3:ListBucket`). `delete` calls it when `probe_prefix` fails and
   DELETEs the exact key that HEADed, never the `key/` marker form. Directory-with-content is still
   refused (the pure prefix HEADs 404), and a nonexistent key is still refused.
2. **The object/prefix name collision, measured.** `spawn_a_keyspace_server_without_listbucket` backs a
   flat keyspace instead of a filesystem, because no filesystem can hold `photos` and `photos/a.jpg` at
   once. Result: the object `photos` is removed, `photos/a.jpg` and `photos/b.jpg` are untouched. Recorded
   in `delete`'s doc, including the cost — a user who meant the folder gets a success and a folder that is
   still there. No client that may not list can tell those intentions apart.
3. **The `start-after` belt**, on the marker-only verdict only, via `probe_prefix_after`, counting
   `entries.len() + filtered_count`. The fixture was taught `start-after` first, and that teaching is
   pinned by its own test so the belt is not measured against a server that ignores the parameter.
4. **`list_failure`** wraps (never replaces) `map_s3_error` for every non-2xx listing; the `s3:ListBucket`
   sentence is conditional on 401/403.
5. The item-6 characterisation test is now filesystem-backed and asserts the measured harm (marker gone,
   `photos/a.jpg` alive) instead of `is_ok()`.

Two pre-existing assertions were **retargeted, not deleted**: the `delete_without_listbucket_...` message
test now uses a key that does not exist (the message is unchanged and still reachable), and
`the_object_operations_..._entitled_to_still_work` now asserts the delete succeeds and the object leaves
the server — that assertion was the pin on the behaviour this ticket exists to undo.

`src/docs/31-network.md` updated: its "therefore can't delete" bullet stated the old behaviour.

180 tests pass; `cargo clippy --all-targets -- -D warnings` clean. Six guard neutralisations, each red on
a distinct test with the real output pasted in the PR body. **Still unmeasured against a real gateway** —
item 1's 404-vs-403 premise and item 2's `start-after` reliance remain the two things to check first when
a real S3/MinIO/Ceph endpoint or the QNAP (CPE-1518) is available.
