---
id: CPE-1715
title: unique_target and resolve_conflict treat a dangling link as a free name, so a move renames onto it
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-13
closed:
---

Related: **CPE-1710** — the sibling half of this hazard, already fixed. It closed the sites that
**refuse** at an occupied slot; this one covers the sites that **pick a different name** instead, and the
fix shape differs for that reason: refusing is wrong here, because the caller's whole job is to keep going
with another name, so a link slot has to read as *occupied* rather than as a *refusal*.

## Problem

Found by CPE-1710's enumeration of every `fs::rename`-destructive site, 2026-08-13.

CPE-1710 closed the **refusal**-shaped sites: a slot that is about to be renamed onto now goes through
`fsutil::rename_slot_refusal`, which pairs the occupancy check with the dangling-link check. Two sites in
`src-tauri` are a **different shape** and were deliberately left out of that fix:

- `unique_target` (`src-tauri/src/lib.rs:520`), reached by `do_move_into` — the bulk move and the watch
  executor;
- `resolve_conflict` (`src-tauri/src/lib.rs:2807`) — the transfer engine's Skip/Keepboth/Overwrite policy.

Both *probe* a candidate name and, on "free", **advance past it** rather than refusing. Both probe with
`try_exists()`, which **follows links**. A dangling link at `dest/report.txt` therefore resolves to
nothing, reads as a free name, and `do_move_into`'s `fs::rename` — which does not follow the final
component — replaces the link. The user's link is gone and the operation reports success.

CPE-1696 hardened both against *stat failures* (an unknown now counts as occupied). Neither was hardened
against a link, because a dangling link is not a stat failure — `try_exists` answers `Ok(false)`
correctly, to the question it was asked.

## Why it is a separate ticket

The fix is not `rename_slot_refusal`. Refusing is the wrong verdict at a name-picking loop: the right
behaviour is to treat a **link slot as occupied** and pick the next candidate (`report - Copy.txt`), which
is what the user asked for. That is a change to `classify_copy_target`'s inputs, not a guard inserted in
front of a rename, and it wants its own tests over the `- Copy (n)` sequence.

## Acceptance criteria

- [x] `unique_target` treats a slot occupied by a link — including a dangling one — as **occupied**, and
      picks the next candidate name instead of returning it as free.
- [x] `resolve_conflict` does the same, so `Skip` skips, `Keepboth` renames, and `Overwrite` is the only
      arm that touches it.
- [x] A test proves a dangling link at the destination **survives** a bulk move, asserted on the slot
      (`symlink_metadata(..).is_symlink()`), not on the returned `Result`.
- [x] Platform-gated the way CPE-1710 did it: `fsutil::make_dangling_link` (symlink, junction fallback on
      Windows) and a loud `writeln!(stderr)` skip if neither can be created. It is `#[cfg(test)]
      pub(crate)` in `cpe-server`, so `src-tauri` needs its own copy or the helper needs promoting.
- [x] Breaking the change turns a distinct test red, with real output pasted in the PR (Evidence Rules).

## Notes

Filed by the CPE-1710 worker. Related: **CPE-1710** (the refusal-shaped sites and the
`rename_slot_refusal` pairing), **CPE-1705**, **CPE-1696** (which hardened these two functions against
stat failures), **CPE-1461** family (symlink-following).

## Work Log (2026-08-17)

**What changed** — `src-tauri/src/lib.rs`:

- Added `probe_name_pick_slot(candidate: &Path) -> io::Result<bool>` (~line 505): the drop-in replacement
  for `candidate.try_exists()` at both collision-picking sites. When `try_exists()` says "nothing resolves
  here" (`Ok(false)`), it additionally takes a `std::fs::symlink_metadata` reading of the *same* candidate
  — which does not follow the final path component, so it sees a dangling link that `try_exists` stepped
  straight through — and folds that into the returned `Result<bool>` exactly as `try_exists` itself would
  have if the slot were genuinely occupied. This is deliberately **a change to `classify_copy_target`'s /
  `copy_target_is_free`'s input**, per the ticket's own framing — neither of those two functions, nor their
  pre-existing CPE-1696 stat-collapse unit tests, needed to change at all.
- Added `classify_link_presence(is_link: io::Result<bool>) -> io::Result<bool>` (~line 546): the pure half
  of the above (mirrors `cpe_server::fsutil::classify_symlink_slot`'s shape — takes the `symlink_metadata`
  outcome pre-reduced to "is it a link" — for the same reason: the dangling-link arm is otherwise only
  reachable with a real filesystem symlink). `Ok(true)` (a link, dangling or live) → `Ok(true)` (occupied).
  `Ok(false)` / an explicit `NotFound` → `Ok(false)` (free, agreeing with the `try_exists` probe that
  produced it). Any other stat failure is threaded through as `Err`, which `classify_copy_target` folds
  into `TargetSlot::Unknown` — the same "cannot prove free, so not free" verdict CPE-1696 already gives an
  unreadable slot.
