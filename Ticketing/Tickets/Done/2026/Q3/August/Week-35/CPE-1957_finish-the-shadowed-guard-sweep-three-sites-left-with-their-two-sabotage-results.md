---
id: CPE-1957
title: finish the shadowed-guard sweep — three sites CPE-1929 measured and deliberately left, with the check that shadows each
type: task
priority: Medium
status: Done
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

- [x] Run the two-sabotage check against each of the three, and **record the actual numbers** (tests
      passed/failed for each sabotage) in the Work Log and at the site.
- [x] For each confirmed shadowed guard, decide **reorder vs delete** and say why. Reorder when the later
      guard asks the more trustworthy question (a handle cannot be substituted after the open; a path
      can). Delete when genuinely redundant.
- [x] Where a guard is kept deliberately as an unreachable backstop, say so **at the site** and say that
      it is untestable and why.
- [x] Site 1 additionally: decide whether the bare `is_reparse_point` should be narrowed to
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
in `fsutil::overwrite_confirmed_no_follow`'s `links > 1` guard — same function, different defect), and
**CPE-1959** (the `fsutil`-writes / `batch_media`-refuses doctrine split recorded at
`batch_media::open_output_verified`). Site 1's narrowing question is CPE-1959's question asked at a
third site, so whichever is worked second should read the other's answer first.

## Work Log

**2026-08-28 — worked and closed.** All numbers below are `cargo test --lib` in `crates/server`, run by
hand on **Windows 11** (`win32`, `x86_64-pc-windows-msvc`). Every run recompiled and took tens of
seconds; none was a cached "Finished in 0.5s".

**Re-measured baseline: 2,460 passed / 0 failed / 14 ignored** (86.27 s). The ticket quoted 2,425/0/11
from CPE-1929's merge; several PRs have landed since (#1089, #1098 among them).

**The bug is plantable, not just a cloud-sync accident — review finding, and it changes what this was.**
The Reviewer went past this ticket's GUID-tag fixture to the real tags: a hand-built `REPARSE_DATA_BUFFER`
carrying OneDrive Files-On-Demand `0x9000001A` and Windows-Container-Isolation `0x80000018` is accepted by
`FSCTL_SET_REPARSE_POINT` **from an unprivileged process**. So before this fix, any local code running as
the user could mark a file inside a vault session directory and have the lock report success over
untouched plaintext. The pre-fix defect was therefore a plantable retention hole, not only a defect a
OneDrive user could stumble into. Recorded at the test site and in the PR body.

**Second review finding — the narrowing also fixes a whole unwiped subtree.** A non-surrogate reparse
*directory* previously `continue`d with everything beneath it left unwiped; it is now descended, and the
inner file is confirmed zeroed rather than redirected. The same improvement reaches `create_vault`'s
shred-original, where a user-picked folder that was itself a placeholder used to be refused outright.

**Revision provenance, because a number is a fact about a revision (CPE-1933).** Every sabotage below
was measured at base `eca04c22`. The branch was later rebased onto `2c7f69ff` (#1099 and #1100 having
merged in between) and the suite re-measured: **2,461 / 0 / 14** — the same 2,460 baseline plus this
ticket's one new test. The baseline is unmoved, so the numbers below stand as measured; each site names
the revision so that if a later change *does* move the count, the next reader re-runs the sabotages
rather than quietly adjusting the figures.

### The two-sabotage results

| Site | disable (`if false && …`) | predicate lies (`if true \|\| …`) | verdict |
|---|---|---|---|
| 1 — `overwrite_pinned_file`'s reparse/dir refusal | **2,460 / 0** (identical to baseline) | **2,434 / 26** | shadowed |
| 2 — `same_object_or_refuse`'s `now.is_link` | **2,458 / 2** | **2,433 / 27** | **NOT shadowed** |
| 3 — `revert_engine`'s `Create` occupancy check | **2,457 / 3** | **2,440 / 20** | **NOT shadowed** |

