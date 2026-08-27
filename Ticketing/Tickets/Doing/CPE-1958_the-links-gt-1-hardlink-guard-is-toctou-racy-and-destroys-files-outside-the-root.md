---
id: CPE-1958
title: `overwrite_confirmed_no_follow`'s `links > 1` hard-link guard is TOCTOU-racy — measured destroying a file outside the root
type: bug
priority: High
status: In Progress
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

Found by PR #1066's independent Security Auditor while verifying the CPE-1929 guard reorder. It is
**pre-existing** — reproduced against a byte-for-byte replica of `main`'s function body in the same
run, at roughly **double** the rate — and the auditor was explicit that it is not that PR's fault.

`fsutil::overwrite_confirmed_no_follow` refuses a destination whose handle reports
`nNumberOfLinks > 1`, on the reasoning that a hard link means another name shares the object. The
check reads a fact that an attacker can change **after** the open and **before** the read.

**Attack:** with write access to the destination directory, loop
`hard_link(outside_victim, slot)` / `remove_file(slot)` while a confirmed Convert writes to `slot`.

**Measured on disk:** the outside victim `RACE_VICTIM.txt` held `ATTACKER PAYLOAD` instead of
`UNTOUCHED`.

| implementation | hard-link-only swapper | mixed swapper |
|---|---|---|
| PR #1066 (guard moved ahead of the path check) | **17 destroyed / 1,000** | 5 / 2,000 |
| `main`'s body, replicated in the same run | **30 destroyed / 1,000** | 27 / 2,000 |

**PR #1066 halves the window but does not close it** — moving `links > 1` ahead of the path check
shrinks the interval, it does not remove the check-then-use.

**Mechanism** (interleaving inferred, effect measured): the open lands on the existing hard link, so
the handle is the victim's object; the attacker unlinks the second name before `handle_facts` runs;
`nNumberOfLinks` reads 1; the guard passes; `set_len(0) + write_all` lands on the victim.

**The racer was proven sensitive**, in the CPE-1937 shape: with all four guards disabled the same
racer destroys the victim, and statically the hard-link leg returns `Ok` with `victim="NEW"`.

**`batch_media::open_output_verified` under the identical racer: 0 destroyed in 2,000 trials.**

## Why re-checking harder will not fix it

The auditor's diagnosis, and it is the load-bearing part: **re-checking the same racy fact does not
help.** `nNumberOfLinks` is a property of the object at the moment it is read, and any number of reads
can each be true and stale. Two shapes that do work:

- **Claim-then-rename** — take the destination under a name only this operation owns, so no second
  name can be attached to the object between the check and the write.
- **Post-write re-verify** — after writing, confirm by **handle identity** that the object written is
  the object claimed, and undo if not.

Both are real designs, not one-line fixes, which is why this is its own ticket rather than a rider.

## Acceptance criteria

