---
id: CPE-1961
title: `claim_destination_handle` and `open_output_verified` are both **live** hard-link TOCTOU races — and `batch_media` is shielded on Windows, not safe
type: bug
priority: High
status: In Progress
tags: ready
estimate: L
created: 2026-08-27
---

## Summary

PR #1070 fixed the `links > 1` race in `fsutil::overwrite_confirmed_no_follow` (36/2,000 → 0/2,000) and
then raced the neighbours. **Two more sites are live**, both measured:

| site | Windows | Linux |
|---|---|---|
| `fsutil::claim_destination_handle` (`fsutil.rs:1734`) | **45 / 2,000** | **90 / 1,000** |
| `batch_media::open_output_verified` | 3 / 2,000 | **~30 / 1,000** |

Every "destroyed" is the outside file's bytes read back off disk, not a verdict.

## Correcting CPE-1958, which was wrong on this point

CPE-1958 asserted that `batch_media::open_output_verified` *"measured 0 / 2,000 under the identical
racer"* and asked whoever fixed `fsutil` to work out **why it was safe**. That premise came from the
Foreman and it is wrong.

**It is not safe — it is *shielded*, and only on Windows.** `classify_output_containment` runs
**before** the open and refuses a flickering destination outright, so far fewer trials reach the
identical check-then-use (681 `Ok` vs `overwrite_confirmed_no_follow`'s 1,249 in the same run). That is
a **path gauntlet, not containment** — and Linux does not have it, which is why Linux measures ~30/1,000.

A guard that survives a race because an earlier check happens to reject most of the attacker's attempts
is the same family as CPE-1929's shadowed guards: it looks like a property and is a coincidence.

## Why `claim_destination_handle` cannot take PR #1070's fix as-is

PR #1070 closed its site with **claim-then-rename staging** — bytes into a `create_new` file beside the
destination, committed with a rename, so *the only object written is one created a moment ago that has
never had another name.* That does not transfer directly here:

- its handle comes from **`create_beneath`**, so staging would need a **handle-relative rename**, which
  `std` does not provide (the same gap that kept `copilot::apply_op` deferred through CPE-1937 —
  `remove_file_beneath` landed, `renameat` did not); and
- it feeds **`backup::landed_inside`**, so changing what object ends up at the destination has a second
  consumer that reasons about identity.

## Acceptance criteria

- [ ] **Reproduce both before fixing**, with PR #1070's in-tree racer (`cpe_1958_race_report`), on
      **both** platforms. Report destroyed/trials and swap counts.
- [ ] **Keep the sensitivity control**, and heed #1070's harness lesson: **when the fix changes the
      timing, re-validate the harness before trusting the number.** Its fix made the original attacker
      land far more hard links than before, which would have let a *narrowed* window read as a *closed*
      one. It solved that with a **planted** shape — the harness plants the link on the main thread
      immediately before each trial and *counts* whether it was in place — use it. **Note the
      correction #1070 round 2 made to its own description:** the attacker thread does NOT stop
      re-linking in the planted shape. It cycles `hard_link`/`remove_file` in both shapes; there is no
      `harness_plants` branch in its body. What `harness_plants` adds is the second, counted planter.
- [ ] Decide the containment shape for `claim_destination_handle`. If it needs `renameat` /
      handle-relative rename in `open_beneath`, **that is the ticket**, and it also unblocks
      `copilot::apply_op` — say so, and check what `backup::landed_inside` needs from the identity.
- [ ] For `batch_media`, decide whether to give it real containment or to make the Windows shield
      explicit and add its equivalent on Linux. **Do not leave "it measured low on Windows" as the
      answer.**
- [ ] **Assert on the filesystem** — the outside victim byte-identical — never on a verdict enum.
- [ ] Add a **deterministic** CI guard alongside the `#[ignore]`d race, as #1070 did with
      `ProbeInjection::HandleUnderReportsLinks`. The race is the evidence; the deterministic test is
      what CI actually runs.
- [ ] Re-run #1070's enumeration and confirm nothing else moved: `real_target_containment` (plan-time
      path probe), `name_links` (advisory), `revert_engine:1146` (asks after the write settles — safe),
      `vault_manager::overwrite_pinned_file` (CPE-1957 site 1).

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1070's enumeration, which found both while fixing
CPE-1958 and left them deliberately with stated reasons.

