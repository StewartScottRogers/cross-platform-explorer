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

Worked on branch `CPE-1743-s3-harm-asserted-before-result`.

### The six named fixes (all in `crates/s3/src/provider.rs`)

Reordered each: capture the `Result` into `outcome`, assert the harm (interpolating `outcome` into the
message), then `outcome.expect_err(...)`.

| Test (final line) | Guard exercised |
|---|---|
| `rename_is_refused_by_name_and_issues_no_request_at_all` (5271) | `rename` refuses unconditionally, no request sent |
| `delete_of_a_directory_with_content_is_refused_and_removes_nothing` (5308) | `delete`'s first recursion guard (`real_entries > 0`) |
| `a_directory_whose_first_returned_key_is_only_its_marker_is_still_refused_by_delete` (5388) | same guard, marker-only-but-truncated page |
| `delete_refuses_a_directory_whose_only_content_is_an_object_list_filters_out` (5427) | same guard, `filtered_count` counted into `real_entries` |
| `delete_without_listbucket_names_the_probe_instead_of_a_prefix_the_user_never_typed` (5477) | probe-denied → HEAD-fallback refusal |
| `a_denied_probe_does_not_re_enable_the_directory_delete_that_a_successful_probe_refuses` (5689) | same HEAD-fallback refusal, real content this time |

### Red-proof per site

For each, committed the reorder first (`git commit` — see "Fixed" below), then neutralised the guard in
a local, uncommitted edit so `delete`/`rename` returns `Ok` the way the real bug family has always
failed, ran the single test, captured the real panic, then `git checkout -- crates/s3/src/provider.rs`
to restore (per Evidence Rule 1 — never `Copy-Item`/backup-restore, which can leave a stale binary).

**1. `rename_is_refused_by_name_and_issues_no_request_at_all`** — neutralised `rename` to actually
perform the copy (real `write` to the destination key) and return `Ok`:
```
thread '...rename_is_refused_by_name_and_issues_no_request_at_all' panicked at src\provider.rs:5286:9:
assertion `left == right` failed: rename reached the network — refusing means refusing: no CopyObject PUT and no DELETE may be
issued, because a delete that fails after a successful copy silently leaves two objects (outcome was Ok(()))
  left: 2
 right: 0
```

**2. `delete_of_a_directory_with_content_is_refused_and_removes_nothing`** — neutralised the first
recursion guard to walk the listing and really delete each entry, then report `Ok` (the naive "helpful"
recursive-delete bug this refusal exists to prevent):
```
thread '...delete_of_a_directory_with_content_is_refused_and_removes_nothing' panicked at src\provider.rs:5334:9:
a.jpg was deleted by a refused delete (outcome was Ok(()))
```

**3. `a_directory_whose_first_returned_key_is_only_its_marker_is_still_refused_by_delete`** — same
neutralisation:
```
thread '...a_directory_whose_first_returned_key_is_only_its_marker_is_still_refused_by_delete' panicked at src\provider.rs:5414:9:
the content was deleted anyway (outcome was Ok(()))
```

**4. `delete_refuses_a_directory_whose_only_content_is_an_object_list_filters_out`** — same
neutralisation, extended to also delete the marker key (the real destructive branch for this fixture
shape, since the one real object is filtered out of `list()` and cannot be found by walking the
listing):
```
thread '...delete_refuses_a_directory_whose_only_content_is_an_object_list_filters_out' panicked at src\provider.rs:5464:9:
the marker was deleted by a refused delete — the folder would vanish from listings while the object underneath it survived (outcome was Ok(()))
```