- [ ] **Reproduce first, with the auditor's racer.** Do not start from the fix. Report destroyed/trials
      for the current code, and keep the **sensitivity control** — with the guards disabled the racer
      must destroy the victim, or the harness proves nothing (CPE-1937's lesson).
- [ ] Pick claim-then-rename or post-write re-verify, **record why**, and say what it costs. A design
      that closes the window is worth more than one that narrows it further.
- [ ] **Assert on the filesystem** — the outside victim byte-identical — never on a verdict enum. This
      family's whole history is reports that look healthy while files are destroyed.
- [ ] Red-proof by racing, not by reading. Report before/after at comparable trial counts.
- [ ] Check the **other** `links > 1` sites for the same shape. `batch_media::open_output_verified`
      measured 0/2,000 under the identical racer — establish *why* it is safe and whether that property
      can be moved to `fsutil`, rather than treating the difference as luck. Enumerate rather than
      recall (CPE-1932).
- [ ] While there: PR #1066's Auditor notes the two sites now state **opposite doctrines** about a
      non-surrogate reparse point — `fsutil` **writes** it (CPE-1896's dehydrated-placeholder rule),
      `batch_media::open_output_verified` **refuses** it on the bare bit, now cemented by a new test.
      Neither is wrong, but only one is documented as a deliberate choice. Record the split or unify it.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1066's Security Auditor (**SEC PASS** — it did not
block on this, since the PR halves the rate rather than causing it).

Family: **CPE-1896** (the handle gate), **CPE-1913** (the containment gates), **CPE-1937**
(`remove_file_beneath`, and the racer shape this used), **CPE-1929** (the reorder that surfaced it, PR
#1066), **CPE-1957** (the three shadowed sites left unmeasured).

## Work Log

### 2026-08-27 — reproduced, then fixed by containment rather than by a better check

**Reproduced first, on `main`, before touching the fix.** The racer now lives in the tree as
`fsutil::tests::cpe_1958_race_report` (`#[ignore]`d — it is a measurement), five arms racing one attacker
thread in a single run so before/after is a within-run comparison rather than two runs on two machines.
Every "destroyed" is the OUTSIDE file's bytes read back off disk; no arm asserts on a verdict.

- A **unguarded** truncate+write — the sensitivity control (CPE-1937's lesson).
- B **the pre-fix body of `overwrite_confirmed_no_follow`, replicated**, so the old shape is raced beside
  the new one.
- C the **live** `overwrite_confirmed_no_follow`.
- D the **live** `batch_media::open_output_verified` + `write_all`.
- E the **live** `fsutil::copy_file_onto_no_follow` (i.e. `claim_destination_handle`).

**Baseline on `main`, Windows 11 / NTFS** (attacker owns both halves):

| arm | run 1 (1,000) | run 2 (1,000) | run 3 (2,000) |
|---|---|---|---|
| A unguarded control | 38 | 92 | 84 |
| B pre-fix replica | 28 | 43 | 68 |
| C **live `overwrite_confirmed_no_follow`** | **40** | **40** | **36** |
| D `batch_media::open_output_verified` | 2 | 1 | 2 |
| E `copy_file_onto_no_follow` | 14 | 21 | 30 |

C and B agree, which is what validates the replica.

#### The racer shape had to be fixed before before/after meant anything

After the fix the original loop landed **2,825** hard links where it had landed ~250, because a staged
write leaves a fresh single-linked file at the slot while an in-place write leaves the attacker's own
link sitting there. More attacker activity and less destruction is the right direction, but "the attacker
got a different number of chances" is exactly the confound that lets a *narrowed* window read as a
*closed* one. So the harness now also runs a second shape in which **it** plants the hard link before
every trial and the attacker only unlinks — every arm faces an identical starting condition, and
`planted` (trials that genuinely began hard-linked) is the comparability number. Both shapes are kept and
both are run; the weaker one is the shape PR #1066 used.

A pure-unlink attacker was tried and rejected, measured: it is *too fast*, dropping the name before the
open rather than inside the window, and even the unguarded control falls to **2 / 2,000**.

#### After — one run, both shapes, 2,000 trials per arm (Windows 11 / NTFS)

| arm | attacker owns both halves | harness plants each trial |
|---|---|---|
| A unguarded control | 200 / 2,000 | **846 / 2,000** (1,032 planted) |
| B pre-fix body, replicated | **76 / 2,000** | **354 / 2,000** (857 planted) |
| C `overwrite_confirmed_no_follow`, fixed | **0 / 2,000** | **0 / 2,000** (784 planted) |
| D `batch_media::open_output_verified` | 3 / 2,000 | 2 / 2,000 (668 planted) |
| E `copy_file_onto_no_follow` | 45 / 2,000 | 167 / 2,000 (686 planted) |

**784 planted against B's 857** — the fixed arm did not get an easier attacker.

#### And on Linux (WSL, ext4 `/tmp`, 1,000 trials per arm)

| arm | attacker owns both halves | harness plants each trial |
|---|---|---|
| A unguarded control | 131 | 140 (548 planted) |
| B pre-fix body | 29 | 8 (322 planted) |
| C fixed | **0** | **0** (621 planted) |
| D `open_output_verified` | **29** | **32** (501 planted) |
| E `copy_file_onto_no_follow` | 90 | 54 (537 planted) |

### The shape chosen: claim-then-rename, in its staging form

`overwrite_confirmed_no_follow` keeps **every** refusal it had, in the same order and with identical
wording, and stops writing through the handle it checked. The bytes go into a file created with
`create_new` beside the destination and are then committed over the name with a rename, via
`stage_and_replace_at` — the editor-save staging primitive, extracted out of `stage_and_replace` so there
is one implementation rather than two.

**Why not post-write re-verify.** It repairs damage instead of preventing it; the undo needs the victim's
original bytes read and held first (unbounded, and racy in its own right); and an undo that fails leaves
exactly the outcome it exists to prevent.

**Why the staging form of claim-then-rename rather than renaming the destination away.** Renaming the
destination to a private name and writing through the original handle preserves the inode, mode and
Windows ACL — but its safety rests on an interleaving argument (the private name is *listable* by anyone
who can write to that folder, so it has to be paired with a handle-identity confirmation), and a crash
mid-operation strands the user's own file under a temp name. The staging form's argument is structural
instead: **the only object the function writes into is one it created a moment ago that has never had
another name.** A reader checks that in one glance. It also fails better — the original is intact until
the atomic rename, and the residue is an empty `.cpe-tmp`, the same residue every editor save leaves,
with the same collector.

**What it costs**, recorded at the site and in `src/docs/organizing-macros.md`: the destination's
inode/file-id changes; on Windows the ACL, attribute word and alternate data streams are **not** carried
(the commit is a rename, not `ReplaceFileW`, because `ReplaceFileW` resolves the destination path inside
the very window this closes) — Unix still carries mode and xattrs, read off the handle's own metadata;
write access to the destination's **folder** is now required, not just to the file; one `fsync` and one
rename per confirmed overwrite.

The `links > 1` refusal is retained as a **policy** verdict — a user overwriting one of two names for the
same file wants to be told — but it is no longer load-bearing for containment, and the site says so.

### Red-proofed by racing and, separately, deterministically

CI does not run the racer. It runs
`cpe_1958_a_lying_link_count_cannot_destroy_a_file_outside_the_folder`, which plants a **real** hard link
to a file outside the folder and uses a new `ProbeInjection::HandleUnderReportsLinks` to make
`handle_facts` report the link count an attacker's `remove_file` produces — `1`. The guard is defeated
exactly as the racer defeats it, with no race in the test. It asserts the outside file is byte-identical
**and** that the confirmed destination got the new bytes.

Two sabotages, both run:

- revert the write half to `set_len(0)` + `write_all` through the checked handle: the test **FAILS**,
  victim holding `ATTACKER PAYLOAD`.
- delete the `links > 1` refusal entirely (`if false ... facts.links > 1`): the test still **PASSES**, and
  `overwrite_confirmed_no_follow_refuses_a_hard_linked_destination` is the one that reds. That is the
  point: the refusal is policy, the containment does not depend on it, and each is pinned by its own test.

### Enumeration of the other `links > 1` sites — derived, not recalled (CPE-1932)

`grep -rnE "\.links\s*[><=!]|links\s*>\s*1|nlink\(\)|nNumberOfLinks|NameLinks::"` over `crates/`,
`src-tauri/src`, `sidecar/`:

| site | shape | measured |
|---|---|---|
| `fsutil::overwrite_confirmed_no_follow` | **fixed here** | 0 / 2,000 both shapes, both OSes |
| `fsutil::claim_destination_handle` (`fsutil.rs:1734`) | **identical check-then-use**, then writes through the checked handle. Backup/restore + revert/snapshot. | **live: 45 / 2,000 (Win), 90 / 1,000 (Linux)** |
| `batch_media::open_output_verified` (`batch_media.rs:2041`) | same check-then-use, with a directory census when the count reads > 1 | **live: 3 / 2,000 (Win), 29 / 1,000 (Linux)** |
| `batch_media::real_target_containment` (`batch_media.rs:1034`) | plan-time **path** probe, not a handle; `open_output_verified` is its write-time authority | not the enforcement point |
| `batch_media::name_links` (`batch_media.rs:1379`) | path probe feeding `archive`, `transfer`, `revert_engine` | advisory |
| `revert_engine` (`revert_engine.rs:1146`) | asks the count **after** the write is settled, only to word a refusal | safe by construction |
| `vault_manager::probe_no_follow` + `overwrite_pinned_file` (`vault_manager.rs:1072/1139/1956`) | count read off the handle, then shred passes written **through that handle** — an under-reported count shreds a file whose other name is outside the session dir | not raced here; CPE-1957 site 1 |

**Not fixed here, deliberately.** `claim_destination_handle` cannot take the staging fix as-is: its
destination handle comes from `open_beneath::create_beneath`, which resolves component-by-component
against a held root handle, so staging beside it needs a `create_beneath`-created staging name and a
**handle-relative rename** (`FileRenameInfo` / `renameat`), neither of which is in `std`. It also hands
its written-object identity to `backup::landed_inside`. That is a design, not a line edit.
`open_output_verified` is closer — `VerifiedOutput::write_all` is its only writer and could stage — but it
is the sibling engine with its own window-ordering tests, and this ticket is scoped to `fsutil`. Both want
their own ticket; E is the higher-impact of the two.

### `batch_media`'s 0 / 2,000 was luck, not a property

The ticket asked why the sibling was safe and whether the property could be moved. It is **not safe**:
under the identical racer it destroys the victim **3 / 2,000 on Windows and ~30 / 1,000 on Linux**. On
Windows it is *shielded* rather than safe — `classify_output_containment` runs **before** the open and
refuses a flickering destination outright, so fewer trials reach the identical check-then-use (681 writes
reported `Ok` against C's 1,249 in the same run), and a ~1-per-1,000 rate sits inside a 2,000-trial
sample's noise. That shielding is a path gauntlet, not a containment property, and Linux does not have it.

### The reparse-point doctrine split: left, not unified

PR #1066 recorded that `fsutil` (post-#1066) writes a non-surrogate reparse point while `batch_media`
refuses it on the bare bit, deliberately unresolved. **Still unresolved; nothing was quietly unified.**
One input to it did move, and that is recorded at the site: this function no longer writes *through* the
destination handle, so the question that drove the refusal ("what does `set_len(0)` do to a dehydrated
placeholder") no longer arises here — the placeholder would be replaced by name. That is a reason the
split may be resolvable at this site later, not a reason to resolve it in a ticket about hard links.

### Verification

- `crates/server` Windows: **2,424 passed / 0 failed / 12 ignored**; Linux (WSL, sources touched first):
  **2,411 / 0 / 12**.
- `src-tauri cargo test --lib`: **230 / 0** — run because ~20 assertions downstream depend on this
  family's refusal wording; none of it changed.
- `cargo clippy --all-targets -- -D warnings` clean on `crates/server` (default **and** `--all-features`),
  on `src-tauri`, and on `crates/server` under the WSL toolchain.


## Correction 2026-08-27 — `batch_media` is NOT safe, and this ticket's premise was wrong

This ticket said `batch_media::open_output_verified` *"measured 0 / 2,000 under the identical racer"*
and asked whoever fixed `fsutil` to establish **why it was safe** rather than treating the difference
as luck. That framing came from the Foreman and PR #1070's worker disproved it.

**It is not safe — it is *shielded*, and only on Windows.** `classify_output_containment` runs
**before** the open and refuses a flickering destination outright, so far fewer trials reach the
identical check-then-use (681 `Ok` vs this site's 1,249 in the same run). A **path gauntlet, not
containment.** Linux has no such shield and measures **~30 / 1,000**.

That is CPE-1929's shape: a guard that survives because an earlier check happens to reject most of the
attacker's attempts looks like a property and is a coincidence. **The instruction to find out why it
was safe was the right instruction; the premise it rested on was not.**

`claim_destination_handle` (`fsutil.rs:1734`) is live too — **45 / 2,000** Windows, **90 / 1,000**
Linux. Both are now owned by **CPE-1961**.