Family: **CPE-1958** (the fixed site, PR #1070 — and the ticket this corrects), **CPE-1896** /
**CPE-1913** / **CPE-1937** (the containment work), **CPE-1929** (shadowed guards — the shape
`batch_media`'s Windows shield belongs to), **CPE-1957** (`vault_manager`, same guard family),
**CPE-1959** (the reparse-point doctrine split).


## Independent corroboration of the `copy_file_onto_no_follow` figures (2026-08-27, PR #1070 round 2)

Arm **E** (`fsutil::copy_file_onto_no_follow` → `claim_destination_handle`) has now been measured by
**three** separate runs on the *fixed* CPE-1958 branch, by three people who were not sharing a harness
run. Recorded here because a single racer's number is the weakest kind of evidence this family accepts,
and these agree:

| source | Windows (planted shape) | Linux (planted shape) |
|---|---|---|
| PR #1070 round-1 worker | 167 / 2,000 | 54 / 1,000 |
| PR #1070 round-2 **Reviewer**, independent run | **51 / 1,000** | **55 / 1,000** |
| PR #1070 round-2 worker, re-taken on the MERGED state | **149 / 2,000** (710 planted) | **107 / 10,000** (9,658 planted) |
| PR #1070 round-3 **Security re-audit**, independent run, ext4 | — (Linux only) | **2,706 / 10,000** and **2,122 / 10,000** (the two shapes) |

So `claim_destination_handle` is live on both platforms in every run anyone has taken. The *rate* moves a
lot with how the harness's timing falls — so **do not read the spread as instability in the defect**;
read it as the reason to re-take rather than quote.

### Do NOT quote this ticket at the low Linux figures

The round-2 worker's Linux numbers (**E 799 / 107**, **D 630 / 97** per 10,000) are the *lowest* anyone
has measured, and the round-3 Security re-audit — 10,000 trials × 2 shapes × 5 arms, its own harness, on
a real ext4 root — came back **much** higher on the same two arms:

| arm | round-2 worker (per 10,000) | round-3 Security re-audit (per 10,000) |
|---|---|---|
| D `batch_media::open_output_verified` | 630 / 97 | **1,921 / 2,574** |
| E `fsutil::copy_file_onto_no_follow` | 799 / 107 | **2,706 / 2,122** |

That is a **3× to 26× spread run to run on the same code**, which is what a timing-sensitive racer does
and is exactly why this section exists. Two consequences for whoever picks this up:

1. **Quote a range, or re-take.** Roughly **1 in 100 to 1 in 4** confirmed operations destroyed,
   depending on load. Citing "97 / 10,000" as *the* rate understates the defect by more than an order of
   magnitude; citing 2,574 as *the* rate overstates the floor. Both are real measurements of the same
   bug.
2. **It strengthens CPE-1958's arm C rather than weakening it.** Arm C measured **0 / 10,000 in both
   shapes** in the very run where D and E were at their *highest* — 4,877 planted against arm B's 2,885,
   i.e. facing the harder attacker for that zero. A zero taken against hot controls is worth more than a
   zero taken against sleepy ones.

Arm D (`batch_media::open_output_verified`) is live in every run too, so **both arms in this ticket's
title are confirmed by two independent harnesses**; on the round-2 numbers E led D, on the round-3
numbers D's second shape led E's — so **do not order the fixes by rate**, fix both.

## Related: the OTHER half of the rename, now filed as CPE-1963

While re-measuring, #1070 round 2 found and filed **CPE-1963**: `stage_and_replace_at`'s commit names
its *source* by path (`*.cpe-tmp`, enumerable, in an attacker-writable folder), so the commit itself can
be aliased onto a file outside the root — 2,834/3,000 on Linux ext4, 6/3,000 on Windows, with the
victim's content never changed. **It needs the same missing primitive this ticket names**: one
handle-relative `renameat` in `open_beneath` would unblock CPE-1963, this ticket's
`claim_destination_handle` arm, and `copilot::apply_op`. Whoever picks up either should read the other
first and consider doing the primitive once.

## Work Log

### 2026-08-28 — both arms reproduced, then closed by containment; `renameat` added to `open_beneath`

**Reproduced first, on this branch's base (the merged CPE-1958 state), before touching anything**, with
PR #1070's in-tree racer `cpe_1958_race_report`. Every "destroyed" is the OUTSIDE file's bytes read back
off disk; no arm asserts on a verdict. `planted` is trials that genuinely began hard-linked.

**BEFORE — Windows 11 / NTFS, 2,000 trials per arm; Linux / ext4 (WSL, `TMPDIR` on the ext4 root),
10,000 trials per arm:**

| arm | Win, attacker owns both | Win, planted | Linux, attacker owns both | Linux, planted |
|---|---|---|---|---|
| A unguarded (control) | 577 | 833 (854 planted) | 4,460 | 9,245 (506 planted) |
| B pre-CPE-1958 body | 351 | 424 (797 planted) | 126 | 2,322 (1,608 planted) |
| C `overwrite_confirmed_no_follow` | **0** | **0** (665) | **0** | **0** (7,641) |
| D `batch_media::open_output_verified` | **24** | 1 (681) | **2,890** | **2,224** (943) |
| E `fsutil::copy_file_onto_no_follow` | **166** | **188** (773) | **2,887** | **2,563** (1,538) |

**AFTER — same machines, same trial counts:**

| arm | Win, attacker owns both | Win, planted | Linux, attacker owns both | Linux, planted |
|---|---|---|---|---|
| A unguarded (control) | 568 | 749 (856 planted) | 4,399 | 8,853 (675 planted) |
| B pre-CPE-1958 body | 318 | 517 (801 planted) | 176 | 1,300 (2,421 planted) |
| C `overwrite_confirmed_no_follow` | **0** | **0** (859) | **0** | **0** (7,345) |
| D `batch_media::open_output_verified` | **0** | **0** (1,008) | **0** | **0** (8,219) |
| E `fsutil::copy_file_onto_no_follow` | **0** | **0** (870) | **0** | **0** (8,975) |

Attacker swap counts, after, Windows: A 3,444 / 5,374, B 2,561 / 4,455, C 4,976 / 6,890, D 3,157 / 3,756,
E 5,568 / 7,867 (shape 1 / shape 2). Linux swaps run 85k–1.6M per arm and are printed by the harness.

**The spread the ticket warned about is real and my own numbers land at the HIGH end.** D and E measured
**2,890** and **2,887** per 10,000 on Linux — within the round-3 Security re-audit's band (1,921/2,574 and
2,706/2,122) and **20–30x** the round-2 worker's low figures (97 and 107). So the honest rate for both
arms is **roughly 1 in 100 to 1 in 4 confirmed operations**, load-dependent. Do not quote a single number.

**The `planted` columns run the right way on BOTH platforms this time**, which is the comparability check
CPE-1958 round 2 had to retract for Windows: the fixed D began **1,008** trials genuinely hard-linked and
the fixed E began **870**, against the pre-fix body's **801** and the control's **856**. On Linux, 8,219
and 8,975 against B's 2,421. The zeros were taken against the *harder* attacker, not an easier one, and
the controls stayed hot in every run (A 568–8,853, B 176–1,300).

### The decision: yes, `renameat` belongs in `open_beneath` — and it unblocks all three

`crates/server/src/open_beneath.rs` gains two primitives:

- **`create_staging_beneath(root, rel)`** — an exclusive create with no open-if-exists fallback,
  `DELETE | READ_CONTROL | WRITE_DAC` on Windows (the first is what the rename needs on the source
  handle, the other two are what `HandleCarryover::apply` needs) and `0600` at birth on Unix.
- **`rename_beneath(root, staged, from_rel, to_rel)`** — the handle-relative commit. **Windows: the
  source operand is the HANDLE** (`NtSetInformationFile` + `FileRenameInformation`, `RootDirectory` set
  to the destination's parent handle), so neither operand is a path. **Unix: `renameat(parent, from,
  parent, to)`** — both leaves resolved inside one held directory object, which is the strongest rename
  POSIX has; there is no fd-sourced rename in POSIX, Linux or any BSD.

**It unblocks all three, with the split stated rather than smoothed over:**

- **CPE-1961** (this ticket) — done here.
- **CPE-1963** — closed **on Windows** by the handle-sourced form (the enumerable `*.cpe-tmp` source can
  no longer be aliased, because the source is not a name). **Not closed on Unix**: the source is still a
  name, and CPE-1963's 2,834/3,000 Linux measurement stands for the `ByPath` commit. What changes for
  the `Beneath` arm is that the aliasing is now *detectable* — `ClaimedDestination::written` is the
  identity of the object this call created, so `backup::landed_inside` refuses an aliased commit instead
  of reporting success. CPE-1963 should take the primitive and decide what to do about the Unix half.
- **`copilot::apply_op`** — the missing half CPE-1913 and CPE-1937 both named (`remove_file_beneath`
  landed, `renameat` did not) now exists. Not wired here; that is `apply_op`'s own ticket.

**A measured trap for whoever touches it next, recorded at the site:** Win32's
`SetFileInformationByHandle(FileRenameInfo)` refuses a non-null `RootDirectory` with
`ERROR_INVALID_PARAMETER (0x80070057)` — the entire `transfer` and `archive` suites reddened on it, every
entry, until the call was moved down to the NT layer. The Win32 wrapper is not a superset.

### What `backup::landed_inside` needs from the identity, checked before changing what lands

It needs `written` to be **the identity of the object that ends up at `dst`** — it canonicalises `dst`,
re-opens it no-follow, and compares. Before this change `written` was the identity of an object the call
merely *found* at the destination; now it is the identity of one the call *created*. That is strictly
stronger and nothing in `landed_inside` had to change: the degenerate-identity fallback, the
`handle_facts == None` fallback and the "opened but could not be described" refusal all keep their
meanings. `written` can also no longer be `None` on any production path (the staged-handle refusal below
sees to that), which is the same invariant `claim_destination_handle`'s existing `else` arm established.

### The shape: claim-then-rename, in its staging form, at both sites

`fsutil::claim_destination_handle` keeps **every** refusal it had, in the same order and with identical
wording, and stops handing back the destination handle. The closure parameter became a
`DestinationSite` — `ByPath` (revert, snapshot, every `copy_file_onto_no_follow` caller) or `Beneath
{ root, rel }` (backup, archive, transfer) — because the staging sibling has to be created and committed
*the same way the destination is addressed*, and a closure could only describe the destination.
`ClaimedDestination` is now `#[must_use]`, carries the staged handle, and has `commit()`; dropping it
uncommitted removes the staging sibling and, when the claim created the destination name, the empty
destination too.

`batch_media::VerifiedOutput::write_all` routes through the **same** implementation
(`fsutil::stage_bytes_over_checked_handle` → `stage_and_replace_at`) rather than growing a second staging
routine beside it.

**Why post-write re-verify was not added as a second mechanism**, even though it would catch CPE-1963's
Unix residual at all five call sites: CPE-1958 rejected that shape for this family with reasons that
still hold, `backup::landed_inside` already performs exactly that check for the leg that most needs it,
and adding an unmeasured second mechanism at the end of an L ticket is how a guard that reads as coverage
gets shipped. Recorded as CPE-1963's decision to make, with the primitive now in its hands.

### `batch_media`: real containment, and the Windows shield made explicit — both

The ticket said not to leave "it measured low on Windows" as the answer, and both halves were done.

**Real containment**: `write_all` no longer truncates and writes through the checked handle, so the
`links > 1` census stops being a defence on either platform. D is now 0/2,000 on Windows and 0/10,000 on
Linux, in both racer shapes.

**The shield made explicit**: `classify_output_containment` runs before the open and refuses a
*flickering* destination outright — 626 writes reported `Ok` against E's 1,337 in the same run — which is
the whole of the two-order-of-magnitude Windows/Linux gap on identical code. That is stated at the site
and in the new test's doc as a **path gauntlet, not containment**, and it is no longer load-bearing.

**And it turned out to be shadowing, not just shielding — a genuine CPE-1929 finding at this site.** The
`links > 1` census failed both halves of the pair: `if false && facts.links > 1` left the suite
**2,430 passed / 0 failed**, and `HandleFacts { links: 1, ..facts }` left it **2,430 / 0** as well. Two
green sabotages.

**Reorder was impossible and delete would have been wrong**, and the reason is that the two guards are
not asking the same question at the same moment: the path probe reads the NAME *before* the open, the
census reads the OBJECT *after* it, and a hard link planted **in between** is visible to exactly one of
them — the census. It cannot move earlier (there is no handle before the open) and it is not redundant.
What it lacked was a fixture that could reach that window. So CPE-1961 adds
`between_containment_and_open`, a test-only seam of exactly the shape
`open_beneath::between_descent_and_leaf` already has, plus
`cpe_1961_a_link_planted_after_the_path_check_is_still_caught_by_the_handle_census`. **With that test in
the suite both sabotages now red — 2,430 passed / 1 failed each, and the failing test is that one.**

### The CPE-1929 pairs, all run, all four numbers at the site

| refusal | disable it | force the predicate to lie | verdict |
|---|---|---|---|
| `claim_destination_handle`'s `links > 1` | **10 failed** | the **same 10** failed | reachable, pinned; demoted to policy |
| the staged handle's `handle_facts == None` (new) | **2,430 / 0 — green** | **79 failed** | untestable **by construction**, stated at the site |
| `open_output_verified`'s `links > 1` census | was 2,430/0 green, **now 1 failed** | was 2,430/0 green, **now 1 failed** | was shadowed; un-shadowed here |

The ten that red for the first row are `fsutil::cpe_1857_…`, `backup::cpe_1879_…`,
`archive::cpe_1857_…` x3, `transfer::cpe_1857_…`/`cpe_1913_…`, `revert_engine::cpe_1857_…`/`cpe_1881_…`.
In **both** of those runs the new containment test stayed green, which is the point of the demotion: the
containment does not depend on the refusal, and each is pinned by a different test.

The second row is green-then-red, which is **not** CPE-1929's shadowed tell (that needs both halves to be
no-ops). It is the other category the standard names — nothing can fail
`GetFileInformationByHandle`/`File::metadata` on a handle the kernel returned from an exclusive create
three lines earlier — and it is said at the site so the next person's green sabotage is expected.