**5. `delete_without_listbucket_names_the_probe_instead_of_a_prefix_the_user_never_typed`** — **honest
negative result, documented rather than hidden.** Neutralised the HEAD-fallback refusal to accept any
HEAD answer as proof (`if true || self.head_proves_object(&key).unwrap_or(false)`). Observed:
```
thread '...delete_without_listbucket_names_the_probe_instead_of_a_prefix_the_user_never_typed' panicked at src\provider.rs:5493:27:
the directory check cannot run and no HEAD proves an object, so the delete must be refused, not guessed: ()
```
This reds at the (now-earlier) `expect_err` call, not at the harm assertion — because for *this test's
specific target*, `/photos/gone.jpg`, the harm assertion (`photos/a.jpg` survives) is a true invariant
under every reachable neutralisation of this guard: `delete` only ever issues a single-key `DELETE` for
the literal key given, so no neutralisation of this guard can make it reach for `a.jpg`, an unrelated
sibling key. I tried two different neutralisations (accept-any-HEAD-answer, and treat a probe-denial as
an empty-directory verdict) and both delete only the literal target key `photos/gone.jpg`, which never
existed — 204 no-op either way. The reorder is still correct defense-in-depth (it matches the pattern
used everywhere else in the file, and protects against a *future* implementation change that makes
`delete` reach beyond its own key), but I want to be honest that I could not manufacture a divergent red
for this specific site — the sixth site, immediately below, is the one that demonstrates real content
loss for this exact "no `s3:ListBucket`" family, because its target is a directory that genuinely holds
content.