**Site 1 — shadowed, confirmed, and the second sabotage is the wrong instrument here.** Forcing the
predicate true reds only because the line sits on the hot path of every ordinary file; that says
nothing about whether the *refusal* can fire. The measurement that does answer it: disabling **both**
by-path checks in `shred_dir_pinned` gives **2,459 / 1**, and the single failure
(`a_link_planted_inside_the_session_tree_…`) is refused by **site 2** on the directory route, not by
site 1. So on Windows a surrogate at a *file* name cannot reach site 1 at all —
`entry.file_type().is_symlink()` catches the symlink spelling, and a junction is a directory.
**Disposition: kept as a deliberate backstop against a probe-to-open swap, with the numbers and the
"untestable, and here is why" note written at the site.** Not deleted: it asks the more trustworthy
question (a handle cannot be substituted after the open), which is the reorder/delete rule's own test.

**Site 2 — the ticket's expectation was wrong, and the measurement says so.** It was filed as a probable
duplicate deserving an unreachable-backstop note. Both sabotage legs are red: two tests
(`a_link_is_refused_even_when_there_is_no_identity_to_compare_it_against`,
`shred_tree_refuses_a_root_that_is_itself_a_link`) pin it. The reason is already in its own doc
comment — the **root** call reaches it before any enumeration, so there is no earlier by-path check in
front of it to shadow it. **Disposition: behaviour unchanged; the site now records that it was measured
and is covered, so nobody re-files it as a duplicate.** Writing the requested "untestable backstop"
note would have been a false claim of exactly the kind this pattern exists to stop.

**Site 3 — not shadowed, and a reorder would be a downgrade.** With it disabled the three failures are
not "some other guard refused with different wording" — they are `HARM:` assertions showing the revert
**destroyed the user's file** (`RestoreReport { applied: 1, skipped: [] }`, attacker bytes on disk).
Nothing downstream covers it. The ticket's narrow claim is true (for a `Create` onto a link the
occupancy check refuses first, so `claim_destination_handle`'s link refusal cannot decide), but the
right response is a note: reordering would trade a permanent, correctly-worded occupancy refusal for a
link-specific message covering a strict subset, and leave every non-link occupant to fall through to a
guard that never asks about occupancy. **Disposition: note at the site, no behaviour change.**

### Site 1's narrowing decision — narrowed, and the bug was one check earlier than the ticket said

The ticket predicted that a placeholder "makes the wipe **refuse** rather than overwrite". **It does
not — it makes the wipe silently skip it.** `probe_no_follow`'s `is_link` was the bare
`FILE_ATTRIBUTE_REPARSE_POINT` bit, and `shred_dir_pinned`'s sole reader of it `continue`s. So a
WOF-compressed, dedup'd or OneDrive-placeholder file in a session dir was dropped from the file list,
never overwritten, and then unlinked by `remove_dir_all` — **plaintext extents left on the volume while
the lock reported success.** That is worse than the predicted refusal because it is silent, and it is
the vault's one job.

**Narrowing only one of the two makes matters worse, and that is measured rather than argued:**

- Un-narrowing `EntryProbe::is_link` alone gives **2,460 / 1**, failing on the new test's `HARM:`
  assertion with the secret still readable. That is the live bug, reproduced.
- Narrowing the path check but leaving the handle check on the bare bit also gives **2,460 / 1**,
  failing on the new test's `expect` with `refusing to wipe …` — i.e. exactly the mid-wipe refusal the
  ticket predicted, now reachable for the first time.

Both were narrowed to `reparse_name_surrogate(..).unwrap_or(true)`, calling the crate's single owner of
the tag rule rather than re-spelling the bit test. Unix is unchanged (`file_type().is_symlink()` is
already the narrow question), so both platforms now answer alike.

**New test:** `cpe_1957_a_non_surrogate_reparse_point_in_the_session_tree_is_overwritten_not_skipped`,
using the two-halves `make_guid_reparse_point` fixture (`0x0000_1957` / `0x2000_1957`, one bit apart,
no privilege required) as the acceptance criterion asked. It calls `shred_dir_pinned` directly rather
than `wipe_session_dir`, because the public entry point removes the tree and "the call returned `Ok`"
is satisfied just as well by the skip as by the fix — so it asserts on the **bytes**. Both halves
red-proofed; the two runs above are that proof. Confirmed with `--nocapture` that the fixture really
staged rather than quietly emitting a skip notice.