### The deterministic CI guards (the race is the evidence; these are what CI runs)

- `fsutil::cpe_1961_a_lying_link_count_cannot_destroy_a_file_outside_the_folder_through_a_copy` — a
  **real** hard link to a file outside the folder plus `ProbeInjection::HandleUnderReportsLinks`, which
  makes `handle_facts` report the count an attacker's `remove_file` produces. Asserts the outside file is
  byte-identical **and** that the destination got the new bytes.
- `batch_media::cpe_1961_a_write_never_lands_in_a_file_outside_the_batchs_folder` — two halves: the path
  gauntlet's refusal is pinned in its own right, then the gauntlet is stepped around and the containment
  property is asserted on the filesystem.
- `batch_media::cpe_1961_a_link_planted_after_the_path_check_is_still_caught_by_the_handle_census` — the
  un-shadowing test above.
- The `#[ignore]`d racer now **asserts** arms D and E are zero in both shapes, not just arm C. Before
  this ticket both were live and the harness only printed their numbers.

**Red-proofed by sabotage, run rather than argued:**

| sabotage | result |
|---|---|
| `claim_destination_handle` reverted to `set_len(0)` + hand back the destination handle | **FAILED** — `RACE_VICTIM.txt` held `ATTACKER PAYLOAD` |
| `VerifiedOutput::write_all` reverted to `set_len(0)` + `write_all` + `flush` through `self.file` | **FAILED** — victim held `ATTACKER PAYLOAD` **and the call returned `Ok(())`** |

