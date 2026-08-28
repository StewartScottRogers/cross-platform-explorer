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
**(superseded — see round 2.)** These are pre-#1066 figures and the claim is **false on Windows** at the
merged state: re-measured, arm C faced **617** planted against arm B's **838**, about a quarter *fewer*.
Arm C's zero is carried by the controls being hot in the same run, not by it facing the harder attacker.
The round-2 tables below (and `fsutil.rs:8272-8280`) are the live numbers; this whole section is history.

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
refuses it on the bare bit, deliberately unresolved. **Still unresolved; nothing was quietly unified** — and it now has its own ticket, **CPE-1959**,
filed while this was in flight, so it is no longer riding on a hard-link ticket.
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


## Work Log — round 2 (2026-08-27)

Both gates confirmed the core fix and both returned findings. Nobody asked for the write mechanism to
change, and it did not. What changed is a rebase, one measured security regression, and a set of
recorded claims that did not survive measurement.

### 0. Rebased onto `main`, and EVERY number re-taken

PR #1066 (CPE-1929) landed and touched both files, moving the `links > 1` guard **ahead** of the path
check — i.e. changing the very window this closes. The two conflicts were resolved by keeping #1066's
ordering (handle checks first, `symlink_metadata` demoted to the documented second net) and this
ticket's staging tail. **Round 1's race figures were taken against pre-#1066 `fsutil` and do not carry
over; every table in the code has been replaced with a merged-state measurement.**

**Windows 11 / NTFS, 2,000 trials per arm:**

| arm | attacker owns both halves | harness plants each trial |
|---|---|---|
| A unguarded (control) | 97 | **812** (978 planted) |
| B pre-CPE-1958 body, replicated | 51 | **356** (838 planted) |
| C `overwrite_confirmed_no_follow`, fixed | **0** | **0** (617 planted) |
| D `batch_media::open_output_verified` | 0 | 1 (715 planted) |
| E `fsutil::copy_file_onto_no_follow` | 21 | **149** (710 planted) |

**Linux / ext4, 10,000 trials per arm** (`TMPDIR` on the ext4 root, *not* `/mnt/z`):

| arm | attacker owns both halves | harness plants each trial |
|---|---|---|
| A unguarded (control) | 1,580 | 2,902 (3,076 planted) |
| B pre-CPE-1958 body, replicated | 507 | 188 (7,613 planted) |
| C `overwrite_confirmed_no_follow`, fixed | **0 / 10,000** | **0 / 10,000** (9,267 planted) |
| D `batch_media::open_output_verified` | 630 | 97 (8,529 planted) |
| E `fsutil::copy_file_onto_no_follow` | 799 | 107 (9,658 planted) |

C is 0 everywhere, controls red everywhere. Rates move a lot run to run (three Linux runs put the
planted-shape control at 1,159 / 1,669 / 2,902 per 10,000); **C measured 0 in every run, both
platforms, both shapes.** **One round-1 claim did not survive the rebase**: on Linux the fixed arm faced
9,267 hard-linked trials against the pre-fix body's 7,613, but on **Windows** it faced 617 against 838 —
about a quarter *fewer*. "The fixed arm always got the harder attacker" is false on that platform and is
stated as such at the site.

### 1. HIGH — the rename commit was stripping the destination's ACL and alternate data streams

The Auditor's measurement, reproduced: a destination with inheritance broken and one owner-only ACE plus
a real `Zone.Identifier` came back `AreAccessRulesProtected=False`, four inherited ACEs including
`Authenticated Users: Modify`, and no `Zone.Identifier`. That is not a "preservation cost" — it is
**CPE-1739's downgrade** ("a file that is more readable than the one you saved") arriving via a change in
a *different* function, plus a stripped **Mark-of-the-Web**.

**Fixed, not reworded.** New `HandleCarryover` reads the DACL (with its `SE_DACL_PROTECTED` control), the
named streams and the attribute word **off the destination HANDLE** — `GetKernelObjectSecurity`, and
`ReOpenFile` + `BackupRead` for the streams. `ReOpenFile` takes a *handle*, so no path is re-resolved and
CPE-1958's own property is untouched. `ReplaceFileW` was rejected for exactly the reason
`Commit::ReplacingTheName` exists: it resolves the destination path at commit time.

