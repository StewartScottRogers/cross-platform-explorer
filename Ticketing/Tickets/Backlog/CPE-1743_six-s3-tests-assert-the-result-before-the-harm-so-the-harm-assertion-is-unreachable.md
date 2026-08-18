---
id: CPE-1743
title: Six cpe-s3 tests assert the Result before the harm, so the harm assertion cannot fire
type: test
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-14
closed:
---

## Problem

Found by the **PR #903 review, round 6**, immediately after that round fixed the identical defect in the
two tests it had just written. That is the point of the ticket: **round 6 fixed the instances, not the
pattern.**

The shape:

```rust
let err = provider.delete("/photos").expect_err("...");   // verdict FIRST
assert!(root.join("photos/a.jpg").is_file(), "the subtree must survive ...");   // harm SECOND
```

If the guard fails by returning `Err`, this is fine. **If it fails by returning `Ok` — which is how every
bug in this family has behaved — the run stops at `expect_err` and the assertion carrying the damage
never runs.** The test still reds, so it looks like it is doing its job; it reds on "expected an error,
got `()`" rather than on "the user's files are gone".

That distinction is not cosmetic. The round-5 UAT caught it in the two new tests and the Foreman's own
summary had reported *"delete `Ok`, marker gone"* when only the first half was ever asserted.

## The six sites

All in `crates/s3/src/provider.rs`. **Scope of this list: that one file, from a review scan — no other
file was searched**, and other crates are unexamined.

| Line | Test |
|---|---|
| 4619 | `rename_is_refused_by_name_and_issues_no_request_at_all` |
| 4653 | `delete_of_a_directory_with_content_is_refused_and_removes_nothing` |
| 4724 | `a_directory_whose_first_returned_key_is_only_its_marker_is_still_refused_by_delete` |
| 4757 | `delete_refuses_a_directory_whose_only_content_is_an_object_list_filters_out` |
| 4805 | `delete_without_listbucket_names_the_probe_instead_of_a_prefix_the_user_never_typed` |
| 5012 | `a_denied_probe_does_not_re_enable_the_directory_delete_that_a_successful_probe_refuses` |

**Line 4653 is the consequential one.** CPE-1727's round-1 guard evidence (G2) leaned on it as *the* test
carrying the subtree-destruction proof — and that proof is reachable only when the guard fails by
returning `Err`. Round 1 disclosed the limitation honestly rather than claiming harm it had not asserted,
which is why this is a follow-up and not a defect in what shipped.

## What to do

- [ ] Capture the outcome, assert the effect, **then** unwrap — the same one-line reorder round 6 applied
      twice:
      ```rust
      let outcome = provider.delete("/photos");
      assert!(root.join("photos/a.jpg").is_file(), "... (outcome was {outcome:?})");
      let err = outcome.expect_err("...");
      ```
      Interpolating the outcome into the harm message is part of it — the round-6 red reads
      `THE HARM: ... the marker was deleted (outcome was Ok(()))`, which names the damage *and* the
      cheerful success in one line.
- [ ] **Prove each reorder is not cosmetic.** For each site, neutralise the guard it exercises so that it
      returns `Ok`, and show the red now names the harm rather than `expected an error, got ()`. A reorder
      nobody demonstrated is indistinguishable from a reorder that changed nothing.
- [ ] **Then look for the pattern rather than the list.** This ticket exists because six instances
      survived a round that fixed two. Search the other crates — `webdav`, `ftp`, `sftp`, `server`,
      `src-tauri` — for `expect_err`/`unwrap_err` followed by a filesystem assertion, and **write the
      scope of that search down**, including where you did not look.
- [ ] Consider whether a lint, a helper, or a line in `Ticketing/wiki.md`'s Evidence Rules would stop the
      next one. The rule *"assert on the filesystem, never on the returned `Result`"* is already written
      and was still broken here — by the people enforcing it — so the rule alone is demonstrably not
      enough. Say what would be.

## Notes

Filed by the Foreman from the PR #903 review, 2026-08-14. Non-blocking there: the tests pass, they still
red, and nothing shipped is wrong.

Related: **CPE-1727** (where the pattern was found and fixed twice), **CPE-1740** (the other deferral from
the same review), and `Ticketing/wiki.md` → Evidence Rules, rule 1 — a test that cannot fail is not
evidence, of which this is the subtler variant: a test that cannot fail *for the reason it names*.


## Work Log

