---
id: CPE-1723
title: S3 delete and stat silently require s3:ListBucket, and four smaller gaps from the same review
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-13
closed:
---

## 1. `delete` and `stat` now silently require `s3:ListBucket` *(the substantive one)*

Found by the PR #896 (CPE-1684) reviewer, 2026-08-13.

Both ops run a `ListObjectsV2` probe first — `delete` to refuse a non-empty virtual directory, `stat` to
decide whether a key is a directory — and both propagate the probe's failure with `?`.

A credential holding `s3:DeleteObject` / `s3:GetObject` but **not** `s3:ListBucket` therefore gets an
access-denied error **naming a prefix the user never typed**: deleting `/photos/a.jpg` errors about
`"photos/a.jpg/"`, trailing slash and all, instead of performing the delete it is fully entitled to perform.

Mostly masked on AWS proper — without `ListBucket`, a HEAD answers 403 rather than 404, so `stat` never
reaches its probe — but **reachable on MinIO and Ceph**, which are exactly the endpoints a self-hosting user
points this at.

**Fix shape, either is acceptable:** name the probe in the message so the error explains itself, or fall
back to the un-probed single-key path when the probe is specifically denied. Record which and why. If you
fall back, be careful that a denied probe does not silently re-enable the directory-delete this ticket's
parent refuses.

## 2. A `..` directory is browsable but nothing inside it is openable

`list` puts the prefix in the **query string**, which `ureq`/`url` does not normalise (measured), so
`list("/a/../b")` works. But `stat`, `read` and `write` on every child are refused by the dot-segment guard,
because those put the key in the **path**, which *is* normalised.

CPE-1704 filters `..` leaves out of listings so a user cannot click their way there — but a **typed** path
yields a folder that opens and lists normally, in which every single file errors.

Fold into **CPE-1721** if that ticket is worked first; it is the same root cause.

## 3. The parser's `leaf.is_empty()` marker filter has zero coverage

**Corrected 2026-08-13 by the PR #896 re-UAT: this is no longer zero coverage.** Round 2's `filtered_count` fix gave it coverage incidentally -- with the marker's empty leaf counted as content, an empty directory becomes undeletable, so disabling `leaf.is_empty()` now reds **two** real tests. What remains unasserted is the **display** symptom below. Originally measured as: disabling it left 164/164 green. `is_safe_s3_leaf("")` is false, so the marker is caught by the *second*
filter and merely reclassified from "ignored" to "filtered".

That reclassification is not harmless: it would make every `mkdir`-created folder report
`filtered_count = 1` to `crates/vfs::connect::remote_dir_entries`, and once CPE-1708 surfaces that count,
**show the user a spurious "1 entry hidden" on every empty folder they create.**

The missing assertion is that an empty directory lists with **`filtered == 0`**. PR #896 round 2 added the
hostile half (`filtered == 1` for a genuinely refused leaf), so this is cheap to complete.

## 4. `every_object_op_works_through_a_trait_object`'s rename assertion measures nothing

`rename("/dyn/x", "/dyn/y").is_err()` is satisfied by a **failed copy of a nonexistent source**. The
reviewer substituted a fully working copy-then-delete and the test stayed green.

Use a source object that exists, or assert on the message. The concrete-type rename tests are fine — this
is the trait-object one only.

## 5. `probe_prefix`'s `max-keys=2` rationale does not match what the belt does

The doc cites a gateway that both under-fills **and** fails to set `IsTruncated`. But if it under-fills to
one key, asking for two gains nothing. The belt actually helps against a gateway that **honours** `max-keys`
and lies about `IsTruncated` — which is precisely what PR #896 round 2's new
`a_server_that_underfills_without_setting_is_truncated_...` exercises. Align the sentence with the test.

## Acceptance criteria

- [ ] A credential without `s3:ListBucket` can still delete a single object and stat it, or gets an error
      that **names the probe** as the thing that failed. Test against the denied-probe case specifically.
- [ ] Item 2 resolved or explicitly folded into CPE-1721 with a note here saying so.
- [ ] Item 3: an empty directory asserts `filtered == 0`, and disabling the `leaf.is_empty()` filter turns a
      **distinct** test red.
- [ ] Item 4: the trait-object rename assertion fails when a working copy-then-delete is substituted. Prove
      it by substituting one.
- [ ] Item 5: the doc sentence matches the test.
- [ ] Each guard broken **on its own** turns a **distinct** test red, real output pasted in the PR, per the
      Evidence Rules in `Ticketing/wiki.md`.

## Notes

Filed by the Foreman from the PR #896 review, 2026-08-13. All five were marked non-blocking there and
correctly kept out of that PR.

Item 1 is the reason this is Medium rather than Low: it is a real permission configuration that a
self-hosting user will hit, and the failure names a path they never typed, which is the hardest kind of
error to act on.

Related: **CPE-1684** (which introduced the probes), **CPE-1721** (the ureq dot-segment guard),
**CPE-1722** (the leading-slash collapse), **CPE-1708** (which would surface the spurious count in item 3),
**CPE-1682** (the missing-vs-denied line these probes must not blur).

## 6. A gateway that both under-fills AND misreports `IsTruncated` is caught by nothing

Measured by the PR #896 re-UAT, on round 3's own terms:

```
pure under-filler that ADMITS truncation -> delete refused (IsTruncated carries it)
under-filler that DENIES truncation      -> delete Ok
```

Round 3 does not claim otherwise — that overstatement was round 2's wording, and it was corrected — so
nothing in the code is dishonest. But it is an uncovered **non-conforming-server** shape that no
probe-based check can reach, and it is worth writing down rather than rediscovering.

There may be no proportionate fix. If the conclusion is "a server that lies about both cannot be defended
against by asking it questions", **say that and close this item** rather than inventing a third belt.

## 7. Nothing here has been measured against a real gateway

Every measurement behind items 1–6, and behind CPE-1684 itself, is in-process (`tiny_http` and raw sockets)
over `ureq` 2.12.1 on Windows. **No real S3, no MinIO, no QNAP.** Real-gateway pagination behaviour and real
403/404 policy are unmeasured — which matters most for item 1, whose whole premise is what a particular
credential configuration does on MinIO/Ceph.

The QNAP on the LAN is the obvious first real target.
## 8. One code comment is over-broad by exactly one case (disclosed by the author)

`probe_prefix`'s comment lists **"a control byte"** among the leaf shapes that reach the counting bug. It
does not: a raw control character makes the listing non-XML and `roxmltree` rejects the whole document, so
that case **fails loudly** rather than falsely succeeding. Three reachable shapes (`\`, exactly `..`,
exactly `.`), not four.

The test itself is sound — its fixture sentinel uses the backslash shape, which is genuinely reachable. The
author disclosed this rather than push a fourth round for a comment, which was the right call on timing.
Fix it whenever that file is next touched.