**Red-proofed by sabotage, run rather than argued**
(`cpe_1958_a_confirmed_overwrite_keeps_the_destinations_acl_and_alternate_data_streams`):

| sabotage | result |
|---|---|
| `HandleCarryover::apply` → `Ok(())` (what round 1 shipped) | **FAILED** on the protected-DACL assertion |
| `SetKernelObjectSecurity` alone disabled | **FAILED**, same assertion |
| `BackupWrite` stream replay alone disabled | **FAILED** on the `Zone.Identifier` assertion |
| `PROTECTED_DACL_SECURITY_INFORMATION` forced off | **GREEN** — recorded at the site: the descriptor's own control word already drives auto-inherit, so that argument is belt-and-braces, not the mechanism |

A fourth thing came out of it: `ReOpenFile` does **not** inherit the original open's flags, so without
`FILE_FLAG_OPEN_REPARSE_POINT` it *resolves* the reparse point and fails `ERROR_CANT_ACCESS_FILE` on a
GUID reparse point with no filter driver — reddening
`cpe_1929_overwrite_confirmed_refuses_a_surrogate_but_writes_a_non_surrogate_reparse_point`, i.e. taking
every dehydrated cloud file back to "failed operation". Caught by the suite, fixed, recorded at the site.

**Policy**: a volume with no ACLs (`ERROR_NOT_SUPPORTED`/`INVALID_FUNCTION`/`CALL_NOT_IMPLEMENTED`) has
nothing to downgrade and the save proceeds; a destination that HAS them and cannot be read **fails the
save**, temp removed, original untouched — CPE-1739's posture.

### 2. MEDIUM — the foreign `SHARE_READ|WRITE` handle

Confirmed and now **documented at the site, in `src/docs/organizing-macros.md`, and pinned** by
`cpe_1958_a_foreign_share_read_write_handle_blocks_the_confirmed_overwrite_without_damage`, which also
asserts the clean half: original byte-for-byte intact, no `.cpe-tmp` left. `commit_replacement` documents
and pins the identical gap for the editor's save; this path now matches.

### 3. MEDIUM — the directory-write requirement, and the wrong justification

The behaviour is right (fails closed). The **justification was wrong**: "the unconfirmed sibling path
already needed it, so this only narrows the confirmed path to match" does not hold, because on `main` the
confirmed path needed only *file*-write. Corrected to say it is a real narrowing, with both platforms'
refusals named. The message no longer leads with the pid-nanos `.cpe-tmp` name — it leads with
`target.display()` and mentions the staging name second.

### 4. Recorded claims that did not survive measurement — all corrected

1. **"the same residue, and the same collector, as every editor save"** — the collector half was false;
   `sweep_stale_temp_siblings` had one call site, in `stage_and_replace`. **Moved the sweep** into
   `stage_and_replace_at` instead of editing the sentence, so both staging callers now collect. The
   CPE-1738 doc says where it is called from and why it moved.
2. **"the attacker thread only unlinks"** — false; there is no `harness_plants` branch in the attacker
   body. Corrected in the harness doc, in this Work Log, and in **CPE-1961**, which had inherited it.
3. **"that caller never asks the destination path a second question"** — false on Unix:
   `carry_xattrs` did `xattr::list(target)`/`xattr::get(target, …)`, path-based and symlink-following,
   after the handle checks. **Closed rather than documented**: the Unix `HandleCarryover` reads the
   attributes off the descriptor and `carry_protections`' path copy is switched off when it is present.
4. **`file.metadata().ok()`** silently dropped CPE-1739's refusal policy. It refuses now.
5. **Docs** — `src/docs/organizing-macros.md` now covers the ADS/Mark-of-the-Web behaviour, the
   folder-permission requirement, and the foreign-handle block. No new `sectionDocs.ts` slug (verified).