Worked on branch `CPE-1743-s3-harm-asserted-before-result`. This log went through two rounds of
independent review (Reviewer + UAT) that caught real problems in the first draft — corrected below,
with the corrections left visible rather than silently rewritten, because the corrections are
themselves the most valuable output of this ticket.

### The ten fixes (all in `crates/s3/src/provider.rs`)

Reordered each: capture the `Result` into `outcome`, assert the harm (interpolating `outcome` into the
message), then `outcome.expect_err(...)`. The six named in the ticket, plus two more found while
covering the rest of `crates/s3` per the ticket's own scope note, plus two more the first review pass
missed (F1):

| # | Test (final line) | Guard exercised |
|---|---|---|
| 1 | `rename_is_refused_by_name_and_issues_no_request_at_all` (5271) | `rename` refuses unconditionally, no request sent |
| 2 | `delete_of_a_directory_with_content_is_refused_and_removes_nothing` (5308) | `delete`'s first recursion guard (`real_entries > 0`) |
| 3 | `a_directory_whose_first_returned_key_is_only_its_marker_is_still_refused_by_delete` (5388) | same guard, marker-only-but-truncated page |
| 4 | `delete_refuses_a_directory_whose_only_content_is_an_object_list_filters_out` (5427) | same guard, `filtered_count` counted into `real_entries` |
| 5 | `delete_without_listbucket_names_the_probe_instead_of_a_prefix_the_user_never_typed` (5477) | probe-denied → HEAD-fallback refusal |
| 6 | `a_denied_probe_does_not_re_enable_the_directory_delete_that_a_successful_probe_refuses` (5689) | same HEAD-fallback refusal, real content this time |
| 7 | `a_server_that_rejects_start_after_now_refuses_an_empty_directory_delete_it_used_to_allow` | the third belt (`probe_prefix_after` error handling) |
| 8 | `every_object_op_works_through_a_trait_object_the_way_production_holds_the_provider` | `rename` again, exercised through `&dyn FileSystemProvider` |
| 9 | `is_truncated_true_with_no_continuation_token_is_refused_not_silently_truncated` (found in review F1) | the pagination-continuation guard in `list_with_filtered_count` |
| 10 | `sending_a_request_with_a_header_byte_ureq_would_drop_is_refused_before_any_bytes_leave_the_process` (found in review F1) | `guard_header_sendable` on the `Authorization` header |

Every `expect_err`/`unwrap_err`/`is_err()` site remaining in `crates/s3` was (re-)read after both review
rounds; none has a state assertion after it. `cargo test -p cpe-s3`: 195 passed, 0 failed, both before
and after every neutralisation below was reverted.

### Red-proof — corrected methodology after review

**What went wrong in the first pass, and why it matters.** The first draft of this Work Log pasted a
"red" for sites 2, 3 and 6 that I now know was not evidence: I "neutralised the guard" by writing a
**brand-new recursive-delete walker inside the test-only neutralisation** ("list the directory, delete
each real entry, then report `Ok`") to force those tests to fail on the harm message. That code does not
exist in production — `delete` (`provider.rs:2408-2513`) issues exactly one non-recursive `DELETE` of one
computed key, by design, and no fault to *existing* code can make it reach a child object it was never
going to touch. An independent Reviewer and an independent UAT both initially accepted this as proof;
a later, sharper pass (**minimal fault only: flip the exact boolean the test names, add no new code**)
showed the reorder does not change the observed failure for those three sites today. I did the same
thing to F1's two new finds under first inspection and initially assumed both would diverge; a minimal-
fault re-check found one does and one structurally cannot (see below). This is now recorded in
`Ticketing/wiki.md`'s Evidence Rules as its own trap, separate from the underlying ticket.

**Method used below, consistently:** for every site, the *only* change made under "red-proof" is
flipping the exact boolean/condition the test's own docstring names, or (for the two rename-shaped
tests) performing the copy-then-`Ok` emulation that `rename`'s own doc comment names as "the tempting
implementation" this function exists to refuse — not a walker, not a loop, not any capability `delete`
doesn't already have. Every neutralisation was committed-around (fix committed first), applied, tested,
captured, then reverted with `git checkout -- crates/s3/src/provider.rs`, confirmed by a subsequent full
`cargo test -p cpe-s3` passing clean (195/195) before moving to the next site.

#### Group A — proven under a minimal, faithful fault (4 of 10)

