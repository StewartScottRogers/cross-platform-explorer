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

### Verification (ROUND 1 — WITHDRAWN, see round 2's "Where 2,414 came from")

> **Do not quote the numbers that stood here.** Round 1 reported a Linux `crates/server --lib` figure of
> **2,414 / 0 / 13** and Linux clippy as clean. **Neither run can have happened on this branch: the tree
> did not compile on Linux or macOS at `23bf35dd`.** The figures are withdrawn rather than edited, and
> the round-2 section below records what was measured instead, plus what is known about where 2,414 came
> from. The frontend diagnosis that stood here (`EBUSY … rmdir`) was also wrong and is corrected below.

---

## Round 2 — Reviewer + Security Auditor on PR #1089

### SEC-1 — `crates/server` did not compile on Linux or macOS

`archive.rs`: round 1 renamed the local `f` to `claimed` and moved the write to `claimed.file`, but the
`#[cfg(unix)]` mode block still said `f` — `error[E0425]: cannot find value 'f' in this scope`,
`archive.rs:4113`. The whole Windows CI leg stayed green, because a Windows build never parses past
`#[cfg(unix)]`.

**It was not a rename.** `ClaimedDestination::commit(mut self)` *consumes* the claim, so the block could
not stay where it was. It now sits **above** the commit and acts on `claimed.file` — which is also the
only position in which the CPE-1938 comment on it is still true (*"the mode is set through the HANDLE the
bytes went into, not by name"*) and the ordering the rest of the ticket argues for everywhere else: the
mode goes onto the staged file while it is still nameless, so the file takes the destination name already
wearing its final mode and there is no instant at which `out` exists with the wrong bits.

**And nothing covered the leg, so a test now does.**
`archive::tests::cpe1961_a_zip_entrys_unix_mode_lands_on_the_committed_file` asserts four things: the
archive's mode reaches the extracted file, the bytes reach it too, no `.cpe-tmp` residue survives, and the
name is never observed holding **content** at a non-final mode.

Red-proofed by hand on Linux (`Compiling cpe-server` line confirmed present each time — a `touch` on
`/mnt/z` does not force a rebuild):

- Disable the mode block → **red**, `left: 384 (0o600) / right: 493 (0o755)`. The `0o600` is the staging
  birth mode, which is independently the measurement behind cost row 3 below.
- Move the block back below `commit()` → does not compile. That is the defect, and it is why the position
  is called out in a comment at the site.

**The fourth assertion was written wrong first, and the test caught it** — worth recording, because the
thing it caught is a real behaviour nobody had written down. The first draft asserted the destination name
is never seen at anything but `0o755` and came back red with five observations of `0o644`.
`claim_destination_handle` opens the destination through `create_beneath`, which **creates the name** when
it is absent, so the guards have an object to interrogate. That placeholder is an empty file at the
platform default mode and it stands at the destination name for the whole of the caller's write. The
assertion is now scoped to observations where `len > 0`; the placeholder is cost row 4.

### Where 2,414 came from — the honest answer, and what is actually known

It is not knowable from here *which* run produced it, so here is what was **measured** instead of guessed,
which is what the next person needs:

| revision | Linux `--lib` | note |
|---|---|---|
| `c2a524d4` (merge base) | **2,429 / 0 / 13** | measured this round, in a throwaway worktree |
| `23bf35dd` (PR head, round 1) | *does not build* | `E0425` |
| `23bf35dd` + SEC-1 fix | **2,432 / 0 / 13** | Reviewer's figure |
| round 2 (this) | **2,433 / 0 / 14** | +1 test, +1 `#[ignore]`d racer |

**2,414 matches no revision in this PR's lineage** — not the branch (which does not build), not the merge
base, not the fixed branch. It is 15 below the merge base, so it is a number from an older tree: carried
forward from an earlier point in the shift, or from a different worktree. The PR added exactly 3 tests and
0 `#[ignore]`s, which is what makes 2,429 → 2,432 checkable rather than asserted.

**The lesson, which matters more than the number.** A wrong count is a slip. A reported measurement from a
platform where the code does not build is a claim about work that did not happen, and it is the exact
class of claim CLAUDE.md's *"derive provenance, don't claim it"* exists to kill. The corrective is not
"be more careful" — it is that a gate figure must come from the same command in the same shell that
printed it, in the same session as the diff it describes.

### SEC-4 — one planted alternate data stream aborted a whole extraction

`HandleCarryover::capture` failing, and the destination-metadata read next to it, were
`Refusal::failure` — `policy: false` — which `extract_zip_archive_stream` matches as *"a file the user
asked for and did not get"* and turns into `return Err(...)`, killing the run. Writing an ADS needs only
write access to the file, so **one planted 9 MiB stream on one pre-existing name inside the extraction
folder denied the entire extraction**. On `main` these legs never read streams at all, so the denial
arrived *with* the carry-over.

Both are now `policy: true`, which is also the honest classification and not just the convenient one:
`policy` means *"not writing is the correct outcome"*, and both of these say exactly that — we cannot
describe this destination, so we decline to replace it with a file whose protections we would have to
guess. Same family as "this name is a link". The entry is reported to the user as a named skip with its
reason; the rest of the job continues.

**And the ADS refusal message no longer says "this confirmed overwrite"** — it was written when the
editor's save was the only caller, and it is now reachable from a backup, a restore, a revert, a download
and a zip extraction, none of which is a confirmed overwrite. Kept deliberately verb-free rather than
plumbing a `LinkGuardWording` down through `HandleCarryover::capture`: what the sentence says is true of
every caller, and a sixth caller should not have to remember to pass its verb to keep it true.

### SEC-2 / SEC-3 — the scope corrections, and the race arm that was missing

**Three doc sites named the Win32 call that was measured to refuse this.** `sys::rename` records that it
uses `NtSetInformationFile(FileRenameInformation)` and **not** `SetFileInformationByHandle`, because the
wrapper refuses a non-null `RootDirectory` with `0x80070057` — and warns that a future "simplification"
back to the wrapper fails every commit on the `Beneath` arm. But `create_staging_beneath`'s doc,
`rename_beneath`'s **public** doc and `ClaimedDestination::commit`'s doc all described the implementation
as being that wrapper. The reader most likely to attempt the simplification is the one reading the public
doc. All three now say `NtSetInformationFile(FileRenameInformation)`.

**"CPE-1963 is closed on Windows" is true of the `Beneath` arm and false of `ByPath`**, and that split is
now stated at `ClaimedDestination::commit` with the Auditor's numbers. The two legs that arrive by path —
`revert_engine::apply_write` and `snapshot_capture::restore` — do not have it.

**`written` is read by ONE of the five legs.** `backup::copy_one_verified` consumes it via
`landed_inside`; `archive`, `transfer`, `revert_engine` and `snapshot_capture` all discard it. So round
1's unqualified *"the residual aliasing becomes detectable rather than silent"* is true of `backup` only.
Recorded at the field.

**The `Beneath` commit had no racer in-tree**, so its headline property was measured by nothing here —
arms D and E are both `ByPath`. `fsutil::tests::cpe_1961_beneath_commit_report` (`#[ignore]`d, like every
racer in this module) now lands it, and both platforms were re-measured with it this round:

```text
                            aliased        relinks (liveness)  victim bytes changed
Windows/NTFS   (Auditor)     0 / 3,000       11,666             0
Windows/NTFS   (this fn)     0 / 3,000        9,692             0
  ...delete-only CONTROL     0 / 3,000            0             0
Linux/ext4     (Auditor) 2,785 / 3,000      (not reported)      0
Linux/ext4     (this fn) 2,790 / 3,000      714,892             0
  ...delete-only CONTROL     0 / 3,000            0             0
```

**2,790 against 2,785, and 0 against 0, from two harnesses written independently** — that agreement is
what makes either set worth quoting. Linux rows taken on real ext4 with `TMPDIR` off `/tmp` (tmpfs on
WSL). The Windows 0 is not the attacker failing to get in: it got in 9,692 times and changed nothing. The
Linux figure is CPE-1963's open residual, not a regression and not this ticket's to close — POSIX has no
fd-sourced rename. The harness asserts only the victim-content control (0 everywhere); aliasing is
reported.

### SEC-5 — the cleanup unlinked by path across the caller's whole write

`Drop` and `commit`'s failure path both did `std::fs::remove_file` on an absolute path, defended as the
same bounded exception the refusal arms record — *a name this call created moments ago*. **"Moments ago"
had stopped being true.** `Drop` fires on any early return between the claim and the commit, and the
caller's write sits in that gap: a multi-gigabyte download, a whole archive entry. A by-path unlink across
that window is a deletion primitive.

Both now go through `Staged::abandon`, one implementation so the two halves cannot drift. The `Beneath`
arm unlinks through the held root handle (`open_beneath::remove_file_beneath`, which CPE-1937 added for
exactly this and whose doc records 89 files deleted outside the root in 200 trials when the leaf is
by-path after the same descent). The `ByPath` arm keeps `std::fs::remove_file` and that is recorded as a
**residual, not a decision that it is safe** — those callers hold no root handle, and giving them one is
wiring `open_beneath` into `revert_engine` and `snapshot_capture`, which CPE-1913 scoped out deliberately.

**Minor, same finding:** `rename_beneath`'s descent ran as `Act::Write`, whose disposition is
`FILE_OPEN_IF` / `mkdirat`-if-missing. A commit arrives after the staging file is already sitting in those
parents, so that could only ever *re-create* a directory something removed under us and then fail at the
rename anyway — debris behind a failure, the precise thing CPE-1937 gave the delete leg `FILE_OPEN` to
avoid. New `Act::Commit` variant: the delete leg's disposition, the write leg's wording.

### SEC-6 — three cost rows the audit was missing, plus a doc that read as cross-platform

The audit was accurate on what it listed and incomplete. All four rows are now at
`ClaimedDestination`'s doc, and the user-facing ones are in `src/docs/safety-undo.md`:

1. **Peak disk space doubles for the file being written.** The destination's folder transiently holds the
   old file *and* the new one; `set_len(0)`-then-write peaked at the new size. **On the backup leg —
   overwriting a large file on a nearly-full external drive — this turns operations that used to succeed
   into `ENOSPC`.** Per-file, not per-job. Unavoidable while keeping the containment property, because
   "do not write into the object that is already there" *is* the containment property.
   Derived from the mechanism, **not measured** — no run in this ticket filled a volume — and said so at
   the site (round 3, Reviewer F4: rows 3 and 4 *were* measured, so an unlabelled derivation next to them
   reads as one).
2. **On Unix the destination's owner changes.** The object landing at `dst` was created by the running
   user; writing through the pre-existing handle preserved the destination's owner and group.
   `carried_mode` handles the privilege consequence (setuid/setgid dropped when the owner changed), but
   ownership itself is neither carried nor recoverable — `fchown` to another uid needs `CAP_CHOWN`.
   Derived from the mechanism, **not measured**, and said so at the site.
3. **A destination created at a brand-new name lands at `0600`**, not `0666 & ~umask`: the staging file is
   born at `STAGING_MODE` and `HandleCarryover` only runs when `created == false`, so nothing widens it.
   `transfer::download_tree` is the leg that feels it. Measured, not reasoned — it is the `0o600` in the
   archive test's red-proof above.
4. **The destination name exists as an empty placeholder for the whole of the caller's write** (found by
   the new test, above). Not a regression — before, the same name existed and *grew* — but a caller that
   reads "the name exists" as "the file is finished" was wrong before and is still wrong.

