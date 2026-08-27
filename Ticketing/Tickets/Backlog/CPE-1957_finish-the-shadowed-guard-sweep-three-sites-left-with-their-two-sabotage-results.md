---
id: CPE-1957
title: finish the shadowed-guard sweep — three sites CPE-1929 measured and deliberately left, with the check that shadows each
type: task
priority: Medium
status: Open
tags: ready
estimate: S
created: 2026-08-27
---

## Why this exists

CPE-1929 swept `crates/server` for **shadowed guards** — a check that is simultaneously *safe* and
*unverifiable* because an earlier check answers on the same fact. It fixed the two highest-blast-radius
sites (`batch_media::open_output_verified`, `fsutil::overwrite_confirmed_no_follow`) and documented two
dead disjuncts. It ran out of proportion before these three. **Each is already located and reasoned
about; nobody needs to re-derive them** — but none has had the two-sabotage check actually run against
it, so each is a *lead*, exactly as `open_output_verified` was before CPE-1929 measured it.

The method, and it is not optional: **run the two sabotages, do not reason about them.** Disable the
later guard (`if false && …`) and see whether the suite stays green; separately force its predicate to
lie and see whether behaviour changes. **Both green means shadowed**, and the fix is reorder or delete —
never leave it, because a shadowed guard reads as coverage. See CLAUDE.md → "Guards and ratchets" →
"Shadowed guards".

Baseline for comparison: `cargo test --lib` in `crates/server` is **2,425 passed / 0 failed / 11
ignored** on Windows at CPE-1929's merge.

## The three sites

### 1. `vault_manager::overwrite_pinned_file` — the strongest of the three

**All line numbers below are POST-merge** — taken from CPE-1929's branch, not from the `origin/main` it
was written against, because this ticket is picked up after that merges. CPE-1929 inserts comment lines
in `vault_manager.rs` that move every one of these; citing the pre-merge numbers would have made the
whole set stale on arrival. Symbol names are given alongside so a later drift is survivable.

- Path checks first, in the caller `shred_dir_pinned`: `crates/server/src/vault_manager.rs:1819`
  (`if ft.is_symlink() { continue }`), `:1824` (`if probe.is_link { continue }`), `:1829`
  (routing on `probe.is_dir`). Call site `:1841` (`overwrite_pinned_file(path, probe, ..)`).
- Handle check second: `fn overwrite_pinned_file` at `:1915`, `handle_facts(&file)` at `:1930`
  → `:1942` `if facts.is_reparse_point || facts.is_dir`.
- **On Windows these are literally the same expression.** `probe_no_follow`'s `is_link`
  (`vault_manager.rs:1155`) and `handle_facts`'s `is_reparse_point` (`batch_media.rs:2171`) are both
  `dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0`; `probe.is_dir` (`:1154`) mirrors
  `facts.is_dir`. On Unix `is_reparse_point` is hard-coded `false`, so the later check is dead there by
  construction.
- Only reachable via a swap between the enumeration probe and the open — and that same swap is *also*
  caught by the identity comparison at `:1950-1951` (`Some(before) if before == facts.id`). Expect both
  sabotages green.
- Carries the same **bare-reparse-bit** defect CPE-1929 fixed at `fsutil::overwrite_confirmed_no_follow`:
  it refuses any reparse point rather than asking `reparse_name_surrogate`, so a dehydrated cloud
  placeholder inside a vault session dir is refused where CPE-1896 established it should be handled.

### 2. `vault_manager::same_object_or_refuse`'s link check — `vault_manager.rs:1874`

`if now.is_link` re-asks, **by path**, the question the parent already answered at `:1824` for the same
path before pushing it into `subdirs`. Reachable only on a swap in between — which is what it is for, so
this is a *defensible* second net rather than a plain duplicate. Likely outcome: keep it, and add the
"deliberately unreachable backstop, untestable and here is why" note at the site, so the next person's
green sabotage is expected rather than alarming.

### 3. `revert_engine.rs:1091` — occupancy check shadowing the write gate, for one op

`if action.op == RestoreOp::Create && fs::symlink_metadata(&target).is_ok()` (`revert_engine.rs:1091`)
runs before the `copy_file_onto_no_follow` call it gates. For a `Create` action *any* link at the name is an existing entry,
so this refuses first and the downstream link refusal in `claim_destination_handle` can never be the
decider **for Create ops**. The two guards state different properties (occupancy vs link), so this is not
a plain duplicate — but it does shadow for that one op, which is worth either a note or a reorder.

## Acceptance criteria

- [ ] Run the two-sabotage check against each of the three, and **record the actual numbers** (tests
      passed/failed for each sabotage) in the Work Log and at the site.
- [ ] For each confirmed shadowed guard, decide **reorder vs delete** and say why. Reorder when the later
      guard asks the more trustworthy question (a handle cannot be substituted after the open; a path
      can). Delete when genuinely redundant.
- [ ] Where a guard is kept deliberately as an unreachable backstop, say so **at the site** and say that
      it is untestable and why.
- [ ] Site 1 additionally: decide whether the bare `is_reparse_point` should be narrowed to
      `reparse_name_surrogate(..).unwrap_or(true)`, matching `fsutil::claim_destination_handle` (CPE-1896)
      and `fsutil::overwrite_confirmed_no_follow` (CPE-1929). If it is narrowed, it needs the two-halves
      GUID-reparse-point fixture those two use (`make_guid_reparse_point`, no privilege required), not a
      symlink — a symlink is refused by the path check for free and proves nothing.

## Why Medium and not Low

Site 1 is not merely an unverifiable guard — it carries **the same bare-reparse-bit defect CPE-1929
fixed in `fsutil`**. `if facts.is_reparse_point || facts.is_dir` refuses *any* reparse point, so a
dehydrated cloud placeholder (OneDrive Files-On-Demand, dedup, WOF) sitting inside a vault session dir
makes the wipe refuse rather than overwrite. That is a live, user-visible behaviour bug of exactly the
class CPE-1896 removed from the backup path — where refusing dehydrated files turned every one of them
into a failed backup entry — and it is worse here, because the refusal lands mid-wipe on a vault the
user is trying to lock. The shadowing is what makes it invisible; the narrowing is what makes it
matter. Raised from Low on CPE-1929's review.

## Notes

Filed 2026-08-27 by CPE-1929's worker under that ticket's own scope-control instruction: do the
highest-blast-radius ones and file the rest with file:line, the shadowing check, and the two-sabotage
result. The two-sabotage results are the one thing **not** carried over — none was run against these
three, and saying otherwise would be exactly the "reads as coverage" failure the pattern is about.

Related: **CPE-1929** (the sweep), **CPE-1896** (where the pattern was found), **CPE-1937** (a guard with
zero CI coverage that nobody noticed), **CPE-1958** (the TOCTOU race CPE-1929's security audit measured
in `fsutil::overwrite_confirmed_no_follow`'s `links > 1` guard — same function, different defect; and
the place the `fsutil`-writes / `batch_media`-refuses doctrine split recorded at
`batch_media::open_output_verified` should be settled).
