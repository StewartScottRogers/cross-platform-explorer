---
id: CPE-1723
title: S3 delete and stat silently require s3:ListBucket, and four smaller gaps from the same review
type: bug
priority: Medium
status: Done
tags: ready
estimate: M
created: 2026-08-13
closed: 2026-08-14
---

## 1. `delete` and `stat` now silently require `s3:ListBucket` *(the substantive one)*

Found by the PR #896 (CPE-1684) reviewer, 2026-08-13.

Both ops run a `ListObjectsV2` probe first â€” `delete` to refuse a non-empty virtual directory, `stat` to
decide whether a key is a directory â€” and both propagate the probe's failure with `?`.

A credential holding `s3:DeleteObject` / `s3:GetObject` but **not** `s3:ListBucket` therefore gets an
access-denied error **naming a prefix the user never typed**: deleting `/photos/a.jpg` errors about
`"photos/a.jpg/"`, trailing slash and all, instead of performing the delete it is fully entitled to perform.

Mostly masked on AWS proper â€” without `ListBucket`, a HEAD answers 403 rather than 404, so `stat` never
reaches its probe â€” but **reachable on MinIO and Ceph**, which are exactly the endpoints a self-hosting user
points this at.

**Fix shape, either is acceptable:** name the probe in the message so the error explains itself, or fall
back to the un-probed single-key path when the probe is specifically denied. Record which and why. If you
fall back, be careful that a denied probe does not silently re-enable the directory-delete this ticket's
parent refuses.

## 2. A `..` directory is browsable but nothing inside it is openable

`list` puts the prefix in the **query string**, which `ureq`/`url` does not normalise (measured), so
`list("/a/../b")` works. But `stat`, `read` and `write` on every child are refused by the dot-segment guard,
because those put the key in the **path**, which *is* normalised.

CPE-1704 filters `..` leaves out of listings so a user cannot click their way there â€” but a **typed** path
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

Use a source object that exists, or assert on the message. The concrete-type rename tests are fine â€” this
is the trait-object one only.

## 5. `probe_prefix`'s `max-keys=2` rationale does not match what the belt does

The doc cites a gateway that both under-fills **and** fails to set `IsTruncated`. But if it under-fills to
one key, asking for two gains nothing. The belt actually helps against a gateway that **honours** `max-keys`
and lies about `IsTruncated` â€” which is precisely what PR #896 round 2's new
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

Round 3 does not claim otherwise â€” that overstatement was round 2's wording, and it was corrected â€” so
nothing in the code is dishonest. But it is an uncovered **non-conforming-server** shape that no
probe-based check can reach, and it is worth writing down rather than rediscovering.

There may be no proportionate fix. If the conclusion is "a server that lies about both cannot be defended
against by asking it questions", **say that and close this item** rather than inventing a third belt.

## 7. Nothing here has been measured against a real gateway

Every measurement behind items 1â€“6, and behind CPE-1684 itself, is in-process (`tiny_http` and raw sockets)
over `ureq` 2.12.1 on Windows. **No real S3, no MinIO, no QNAP.** Real-gateway pagination behaviour and real
403/404 policy are unmeasured â€” which matters most for item 1, whose whole premise is what a particular
credential configuration does on MinIO/Ceph.

The QNAP on the LAN is the obvious first real target.

## Work Log

**2026-08-13 â€” worked on branch `cpe-1723-s3-listbucket-and-gaps`.**

### Item 1 â€” fix shape chosen: **name the probe**, do not fall back

