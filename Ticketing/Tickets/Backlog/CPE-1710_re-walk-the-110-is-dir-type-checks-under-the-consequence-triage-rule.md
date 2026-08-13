---
id: CPE-1710
title: Re-walk the ~110 `.is_dir()` type-checks under the consequence-triage rule
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: L
created: 2026-08-13
closed:
---

## Problem

CPE-1705 established a triage rule and then, by its own admission, did not finish applying it:

> **Step 1 — enumerate by syntax. Step 2 — classify by consequence.**
>
> **A type-check whose false branch discards state is an absence claim, not a type claim.**

That rule is what caught `snapshot_capture.rs`'s `load_store` — the most destructive site in the whole
six-round chain, which an *exhaustive* sweep had correctly **enumerated** and then **mis-filed** into the
"harmless type check" family because nobody asked what its `else` branch did. It returned an empty blob
store, which is an absence claim wearing a type check's syntax, and `capture()` then wrote that empty
store back over the real `index.json`.

**The rule overturns CPE-1692's documented decision to skip the `.is_dir()` family wholesale by syntax.**
CPE-1705 sized that cost up front (~110 production hits) and deliberately did **not** fold it in, on the
grounds that bundling scope is what made rounds one through five under-cover their own conclusions. This
ticket is the deliberate split it promised.

## Scope

Every production `.is_dir()` / `.is_file()` hit not already fixed by CPE-1678/1687/1692/1696/1705. For
each one, read the **false branch** and classify:

- **Discards state / claims absence** → fix, using `fsutil::classify_target_slot` or a site-specific pure
  classifier following `snapshot_capture::classify_store_index`.
- **Reads `meta.is_dir()` off a `metadata()` that already succeeded** → genuinely excluded, there is no
  failed stat and no fail-open branch. `duplicates.rs:73`, `checksum.rs:54`, `compare.rs:52` and the rest
  of that family are the reference examples; CPE-1705's reviewer validated the rule stays silent on them.
- **Refuses either way** (e.g. `if !root.is_dir() { Err("not a folder") }` gating a batch that then
  declines to run) → a real inaccuracy but a materially smaller lie: it misreports the *type*, not the
  *existence*, and nothing destructive proceeds. Record the decision; fix only if cheap.

## Acceptance criteria

- [ ] Every production `.is_dir()`/`.is_file()` hit is classified by **consequence**, with the verdict
      recorded per site — not skipped by syntax, which is the decision this ticket exists to overturn.
- [ ] Sites whose false branch discards state are fixed and carry a pure classifier with unit tests that
      run on all three CI OSes.
- [ ] Each new guard broken **on its own** turns a **distinct** test red, real output pasted in the PR
      (`Ticketing/wiki.md` Evidence Rules).
- [ ] Where an ACL-staged test is used, it denies the target **and** `(RD)` on the parent — see
      `fsutil::deny_stat_of`, and CPE-1705's "CORRECTION 4". A target-only deny leaves `Path::exists()`
      answering `true` via `fs::metadata`'s `FindFirstFileW` fallback, and the test passes against the
      bug. That mistake has now been made four times in this chain.
- [ ] State the scope of the sweep and of any negative result.

## Notes

Filed by the CPE-1705 worker, 2026-08-13, as that ticket's PR promised — the ~110-site re-walk was sized
up front and split out deliberately rather than quietly narrowed.

Related: **CPE-1678**, **CPE-1687**, **CPE-1692**, **CPE-1696**, **CPE-1705** (the same bug class, six
rounds), **CPE-1673** (the taxonomy).