**Not listed as a cost, deliberately:** the hard-linked-batch change. An output that was a second name for
another file inside the selected folder used to have both names updated; it now updates only the output
asked for. That is an operation no longer mutating a name it was never pointed at — an improvement, in
the docs page as a behaviour change.

**And `HandleCarryover`'s "fails rather than downgrades" is a Windows property, not the type's.** On Unix
neither half can fail: `capture` reads xattrs behind `if let Ok(names)` and `apply` discards every
per-attribute error. A destination carrying a POSIX ACL this process cannot re-apply degrades silently to
mode bits. Left as-is — making `apply` fail would turn the constant, harmless `security.selinux` refusal
into a failed backup on every SELinux machine — but recorded, because the doc read as cross-platform and
was not.

### SEC-7 — an access right nothing goes on to use

`create_staging_beneath` asked for `DELETE | READ_CONTROL | WRITE_DAC` unconditionally, defended as free
because Windows grants a creator the last two implicitly. But `HandleCarryover::apply` runs **only when
`created == false`**, and the common case for all five legs — a first backup, a fresh extraction, a
download into an empty tree — is `created == true`. That is exactly the shape `create_beneath`'s own
comment refuses, in the sentence the doc quoted approvingly: *an access right nothing goes on to use is
one more thing a network redirector can refuse.* Local NTFS grants it; SMB/WebDAV is where a backup
destination lives.

New `carrying` parameter (`carried.is_some()`, i.e. `!created`) gates `READ_CONTROL | WRITE_DAC`; `DELETE`
stays unconditional because the handle-sourced rename requires it. The `ByPath` arm splits on the same
flag between `create_staging_file_for_carryover` and `create_staging_file`. **Unmeasured against a real
SMB/WebDAV/NFS share**, and stated so at the site rather than claimed as verified.

### SEC-8 — the memo and the filter disagreed about a unit

`sweep_stale_temp_siblings_once_per_directory` remembers a **directory**; the filter matched only
`<this target's own name>.<stamp>.cpe-tmp`. In a 10,000-file directory the first file swept its own temps
and the other 9,999 never swept theirs — and the next run, walking in the same order, memoises on the same
first file and skips them again. A killed process's staging file for any non-first name is collected
**never**, not "by some later run": a partial copy of the user's data left in the user's own folder
indefinitely.

