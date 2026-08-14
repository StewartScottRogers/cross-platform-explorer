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