### What it costs — audited, not assumed (non-negotiable 7)

The fix changes what object ends up at the destination, so it inherits CPE-1739's whole property set. It
is handled by **reusing** the audited machinery rather than re-deriving it: `HandleCarryover` is captured
off the destination handle (Windows: DACL with its `SE_DACL_PROTECTED` control, named streams via
`ReOpenFile` + `BackupRead`, the attribute word; Unix: extended attributes off the descriptor) and
applied to the staged file while it is still empty; `carry_protections` carries the mode; a destination
that exists and cannot be described **fails the entry** rather than being replaced by a guess. Without
this, five more legs would have shipped exactly the ACL/Mark-of-the-Web downgrade CPE-1958's round-2
Auditor measured.

Costs now on five legs that did not have them (all recorded at the site and in `src/docs/safety-undo.md`):
the destination's file id changes; write access to the destination's **folder** is required, not just to
the file; one `fsync` and one rename per entry, plus one extra per-component descent on the `Beneath`
arm; on Windows a destination carrying more than 8 MiB of alternate data streams now fails the entry with
the original untouched; and a foreign `SHARE_READ|WRITE` handle can block the commit.

**One cost is the batch engine's own**: an output that is a second name for another file *inside* the
selected folder was allowed and used to update both names. It now updates only the output. That is why
`batch_execute::cpe_1652_a_census_past_the_cap_refuses_the_write_rather_than_allowing_it` has to re-link
its fixture between its two phases — recorded at both sites rather than silently patched.