- `unique_target`'s loop (line ~602): `classify_copy_target(candidate.try_exists())` →
  `classify_copy_target(probe_name_pick_slot(&candidate))`.
- `resolve_conflict` (line ~3045): `copy_target_is_free(base_target.try_exists())` →
  `copy_target_is_free(probe_name_pick_slot(base_target))`.
- Updated three stale in-code comments at `do_move_into` (line ~2650) and the transfer engine's move arm
  (line ~3298) that had described this as a still-open residual hazard tracked under CPE-1715; they now
  describe the fix and where it lives.

**Class boundary drawn.** The ticket names exactly two sites (`unique_target`, `resolve_conflict`), both in
`src-tauri`. While auditing for siblings I found the *same* `classify_target_slot`/`TargetSlot::Free`
name-picking shape in `crates/server/src/batch_media.rs` (`Slot::Free` around line 2108) and
`crates/server/src/snapshot_capture.rs` (lines 165, 553–564) — but both of those pick a name for a file
**about to be created** (`File::create`/`fs::write`), not renamed onto. A dangling link at a create-shaped
site is a *different* hazard (the create follows the link and writes through it; CPE-1716/1718 already gave
that shape its own family — `cpe_server::fsutil`'s create-slot guard) with a different fix shape, not "pick
the next candidate instead of writing through the link" — teaching those two loops to treat a link as
occupied would silently mask exactly the case CPE-1716/1718 exist to catch (writing through a *live* link
is sometimes the correct behaviour there; it never is for a rename). Left those two untouched, deliberately,
as out of this ticket's scope — filing a follow-up is left to the Foreman/PM if the class is judged worth
closing.

`make_dangling_link` was already `pub` in `cpe_server::fsutil` (promoted during CPE-1710, per its own doc
comment: "the app adapter's tests need it too"), not `#[cfg(test)] pub(crate)` as the ticket's filing
assumed — so no promotion or `src-tauri`-local copy was needed; the new tests call
`cpe_server::fsutil::make_dangling_link` directly, same as the existing CPE-1710/1716 tests in this file.

**Tests added** (`src-tauri/src/lib.rs`, right after the CPE-1710 dangling-link block, ~line 15824):

1. `cpe_1715_classify_link_presence_treats_any_link_as_occupied_and_only_notfound_as_free` — pure, no disk.
2. `cpe_1715_probe_name_pick_slot_reports_a_dangling_link_as_occupied` — disk-backed, real dangling link.
3. `cpe_1715_unique_target_skips_a_dangling_link_and_picks_the_next_candidate`
4. `cpe_1715_resolve_conflict_skip_skips_a_dangling_link_instead_of_treating_it_as_free`
5. `cpe_1715_resolve_conflict_keepboth_renames_past_a_dangling_link_instead_of_returning_it_as_free`
6. `cpe_1715_resolve_conflict_overwrite_is_the_only_arm_that_touches_a_dangling_link`
7. `cpe_1715_do_move_into_never_renames_onto_a_dangling_link_the_link_survives_a_bulk_move` — the AC's
   required "survives a bulk move" test, through the real `fs::rename` in `do_move_into` (the function the
   ticket itself names as reached by "the bulk move command and the watch executor").

All seven ran for real on this machine — no `[CPE-1715] SKIPPED` lines, meaning `make_dangling_link`
succeeded (this box has symlink privilege or the junction fallback worked), so the dangling-link legs
executed rather than degrading to "nothing covered". Every disk-backed test asserts on the **filesystem
slot** (`symlink_metadata(..).is_symlink()`) or the **picked name** before ever looking at the returned
`Result`, per the CPE-1710 lesson that a "successful" `Result` is exactly what this bug class produces.

**Red-proof.** Committed the fix + tests first (`git commit`, so `git checkout --` would restore
correctly), then neutralised `probe_name_pick_slot` back to a bare `candidate.try_exists()` (the pre-fix
shape) and reran `cargo test --lib cpe_1715`. 6 of the 7 new tests went red (the 7th,
`classify_link_presence...`, is a pure test of a function the neutralisation didn't touch, so it correctly
stayed green — it is proof of the classifier's own arms, not of the wiring). Real output:

```
running 7 tests
test tests::cpe_1715_classify_link_presence_treats_any_link_as_occupied_and_only_notfound_as_free ... ok

thread 'tests::cpe_1715_probe_name_pick_slot_reports_a_dangling_link_as_occupied' panicked at src\lib.rs:15884:9:
the dangling link must read as OCCUPIED, not free

thread 'tests::cpe_1715_resolve_conflict_overwrite_is_the_only_arm_that_touches_a_dangling_link' panicked at src\lib.rs:16007:9:
and, having been explicitly authorised, the link itself must actually be gone

thread 'tests::cpe_1715_resolve_conflict_keepboth_renames_past_a_dangling_link_instead_of_returning_it_as_free' panicked at src\lib.rs:15972:9:
assertion `left == right` failed: a dangling link must be treated as occupied, so Keepboth must pick a different name instead of handing the link's own name back as free
  left: "...\\cpe_test_cpe1715_resolve_keepboth_34132\\keep-me.txt"
 right: "...\\cpe_test_cpe1715_resolve_keepboth_34132\\keep-me - Copy.txt"

thread 'tests::cpe_1715_resolve_conflict_skip_skips_a_dangling_link_instead_of_treating_it_as_free' panicked at src\lib.rs:15940:9:
a dangling link must be treated as occupied, so Skip must actually skip it (got Some("...\\cpe_test_cpe1715_resolve_skip_34132\\skip-me.txt"))

thread 'tests::cpe_1715_unique_target_skips_a_dangling_link_and_picks_the_next_candidate' panicked at src\lib.rs:15907:9:
assertion `left == right` failed: a dangling link's slot must be treated as occupied, so the picker must advance past it rather than hand it back as free
  left: "...\\cpe_test_cpe1715_unique_target_34132\\report.txt"
 right: "...\\cpe_test_cpe1715_unique_target_34132\\report - Copy.txt"

thread 'tests::cpe_1715_do_move_into_never_renames_onto_a_dangling_link_the_link_survives_a_bulk_move' panicked at src\lib.rs:16044:9:
the dangling link at the destination was DESTROYED by the bulk move (result was Ok("...\\dest\\report.txt"))

test result: FAILED. 1 passed; 6 failed; 0 ignored; 0 measured; 182 filtered out; finished in 0.01s
```

The `do_move_into` end-to-end failure message is the money shot: with the fix removed, the destination
link is destroyed and the `Result` still reports `Ok` — the exact silent-success shape this ticket exists
to close. Restored the fix with `git checkout -- src-tauri/src/lib.rs` and reran; all 7 green again.

**Verification, all green:**
- `cargo build --lib` (src-tauri) — clean.
- `cargo test --lib` (src-tauri) — 189 passed, 0 failed, 0 ignored (includes the 7 new CPE-1715 tests and
  every pre-existing CPE-1696/1705/1710 test, unaffected).
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo clippy --all-targets --features sidecar-platform -- -D warnings` — clean.
- No `specta::Type` struct touched → no `bindings.gen.ts` regen needed.
- No Rust dependency changed → no `Cargo.lock` regen needed.
- No PowerShell used to write any repo file (Edit tool + bash heredoc only); every scratch dir was created
  under `scratch()` (this file's own existing test-fixture helper, `std::env::temp_dir()`-based, matching
  every neighbouring CPE-1696/1705/1710/1716 test) and removed with `fs::remove_dir_all` on every exit path
  of every new test, including each `make_dangling_link` failure/skip branch.

**Assumptions / boundary notes:**
- `probe_name_pick_slot` takes one extra `symlink_metadata` syscall per candidate, but only on the branch
  where `try_exists` already said "free" (i.e. once per successful pick, not once per occupied collision) —
  no change to `MAX_CONSECUTIVE_UNKNOWN_SLOTS` behaviour, since an unreadable link still surfaces as
  `TargetSlot::Unknown` through the existing `Err(_)` arm.
- `resolve_conflict`'s `Overwrite` arm was not changed. It already deleted whatever occupied `base_target`
  before this ticket; extending "occupied" to include a link just means a link now reaches that
  pre-existing branch too (`fs::remove_file` correctly removes the link itself, not its target, on both
  platforms) — verified by the new
  `cpe_1715_resolve_conflict_overwrite_is_the_only_arm_that_touches_a_dangling_link` test.
- Did not re-run the full three-OS CI matrix locally (Windows-only box); relying on the pushed PR's GitHub
  Actions run for the Linux/macOS legs, watched synchronously per the sprint runbook.

## Work Log — independent review round (2026-08-17, PR #924)

An independent Opus review confirmed the core fix and the `do_move_into` e2e test's shape (harm-before-`Result`)
were correct, and found six issues. Addressed all six:

1. **BLOCKER, production bug — `resolve_conflict`'s `Overwrite` arm couldn't actually clear a dangling
   *directory* link on an unprivileged runner.** `make_dangling_link` falls back to an NTFS **junction**
   when `SeCreateSymbolicLinkPrivilege` is absent (the unprivileged Windows CI runner; my dev box has the
   privilege, so my original local run only ever exercised the real-symlink leg and never caught this).
   Measured by the reviewer on a dangling junction: `is_dir()` follows the link to `false` (nothing
   resolves), so the `else` branch runs `fs::remove_file`, which refuses a junction with
   `PermissionDenied` (os error 5) — the reparse point is a directory object and Windows will not
   `DeleteFile` one. The slot was never cleared. Fixed at `src-tauri/src/lib.rs`'s `resolve_conflict`
   Overwrite arm: on `remove_file` failure, fall back to `fs::remove_dir` (which removes the reparse point
   itself without following it). **I could not reproduce the red locally** — this dev box has symlink
   privilege, so `make_dangling_link` stages a real symlink here and the pre-fix code already passed; I
   confirmed that by reverting the fix and re-running
   `cpe_1715_resolve_conflict_overwrite_is_the_only_arm_that_touches_a_dangling_link`, which stayed green
   (documented, not fabricated — see the CI run for the real red/green on the unprivileged Windows leg,
   which is exactly the runner this bug needs).
2. **Temp-dir hygiene — no `Drop` guard.** All six disk-backed CPE-1715 tests used a trailing
   `let _ = fs::remove_dir_all(&d);`, which a panicking assertion skips. Added `struct
   Cpe1715Scratch(PathBuf)` with a `Drop` impl (mirrors the `Restore` pattern in
   `cpe_server::dispatch`/`split_join`'s tests), armed via `let _clean = Cpe1715Scratch(d.clone());`
   immediately after every `scratch(..)` call — including the `make_dangling_link` skip branches — and
   removed every trailing `remove_dir_all`. Verified: reran the red-proof (finding below) and confirmed no
   `cpe1715` directories were left in `%TEMP%` afterward.
3. **Wrong crate.** `probe_name_pick_slot`/`classify_link_presence` were the thirteenth copy of
   `symlink_slot_refusal`/`classify_symlink_slot`'s stat shape, living in the app adapter where
   `crates/server/src/batch_media.rs` and `snapshot_capture.rs` cannot reach them — directly contradicting
   CPE-1705's own "twelve copies is how the thirteenth gets missed." Moved both, verbatim in spirit, into
   `crates/server/src/fsutil.rs` as `pub fn name_pick_slot_probe` and `pub fn classify_link_presence`
   (next to `classify_symlink_slot`). `src-tauri/src/lib.rs`'s `probe_name_pick_slot` is now a one-line
   alias (`cpe_server::fsutil::name_pick_slot_probe(candidate)`), so both call sites and all six
   disk-backed tests are unchanged. The pure classifier test moved with it, to
   `crates/server/src/fsutil.rs`'s own `tests` module.
4. **Stale/false comment.** `resolve_conflict`'s Overwrite arm still claimed it was "reached only after
   `base_target.exists()` returned true above" — no longer true post-fix (it's now also reached for
   Unknown and dangling-link slots). Replaced with an explanation of why `contained_under`'s vacuous `Ok`
   on a non-resolving path is still sound there.
5. **Residual: TOCTOU / non-symlink reparse points.** The original `probe_name_pick_slot` fed
   `classify_link_presence` the narrow `is_symlink()` bit, so an entry `symlink_metadata` could see but
   `try_exists` could not resolve — a plain file created between the two stats, or a non-symlink reparse
   point such as a cloud-storage placeholder or dedup stub — would still read as `Free`. Took the
   reviewer's suggested fix rather than just documenting it: `name_pick_slot_probe` now maps *any*
   successful `symlink_metadata` stat to occupied (`.map(|_| true)`), not only a confirmed link, so only a
   genuine `NotFound` reads as free.
6. **Sibling sites — logged, not built.** Reviewer is filing **CPE-1769** and **CPE-1770** for the
   unfixed create-shaped siblings (`crates/server/src/batch_media.rs:2109`,
   `crates/server/src/batch_execute.rs:225`, `crates/server/src/snapshot_capture.rs:165`, the two
   trash-restore sites, `crates/server/src/folder_template.rs:176`) found during this review. Not built
   here — this ticket's ACs name only `unique_target`/`resolve_conflict`, both satisfied, and those sites
   are a different hazard shape (create-time, not rename-time) per the class boundary already drawn above.

Re-verified after all six: `cargo build --lib`, `cargo test --lib` (188 passed in `src-tauri` — one test
count lower than the first round because the pure classifier test moved to `cpe-server`), `cargo test -p
cpe-server` / `cargo test --lib` from `crates/server` (2199 passed, 4 ignored, 0 failed), `cargo clippy
--all-targets -- -D warnings` in `src-tauri`, `crates/server`, and `src-tauri --features sidecar-platform`
— all clean. Red-proof redone for the probe fix (all 6 disk-backed tests red again, including
`do_move_into` on the harm message with `Ok(...)` printed) and restored; the Overwrite arm's own red could
not be forced locally for the privilege reason above, documented rather than faked.
