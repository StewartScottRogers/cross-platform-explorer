---
id: CPE-1961
title: `claim_destination_handle` and `open_output_verified` are both **live** hard-link TOCTOU races — and `batch_media` is shielded on Windows, not safe
type: bug
priority: High
status: Open
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