**1. `rename_is_refused_by_name_and_issues_no_request_at_all`.** Minimal fault: `rename`'s own doc names
the copy-then-delete emulation as the historically tempting bug this function exists to refuse, so
implementing exactly that emulation (real `write` to the destination key, then `Ok`) is the faithful
fault, not an invention — `S3Provider` already has `write`; nothing new was added.
```
thread '...rename_is_refused_by_name_and_issues_no_request_at_all' panicked at src\provider.rs:5286:9:
assertion `left == right` failed: rename reached the network — refusing means refusing: no CopyObject PUT and no DELETE may be
issued, because a delete that fails after a successful copy silently leaves two objects (outcome was Ok(()))
  left: 2
 right: 0
```

**4. `delete_refuses_a_directory_whose_only_content_is_an_object_list_filters_out`.** Minimal fault, at
the single shared source of truth both of `delete`'s recursion checks read
(`probe_prefix_after`'s return, `provider.rs:1788`): reverted `page.entries.len() + page.filtered_count`
to `page.entries.len()` alone — the literal pre-CPE-1706 bug this computation exists to fix. One line,
no new code, and it defeats BOTH the first check and the belt at once because they share this one
function — which is exactly why this site (unlike 2/3/6 below) is reachable under a minimal fault: its
guard has no independent second gate to catch what the first one misses.
```
thread '...delete_refuses_a_directory_whose_only_content_is_an_object_list_filters_out' panicked at src\provider.rs:5457:9:
the marker was deleted by a refused delete — the folder would vanish from listings while the object underneath it survived (outcome was Ok(()))
```
Confirmed this is a distinct, single-guard break: with the fault applied, `cargo test -p cpe-s3` showed
exactly 1 failure (194 passed), not a cascade.

**7. `a_server_that_rejects_start_after_now_refuses_an_empty_directory_delete_it_used_to_allow`.**
Minimal fault: the third belt's own doc says the danger it defends against is "treating a failed
confirmation as consent" — so `probe_prefix_after(...).map_err(|refusal| marker_confirmation_failure(...))?`
becomes `.unwrap_or((0, 0, false))`, i.e. exactly that: a failed confirmation defaults to "nothing found",
no new code.
```
thread '...a_server_that_rejects_start_after_now_refuses_an_empty_directory_delete_it_used_to_allow' panicked at src\provider.rs:5993:9:
the empty folder is still there — that is the cost this test exists to record (outcome was Ok(()))
```
(The assertion fires because the marker really is gone — the folder is not "still there".)

**8. `every_object_op_works_through_a_trait_object_the_way_production_holds_the_provider`.** Same
`rename` emulation as site 1, exercised through `&dyn FileSystemProvider`:
```
thread '...every_object_op_works_through_a_trait_object_the_way_production_holds_the_provider' panicked at src\provider.rs:6760:9:
assertion `left == right` failed: rename reached the network through dyn: an emulation whose copy lands and which then reports
an honest-looking error still leaves the user two objects believing they have one, and only 'no request was sent' can tell that
apart from a real refusal (outcome was Ok(()))
  left: 11
 right: 9
```

#### Group B — reachable ONLY by inventing a production code path that does not exist today (3 of 10)

For sites 2, 3 and 6, `delete`'s only destructive act on the probe-denied / recursion-refused path is a
single non-recursive `DELETE` of one already-computed key. Under a **minimal** fault (just disabling the
refusal check, no new code), that single `DELETE` targets a key that is either (a) a marker key with no
real object behind it in this fixture, so the fixture's own `remove_dir`/`remove_file` no-ops silently
(204 either way — S3's real idempotent behaviour), or (b) the literal path string with no trailing
slash, which is a directory on disk and `remove_file` on a directory fails silently. Either way, the
asserted child file survives regardless of assertion order, and both branch and main show the same
generic panic. Real output, minimal fault, both booleans of the recursion-refusal check disabled
(`if false && (...)`, no new code):

```
thread '...delete_of_a_directory_with_content_is_refused_and_removes_nothing' panicked at src\provider.rs:5340:27:
S3 answers 204 to a DELETE of a key that never existed, so a single-key delete of a directory prefix would report success while the whole subtree stayed put: ()

thread '...a_directory_whose_first_returned_key_is_only_its_marker_is_still_refused_by_delete' panicked at src\provider.rs:5416:27:
a server that returned only the marker on the first page must not be read as an empty directory — IsTruncated said there was more: ()
```
`cargo test -p cpe-s3` under this fault: 192 passed, 2 failed (exactly sites 2 and 3 — confirms the fault
is scoped to this one guard, not a cascade; site 6 uses an unrelated code path and stayed green under
this specific fault, confirming it needed its own).

Site 6, minimal fault on its own gate (`if self.head_proves_object(&key).unwrap_or(false)` →
`if true || ...`, no new code):
```
thread '...a_denied_probe_does_not_re_enable_the_directory_delete_that_a_successful_probe_refuses' panicked at src\provider.rs:5720:17:
falling back to an un-probed single-key DELETE here would return 204 and report a whole subtree removed while every object in it stayed put: ()
```
This IDENTICAL panic — same message, same `: ()` shape — is what **main** (unreordered) already
produces for this guard failure; the reorder changes nothing observable for this specific site today.
My first draft's "harm" red for these three sites (`a.jpg was deleted by a refused delete (outcome was
Ok(()))`) was produced only by adding a walker that iterates `self.list(path)` and issues a real `DELETE`
per child — genuine production capability `delete` does not have. That proof was withdrawn; it is not
evidence for today's code, only for a hypothetical future where recursive deletion is implemented.

#### Group C — conceded undemonstrable under any fault reachable today (3 of 10)

**5. `delete_without_listbucket_names_the_probe_instead_of_a_prefix_the_user_never_typed`.** Its target,
`/photos/gone.jpg`, never exists. `delete` only ever issues a single-key `DELETE` of the literal key
given, so no fault — minimal or invented — can make it reach `photos/a.jpg`, an unrelated sibling key.
Two different neutralisations tried (accept-any-HEAD, and treat probe-denial as an empty-directory
verdict); both delete only the never-existent literal target (204 no-op either way):
```
thread '...delete_without_listbucket_names_the_probe_instead_of_a_prefix_the_user_never_typed' panicked at src\provider.rs:5504:27:
the directory check cannot run and no HEAD proves an object, so the delete must be refused, not guessed: ()
```

**9 (F1a). `is_truncated_true_with_no_continuation_token_is_refused_not_silently_truncated`.** The
fixture returns the byte-identical truncated response to every request regardless of continuation
token, so the only way `list` can reach `Ok` is to stop after exactly the first page (treat the
malformed page as terminal) — which is also the only fault that keeps the request count at exactly 1,
the value the harm assertion checks for. There is no fault, minimal or invented, that produces `Ok`
*and* a request count other than 1 against this fixture, so the harm assertion cannot diverge:
```
thread '...is_truncated_true_with_no_continuation_token_is_refused_not_silently_truncated' panicked at src\provider.rs:4208:27:
IsTruncated=true with no NextContinuationToken must be a loud error, not a silently truncated-but-reported-complete listing: [ProviderEntry { name: "f0.txt", is_dir: false, size: 1 }]
```

**10 (F1b). `sending_a_request_with_a_header_byte_ureq_would_drop_is_refused_before_any_bytes_leave_the_process`.**
Corrected from the earlier assumption that this one *is* provable. The test's own module-level doc says
outright that `ureq` 2.12.1 already refuses this exact byte range independently, before any bytes reach
the network — our `guard_header_sendable` call is now redundant with `ureq`'s own validation, kept only
because it fails "one beat sooner with a clearer message." Skipping our guard (the only fault available
to code we control) still leaves `ureq`'s own check as the backstop: the request never reaches the
fixture (`requests.load()` stays `0`, the harm assertion), and `list` still returns an `Err` — just
`ureq`'s own error text instead of ours, so the test reds later, on `err.contains("ureq")` failing:
```
thread '...sending_a_request_with_a_header_byte_ureq_would_drop_is_refused_before_any_bytes_leave_the_process' panicked at src\provider.rs:4244:9:
s3: http://127.0.0.1:61510/test-bucket?delimiter=%2F&list-type=2&max-keys=1000: Bad Header: invalid header 'Authorization: AWS4-HMAC-SHA256 Credential=AKIAéEXAMPLE/20260818/us-east-1/s3/aws4_request, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature=feba0d57b505b5e6bebf512aee2634bca82cac1c6028c3f98f563e1a6ca34fa4'
```
No fault to code inside this crate can bypass `ureq`'s own validation, so `Ok` is never reachable for
this exact input at all — the harm ("bytes leave the process") is structurally unreachable today, not
merely undemonstrated. The reorder is still correct (the harm assertion is exactly right if `ureq` ever
loosens its own validation, or if a different unsendable byte pattern ureq tolerates but this crate
should not), but I cannot paste a red that proves it bites today, and pretending otherwise would repeat
this ticket's own mistake.

### Why the reorder is still right despite Groups B and C

It is **strictly non-worse everywhere** (no site got harder to diagnose), **demonstrably better at four
sites today** (Group A), and **correct in advance** for the day `delete` grows real recursive-delete
support or `ureq`/a different byte pattern stops being a redundant backstop — at which point Groups B
and C's tests start actually catching what they were written to catch, with no further changes needed.

### Two more sites found and fixed in the same file (recap, code unchanged from earlier report)

The wider scan found 2 more instances of the identical shape still in `provider.rs` (now rows 7-8 in the
table above), which the ticket's own scope note said explicitly to also cover ("at minimum the rest of
`crates/s3/`"), and a subsequent independent review found 2 more still (rows 9-10, F1). `crates/s3` was
re-read in full after each round; no `expect_err`/`unwrap_err`/`is_err()` site remains with a state
assertion after it.

### Wider scan: what was searched, what was found — corrected counts (review F3)

The first pass of this scan under-counted by roughly 3x, and mis-filed 4 sites under the wrong file
name. An independent brace-matched scan (verdict token — `expect_err`/`unwrap_err`/`.is_err()` — followed
later in the same test body by an assertion whose arguments contain `.is_file()|.is_dir()|.exists()|
fs::read|read_to_string(|.load(Ordering|fs::metadata|read_dir(`) found the corrected counts below, spot-
checked as real (e.g. `split_join.rs:1281`: `unwrap_err()` then
`assert_eq!(fs::read(&out).unwrap(), b"pre-existing content", "must not touch the existing file")`, and
`split_join.rs:1295`).

**Searched, matches found (fixed — see above):**
- `crates/s3/` (whole crate) — **10** instances, all now fixed.

**Searched, matches found, NOT fixed (too large for this ticket — corrected list for the follow-up):**
- `crates/server` and `src-tauri` combined — **~49** instances (corrected from the first pass's ~18; treat
  as a well-supported lower bound with a small false-positive tail, not an exact count):
  - `batch_execute.rs` — **12** sites (unchanged from the first pass; lines 767->773, 969->971, 1020->1023,
    1096->1100, 1129->1137, 1212->1220, 1264->1271, 1325->1330, 1377->1384, 3730->3735, 4264->4269-4271,
    4296->4300-4302).
  - `src-tauri/src/lib.rs` — **9** (first pass said 3; the first pass's `:14042`, `:16406`, `:16458` were
    real but undercounted -- 6 more of the same shape were missed on the first read).
  - `split_join.rs` — **9** (first pass said 1; `:634` was real, 8 more were missed, including the
    spot-checked `:1281` and `:1295`).
  - `vault_manager.rs` — **8** (first pass said 2; `:4177` and `:4681` were real, 6 more were missed).
  - `batch_media.rs` — **4** (the first pass filed these under `batch_execute.rs` — **wrong filename**;
    they are a separate file and were never actually read).
  - `transfer.rs` — **2** (matches first pass: `:1570`, `:1602`).
  - `fsutil.rs` — **2** (matches first pass: the link-staging test's dangling and live legs).
  - `snapshot_capture.rs` — **1** (first pass listed this file as already-correctly-ordered — **wrong**,
    it was never actually checked closely enough).
  - `secure_shred.rs` — **1** (matches first pass, `:445` — **destructive/irreversible domain, highest
    severity**).
  - `folder_template.rs` — **1** (matches first pass, `:389`).
  - **Remove `vault_crypto.rs` from the "already correctly ordered" list** — `vault_crypto.rs:1148`
    `extraction_refuses_nonempty_out_dir_without_clobbering` is exactly the `.is_err()` variant of this
    same defect, which the first pass itself flagged as "not exhaustively grepped" and then, correctly by
    its own caveat, missed.
  - Everything else the first pass listed as already-correctly-ordered (`archive.rs` — whose own doc
    comment names this exact defect pattern and defends against it — `backup.rs`, `fs_route.rs`,
    `log_window.rs`, `op_plan.rs`, `macro_run.rs`, `rar.rs`, `media_meta.rs`, `media_meta_write.rs`,
    `net_share.rs`, `native_meta.rs`, `disk_usage.rs`, `image_diff.rs`, `copilot.rs`,
    `copilot_planner.rs`, `action_macro.rs`, `video_meta_write.rs`, `thumb_video.rs`, `thumb_source.rs`,
    `thumb_font.rs`) stands, **except** `vault_crypto.rs` and `snapshot_capture.rs`, both corrected above.

**Searched, no matches (confirmed correct by the independent scan too):**
- `crates/webdav`, `crates/ftp`, `crates/sftp` — no defect.
- `crates/net`, `crates/vfs`, `crates/contract` — message-only `expect_err` sites, no defect.
- `crates/mdns`, `crates/security`, `crates/updater-verify` — no `expect_err` at all.

**Not searched (explicitly out of this ticket's scope):**
- `src/` (Svelte/TS frontend) and `sidecar/` — out of this ticket's Rust-provider scope.

**Total found beyond the ten fixed in `crates/s3`: ~49 confirmed-or-strongly-likely instances**,
concentrated in `crates/server` (worst offenders `batch_execute.rs` at 12 and `split_join.rs`/
`vault_manager.rs` at 8-9 each) and `src-tauri/src/lib.rs` (9). This is well beyond one S-ticket's
scope — recommend the Foreman file a follow-up ticket (small epic, given the count and file spread)
scoped to `crates/server` + `src-tauri`, prioritising `batch_execute.rs`, `vault_manager.rs`, and
`secure_shred.rs` first given their destructive/irreversible blast radius, using `archive.rs`'s
already-correctly-ordered tests as the reference pattern. **Given this ticket's own F1/F2/F3 review
history, the follow-up should re-verify counts itself before committing to a scope** rather than
trusting either scan's numbers as final — both were independently found short at least once.

### Guard decision

**Did not build a mechanical scanner/lint.** Reasoning:

- A text/regex-based scanner (matching the repo's own precedent, e.g. `src/lib/epicsQueueLayout.test.ts`)
  is *plausible* — flag any `#[test] fn` whose body contains `.expect_err(`/`.unwrap_err()`/an
  `is_err()` check followed later in the same function by a call recognisable as a state assertion. But
  getting this right without false positives needs real function-boundary and statement-order tracking,
  and this ticket's own F3 finding is direct evidence of the risk in the other direction: even a careful
  *human-directed* brace-matched scan under-counted by 3x and mis-filed 4 sites under the wrong file name
  on its first pass. A scanner built under similar time pressure would likely have the same failure mode,
  and a scanner that cries wolf (or silently under-counts) gets ignored rather than fixed, which is worse
  than no scanner.
- The wider scan proved the softer, cheaper intervention — **a strengthened, written-down rule** — was
  not sufficient on its own before this ticket (the rule already existed in prose, and was still broken
  ten times in one file by the people who wrote that rule). So I added two paragraphs to
  `Ticketing/wiki.md` → Evidence Rules, rule 1: the reorder pattern itself, and — new, out of this
  ticket's own review — a second paragraph warning against the specific trap this ticket's review walked
  into twice (a worker, an independent Reviewer, and an independent UAT all initially accepted a red
  produced by inventing new production code inside a neutralisation as if it proved something about
  today's code).
- **What would close the gap for real:** given the corrected wider scan found ~49 more instances
  concentrated in two crates, I think the right next mechanical step is a **scoped guard test in
  `crates/server`** (by far the largest concentration) once the follow-up ticket fixes that crate's
  instances and re-verifies the count itself — at that point the crate is a known-clean, correctly-
  counted baseline, and a same-crate scanner has a much smaller, better-understood false-positive surface
  than a repo-wide one attempted cold, alongside an S-sized ticket, under time pressure that this
  ticket's own F3 finding shows produces real misses.

### Assumptions / unverified

- The corrected wider-scan line numbers (F3) came from the coordinator relaying an independent scan's
  findings, spot-checked by that scan at two sites (`split_join.rs:1281`, `:1295`) but not independently
  re-verified line-by-line by this ticket's worker against current `HEAD` for every row. The Foreman
  filing the follow-up should re-grep for exact current line numbers before committing to a scope, per
  the note above.
- Clippy was run for `crates/s3` only (`cargo clippy --all-targets -- -D warnings`, matching this crate's
  one CI invocation — confirmed by an independent reviewer that `crates/s3/Cargo.toml` has no
  `[features]` section and CI has a single `s3 — clippy + test` step, so "both feature modes" does not
  apply here). `crates/server` and `src-tauri` were not touched, so their own (multi-feature) clippy legs
  were not re-run.