6. **Harness F4** — `measured` was pushed only in the planted shape, so the control *and*
   `assert_eq!(arm C, 0)` covered one shape. **Both shapes are asserted now**, and the doc names the
   filesystem the racer needs (a WSL `drvfs` mount is not one).
7. **Doc drift** — `src-tauri/src/lib.rs:7163`/`:7222` no longer say "truncated and overwritten in
   place". The CPE-1755 comment no longer claims the `existing == None` branch is test-only: it is
   reached whenever `created == true`.

### 5. FILED, not fixed: the rename's SOURCE is unprotected — **CPE-1963**

Re-measured on the merged state with a new `#[ignore]`d racer, `cpe_1958_rename_source_report`:

| shape | aliased | `Ok` without writing the user's bytes | victim CONTENT changed |
|---|---|---|---|
| relink an outside victim — Linux ext4 | **2,834 / 3,000** | 2,834 / 3,000 | **0** |
| relink an outside victim — Windows NTFS | **6 / 3,000** | 6 / 3,000 | **0** |
| delete-only (CONTROL), both platforms | 0 / 3,000 | 0 / 3,000 | 0 |

The victim's content never changed in 24,000 trials, so this is not CPE-1958's destruction bug. It is a
successful-looking confirmed overwrite that did not write the user's bytes and left the destination
aliased outside the root. Pre-existing in `stage_and_replace`; newly on the confirmed path. Needs a
handle-relative `renameat` in `open_beneath` — the same primitive CPE-1961 names and `copilot::apply_op`
waits on. `Commit::ReplacingTheName`'s invariant now says what it actually covers.

**The trade, with numbers rather than a claim of an unqualified win:** the confirmed path swaps a
*destruction* race (bytes lost outside the root — 356/2,000 Windows, 188/10,000 Linux against the
pre-fix body) for an *aliasing* race (bytes not written, destination aliased). Better position, not a
clean sweep.

### 6. The Reviewer's F6 was wrong, and the reason matters

It reported that `CPE-1957` appears nowhere in `Ticketing/` and that no ticket owns
`claim_destination_handle` / `open_output_verified`. Both exist on `main`
(`Ticketing/Tickets/Backlog/CPE-1957_*.md`, `CPE-1961_*.md`); its worktree branched off this PR's base,
which predates them. Verified after the rebase; the pointers stay. What **does** survive is its
independent re-measurement of arm E at 51/1,000 (Windows) and 55/1,000 (Linux) — added to CPE-1961 as
corroboration, where it now sits beside two other runs that agree.

### Verification (round 2)

- `crates/server --lib`: Windows **2,428 passed / 0 failed / 13 ignored**; Linux (WSL, sources touched
  first) **2,411 / 0 / 13**. Round 2 adds two tests and one `#[ignore]`d measurement harness.
- `src-tauri cargo test --lib`, frontend `npm test`, and `cargo clippy --all-targets -- -D warnings` in
  both feature modes on both crates — figures in the PR body.

## Work Log — round 3 (2026-08-27)

An independent Security re-audit returned **SEC FINDINGS, no blocker**, and round 3 is **documentation
only**: five fixes, no behaviour change, no new test. What the re-audit added is worth recording
separately from the fixes, because it verified the PR's central property *mechanically* rather than by
reading — an `awk`+`grep` over the whole `HandleCarryover` region for `std::fs::` / `OpenOptions` /
`CreateFile` / `::open(` / `canonicalize` / `metadata(` / `PCWSTR` / `wide(` returns **five hits, all of
them `&std::fs::File` type annotations in signatures**. Every syscall in the carry-over is handle-based.
It also probed the *common* case the shipped test does not (ordinary inherited ACL, HIDDEN set, two named
streams, one of them 3 MiB): no ACE duplication, no widening, attributes carried, both streams intact,
main `$DATA` correct — and the same over the **Windows SMB redirector** (`\\localhost\C$\…`), where
`ReOpenFile` + `BackupRead` work through `mrxsmb` and a 26-byte `Zone.Identifier` survives. All four
round-2 sabotages reproduced exactly, **including the one that stays GREEN**
(`PROTECTED_DACL_SECURITY_INFORMATION` forced off — the descriptor's own `SE_DACL_PROTECTED` control bit
carries it, so the flag is redundant; recording that rather than claiming a red was the right call).
`ReOpenFile` has **exactly one call site repo-wide**, so no sibling shares the flag-omission hazard.