**One performance guard added rather than discovered later**: `sweep_stale_temp_siblings` does a
`read_dir` of up to 4,096 entries, and `commit()` is called in a loop over thousands of files. It is
memoised on the last directory swept per thread (`sweep_stale_temp_siblings_once_per_directory`); a
depth-first walk therefore pays it once per directory instead of once per file.
`stage_and_replace_at` deliberately keeps calling the unmemoised form, so the editor's save is unchanged.

### Test seam changed, and the change is the fix working

`cpe_1667_write_all_removes_a_file_it_created_when_the_write_itself_fails` and its companion injected
their failure by handing `VerifiedOutput` a **read-only** handle. `write_all` no longer writes through
that handle, so a read-only one now succeeds — keeping the old injection would have left both tests green
while asserting nothing. They inject `ProbeInjection::StagingCreateFails` instead (the exclusive create
of the staging sibling is what can still fail, and staging it portably needs a directory this process may
open but not create in — one `chmod` on Unix, a hand-built denying DACL on Windows). Every assertion is
still on the filesystem.

### Re-ran PR #1070's enumeration — nothing else moved

- `batch_media::real_target_containment` (`batch_media.rs:1034`) — plan-time **path** probe; still not the
  enforcement point, and now identified as the check that was *shadowing* the handle census.