New `SweepScope`: the memoised caller sweeps `EveryDestination` so one scan per directory collects that
directory's residue, while `stage_and_replace_at` keeps the unmemoised, name-scoped form unchanged. What
this widens is stated at the variant — the "a pathologically slow save can have its own live temp swept"
window no longer needs the two saves to be of the same destination. Bounded by the same two things it
always was, neither weakened: a structurally valid `<digits>-<digits>` stamp, and an mtime
`STALE_TEMP_FLOOR` behind the just-committed target's. An actively-written staging file has a *fresh*
mtime, so reaching the floor takes a writer that has produced no bytes for five minutes.

### ASK 4 — two doc claims that were too optimistic

- **`LAST_SWEPT_DIR` is thread-local and never cleared at a run boundary**, so the memo is once per
  directory per *thread lifetime*, not per run. Round 1 said residue is "not collected until some later
  run enters that directory first"; on a reused pool thread a later run **on that same thread skips it
  too**. Only a different thread sweeps. It cannot go stale in any load-bearing way — it suppresses
  best-effort cleanup and can never affect where bytes land.
- **`create_staging_beneath`'s doc promised an internal retry that does not exist.** It said a collision
  means "the caller's next attempt gets a fresh pid+nanosecond stamp". `staging_sibling_name` is called
  **once** and a collision refuses the entry. That is the right behaviour — the name carries this
  process's pid and a nanosecond stamp, so something already at it is a signal — but it is not what the
  sentence said.

### Item 5 — "untestable by construction" was the wrong heading; it is SHADOWED

The `handle_facts(&claimed.file)` refusal's CPE-1929 pair was green-then-red (2,430 green disabled;
79 failed with the predicate lying), and round 1 filed that as *untestable by construction*. The real
reason is that the only instrument that can make `handle_facts` answer `None` —
`ProbeInjection::HandleUndescribable` — is **thread-global**, so arming it trips the *earlier*
`handle_facts(&w)` call on the destination handle first and that arm returns before control reaches here.
The 79 failures are hitting the earlier refusal. Reorder is meaningless (two different handles at two
different moments; this one cannot run before the staged file exists) and delete fails open into
`landed_inside`, so it is kept **deliberately as an unreachable fail-closed backstop** — the third
disposition CPE-1929 allows, provided the site says so, which it now does.

### ASK 5 — the frontend diagnosis was wrong and would have misdirected

Round 1 wrote the 19 failures as *"`EBUSY … rmdir` in `%TEMP%`, environmental"*. They are
`expect(status).toBe(0)` on a shelled-out script. **Cause: bare `bash` resolves to
`C:\Windows\System32\bash.exe`, the WSL launcher**, which cannot run these scripts against Windows paths.
Measured both directions this round on the same four files
(`catalogPublishFreshnessGuard`, `catalogPublishLoudFailure`, `catalogPublishVersion`,
`releaseVerifyWiringGuard`):

```text
System32\bash.exe first on PATH   4 files failed   19 failed / 41 passed
Git Bash first on PATH            4 files passed   101 passed / 2 skipped
```

Both explanations agree the PR is not the cause, but the round-1 one would have sent the next person
hunting a temp-dir lock that is not there. **`bash` must resolve to Git Bash, not `System32\bash.exe`.**

### Verification (round 2 — every figure below taken after the final edit)

- `crates/server --lib`: Linux (WSL, `TMPDIR` off tmpfs) **2,433 passed / 0 failed / 14 ignored**;
  Windows **2,442 / 0 / 14**. The +1 ignored on both is the new `Beneath` racer; the +1 passed on Linux
  only is the new `#[cfg(unix)]` archive test.
- `crates/server --tests` on Linux, all integration targets: **21 / 22 / 2 / 1 / 1 / 45 / 16 / 32**, all
  `ok`, 0 failed.
- `cargo clippy --all-targets -- -D warnings`: clean on `crates/server` on **Linux** and **Windows**
  (`Checking cpe-server` present in both, so neither was a cached no-op).
- Frontend, taken after the rebase onto `0d58a816` (which added `src/lib/catalogIndexOneDoor.test.ts`,
  so the totals are 1 file / 30 tests above the pre-rebase 357 / 5,230): `npm run check` **0 errors / 0
  warnings**; `npx vitest run` **358 files, 5,260 passed / 0 failed / 2 skipped** — with Git Bash first
  on PATH, per ASK 5.
- `src-tauri cargo test --lib`: **230 / 0**, unchanged — `open_beneath`'s new `carrying` parameter and
  `Act::Commit` are both `pub(crate)`/private to `cpe-server` and cross no crate boundary.
- `sidecar/host`'s `tests/keyring_roundtrip.rs` cannot link under WSL (`undefined symbol:
  sd_listen_fds`) — a genuine environment gap, not a code failure. It is in `sidecar/host`, which this
  diff does not touch, and it is covered by the Windows run.

## Round 3 — Reviewer on PR #1089

The narrow re-review reproduced every round-2 figure and re-took every gate, including building the
**merge base** itself (2,429 / 0 / 13) — which confirms +4 passed / +1 ignored is exactly the three
round-1 tests plus the round-2 archive test plus the `#[ignore]`d racer, and so that **2,414 matches
nothing in the lineage**. One correction to carry: moving the Unix mode block back below the commit fails
with **`E0382: borrow of moved value`**, not `E0425` — round 1's actual text was `E0425`, which is what
the SEC-1 section says, so both statements stand as written.

### F1 — the call site understated what a commit may delete

`fsutil.rs`'s comment at the `sweep_stale_temp_siblings_once_per_directory` call still read *"on the same
terms `stage_and_replace_at` calls it … scoped to this destination's own `<name>.<pid>-<nanos>.cpe-tmp`
siblings"*. Both clauses stopped being true at SEC-8, which changed that very call to
`SweepScope::EveryDestination`. The function's own doc, the next paragraph, and `SweepScope`'s doc all
said so; the **call-site** comment did not — and that is the one a reader hits first, because it sits at
the call rather than the definition. Rewritten to say plainly that this call may unlink any
staging-shaped name in the directory, minus this process's own, and to point at `SweepScope` for the
exact bound.

### F2 — the SEC-8 defence proved "shaped like ours **and** stale", never "ours"

**The revert was not taken.** The bug SEC-8 fixed is real: with the memo keyed on a *directory* and the
filter keyed on a *name*, residue for every file but the first in a directory was collected **never** — a
killed process's partial copy of user data sitting in the user's own folder indefinitely. Reverting to
doc-only would trade a real defect for a paragraph.

**But the defence as written overstated.** The stamp validator and the 300 s floor cannot establish
ownership, and the ownership evidence was already in the string being parsed: `staging_sibling_name`
writes `<name>.<pid>-<nanos>.cpe-tmp`, and `is_valid_temp_stamp` read the `<pid>` half only far enough to
prove it was digits, then discarded it.