**Final: 2,461 passed / 0 failed / 14 ignored** (baseline plus the new test).
`cargo clippy --locked --all-targets -D warnings` is clean in **both** feature modes (plain and
`--features index`). No `specta::Type` struct was touched, so no bindings regeneration.

### For CPE-1959

Site 1's narrowing is CPE-1959's question answered at a third site, in `fsutil`'s favour. It is recorded
at the `batch_media::open_output_verified` site so that ticket reads it rather than diverging. The
reasoning deliberately does **not** generalise: over-refusing at a *wipe* is not a skipped item, it is
retained plaintext, so the vault's asymmetry runs opposite to the batch's. That leaves `batch_media`'s
choice resting on its own batch-specific argument rather than on "the crate refuses these", which is no
longer true. No narrowing-versus-refusing count is asserted at that site — CPE-1959 owns that
enumeration, and a number written there would be a second unguarded copy of it.

### Review round — PR #1101, APPROVE with three required fixes (all applied)

**F1 — the comments' own counts were stale on the day they shipped.** All three sites said "baseline
2,460 … came back identical", but the tree they ship in measures **2,461**, because this ticket's own new
test moved it. So every figure read one lower than the next person would measure, and the sites'
instruction *"if a later change moves the count, these are stale, re-run them"* would have fired
spuriously on day one. The +1 was explained in the commit message and here, but **not at the site**,
which is the inverse of the rule. This is CLAUDE.md's round-8 shape exactly — *stale the moment it was
written, because the commit that writes such a claim is often the commit that falsifies it* — and I had
read that paragraph while writing the very comments that repeated it. Each site now states the baseline,
its revision, and that the shipping tree reads one higher and why.

**F2 — a number must name its predicate, not only its revision.** Site 1's two sabotage figures were
measured against the pre-fix `facts.is_reparse_point || facts.is_dir` — *a predicate this same diff
replaces*. The comment then sat under the narrowed predicate annotating it with the old one's numbers.
The Reviewer re-ran both legs on the shipped predicate (**2,461 / 0** and **2,434 / 27**) and the verdict
is unchanged, so nothing was wrong; but the site now names the predicate as well as the revision, since a
later reader could not otherwise tell the numbers pre-date the line above them.

**F3 — the test drove the wrong alias policy.** It called `shred_dir_pinned` with `ShredEveryFile`, while
`wipe_session_dir:1265` — the route this whole fix is about — passes `UnlinkAliasesInsteadOfOverwriting`.
The placeholder half now runs under **both**, production policy first, one fresh single-name file each so
the alias question stays out of it. Re-red-proofed: un-narrowing `EntryProbe::is_link` fails with *"the
wipe left the user's plaintext on the volume under UnlinkAliasesInsteadOfOverwriting"*, so the production
leg demonstrably catches the bug rather than merely passing beside it. Suite still **2,461 / 0 / 14** —
the restructure added coverage, not a test, so F1's "+1" wording stays accurate.

**Confirmed by the Reviewer, recorded so it is not re-litigated:** site 1's shadowing argument is
load-bearing — with both by-path checks disabled the single failure carries `same_object_or_refuse`'s
wording (*"a symbolic link or junction is at this directory"*), not `overwrite_pinned_file`'s, so site 2
catches it on the directory route and site 1 never sees it. And site 2 has nothing in front of it
**structurally**: `shred_tree` has two callers, and `create_vault:264` reaches it with no by-path link
check at all. Refusing to write the ticket's requested "unreachable backstop" note was correct.

**F4 — found by the Reviewer, NOT fixed here; the Foreman is filing it.** `vault_manager.rs:1846`'s
`read_dir` enumerates names only and `shred_through` writes the default stream, so an **alternate data
stream** on a session file is never overwritten, and `remove_dir_all` then unlinks the file leaving the
stream's extents on the volume. Measured under the production policy: `main_all_zero=true`,
`ads_readable=true`, `ads_still_secret=true`. It is the same failure mode this ticket just fixed, one
layer down, and streams are mentioned nowhere in `vault_manager.rs` or `docs/design/VAULT-SECURITY.md`.
Pre-existing and out of scope — widening this PR to cover it would have been the wrong call.