- `batch_media::name_links` (`batch_media.rs:1379`) — advisory, unchanged.
- `revert_engine.rs:1146` — asks the count **after** the write is settled, only to word a refusal; safe by
  construction, unchanged.
- `vault_manager::probe_no_follow` + `overwrite_pinned_file` — **still live, still CPE-1957's site 1**,
  deliberately not touched: its shred passes are written through the checked handle by design (the point
  is to overwrite *that object's* bytes), so staging is not the answer there and it needs its own
  reasoning. Not raced here.

### Verification

- `crates/server --lib`: Windows **2,431 passed / 0 failed / 13 ignored**; Linux (WSL, sources touched
  first) **2,414 / 0 / 13**.
- `src-tauri cargo test --lib`: **230 / 0**.
- `cargo clippy --locked --all-targets -- -D warnings` clean on `crates/server` (default **and**
  `--all-features`) on Windows **and** under the WSL toolchain, and on `src-tauri`.
- Frontend `npm test`: 5,081 passed / 19 failed — all 19 in `catalogPublish*` / `releaseVerifyWiring*`,
  which shell out to `bash` and die on `EBUSY: resource busy or locked, rmdir` in `%TEMP%`. Environmental
  on this box and unrelated to this change (no shell script, workflow or TS file is touched by it).