**The worst case is ours, not an attacker's.** `download_tree` / `backup` write N files into one folder.
One write stalls — a hung socket, a paused NAS — and its staging file's mtime freezes. `LAST_SWEPT_DIR`
is a one-slot `Option<PathBuf>`, so a depth-first walk alternating `A/f1`, `A/sub/g1`, `A/f2` re-sweeps
`A` every time it returns. Past the floor the next commit unlinks the **live** sibling. Staging handles
are opened `SHARE_ALL`, which deliberately includes `FILE_SHARE_DELETE`, **so this succeeds on Windows as
well as Unix** — the writer keeps filling a nameless object and its commit then fails. Under
`ThisDestinationOnly` it was impossible: it needed two saves of the *same* destination, which is exactly
why the slow-save window was defensible before SEC-8 and is wider after it.

**Fix, keeping the whole SEC-8 win.** New `stamp_pid_is_this_process`; the `EveryDestination` arm skips a
candidate whose `<pid>` equals `std::process::id()` **unless its base name is the target's own**. Residue
from a *killed* process never carries a live pid, so nothing SEC-8 exists to collect is lost. What is
given up: our own genuinely-orphaned temp, collected on the next launch instead of this run.

**And the doc's bound is corrected rather than restated.** `SweepScope::EveryDestination` now says the
stamp validator and the age floor bound this to *"our name shape, and stale"* — **not** to *"ours"* — and
names the two foreign-file widenings that remain and are accepted: another instance's temp in a shared
folder, and a user file literally named `notes.txt.1-1.cpe-tmp`, which passes every check because every
check looks at a name and a clock. Neither costs user content, and that is the reason they are accepted,
which is a different sentence from "they cannot happen".

New tests, `crates/server/src/fsutil.rs`:

- `cpe_1961_a_directory_wide_sweep_spares_this_processes_own_live_staging_sibling` — three legs, all
  load-bearing: a foreign pid's stale temp is still collected (or the escape would have reverted SEC-8
  rather than bounded it); this process's stale temp under a *different* base name survives; this
  process's stale temp under the *target's own* base name is still collected. Calls
  `sweep_stale_temp_siblings_scoped` directly, because `LAST_SWEPT_DIR` is thread-local and never
  cleared — routing through the memoised wrapper would make the result depend on which other test in the
  binary swept a directory on that thread first.
- `cpe_1961_stamp_pid_is_this_process_reads_the_pid_half_only` — the pure truth table, including the two
  rows whose *direction* of error was chosen: an unparseable stamp answers "not ours" (safe — the
  downstream `is_valid_temp_stamp` rejects it anyway) and a leading-zero pid answers "ours" (safe — errs
  toward keeping a file).

**Red-proof, run** (`if false && …` on the pid skip):

```text
cpe_1961_stamp_pid_is_this_process_reads_the_pid_half_only ... ok
cpe_1961_a_directory_wide_sweep_spares_this_processes_own_live_staging_sibling ... FAILED
  assertion `left == right` failed: a stale staging sibling carrying THIS process's pid, under a base
  name that is not the target's, must survive: …
    left: None
```

