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
      land 2,825 hard links where it landed ~250, which would have let a *narrowed* window read as a
      *closed* one. It solved that with a **planted** shape where the harness plants the link and the
      attacker only unlinks — use it.
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