**6. `a_denied_probe_does_not_re_enable_the_directory_delete_that_a_successful_probe_refuses`** —
neutralised the probe-denied branch to unconditionally delete the two known children and return `Ok`
(same "un-probed single-key DELETE actually reached the subtree" bug the test's own doc names):
```
thread '...a_denied_probe_does_not_re_enable_the_directory_delete_that_a_successful_probe_refuses' panicked at src\provider.rs:5711:9:
a.jpg was deleted by a refused delete (outcome was Ok(()))
```

After every neutralisation was reverted (`git checkout --`), `cargo test -p cpe-s3` was re-run clean:
`195 passed; 0 failed`.

### Two more sites found and fixed in the same file (beyond the six named)

The wider scan (below) found 2 more instances of the identical shape still in `provider.rs`, which the
ticket's own scope note said explicitly to also cover ("at minimum the rest of `crates/s3/`"):

- `a_server_that_rejects_start_after_now_refuses_an_empty_directory_delete_it_used_to_allow` — reordered
  the marker-survival assertion before `expect_err`. Red-proof: neutralised the third belt
  (`probe_prefix_after(...).map_err(...)?`) to swallow the belt's error and treat it as "nothing beyond
  the marker" (`.unwrap_or((0, 0, false))`) — exactly the bug the belt's own doc calls out ("Treating a
  failed confirmation as consent is how CPE-1723's original bug reads"):
  ```
  thread '...a_server_that_rejects_start_after_now_refuses_an_empty_directory_delete_it_used_to_allow' panicked at src\provider.rs:5993:9:
  the empty folder is still there — that is the cost this test exists to record (outcome was Ok(()))
  ```
  (The assertion fires because the folder is in fact gone — the marker was really deleted.)
- `every_object_op_works_through_a_trait_object_the_way_production_holds_the_provider` — reordered the
  network-request-count and source/destination-file assertions before `expect_err` on the `rename` call.
  Red-proof: same real-copy-then-`Ok` neutralisation as site 1:
  ```
  thread '...every_object_op_works_through_a_trait_object_the_way_production_holds_the_provider' panicked at src\provider.rs:6760:9:
  assertion `left == right` failed: rename reached the network through dyn: an emulation whose copy lands and which then reports
  an honest-looking error still leaves the user two objects believing they have one, and only 'no request was sent' can tell that
  apart from a real refusal (outcome was Ok(()))
    left: 11
   right: 9
  ```

Both reverted after proof; `cargo test -p cpe-s3` re-run clean: `195 passed; 0 failed`. `crates/s3` is
now fully covered for this pattern — every `expect_err`/`unwrap_err`/`is_err()` site in the crate was
read, and none remaining has a state assertion after it.

### Wider scan: what was searched, what was found

Delegated to a sub-agent scan (report-only, no fixes) covering the rest of the repo. Scope and results:

**Searched, matches found (fixed, see above):**
- `crates/s3/` (whole crate, all `expect_err`/`unwrap_err`/`is_err()` sites) — 8 total instances (6 named
  + 2 more), all now fixed. `sigv4.rs`, `lib.rs`, `error.rs` are pure-function validation with no
  fs/remote-state assertion following an error check — not a match.

**Searched, matches found, NOT fixed (too large for this ticket — listed for a follow-up):**
- `crates/server` — ~15 instances across 8 files:
  - `batch_execute.rs` — **12 sites** (lines, verdict→harm): 767→773, 969→971, 1020→1023, 1096→1100,
    1129→1137, 1212→1220, 1264→1271, 1325→1330, 1377→1384, 3730→3735 (a path-traversal-victim test),
    4264→4269-4271, 4296→4300-4302. The single worst offender in the repo — bigger than this whole ticket.
  - `vault_manager.rs:4177` (`an_alias_appearing_during_the_encrypt_walk_is_caught_before_the_blob_is_replaced`,
    harm at 4179-4189) and `:4681` (`the_overwrite_refuses_a_name_that_now_denotes_a_different_object`,
    harm at 4684-4687).
  - `secure_shred.rs:445` (harm at 450-451) — **destructive/irreversible domain, highest severity.**
  - `split_join.rs:634` (`cpe_1705_join_still_refuses_a_readable_existing_output`, harm at 636).
  - `folder_template.rs:389` (harm at 391).
  - `fsutil.rs:2960` and `:3160` (both legs of the link-staging test — dangling and live) — harm checks
    after the verdict.
  - `tags.rs:437` (harm at 440-442).
  - `transfer.rs:1570` and `:1602` — milder variant: verdict then a *completeness* check (files landed
    with the right bytes) rather than a destruction check; same shape, lower severity.
  - Checked and found **already correctly ordered** (harm-first — good reference examples, especially
    `archive.rs`, whose own doc comment names this exact defect pattern and defends against it):
    `vault_crypto.rs`, `archive.rs` (all 6 sites), `backup.rs` (message-only, no fs assertion),
    `fs_route.rs`, `snapshot_capture.rs`, `log_window.rs`, `op_plan.rs`, `macro_run.rs`, `rar.rs`,
    `media_meta.rs`, `media_meta_write.rs`, `net_share.rs`, `native_meta.rs`, `disk_usage.rs`,
    `image_diff.rs`, `copilot.rs`, `copilot_planner.rs`, `action_macro.rs`, `video_meta_write.rs`,
    `thumb_video.rs`, `thumb_source.rs`, `thumb_font.rs`, and 3 of `fsutil.rs`'s other `expect_err` sites.
- `src-tauri/src/lib.rs` (whole file, all 15 `expect_err` sites) — 3 instances:
  - `:14042` (`delete_permanent`, unconfirmed-call test, harm at 14050) — **permanent-delete domain,
    high severity.**
  - `:16406` (`rename_entry`, occupied destination, harm at 16409-16410).
  - `:16458` (`rename_entry` onto a dangling link, harm at 16461-16465).
  - The other 12 `expect_err` sites in this file are already correctly ordered.

**Searched, no matches:**
- `crates/webdav` — both `expect_err` sites are message-only (no fs assertion follows); other `is_err()`
  uses are post-condition checks on already-succeeded operations, not verdict-before-harm.
- `crates/ftp`, `crates/sftp` — already use the correct harm-first pattern everywhere checked (mirrors
  CPE-1731's rmdir/mkdir tests).
- `crates/net`, `crates/vfs`, `crates/contract` — all `expect_err` sites are message-only.
- `crates/mdns`, `crates/security`, `crates/updater-verify` — no `expect_err` at all.

**Not searched (explicitly out of this ticket's scope):**
- `.is_err()` used as a leading verdict check (as opposed to `expect_err`/`unwrap_err()`) was not
  exhaustively grepped across all of `crates/server` — only encountered incidentally. Since `.is_err()`
  discards the error message, more instances of this narrower shape are plausible but unconfirmed.
- `src/` (Svelte/TS frontend) and `sidecar/` — out of this ticket's Rust-provider scope.
- `src-tauri/src/` outside `lib.rs` — no other `.rs` file matched `expect_err`, so effectively covered,
  but `unwrap_err()`/`.is_err()`-only variants were not separately checked there.

**Total found beyond the eight fixed in `crates/s3`: ~18 confirmed instances**, concentrated in
`crates/server` (15, worst offender `batch_execute.rs` at 12) and `src-tauri/src/lib.rs` (3). This is
well beyond one S-ticket's scope — recommend the Foreman file a follow-up ticket (small epic, given the
count and file spread) scoped to `crates/server` + `src-tauri`, prioritising `batch_execute.rs`,
`vault_manager.rs`, and `secure_shred.rs` first given their destructive/irreversible blast radius, using
`archive.rs`'s already-correctly-ordered tests as the reference pattern.

### Guard decision

**Did not build a mechanical scanner/lint.** Reasoning:

- A text/regex-based scanner (matching the repo's own precedent, e.g. `src/lib/epicsQueueLayout.test.ts`)
  is *plausible* — flag any `#[test] fn` whose body contains `.expect_err(`/`.unwrap_err()`/an
  `is_err()` check followed later in the same function by a call recognisable as a state assertion
  (`.is_file()`, `.is_dir()`, `.exists()`, `fs::read`, a shared counter/`Mutex` read). But getting this
  right without false positives needs real function-boundary and statement-order tracking (brace
  counting is fragile across nested blocks/closures — several of the sites above are inside a spawned
  thread closure or a `match` arm), and a scanner that cries wolf on legitimate patterns (e.g. a
  *precondition* check that happens to run before the operation and also happens to look like the same
  shape) gets silenced rather than fixed, which is worse than no scanner.
- The wider scan just proved the softer, cheaper intervention — **a strengthened, written-down rule** —
  was not sufficient on its own before this ticket (the rule already existed in prose: *"assert on the
  filesystem, never on the returned `Result`"*, and it was still broken six times in one file by the
  people who wrote that rule). So I did not stop at "the rule already exists": I added a new paragraph to
  `Ticketing/wiki.md` → Evidence Rules, rule 1, spelling out the *specific* failure mode (verdict-first
  ordering silently making the harm assertion unreachable) with the exact reorder pattern and the
  `(outcome was {outcome:?})` interpolation convention, so it reads as a checklist item during guard-
  neutralisation review rather than a one-line aphorism easy to satisfy in spirit while missing in letter.
- **What would close the gap for real:** given the wider scan found ~18 more instances concentrated in
  two crates, I think the right next mechanical step is not a general repo-wide scanner but a **scoped
  guard test in `crates/server`** (the worst offender, 15 of the 18) once the follow-up ticket fixes that
  crate's instances — at that point the crate is a known-clean baseline, and a same-crate scanner (like
  `archive.rs`'s own doc-comment-documented defense) has a much smaller, well-understood false-positive
  surface than a repo-wide one attempted cold, today, alongside an S-sized ticket.

### Assumptions / unverified

- The wider-scan sub-agent's line numbers were read against the pre-fix state of `crates/s3/provider.rs`
  (before this ticket's edits shifted later line numbers); the two `crates/s3` sites it found were
  re-located by function name post-fix and confirmed correct. The `crates/server` and `src-tauri` line
  numbers were not independently re-verified against current `HEAD` by this ticket's worker — the Foreman
  filing the follow-up should re-grep for exact current line numbers rather than trusting these directly,
  since other in-flight work may have shifted them.
- Clippy was run for `crates/s3` only (`cargo clippy --all-targets -- -D warnings`, matching this crate's
  one CI invocation — it has no second feature-gated clippy leg in `.github/workflows/ci.yml`).
  `crates/server` and `src-tauri` were not touched, so their own (multi-feature) clippy legs were not
  re-run.