It reds on **leg 2**, for the reason it exists, rather than because the sweep stopped working — leg 1 (a
foreign pid's stale temp is still collected) ran and stayed green in front of it. Round 4 nit: the
round-3 sentence here said *"legs 1 and 3 stayed green"*, and **leg 3 never ran** — its assertion sits
after leg 2's, so the panic reaches the harness first. Harmless to the conclusion, and exactly the kind
of overstatement this ticket has been pulled up on twice, so it is corrected rather than left.

The assertion was also changed from `fs::read(..).unwrap()` to `.ok().as_deref()` during the red-proof:
the unwrap panicked with a bare `NotFound` and never printed the sentence saying what was lost.

### F3 — the `policy: true` fix had no test

SEC-4's reclassification was correct and behaved exactly as claimed on the reviewer's hand-run, but
nothing in the tree pinned it: `ads_shaped_entry_is_skipped_end_to_end_and_recorded_not_silently_dropped`
covers the ADS-shaped *entry name*, a different hazard on the other side of the write, and
`grep -rn CARRY_CAP crates/server/src` returned one hit — the constant. A refactor could have put
`Refusal::failure` back with every gate green.

New `crates/server/src/archive.rs` test (Windows-only, because ADS are):
`cpe_1961_one_planted_alternate_data_stream_skips_its_entry_and_extracts_the_rest`. Plants a 9 MiB stream
(> `CARRY_CAP`'s 8 MiB) on a pre-existing `victim.txt` inside the destination, extracts a 3-entry zip with
the poisoned name in the **middle**, and asserts on the filesystem first: victim byte-identical, no
`.cpe-tmp` residue, then `(done, failed, skipped) == (2, 0, 1)`, the reason recorded against
`victim.txt:`, and **both** neighbours written — `after.txt` is what separates "skipped one entry" from
"abandoned the run at the first refusal". `failed: 0` is asserted as hard as `skipped: 1`: a refusal
reclassified into the failure bucket is still a wrong answer.

**Red-proof — RE-TAKEN in round 4, and the round-3 transcript that stood here was wrong** (Reviewer
Blocker 2). It ended *"The whole extraction comes back `Err` and `after.txt` is never created"*: true
before the rebase, and CPE-1935 — merged into this branch's base *by* that rebase — had already deleted
the `return Err` it described. Pre-rebase evidence presented as re-taken. Re-run on the round-4 head
(`HandleCarryover::capture`'s refusal back to `policy: false`, `Compiling cpe-server` seen):

```text
cpe_1961_one_planted_alternate_data_stream_skips_its_entry_and_extracts_the_rest ... FAILED
  two entries written, one refused as a policy skip, and NOTHING in the failed bucket … :
  ArchiveReport { done: 2, failed: 1, skipped: 0, cancelled: false, errors:
    ["victim.txt: …\out\victim.txt: its alternate data streams are larger than 8388608 bytes, which
      this app will not copy across onto the replacement — nothing was written, and the original is
      untouched. Nothing was written for this entry. The rest of the archive was extracted; clear
      that and extract again to get this entry too."] }
    left: (2, 1, 0)   right: (2, 0, 1)
```

`Ok`, not `Err`; `after.txt` **is** created. The test still reds, on the classification assert, which is
the assert that should be carrying it — `policy` no longer decides whether the run survives on this leg,
only which bucket the entry lands in and therefore which sentence the user reads.

The same correction applies to the justification comment in `claim_destination_handle`, which argued the
fix on the grounds that the archive leg *"turns into `return Err(...)`, aborting the whole archive … an
attacker-triggerable denial of service on all five legs"*. Rewritten against the callers rather than
recall: only **two** of the five ever abandoned a run over one entry, and only one of those did so
because of `policy` —

| leg | `policy: false` does | aborts the run? |
|---|---|---|
| `archive::extract_zip_archive_stream` | `report.fail` + `continue` (CPE-1935) | no — did, pre-1935 |
| `transfer::download_tree` | `undelivered.push`, per entry | no |
| `backup::apply_backup_plan_walk` | `emit(OpResult::err)`, per entry | no |
| `revert_engine::apply_write` | `Refused::transient`, per file | no |
| `snapshot_capture::restore` | `?` on **any** refusal | yes — equally on `policy: true`, so this line changes nothing there |

The change is still right and still worth making; what it buys is a **named per-entry skip with its
reason** on the four legs that report per entry, not the removal of a five-leg denial of service.

### F4 — cost row 1 was a derivation wearing no label

Row 2 (Unix ownership) said *"Derived from the mechanism … not measured"*; rows 3 and 4 really were
measured. Row 1 (peak disk doubles → `ENOSPC` on a nearly-full backup drive) is equally a derivation — no
run in this ticket filled a volume — and carried no label, so it read as measured by the company it kept.
Labelled, in both `ClaimedDestination`'s doc and the SEC-6 list above.

### Rebase onto `104b0bc5` — two conflicts with CPE-1935 (#1090), both real

CPE-1935 landed on `main` while this PR was open and changed the same two statements in
`extract_zip_archive_stream` that CPE-1961 changes: the entry write and the Unix mode set. Both were
merged rather than picked, because the two tickets want different halves of the same lines:

1. **The entry write.** CPE-1935's `fail`-and-`continue` (one entry's decompression failure must not take
   the run down) now targets `claimed.file` instead of the destination handle. The `continue` drops the
   claim, so the staging file — and the destination name when this call created it — go with it: the
   failure now leaves the entry's name exactly as the extraction found it rather than truncated.
2. **The Unix mode set.** CPE-1935's `fail`-and-`continue` is kept, at CPE-1961's position **above** the
   commit. One word of its message had to change and it is the load-bearing one: CPE-1935 said *"its
   contents were written, but its permissions could not be set"* because the bytes were at the
   destination by then. Under staging they are not — they are in a sibling the `continue` unlinks — so
   the message now says nothing was written for the entry, which is what the filesystem will show. Still
   a `fail` rather than a `done`, for CPE-1935's reason.

### Verification (round 3 — every figure re-taken after the final edit, on the rebased head)

- `crates/server --lib`: Windows **2,451 passed / 0 failed / 14 ignored**; Linux (WSL, `TMPDIR` on ext4)
  **2,441 / 0 / 14**. Against round 2's 2,442 / 2,433: +9 and +8, of which **+3** are this round's tests
  (the two sweep tests on both platforms, the ADS test on Windows only) and the rest arrived with the
  rebase onto `104b0bc5`. The Windows-minus-Linux gap widens 9 → 10 by exactly the Windows-only ADS test.
- `crates/server --tests` on Linux, all 8 integration targets: **21 / 22 / 2 / 1 / 1 / 45 / 16 / 32**, all
  `ok`, 0 failed.
- `cargo clippy --all-targets -- -D warnings` on `crates/server`: clean on **Windows** and **Linux**,
  `Checking cpe-server` present in both, so neither was a cached no-op (sources touched first on Linux —
  `touch` on `/mnt/z` does not force a rebuild by itself, so the line is the proof).
- `src-tauri cargo test --lib`: **230 / 0**, unchanged.
- Frontend: `npm run check` **0 errors / 0 warnings**; `npx vitest run` **358 files, 5,266 passed / 0
  failed / 2 skipped** (up 6 from round 2's 5,260 with the rebase).

## Review round 4 — one measured regression against `main`, and the sweep that should have caught it

### Blocker 1 — `claimed.commit()?` gave a NEW failure point run-abort semantics

`archive.rs`'s `claimed.commit().map_err(|r| r.why)?` was the only bare `?` left in
`extract_zip_archive_stream`'s per-entry body (the two `EntrySlotAction::Abort` returns are the
deliberate hostile-swap aborts). CPE-1961 is what *introduced* that failure point — `sync_all`, then a
rename the filesystem can refuse — and it landed inside the loop whose base commit, CPE-1935 (#1090),
exists to remove run aborts from it.

Reachable with no race and no privilege: a destination held open by another process with
`FILE_SHARE_READ|FILE_SHARE_WRITE` and **no** `FILE_SHARE_DELETE`, which is what a program not using
Rust's `std` opens a file with by default. Same three-entry zip:

| | outcome | before.txt | victim.txt | after.txt |
|---|---|---|---|---|
| base `104b0bc5` (main) | `Ok(done: 3)` | BEFORE | REPLACEMENT | AFTER |
| head `9902e1f5` (round 3) | `Err("… could not be replaced by the staged copy …")` | BEFORE | ORIGINAL | **absent** |

Refusing the *entry* is right and unchanged. Aborting the archive is a regression against `main`, and it
contradicts `src/docs/explorer-archives.md`, which lists a full disk under **Failed** ("the extraction
keeps going") and says only the whole destination can stop a run — while cost row 1 predicts `ENOSPC`
from staging, which under ext4's delayed allocation lands at `sync_all`, i.e. at that `?`.

Now `report.fail(&name, &EntryFailure::retryable(r.why))` + `continue`, the same shape as the two
statements above it. `retryable` because a `Refusal` carries no `io::Error` to classify and every way
this line fails — a lock, a sharing violation, a full disk, a share that dropped — is something the user
can clear and extract again into.

New test `archive::tests::cpe_1961_a_destination_the_commit_cannot_replace_costs_one_entry_not_the_run`
(Windows-only; `rename(2)` over an open file always succeeds on Linux and `sync_all` cannot be made to
fail unprivileged, so the *fixture* does not exist there — the `?` was reachable on both).

**Red-proof, run** (`?` put back; `Compiling cpe-server` seen):

```text
cpe_1961_a_destination_the_commit_cannot_replace_costs_one_entry_not_the_run ... FAILED
  THE POINT: the entry AFTER the blocked one must be written too … :
  Err("… the path component \"victim.txt\" could not be replaced by the staged copy of it
       (Access is denied. (os error 5)) [staged as \"victim.txt.33412-….cpe-tmp\"] …")
    left: None   right: Some([65, 70, 84, 69, 82])
cpe1935_a_blocked_entry_never_takes_the_run_down ... ok
```

**That second line is the finding behind the finding.** CPE-1935's own test does *not* red on this,
because nothing in the tree drove a commit failure until the new test existed. A clean interdiff proved
the rebase resolutions textually correct and could not see that the loop's contract had changed
underneath them.

### The sweep the reviewer asked for — every new failure point against each loop's contract on `main`

`commit()` is reachable on all five legs. Asked of each caller rather than recalled:

| leg | what a per-entry write failure did | verdict |
|---|---|---|
| `archive::extract_zip_archive_stream` | `?` → **run abort** | **fixed** — `report.fail` + `continue` |
| `transfer::download_tree` | `hard_err` → **run abort**, and it short-circuits every later entry | **fixed** — `record!` → `undelivered` |
| `backup::apply_backup_plan_walk` | `emit(OpResult::err)`, per entry | already right |
| `revert_engine::apply_write` | `Refused::transient`, per file | already right |
| `snapshot_capture::restore` | `?` → run abort | **left, deliberately** — see below |

**A second instance of the same defect, found by the sweep and not by the review: `download_tree`.**
Rounds 1–3 wrote `hard_err = Some(r.why)` on the commit failure, reasoning from the *bucket* (`write_all`
above already sets `hard_err`). What changed is the *input*: on `main`, a local file another program held
open downloaded fine, because writing through an already-open handle is not something a sharing mode
blocks. On this branch it took the whole tree down at the first such file. Now `record!(r)`, which is
`undelivered` — the transfer still ends `Err` naming the file, as CPE-1709 F1 requires, but the walk runs
to completion, which is that ticket's own stated ethos. Pinned by
`transfer::tests::cpe_1961_a_local_file_held_open_costs_that_file_not_the_rest_of_the_download`; both
outcomes are `Err`, so the assertion that separates them is on the filesystem (`z_after.txt`).

**Red-proof, run** (`record!(r)` back to `hard_err`):

```text
cpe_1961_a_local_file_held_open_costs_that_file_not_the_rest_of_the_download ... FAILED
  THE POINT: the entry AFTER the held-open one must still be delivered … left: None
```

`snapshot_capture::restore` is left aborting, and that is a decision rather than an omission: it is the
one caller whose all-or-nothing shape is deliberate (pass 1 pre-flights the whole manifest before a byte
is written). Turning pass 2 into a per-entry reporter asks *what is a half-restored snapshot* and needs
its own ticket. The behaviour change it does inherit — a held-open destination now stops the restore
where on `main` it did not — is recorded at that call site rather than left to be re-measured.

### Blocker 2 — the F3 transcript, and the justification behind it

Both re-taken; the F3 section above now carries the round-4 run and the corrected five-leg table.
Summary: the ADS red-proof comes back `Ok`/`(2,1,0)`, not `Err`; `after.txt` **is** created; the test
still reds on the classification assert, which is the assert that should carry it. The
`claim_destination_handle` comment no longer claims a five-leg denial of service — only two of the five
ever abandoned a run over one entry, and only one of those because of `policy`.

### Major 3 — a long-but-legal entry name, and the stub its refusal left behind

`staging_sibling_name` appended `.<pid>-<nanos>.cpe-tmp` — about 31 bytes — with **no length cap**, so a
244-character entry name (`"n"*240 + ".txt"`, under the 255 that `NAME_MAX` and NTFS both enforce)
stopped extracting. Both platforms; `main` extracts it normally.

Two fixes, because there were two defects:

1. **The cap.** `MAX_COMPONENT_BYTES = 255`, measured against the real suffix rather than an assumed
   width, backing off to a char boundary. UTF-8 bytes are a conservative proxy for NTFS's UTF-16 units
   (every scalar costs at least as many bytes as units), and exact on Unix. Truncating the base is safe
   in the direction that matters: the sweep's parser needs a `.cpe-tmp` suffix, a valid
   `<digits>-<digits>` stamp and a non-empty base, none of which truncation touches — and a truncated
   base stops equalling the target's own name, which routes the candidate into
   `stamp_pid_is_this_process` and gets it **skipped** while our pid is live. Nothing can be deleted that
   round 3 could not.
2. **The stub.** `create_staging_beneath`'s `?` fired *before* `ClaimedDestination` was constructed, so
   `Drop` never ran and the empty destination `create_beneath` had just created survived — under a
   message ending *"Nothing was written for this entry."* New `fsutil::undo_created_destination` closes
   the window in front of the claim, and the two pre-existing arms in that window (the symlink refusal,
   the missing-`file_name` refusal) now go through it too — which also moves the symlink arm's unlink
   off a bare path and onto the root handle it was already holding.

**Red-proof, both halves, run.** Cap removed, undo kept:

```text
cpe_1961_a_long_but_legal_entry_name_still_extracts ... FAILED
  Ok(ArchiveReport { done: 2, failed: 1, skipped: 0, … errors: ["nnn….txt: … the path component
    \"nnn….txt.35788-….cpe-tmp\" could not be created as a staging file (The filename, directory
    name, or volume label syntax is incorrect. (os error 123)). Nothing was written for this entry …"] })
    left: None            right: Some((true, 4))
```

Cap **and** undo removed — same refusal, same counts, one line different:

```text
    left: Some((true, 0))  right: Some((true, 4))
```

That `left` is the stub. With the cap in place that arm is no longer reachable from any input a test can
construct — what is left for it is a quota, a full disk, or a share that drops between the destination
create and the staging create in the same directory — so the undo is a live backstop with no standing
test, said at its site rather than implied by a green suite.

Also new: `fsutil::tests::cpe_1961_a_staging_sibling_name_always_fits_in_one_path_component`, the pure
half — ten base lengths either side of the boundary, each asserted against the cap **and** re-parsed
exactly the way the sweep parses it, including that `stamp_pid_is_this_process` still answers "ours".

### Minor 4 — two zip-only refusals were missing from the archives page

`src/docs/explorer-archives.md` enumerated four Refused reasons and said *"Those four are refused in
**every** format."* Round 2 added two more, zip-only — the destination that cannot be described, and the
>8 MB alternate-data-stream refusal. `safety-undo.md` mentioned the 8 MB case; the page a user reads
about extraction did not. Both are now on that page, in the vocabulary of the page (what a rename does
not carry across, and why the existing file is left alone), and the Failed list gained *"a file another
program is holding open so it cannot be replaced"* — which is what Blocker 1's fix now surfaces.

### Nits

- *"Legs 1 and 3 stayed green"* on the F2 red-proof: leg 1 did; **leg 3 never ran**, its assertion
  sitting after leg 2's. Corrected in the ticket and at the test's own doc.
- Three rows added to `stamp_pid_is_this_process`'s truth table: `9<pid>-1` and `<pid>9-1` (the parse is
  on the `u32`, so neither is a prefix match) and `<pid>.5-1` (a dot never reaches this function, because
  the caller splits on the *last* dot — a dotted base name is the caller's job, and a dot arriving anyway
  reads as "not ours", the keeping direction).

### Verification (round 4 — every figure re-taken after the final edit, on `origin/main` d929fd24)

- `crates/server --lib`: Windows **2,455 passed / 0 failed / 14 ignored**; Linux (WSL, `TMPDIR` on ext4)
  **2,443 / 0 / 14**.
- `crates/server --tests` on Linux, all 8 integration targets: **21 / 22 / 2 / 1 / 1 / 45 / 16 / 32**, all
  `ok`, 0 failed.
- `cargo clippy --all-targets -- -D warnings` on `crates/server`: clean on **Windows** and **Linux**,
  `Checking cpe-server` present in both, so neither was a cached no-op (sources touched first on Linux —
  `touch` on `/mnt/z` does not force a rebuild by itself, so the line is the proof). One real hit on the
  way there and it was this round's own: `clippy::sliced_string_as_bytes` on
  `head[dot + 1..].as_bytes()` in the new fsutil test, fixed to `&head.as_bytes()[dot + 1..]`.
- `src-tauri cargo test --lib`: **230 / 0**, unchanged.
- Frontend: `npm run check` **0 errors / 0 warnings**; `npx vitest run` **358 files, 5,266 passed / 0
  failed / 2 skipped** — all unchanged from round 3, as expected: this round touched no TypeScript and
  one user-facing markdown page that no test parses.

**Delta cross-check.** `git diff 104b0bc5 d929fd24 --stat -- crates/server/src` is **empty** — `main`
moved four commits under this branch and touched none of this crate — so round 3's figures are directly
comparable rather than needing to be re-derived. `git diff origin/main...HEAD -- crates/server/src` adds
**12** `#[test]`s, of which **3** are `#[cfg(windows)]`; four of the twelve are this round's:

| test | gated | Windows | Linux |
|---|---|---|---|
| `archive::…a_destination_the_commit_cannot_replace_costs_one_entry_not_the_run` | windows | +1 | — |
| `archive::…a_long_but_legal_entry_name_still_extracts` | none | +1 | +1 |
| `fsutil::…a_staging_sibling_name_always_fits_in_one_path_component` | none | +1 | +1 |
| `transfer::…a_local_file_held_open_costs_that_file_not_the_rest_of_the_download` | windows | +1 | — |

So Windows 2,451 → **2,455** (+4) and Linux 2,441 → **2,443** (+2), and the Windows-minus-Linux gap
widens 10 → **12** by exactly the two new Windows-only tests. Every figure reconciles.

## Round 5 — Reviewer on PR #1089

The reviewer reproduced Blocker 1, Blocker 2 and Major 3, re-took every round-4 gate figure to the number
reported, and independently re-derived the `.commit()` enumeration as complete. One Major and four Minors
carried over.

### Major 1 — `commit()` CAN return `policy: true`, and the round's second fix rested on the claim that it cannot

Two sentences said the opposite of what the callee does:

- `fsutil.rs` — *"'which bucket does `policy` pick' is the wrong question for it, because `commit` only
  ever returns `Refusal::failure`."*
- `transfer.rs` — *"`record!` with a `policy: false` refusal — which is what `commit` returns — is the
  per-entry bucket."*

Both were reasoned from `DestinationSite::ByPath`, whose commit is
`commit_replacement(...).map_err(Refusal::failure)` and is `policy: false` by construction. The
**`Beneath`** arm returns `open_beneath::rename_beneath`'s `Refusal` **unchanged**, and that function has
three `policy: true` exits — two structural, plus `descend(root, Act::Commit, dirs)` → `refuse_link` on a
directory component that has become a link since the claim. That last one is **this ticket's own threat
model**: the window is the caller's whole `write_all`.

**Executed, not read off.** New test
`fsutil::tests::cpe_1961_a_link_planted_at_an_interior_component_makes_commit_refuse_with_policy_true`
builds a real claim, plants a real link at the interior component in the write window, and commits.
Red-proofed on Linux/ext4 by inverting the assertion:

```text
RED-PROOF: `commit` returns `policy: true` here … Refusal { why: "refusing to write inside the download
folder \"…/root\": the path component \"sub\" is a link (a symlink, junction or other reparse point)…",
policy: true }        0 passed; 1 failed
```

Note the verb: `Act::Commit` and `Act::Write` share `"write"`/`"written"`, so the *wording* cannot tell a
commit-time refusal from a claim-time one. Only the call that produced it can, which is why the fixture
drives `commit()` directly instead of matching on the sentence.

**A platform split the fixture found rather than assumed.** The swap needs the real directory renamed
aside with the staging handle open inside it. Unix does that (an fd keeps the inode). **NTFS refuses it,
`ERROR_ACCESS_DENIED` (os error 5)** — measured, because the fixture's first draft `panic!`ed with the raw
error rather than skipping. So on Windows the staging handle happens to pin its own ancestor chain for
exactly the window that matters. That is a **platform accident, not a contract** — nothing in `commit`,
`rename_beneath` or `descend` arranges or depends on it — so the Windows arm asserts *the block, by its
error code* (it reds if NTFS ever starts permitting the rename) and the callers are corrected on the Unix
arm and on `commit`'s signature, never on "Windows makes it unreachable".

**Which bucket, per leg.** The first draft of this table got two rows wrong by assuming `revert_engine`
and `snapshot_capture` arrive `ByPath`. Read out of the legs instead: **all five are `Beneath`**, and
`ByPath` has no production caller at all — its only route is `copy_file_onto_no_follow{,_with_wording}`,
whose two remaining in-tree callers (`backup.rs:2154`, `revert_engine.rs:3574`) are both inside
`mod tests`. So `policy: true` at commit is live on every row:

| leg | a commit-time `policy: true` lands in… |
|---|---|
| `archive::extract_zip_archive_stream` | `report.skip` — the same arm its claim-time refusal takes, **as of this round**. Was `report.fail`, unconditionally. |
| `transfer::download_tree` | `skipped`, and the call then returns **`Ok`**. Its claim-time link refusal has always done exactly this. |
| `revert_engine::apply_write` | `Refused::permanent` / `::hard_linked`. Already forks on `refusal.policy`, so it arrives correctly classified with no change. |
| `backup::apply_backup_plan_walk` | one more `OpResult::err`, per entry. Reads `policy` nowhere — one bucket, nothing to disagree with. |
| `snapshot_capture::restore` | `?` — aborts, exactly as for any other refusal. |

**The fix, and it is the archive leg.** `transfer` already forked correctly; `archive` was
`report.fail(&name, &EntryFailure::retryable(r.why))` **unconditionally**, so one planted link produced
*"clear that and extract again"* — advice that cannot work, since re-extracting refuses again and refuses
at the **claim**, where the arm ten lines above already calls it a **skip**. One refusal, one folder, two
buckets and two sentences, decided by which microsecond the link was planted in. That commit site now
forks on `policy` exactly as its own claim site has since CPE-1935.

**CPE-1929 pair on the new fork** (Windows `--lib`, `Compiling cpe-server` seen each run; baseline
2,456 / 0 / 14):

```text
A  disable (`if false && r.policy`)   2456 passed / 0 failed   GREEN
B  lie     (`if true  || r.policy`)   2455 passed / 1 failed   RED
   B reds cpe_1961_a_destination_the_commit_cannot_replace_costs_one_entry_not_the_run:
   left: (2, 0, 1)   right: (2, 1, 0)
```

**A green + B red is not the shadowed signature** — that one is *both* green. B reds, so control reaches
the fork and the `else` arm is load-bearing: an I/O commit refusal still has to land in `failed`. A's
green says the narrower, honest thing: the **`policy: true` side specifically** has no in-tree test, and
that is structural — its only input is a component swapped inside `io::copy`'s window, so a leg-level
fixture would have to *race* the extraction and could pass by missing it, which is worse than none. Said
at the site.

**What is NOT changed, and is now said plainly at the call site.** `transfer::download_tree` sends the
link verdict to `skipped` and returns `Ok` — **a planted junction can leave a requested file undelivered
with the download reporting success**, named in `DownloadReport::skipped` with its reason but not in the
`Result`. That is CPE-1709/CPE-1881's standing contract for the leg ("not writing is the correct, safe
outcome") and it has produced exactly this for a *claim*-time link since long before CPE-1961; changing
it is a separate ticket. What this round fixes is that both moments now answer by the same rule, so the
outcome no longer depends on when the link was planted. The two legs still differ in **consequence** —
`archive` returns `Ok(report)` with counted skips either way — and that is the legs' own pre-existing
report shapes, identical for claim-time refusals, not something either site decides.

### Minor 2 — two arms still unlinked by path on an arm that has a root handle

`undo_created_destination`'s own doc said *"there is no reason to keep a by-path unlink on an arm that has
a root handle"* while two sites in the same function went on doing exactly that. Both now call the helper.
One of them (`handle_facts` cannot describe the handle) **is reachable with `created == true`**, so the
`Beneath` arm was doing a real by-path unlink inside the function whose subject is not asking the path
twice; the other is unreachable and is kept, and now says so per CPE-1929. The CPE-1896 paragraph that
called these *"the only PATH writes left … recorded rather than built"* is corrected: the handle-relative
unlink N3 described **is built**, and what remains by path is the `ByPath` arm, which holds no root handle
to be relative to.

### Minor 3 — the symlink arm's note understated the mechanism change

That arm fires only when `symlink_metadata(dst)` says the name is a link, so with `created == true` its
one reachable input is the race. `remove_file_beneath` does not merely unlink a different way — it
**classifies the leaf itself** (`refuse_link` for a junction; `FILE_OPEN_REPARSE_POINT` for a file
symlink). Outcomes are equivalent, so not a defect; recorded because *"same obligation, one
implementation"* reads as a refactor and on this arm it is a mechanism change whose safety rests on the
two mechanisms agreeing about links.

### Minor 4 — `snapshot_capture::restore`'s justification named the wrong reason

Round 4 argued the abort is deliberate *because* "pass 1 pre-flights the whole manifest before a byte is
written". **Pass 1 cannot pre-flight a commit failure** — it creates and opens nothing, so a held-open
destination is invisible to it by construction. The operative reason is three screens up: pass 2
**already** aborts mid-loop on three per-entry causes it likewise cannot pre-flight (`safe_segments`,
`blob_source`, the `written` collision check), two of them worded *"entries written before this one may
already be on disk"*. So the commit failure is a fourth cause of a shape the loop already has and already
tells the user about. Leaving it is consistent, not merely convenient. The behaviour-change paragraph
after it was accurate and is kept.

### Minor 5 — truncation adds a third collision class

`staging_sibling_name`'s doc named two consequences, both about its own residue. A third: truncation makes
one destination's staging base equal to **another** destination's **full** name — `A` exactly `room` bytes,
`B` longer and sharing that prefix — so a commit on `A` recognises `B`'s staging file as its own residue
under `SweepScope::ThisDestinationOnly`, the arm with **no** `stamp_pid_is_this_process` guard. Bounded by
`STALE_TEMP_FLOOR` (300 s): `B` would have to be mid-write for five minutes. Worst outcome is `B`'s entry
refused with a reason, never a wrong file at `B`'s name. Documented, not fixed — narrowing it means giving
`ThisDestinationOnly` the pid guard, which changes `stage_and_replace_at`'s residue collection for a case
this bound already makes remote.

### In-app docs

- `explorer-archives.md` — the Refused/skipped section now says a shortcut appearing **part-way through**
  the write is skipped like one already there, and names the wrong advice it used to carry.
- `31-network.md` — the link-skip section now states the consequence out loud: a link appearing
  mid-download means **the download finishes, reports success, and that one file is not there** — counted
  and named in the skipped list, but not in the overall result.

### Verification (round 5 — every figure re-taken after the final edit, rebased on `origin/main` 8c9ddb60)

| gate | Windows | Linux (WSL, `TMPDIR` on ext4) |
|---|---|---|
| `crates/server --lib` | **2,456 passed / 0 failed / 14 ignored** | **2,444 passed / 0 failed / 14 ignored** |
| `crates/server --tests`, 8 integration targets | 21 / 22 / 2 / 1 / 1 / 45 / 16 / 32, all `ok`, 0 failed | 21 / 22 / 2 / 1 / 1 / 45 / 16 / 32, all `ok`, 0 failed |
| `cargo clippy --all-targets -- -D warnings` (`crates/server`) | clean, `Checking cpe-server` present | clean, `Checking cpe-server` present |
| `src-tauri cargo test --lib` | **230 passed / 0 failed / 0 ignored** | — |
| `npm run check` | **0 errors / 0 warnings** | — |
| `npx vitest run` | **358 files, 5,266 passed / 0 failed / 2 skipped** | — |

**Passed, failed, ignored and skipped stated separately** throughout: no gate above has a non-zero failed
count; the 14 `--lib` ignored are the pre-existing measurement harnesses; vitest's 2 skipped are unchanged
from round 4. `npx vitest run` was taken with **Git Bash first on PATH** — the reviewer's local 19 failures
in four `catalogPublish*` / `releaseVerifyWiringGuard` files are the known bare-`bash` →
`System32\bash.exe` environment issue and are not this PR's (`git diff --stat origin/main` touches no
`src/lib/`, `scripts/` or `.github/`).

One real clippy hit on the way there, this round's own: `clippy::disallowed_methods` on the two
`std::fs::rename` calls in the new fsutil test (`crates/server/clippy.toml` disallows it). Both now carry
`#[allow]` with a one-line reason at the site, per that file's second form — a test fixture staging the
attack, where the raw call *is* the measurement, and a teardown of two names the test created itself.

**Delta cross-check.** `+1` test on both platforms — the new `fsutil` test is ungated, so Windows
2,455 → **2,456** and Linux 2,443 → **2,444**, and the Windows-minus-Linux gap stays **12**. The archive
`policy` fork adds no test (its `policy: false` half is pinned by the existing round-4 test, per the
CPE-1929 pair above). Frontend totals are unchanged from round 4 at 5,266 / 2: this round touched two
user-facing markdown pages that no test parses.