## Closing record — merged as PR #1101 (`5a207fd5`), 2026-08-28

### The ticket predicted a refusal. What it found was silence.

The ticket expected a dehydrated cloud placeholder to make the vault wipe **refuse** mid-lock — loud, and
therefore survivable. **It does not. It silently skips the file.**

`probe_no_follow`'s `is_link` was the bare `FILE_ATTRIBUTE_REPARSE_POINT` bit, and `shred_dir_pinned`'s
only reader of that flag `continue`s. So a OneDrive Files-On-Demand, NTFS-dedup'd or WOF-compressed file
inside a vault session dir was **dropped from the wipe list, never overwritten, then unlinked by
`remove_dir_all`** — **plaintext extents left on the volume while the lock reported success.**

**The reason it survived: a skip returns `Ok` too.** Every assertion on that path was satisfied by not
touching the data. That is why the new test asserts **bytes** (`assert_ne!` on the secret, then
`all(|&b| b == 0)`), never a verdict.

### Two guards were hiding each other, and each half-fix makes things worse

Measured, not argued:

| change | result |
|---|---|
| un-narrow the handle check alone | **2,460 / 1** — `HARM:` assertion, secret readable (the live bug) |
| narrow the path check alone | **2,460 / 1** — mid-wipe refusal (the ticket's *predicted* bug, now reachable) |
| narrow both together | green |

**Only the joint change is correct, and neither half is discoverable from the other's site.** Both are now
`reparse_name_surrogate(..).unwrap_or(true)` — **calling** the crate's single owner of the tag rule rather
than re-spelling it (CPE-1933: the call *is* the derivation).

### The Security Auditor made the bug bigger than the fix's author claimed

It went past the synthetic GUID fixture and planted the **real** tags — OneDrive Files-On-Demand
`0x9000001A` and Windows-Container-Isolation `0x80000018` — with a hand-built `REPARSE_DATA_BUFFER`, on a
session file and a session directory, under the **production** alias policy:

```
P5 file tag 0x9000001a: is_link=false wipe_err=None still_secret=false all_zero=true
P5 dir  tag 0x9000001a: is_dir=true is_link=false inner_still_secret=false
P4 microsoft tags 0x9000001a / 0x80000018 / 0x8000001b: set_reparse_point=true   ← UNPRIVILEGED
```

**`FSCTL_SET_REPARSE_POINT` with those Microsoft non-surrogate tags succeeds from an unprivileged
process.** So the pre-fix defect was **not only a cloud-sync accident — it was a locally plantable way to
make the vault lock report success over untouched plaintext.**

And the narrowing does more than close the leak: a non-surrogate reparse **directory** previously
`continue`d with **its whole subtree left unwiped**, and is now descended and zeroed. The same improvement
applies to `create_vault`'s shred-original, where a user-picked folder that was itself a placeholder used
to be refused outright.

### Two of the ticket's three predicted verdicts were wrong

That is the point of running the sabotages rather than reasoning about them. Reviewer's counts on the
shipped tree (each +1 against the author's pre-PR-tree figures, since this ticket adds one test):

| site | disable | predicate lies | verdict |
|---|---|---|---|
| 1 `overwrite_pinned_file` | **2,461 / 0** | **2,434 / 27** | shadowed → **kept as a declared backstop** |
| 2 `same_object_or_refuse` | **2,459 / 2** | **2,433 / 28** | **NOT shadowed** |
| 3 `revert_engine` occupancy | **2,458 / 3** | **2,441 / 20** | **NOT shadowed** |

- **Site 1 — `if true ||` is the WRONG INSTRUMENT here, and that is the round's most transferable
  finding.** The predicate is also consulted on every ordinary file's hot path, so forcing it to lie tests
  the hot path, not the guard's reachability. The measurement that actually answers it was a **third**
  sabotage nobody prescribed: disable **both** by-path checks (**2,460 / 1**) and read *who* catches it —
  the failure carries **`same_object_or_refuse`'s** wording, not `overwrite_pinned_file`'s. Site 2 catches
  it on the directory route; site 1 never sees it. Kept, because a handle cannot be substituted after the
  open; the site says it is untestable and why.
- **Site 2 was filed as a duplicate wanting an "unreachable backstop" note. Both legs red.** Confirmed
  structurally, not just by sabotage: `shred_tree` has two callers, and `create_vault:264`
  (`ShredEveryFile`, user-picked folder) has **no by-path link check at all** before reaching it.
  **Writing the requested note would have been the exact false-coverage claim the pattern exists to
  prevent.**
- **Site 3 — with it disabled the revert destroys the user's file** (`applied: 1, skipped: []`, attacker
  bytes on disk). Note, not reorder: the occupancy check refuses a **strict superset** of what the link
  check would and names the actual problem; a reorder trades a correct permanent refusal for a narrower
  one.

### Three corrections in review, and the first is this shift's signature

- **Every sabotage figure at all three sites cited a baseline this PR's own new test had moved** (2,460 vs
  the shipping tree's 2,461). The comments would have read one low from the day they landed, and their own
  *"if the count moved, re-run these"* instruction would have **fired spuriously on day one**. The author's
  own note: they had read CLAUDE.md's round-8 paragraph — *"stale the moment it was written, because the
  commit that writes such a claim is often the commit that falsifies it"* — **while writing the comments
  that repeated it.**
- Site 1's numbers were measured against a predicate **this same diff replaces**; the provenance now names
  the predicate as well as the revision.
- The new test drove `ShredEveryFile` while `wipe_session_dir` passes `UnlinkAliasesInsteadOfOverwriting`
  — **the path the whole PR is about.** Now run under **both, production first**, so a red-proof names the
  real route, with a fresh single-name file per policy *"so the two agreeing is a result rather than a
  restatement of the setup."*

### Filed, not fixed

**CPE-1986** — `read_dir` enumerates names only and `shred_through` writes the default stream, so an
**alternate data stream** is never overwritten and its extents survive the unlink. Measured under the
production policy: `wipe_ok=true`, `main_all_zero=true`, **`ads_still_secret=true`**. The same failure mode
one layer down, and **undocumented** — no mention of streams in `vault_manager.rs` or
`docs/design/VAULT-SECURITY.md`.

### The hand-off this refused to make

Site 1's outcome was recorded for **CPE-1959** at the `batch_media` site — and the reasoning was
**deliberately not generalised**: over-refusing at a **wipe** costs retained plaintext, not a skippable
item, so **the vault's asymmetry runs opposite to the batch's.** Same tag rule, same crate, opposite cost
function. **No count was asserted there**, because CPE-1959 owns that enumeration. *(CPE-1959 then resolved
the split in PR #1103, narrowing `batch_media` too — on its own evidence, as intended.)*

### Security audit — `SEC FINDINGS`, none introduced here, none blocking

No shape is newly **followed**. Name-surrogate tags still skipped. Non-surrogate tags written through a
`FILE_FLAG_OPEN_REPARSE_POINT` handle, so the write lands in the object's own stream — measured on both a
file and a directory, no redirection. Hardlinks still unlink-only under the production policy (SEC-847). A
file held exclusively by another process **refuses the lock rather than skipping** — a refusal, retryable.
`EntryProbe::unreadable` still fails closed. The `probe_no_follow` handle-lifetime refactor is correct:
ownership moves to a `File` immediately after `CreateFileW`, `CloseHandle` is gone, every path closes
exactly once on drop. Blast radius fully enumerated — `EntryProbe::is_link` has exactly two readers, both
narrowed. Unix untouched and already asking the narrow question. No capability, no `tauri.conf.json`, no
key material.

### Gates at merge

`cargo test -p cpe-server --lib` **2,461 / 0 / 14** · clippy `--locked --all-targets -D warnings` clean in
**both** feature modes · no `specta::Type` touched · CI `completed success — total_count=26 pending=0
skipped=1 coverage=ok`.

**Family:** CPE-1929 (the shadowed-guard sweep this finishes, and the pattern whose two sabotages proved
insufficient at site 1), CPE-1896 (the dehydrated-placeholder rule), CPE-1959 (PR #1103 — the same question
at `batch_media`, resolved separately), CPE-1986 (the ADS half), CPE-1972 (an absence of information must
never license a delete), CPE-1932 (enumerate, don't recall).