### F5 — a retraction that was complete in the code and standing in two other places

This is the failure mode the re-audit was told to hunt, and it found it. Round 2's retraction of *"the
fixed arm always got the harder attacker"* is complete at `fsutil.rs` (which names the Windows 617-vs-838
inversion and says what carries arm C's zero instead). It was **not** complete in:

1. **This ticket, at the round-1 table** — the sentence *"784 planted against B's 857 — the fixed arm did
   not get an easier attacker"* stood unmarked, with its correction 170 lines further down. It now
   carries `(superseded — see round 2)` **at that line**, with the replacement figures inline.
2. **The PR description** — the same sentence, in the round-1 half, unmarked. It now carries the same
   marker, and the round-1 measurement section carries a blockquote marking that whole half as history.

And round 2's own sentence in the PR body — *"every table **in this PR** and in the code has been
replaced"* — was **the overclaim**: true of the code, false of the description, whose round-1 tables were
still sitting there. (The ticket's copy of that sentence was already correctly scoped to *"every table in
the code"*.) The PR body now says the code's tables were replaced, the description's were **not**, and
they are kept as marked history instead. A retraction complete in one file and standing in two is how a
corrected claim gets re-quoted.

### F1 — the doc contradicted the code; **the sentence was corrected, not the code**

The policy paragraph said a filesystem with no ACLs *"or no named streams"* has nothing to downgrade and
the save proceeds carrying nothing. The three-code fall-through (`NOT_SUPPORTED` / `INVALID_FUNCTION` /
`CALL_NOT_IMPLEMENTED`) exists **only on the DACL branch**; on the streams side *any* `ReOpenFile` or
`BackupRead` error goes to `unreadable(...)` and fails the save.

**Decision: correct the sentence, do not mirror the allowlist.** The two branches are asymmetric on
purpose and the doc now says so, at length. *"This volume has no ACLs"* is a claim a filesystem can make
truthfully — FAT has no security to lose, so carrying nothing downgrades nothing. *"This volume has no
named streams"* is **not distinguishable, from an error code, from** *"this redirector will not tell you
about them"*, and swallowing the second silently drops a `Zone.Identifier` that may well exist — the exact
Mark-of-the-Web downgrade this type was added to stop. Widening a refusal into a swallow, on the one axis
the PR exists to defend, on an **unmeasured** guess, is the wrong direction; the redirector that *was*
measured (Microsoft's, over `mrxsmb`) works.

The cost is stated plainly at the site rather than left implicit: **a redirector that refuses
`ReOpenFile` turns every confirmed overwrite on it into a hard failure.** Measured working: NTFS,
FAT/exFAT, and the Windows SMB redirector. **Unmeasured: the QNAP/Samba box on this LAN**, and
non-Microsoft redirectors generally. If one is ever measured refusing, the site says the fix is a
**measured** allowlist of the codes that box actually returns, mirroring the DACL branch — never a
blanket swallow.

### F3 — the fourth user-visible consequence, now documented

A destination carrying **>8 MiB of alternate data streams now fails the confirmed overwrite outright**
(*"its alternate data streams are larger than 8388608 bytes…"*, original intact, no temp left); on `main`
it saved fine. `src/docs/organizing-macros.md` had been updated for the other three consequences of the
swap and not this one. Added as a fourth bullet (and the "three consequences" count corrected to four),
naming the two real-world shapes that reach it — **Mac resource forks over SMB** (`AFP_Resource`) and
large thumbnail/AV streams — saying the original is untouched and no temp is left, giving the workaround
(convert from a plain local folder), and saying explicitly that this is **new** behaviour.

### F4 — the gap stated at the site

Only `DACL_SECURITY_INFORMATION` is captured; **owner, group and the SACL — mandatory integrity label
included — are dropped**, and the struct enumerated what *is* carried without saying what is not. Now
stated, with the reason it is **unfixable rather than unfixed**: owner becomes the saver as it does for
any rename-based save, and setting it back needs `WRITE_OWNER` (not implicitly granted to an object's
owner, so asking at `create_new` time could fail an ordinary save); group is vestigial on Windows; and
reading a SACL needs `SE_SECURITY_NAME`, which an ordinary user lacks — asking and failing would fail the
save, asking and succeeding only when elevated would make behaviour depend on the token. All three point
the **safe** way; a Low-IL file returning at Medium *narrows* who may write it.

### F2 — the `CARRY_CAP` comment narrowed to what it guarantees

`read_alternate_data_streams` allocates `vec![0u8; name_len]` from the raw `u32` `dwStreamNameSize` of
**every** record, skipped ones included, **before** any `CARRY_CAP` arithmetic — 4 GiB worst case, and a
Rust allocation failure aborts. `CARRY_CAP`'s comment claimed *"a pathological file cannot turn one save
into an unbounded allocation"*: **true of bodies, false of names.**

**Decision: narrow the comment, do not add the cap.** Reaching it takes a **hostile filesystem driver** —
NTFS caps stream names at 255 characters and every record here is filled in by the kernel on a handle, so
this is ~512 bytes on any real volume. There is no injection point between `BackupRead` and the loop, so
a sanity cap could not be red-proofed through any seam this crate can build, which by **CPE-1929's own
rule** is an untestable refusal sitting in the read path that *reads as coverage without being it* — the
one shape this codebase explicitly says not to leave behind. The comment now states exactly what is
bounded (accumulated bodies + headers + names) and what is not (the per-record name pre-allocation),
names the reachability requirement, and instructs anyone who later adds a seam letting a **caller**
supply these bytes to add the cap in that same change.

### Recorded, not fixed — the one fail-open arm

If `GetKernelObjectSecurity`'s null-buffer sizing call ever returned `Ok`, the `if let Err(e) = sized`
falls straight out with `security = None` and the save proceeds carrying nothing, silently — the only
fail-**open** arm in a function where every other arm fails closed. It is unreachable: a real
self-relative descriptor is always ≥20 bytes (revision, control, four offsets), so a null buffer of
length 0 always returns `ERROR_INSUFFICIENT_BUFFER`. A defensive `else { return Err(…) }` would be
untestable through this seam for the same reason as F2, so instead there is a **NOTE at the site**
telling the next editor not to create a path where `Ok` is reachable — e.g. by passing a small stack
buffer instead of null, or by reusing this shape for a query that *can* legitimately succeed at size 0 —
without giving the `Ok` arm an explicit refusal.

### The re-audit's own race numbers, added to CPE-1961

Its 10,000-trial × 2-shape × 5-arm run on real ext4 put **D at 1,921 / 2,574** and **E at 2,706 / 2,122**
per 10,000, against round 2's **630 / 97** and **799 / 107** — a **3× to 26× run-to-run spread on the
same code**. Both are real measurements of the same defect; CPE-1961 now carries both, with an explicit
*"do not quote this ticket at the low figures"* section and a range (**~1 in 100 to ~1 in 4**) rather
than a point estimate, plus the note that on round-3's numbers D's second shape leads E's — so the two
arms should not be ordered by rate, they should both be fixed.

**This strengthens arm C rather than weakening it.** Arm C measured **0 / 10,000 in both shapes** in the
very run where D and E were at their highest, facing **4,877 planted against arm B's 2,885** — the harder
attacker, for a zero, with controls hot in both shapes. A zero taken against hot controls is worth more
than a zero taken against sleepy ones. The re-audit also re-ran `cpe_1958_rename_source_report` at 3,000
trials — **2,685 aliased / 2,685 lying `Ok` / 0 victim-content changes**, delete-only control **0** —
which lands inside CPE-1963's own Linux spread, from a harness nobody here set up; recorded there.

### Verification (round 3)

Documentation-only, so the gates are re-run as a regression check rather than as evidence of new
behaviour. Figures in the PR body.