Recorded reasoning (the long form lives on `probe_failure`'s doc in `crates/s3/src/provider.rs`):

- **`delete` must not fall back.** The probe is the only thing separating an object from a directory, so a
  denied probe means *"I cannot tell which this is"*. S3's `DELETE` answers 204 for a prefix as readily as
  for an object, so an un-probed fallback would report `/photos/2024` deleted while every object under it
  stayed put â€” the exact refusal CPE-1684 exists to make. Letting a *denied* probe re-enable what a
  *successful* probe forbids makes the guard weaker the less the server is willing to say, which is
  backwards.
- **`stat` gains nothing from falling back.** Its probe only runs after a 404 on the object key, so the
  fallback answer is "not found" â€” a claim about a path that may well be a directory the caller is merely
  not permitted to enumerate. That is a confident wrong answer, not the absence of one.
- **A message fixes the actual complaint at zero behavioural risk.** The symptom is a *message* defect: an
  error about a string the user never typed. Naming the probe, saying nothing was deleted, and naming
  `s3:ListBucket` makes it actionable, with no way to make a delete less safe.

Wired at all three probe call sites (`delete`, `stat`'s 404 fallback, `stat`'s bucket-root probe). The
wrapper fires on **every** probe failure, not only 403 â€” a timeout or a 500 produced the same mystery
prefix.

New fixture `spawn_s3_fixture_without_listbucket` models the real credential: every `ListObjectsV2` gets
403 + the `AccessDenied` XML body (a `GET` carries a body, so a bodiless 403 would have exercised the
wrong arm of `map_response_error`), while GET/HEAD/PUT/DELETE on object keys are served normally.

### Item 2 â€” folded into CPE-1721

Same root cause (query string is not normalised, path is). Written into that ticket as its own section
with an extra acceptance criterion, and the user-visible half added to `src/docs/31-network.md`.

### Item 3 â€” the display symptom is now asserted

`a_freshly_created_empty_directory_reports_nothing_filtered_so_no_phantom_hidden_entry_is_shown`, through
`&dyn FileSystemProvider` (the shape `remote_dir_entries` holds). Asserts `filtered == 0` for a fresh
`mkdir` folder â€” the assertion that stops CPE-1708 showing a spurious "1 entry hidden" on every empty
folder a user creates.

### Item 4 â€” the trait-object rename assertion now measures something

It renamed `/dyn/x`, which does not exist, so a working copy-then-delete errored on the read and satisfied
`is_err()`. It now renames a source that really exists, asserts on the message, on the source still being
there, on no destination having been created, and on **zero requests reaching the wire** â€” the last being
the only assertion that catches the dangerous emulation whose copy lands before it returns an
honest-looking `Err`. Proven by substituting a working emulation; see the PR body.

### Item 5 â€” already landed in PR #896 round 3

The doc sentence at `probe_prefix` already says the corrected thing ("It is **not** the gateway that
under-fills â€¦ what the second key defends against is a gateway that **honours `max-keys`** and lies about
`IsTruncated`") and already names the test that stands that server up
(`a_server_that_denies_being_truncated_is_still_seen_as_a_non_empty_directory`). The ticket was filed
against round 2's wording. Verified rather than assumed: the doc's own negative-control claim â€” that
setting `max-keys` to `1` reds that test **and only that test** â€” was re-run and holds.

### Item 6 â€” **no proportionate fix; closed**

Verdict: a server that lies about both its page fill and its `IsTruncated` flag cannot be defended against
by asking it more questions. Every signal `probe_prefix` can consult is a claim by the same server, so one
willing to misreport two can misreport a third as cheaply. The only alternative to asking is proving â€”
enumerating an unbounded prefix, which defeats the one-request design and still ends at the server's own
answer. No third belt was invented. The hole is written into `probe_prefix`'s doc under "what this cannot
defend against", and pinned by a characterisation test
(`an_underfilling_server_that_also_denies_truncation_defeats_both_belts_and_that_is_recorded_not_fixed`)
which reds if anyone ever does find a defence, so the doc cannot silently go stale.

### Item 7 â€” standing caveat, no code change

Written into `probe_prefix`'s doc as a permanent caveat rather than a TODO. Everything here is still
in-process `tiny_http`/raw sockets over `ureq` 2.12.1 on Windows: **no real S3, MinIO, Ceph or QNAP.**
Item 1's premise â€” what a `DeleteObject`-without-`ListBucket` credential does on MinIO/Ceph â€” remains
unverified against a real gateway. The QNAP on the LAN is the first real target.

### Item 8 â€” the disclosed comment was over-broad, but **not by the case the author named**

Measured, not accepted. The disclosure said a control byte cannot reach the counting path because a raw
control character makes the listing non-XML and `roxmltree` rejects the document. True of most of the C0
range and **false for three bytes**: XML 1.0 Â§2.2 exempts tab (`0x09`), LF (`0x0A`) and CR (`0x0D`), and
Rust's `char::is_control` is `true` for all three. So a tab-bearing leaf parses cleanly, is refused by
`is_safe_s3_leaf`, and *is* counted â€” the shape item 8 called unreachable.

New test `a_tab_in_a_key_is_a_control_byte_that_really_does_reach_the_filtered_count_unlike_a_nul` pins
both halves (tab â†’ `filtered_count == 1`; NUL â†’ parse error). The comment now reads **four** reachable
shapes with the control byte narrowed to the three that are legal XML, rather than the three the ticket
asked for. The same over-broad sentence appeared a second time on
`delete_refuses_a_directory_whose_only_content_is_an_object_list_filters_out`; both were corrected in
lockstep.
## 8. One code comment is over-broad by exactly one case (disclosed by the author)

`probe_prefix`'s comment lists **"a control byte"** among the leaf shapes that reach the counting bug. It
does not: a raw control character makes the listing non-XML and `roxmltree` rejects the whole document, so
that case **fails loudly** rather than falsely succeeding. Three reachable shapes (`\`, exactly `..`,
exactly `.`), not four.

The test itself is sound â€” its fixture sentinel uses the backslash shape, which is genuinely reachable. The
author disclosed this rather than push a fourth round for a comment, which was the right call on timing.
Fix it whenever that file is next touched.

## Work Log

**Closed 2026-08-14, merged as PR #900 (`57e382b4`).** Two rounds.

### Item 1 â€” name the probe, deliberately not a fallback

`delete` and `stat` each ran a `ListObjectsV2` probe and propagated its failure with `?`, so a credential
holding `s3:DeleteObject`/`s3:GetObject` but **not** `s3:ListBucket` got an access-denied error **naming a
prefix the user never typed** â€” deleting `/photos/a.jpg` errored about `"photos/a.jpg/"`. Masked on AWS
proper (no `ListBucket` â†’ HEAD answers 403 not 404, so `stat` never reaches its probe) but **reachable on
MinIO and Ceph**, which is exactly what a self-hoster points this at.

**The fallback was refused, and the reviewer proved the refusal right** rather than accepting it â€”
implementing option B and reading past the `expect_err`:

```
delete("/photos/2024") returned Ok(())
a.jpg still on disk = true, b.jpg still on disk = true
```

The app reports a folder deleted while every object in it remains. The argument â€” *letting a denied probe
re-enable what a successful probe forbids makes the guard weaker the less the server will say, which is
backwards* â€” is now measured, not reasoned.

The new fixture answers 403 **with the `AccessDenied` XML body**, because a bodiless 403 exercises the
wrong arm of `map_response_error` (a GET carries a body, unlike the HEADs the existing sentinel is for).
Confirmed by reading the code path, not assumed.

### Item 6 â€” closed as "no proportionate fix", then **disproved**

I authorised closing it: a server lying about both its page fill and its `IsTruncated` cannot be caught by
asking it more questions. The reviewer strengthened that by building a third belt and finding it **breaks
the honest server's empty-directory delete** â€” a defence that catches the liar necessarily breaking the
conforming server.

**The UAT then found one that works.** Re-list the prefix with **`start-after`** past the marker, only on
the marker-only verdict. That is not the same question re-asked: under-filling a page is *legal* S3
latitude so the first lie is free, while **returning zero keys with `IsTruncated=false` when keys exist is
a flat protocol violation** â€” the belt forces a strictly stronger lie.

```
CASE H  honest, genuinely empty:   delete ok = true,  DELETEs sent: ["/test-bucket/photos/"]
CASE L  under-fills AND denies:    delete ok = false, DELETEs sent: []
```

**Full existing suite green with it in place (176).** And it explained *why the reviewer concluded
otherwise*: a naive belt counting `raw_entries` false-positives on the re-returned marker, and the fixture
**ignores `start-after` entirely** so it could not have shown the difference.

The claim was withdrawn from the code with the reasoning recorded â€” including why the earlier conclusion
failed, which is worth more than the conclusion. The belt itself is **CPE-1727**: a new request on a hot
path deserves its own evidence, which is a different reason from "it cannot be done".

### The ticket was wrong about control bytes, and the correction was wrong too

I wrote that a control byte can never reach the counting path because `roxmltree` rejects a non-XML
document. **False for three bytes** â€” XML 1.0 Â§2.2 exempts tab/LF/CR â€” so a tab-bearing leaf parses, is
refused, and **is** counted. The worker measured it and corrected the comment to four reachable shapes.

Then the reviewer found the correction one byte too broad: **Â§2.11 normalises CRâ†’LF**, so a CR-bearing key
arrives as `0x0A`. **Three bytes leave the server, two arrive.** Measured per byte:

```
0x09 parses, arrives 0x09, filtered_count=1
0x0A parses, arrives 0x0A, filtered_count=1
0x0D parses, arrives 0x0A, filtered_count=1   <- CR->LF
0x00 / 0x0B / 0x1F  parse REFUSED
```

Three sibling sentences still asserting the pre-correction set were corrected with it.

### Evidence Rule 1, unmet for a third of the guard

The PR said the wrapper was wired at **all three** probe call sites. Two were pinned; removing the third â€”
`stat`'s **bucket-root** probe â€” left the whole suite green. In a PR whose own body presents
guard-neutralisation as its evidence, the coverage claim was one third wider than the evidence.

Pinned by `stat_of_the_bucket_root_without_listbucket_also_names_the_probe`. Worth noting how the pin was
proved: **my first neutralisation hit the wrong one of the three sites** and redded the already-covered
test. A red that is not the red you aimed at proves nothing. Targeted precisely, exactly one test reds.

All three now red **alone**, each a different test, with the unwrapped error each would produce:

```
bucket-root  ->  s3: "":                s3: HTTP 403 AccessDenied
delete       ->  s3: "photos/a.jpg/":   s3: HTTP 403 AccessDenied
stat 404     ->  s3: "photos/":         s3: HTTP 403 AccessDenied
```

### Also verified

Item 4 both halves in one run: against a **working** copy-then-delete emulation the new trait-object
assertion reds while the old `.is_err()` one **passes green**. Item 3's display symptom pinned through
`remote_dir_entries` on a `&dyn FileSystemProvider` â€” the real channel. Item 5 needed no work; it had
landed in #896 round 3, and was verified rather than assumed by re-running the doc's own negative control.

The entitled operations were driven end to end and all still work: `stat` on a real object, `read`,
`write`, `mkdir`, and a `delete` that correctly refuses while leaving the object on disk.

### Standing caveat, and one CI note

**Nothing here has been measured against a real gateway** â€” no S3, MinIO, Ceph or QNAP. Item 1's whole
premise is what a particular credential configuration does on MinIO/Ceph, so that residual is real and
recorded in the code, the ticket and the PR body.

The GUI smoke leg redded three unrelated specs on a 45-minute run with **no `AssertionError` anywhere** in
the log â€” filed as **CPE-1728**, re-run green. A single green re-run is weak evidence, and that ticket
says so.

Verdicts: Reviewer **APPROVE**, UAT **PASS**. All CI green.

Filed, not fixed: **CPE-1727** (HEAD-proof delete restoring the entitled operation, the `start-after` belt,
`list`'s bare 403 message), **CPE-1728** (the GUI smoke degradation).

