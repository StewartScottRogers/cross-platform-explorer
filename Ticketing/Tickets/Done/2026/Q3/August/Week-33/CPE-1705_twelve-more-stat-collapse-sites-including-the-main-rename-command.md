---
id: CPE-1705
title: Eighteen more stat-collapse sites â€” incl. the main rename command and a snapshot-index wipe
type: bug
priority: High
status: Done
tags: ready
estimate: XL
created: 2026-08-13
closed: 2026-08-13
---

## Problem

The **sixth** round of this bug class (CPE-1678 â†’ 1687 â†’ 1692 â†’ 1696 â†’ this), and the first where the
sweep behind it was genuinely exhaustive: **all 341 tracked `.rs` files, nothing excluded** â€” every file
under `tests/`, `examples/`, `benches/`, `src/bin/`, `build.rs`, and `sidecar/` in full, brace-matched to
separate production code from `#[cfg(test)]`. Found by the CPE-1696 worker, which reported rather than
fixed them, following the precedent CPE-1692 set â€” then **six more** by that PR's reviewer, running the
same sweep independently at seventeen patterns. Eighteen sites in total, and **all of them are unfixed on
`main` right now.**

That the sixth round's exhaustive sweep still missed six sites â€” including the most dangerous one in the
whole chain â€” is itself the lesson. Two people searched the same 341 files with overlapping patterns and
got different answers. Assume this list is still incomplete.

### Class A â€” refuse-to-overwrite via `.exists()`, then `fs::rename`, which replaces silently

The shape: the code checks `.exists()` to refuse clobbering an existing file, then calls `fs::rename`,
which **replaces the destination silently on both Windows and Unix**. A denied or failed stat makes
`.exists()` return `false`, the guard passes, and the rename destroys the file that was there.

- **`src-tauri/src/lib.rs:1786` â€” the main rename command.** This is the most-used operation in a file
  explorer. Rename a file to a name that already exists in a folder you cannot fully stat, and the
  existing file is gone with no warning and no error.
- `src-tauri/src/lib.rs:3317`
- `crates/server/src/copilot.rs:226`, `:255`
- `crates/server/src/organize_apply.rs:85`
- `crates/server/src/folder_template.rs:173`
- `crates/server/src/split_join.rs:111`, `:118`, `:314`
- `src-tauri/src/lib.rs:1869`, `:2102` â€” both carrying a "Never clobber" comment
- `src-tauri/src/lib.rs:156`

### Class B â€” unique-name loop, the exact `unique_target` shape CPE-1696 fixed

- **`crates/server/src/batch_media.rs:2054`** â€” the **planner** feeding the very executor CPE-1696
  hardened. Fixing the executor and leaving the planner means the plan is built on a false premise.
- `crates/server/src/snapshot_capture.rs:444`

### Class C â€” a security guard, same class as `transfer.rs:109`

- `crates/server/src/batch_media.rs:1736`

## CORRECTED â€” an ACL test CAN stage real byte loss on a rename site

**Read this instead of the earlier version of this section**, which said an ACL test can prove refusal but
never byte loss. That was measured against `fs::write`/`fs::copy` and is **false for `fs::rename`** â€” which
is what almost every site in this ticket does. Re-measured independently by the PR #889 reviewer,
non-elevated, local NTFS:

| `icacls <t> /deny <user>:(â€¦)` | `exists()` | `try_exists()` | `fs::write` | **`fs::rename` onto it** |
|---|---|---|---|---|
| `RA`, `RC` | true | `Ok(true)` | Ok | **Ok â€” bytes replaced** |
| `REA`, `RD` | true | `Ok(true)` | Ok | **Ok â€” bytes replaced** |
| **`S`**, `R`, `RX`, `W` | true | **Err(PermissionDenied)** | Err | **Ok â€” bytes replaced** |
| **`F`** | true | Err(PermissionDenied) | Err | **Ok â€” bytes replaced** |
| **`F`** + `(DC)` denied on the PARENT | true | Err | Err | Err |
| `S`/`R`/`RX`/`W` + `(DC)` on the parent | true | Err | Err | **Ok â€” still destroyed** |

So under **any** deny that refuses `try_exists`, including `(F)`: **`try_exists` fails while `fs::rename`
succeeds and destroys the bytes** (measured, `"ORIGINAL"` â†’ `"NEWDATA"`).

**This table has been revised three times; the last revision is the one that matters for writing a test.**
An earlier version said only `(F)` blocks the rename. It does not. Replacing a file needs `DELETE` on the
**target** *or* `FILE_DELETE_CHILD` on its **parent**, and an ordinary scratch parent grants the latter â€”
so **the protective factor is a property of the parent directory, not of the ACE you put on the target.**
Verified as a pair: `(F)` alone â†’ bytes replaced; `(F)` **plus** parent `(DC)` â†’ `PermissionDenied`.

For you that cuts both ways, and both matter:

- **Good:** there is no deny-on-target you could accidentally pick and be silently protected by. Any of
  them lets you stage the real byte loss.
- **Trap:** if you stage it with `(F)`, do **not** also deny `(DC)` on the parent â€” that cuts both routes,
  the rename is blocked, and your assertion passes for the wrong reason. A vacuous pass of exactly the kind
  this chain keeps producing.

**Use `(R)`, not `(F)`.** Measured across three different parent directories, `(R)` destroys the bytes in
every one; `(F)` is parent-dependent. Replacing a file needs `DELETE` on the target **or**
`FILE_DELETE_CHILD` on the parent, and only `(F)` denies the target's own `DELETE` â€” so `(F)` is the one
spec where the parent's ACL can rescue the victim and make your test lie.

**A cautionary note on how this table was arrived at.** It went through three revisions across two
reviewers and the Foreman. The version that said "only `(F)` blocks the rename" was an artifact of one
drive's inherited ACL â€” the same ACE gave opposite answers in two directories on the same machine. The
reviewer found that by **varying the parent**, which nobody had thought to vary, and reported it against
its own earlier finding. If you re-measure anything here, vary the directory too.

`crates/server/src/fsutil.rs`'s doc comment currently carries the second-to-last version of this table and
says "any of the above + parent `(DC)`". **Correct it to the row above as part of this ticket** â€” you will
be editing that comment anyway.

**What this means for you: for a `try_exists`-guarded, rename-destructive site you can write a test that
stages actual byte loss, not merely a refusal message.** Do that â€” it is a strictly stronger test.

- **A bare `expect_err` can still pass vacuously** on `write`/`copy` sites: the neutralised code still
  errors, just from the write rather than the guard. Assert on *which* error, or test the pure classifier.

## CORRECTION 4 â€” deny `RD` on the PARENT. `.exists()` IS reachable after all.

**This supersedes the "no deny refuses `Path::exists()`" line that stood in every earlier version of this
section.** That statement was measured four separate times â€” by me twice, by the PR #889 reviewer, and by
the PR #893 worker â€” and every one of those measurements **denied only the target**. It is true for
target-only denies and false in general. The PR #893 UAT found the missing step and the mechanism behind it.

**Mechanism.** On Windows `std::fs::metadata` does not give up when its `CreateFileW` open returns
`ACCESS_DENIED`. It **falls back to `FindFirstFileW`**, which reads the entry from the **parent directory**
instead of opening the file. That fallback is the entire reason a deny on the target leaves `exists()`
answering `true`. Deny **list-directory (`RD`) on the parent** and the fallback dies.

Crucially **`RD` is not `DC`**: the rename's `FILE_DELETE_CHILD` route on the parent is untouched, so the
rename still replaces the target. That is the combination nobody had tried â€” the stat fails *and* the
destructive operation succeeds.

| deny | `exists()` | `metadata()` | `try_exists()` | `rename` | unfixed `.exists()` guard clobbers? |
|---|---|---|---|---|---|
| `(R)` on target only | true | Ok | Err | Ok | **no** â€” the case we kept measuring |
| `(F)` on target only | true | Ok | Err | Err | no |
| `(S)` on target only | true | Ok | Err | Ok | no |
| **`(R)` target + `(RD)` parent** | **false** | **Err** | Err | **Ok** | **YES â€” bytes destroyed** |
| **`(S)` target + `(RD)` parent** | **false** | Err | Err | Ok | **YES** |
| `(RD)` parent only | true | Ok | Ok(true) | Ok | no |

`symlink_metadata` also fails under the parent-`RD` constructions, so a symlink-slot `Unknown` arm is
reachable too â€” another thing an earlier round declared untestable.

**Consequences, all measured:**

- Real byte loss is stageable locally against an **unfixed `.exists()` guard**, driven through
  `rename_entry_impl` (the real command, not a helper). Pre-fix the victim reads back `RENAMED SOURCE`;
  with the fix it reads `VICTIM ORIGINAL` and the command returns *"could not check what is at â€¦ nothing
  was written"*.
- The claim that the genuine overwrite is reachable **only** through the non-permission tail (`EIO`, dead
  mount, stale handle) is **false**. That tail is still real, but it is not the only route.
- **Fix it once, in the shared helper.** `fsutil::deny_stat_of`'s Windows leg denies the target only;
  `undo_deny_stat_of` already takes `parent` and ignores it on Windows. Adding the parent `RD` deny is
  ~6 lines and upgrades **every** ACL test in the repo at once. Verified: full suite still
  **2093 passed, 0 failed** with the upgrade in place.

**Do not write "byte loss is not stageable via ACLs" into any doc comment again.** It has now been the
wrong conclusion four times, each time from a correct measurement of an incomplete setup. If a future round
cannot stage it, the null result must be scoped to the exact denies applied â€” target *and* parent â€” rather
than stated as a property of ACLs.

### Two traps in implementing the helper fix â€” both measured

Found by the PR #893 reviewer when they actually implemented the "~6 line" change:

1. **`undo_deny_stat_of` must lift the PARENT before the TARGET.** `icacls` resolves its own path argument
   through `FindFirstFile`, so while the parent's `RD` deny stands, `icacls <target> /remove:d` **itself
   fails** and the target's ACE is never lifted â€” leaving the file denied for the rest of the run. With the
   naive ordering, **4 tests fail** at read-back with `Os { code: 5, PermissionDenied }`. With parents-first
   ordering: `2093 passed; 0 failed; 4 ignored`.
2. **The shared helper does not reach the tests that matter most.** `fsutil::deny_stat_of` is `pub(crate)`
   in `cpe-server` and is **never called from `src-tauri`**. Both src-tauri ACL tests inline their own
   `icacls` â€” including the one covering the **main rename command**. Fixing only the helper leaves those
   vacuous; the parent `RD` deny has to be added at the inline sites too.

Cost to expect: the extra `icacls` invocations take the suite from **17 s to 133 s**.

### A second, independent construction

Deny `(R)` on the **resolution target of a symlink** placed in the slot. That also yields
`exists() = false, try_exists() = Err`, and `fs::rename` destroys the link. It exercises the **reparse**
path rather than `fs::metadata`'s `FindFirstFileW` fallback, so it corroborates the parent-`RD` result
independently rather than restating it. Verified through the real `organize_apply` entry point.

## The `lib.rs:1786` severity claim â€” corrected, and it is subtler than first filed

The structural claim is confirmed: `Path::exists()` is `metadata().is_ok()` and collapses, and `fs::rename`
onto an existing readable file silently replaces it (measured: `Ok(())`, no warning, target's contents
replaced). A denied stat *would* pass the guard and clobber.

**But the reviewer could not construct the denied stat**, and said so rather than letting the framing stand:

- **Windows: unreachable via ACLs.** Every deny including `(F)` leaves `Path::exists()` returning `true`.
- **Unix: mitigated by a coincidence.** `target = parent.join(new_name)` and the source is in that same
  parent, so a `chmod` that makes `stat` fail with EACCES also denies `rename(2)`, which needs write+execute
  on that same directory.

So the reachable routes are the **non-permission tail** â€” `EIO`, a stale handle, a dead mount, `ELOOP` â€”
plus one the reviewer demonstrated end to end:

> **`exists()` follows symlinks and `rename` does not.** On a dangling symlink: `exists() = false`,
> `symlink_metadata = Ok`, and `fs::rename` onto it returns `Ok`, silently destroying the link.

**Note carefully: `try_exists()` also returns `Ok(false)` there, so this ticket's remedy would NOT fix that
route.** It is a CPE-1461-family symlink-following issue, not a stat-collapse one. Fix it, but fix it as
what it is, and do not let a `try_exists` swap be mistaken for having closed it.

Treat `lib.rs:1786` as priority â€” the structure is genuinely wrong and the tail is genuinely reachable â€”
but do not repeat "demonstrated byte loss in the most-used operation" as though the permission model did
not mitigate it on both platforms. Over-claiming is what produced rounds one through four of this chain.

`batch_media.rs:2054` is confirmed **without reservation**: it is the `plan()` producing the `PlannedItem`s
that `execute_plan` consumes, so now that CPE-1696 has hardened the executor, the collapsed planner will
produce plans the executor **refuses** â€” a visible symptom, not merely a latent one.

## Six more sites, found by the PR #889 reviewer's own sweep

Its scope: all 341 tracked `.rs` files, seventeen patterns, production/test split by brace-matching. It
reproduced CPE-1696's sweep line-for-line â€” and then found six sites in neither that triage nor its
excluded list. **One is worse than anything either of us named:**

- **`crates/server/src/snapshot_capture.rs:374` â€” `load_store`.** `if !path.is_file() { return
  Ok(BlobStore::new()) }`. A stat failure on `index.json` reads as *"first capture ever"*. `capture()`
  (`:142`) then loads that empty store, mutates it, and `save_store`s it back (`:167`) â€” **overwriting the
  real index with one containing only this capture's blobs, erasing every other snapshot's refcounts.** The
  next `delete_snapshot`/GC then frees blobs older snapshots still reference. Permanent cross-snapshot data
  loss from a single transient stat, and a plausible route on a network-backed store dir â€” the QNAP is an
  explicitly supported target. Note `:444` in the same file was already triaged; **`:374` is the more
  dangerous one and was missed**, which is the "file looks fully triaged, wasn't" shape.
- `snapshot_capture.rs:156` â€” `if dest.exists() { continue }` then `fs::copy` onto a blob. Benign because
  content-addressed, but the same fail-open-into-overwrite shape.
- `content_index.rs:320` â€” `load_at`; a stat failure reads as "needs build". Diagnosis severity.
- `thumb_source.rs:107` â€” `if let Ok(meta) = metadata(path) { size gate }` **with no `else`**. Structurally
  identical to the `transfer.rs` leaf CPE-1696 fixed for being the same hole; here a stat failure bypasses
  the CPE-1447/1449 memory cap and falls into an unbounded `fs::read`. Deliberate per its comment â€” but
  name the inconsistency.
- `links.rs:98`, `:120` and `src-tauri/src/lib.rs:647` â€” `symlink_metadata(..).map(is_symlink).unwrap_or(false)`;
  a denied `lstat` reads as "not a symlink".

One accuracy note on a site already listed: **Class C's `batch_media.rs:1736` is a self-described "belt and
braces" backstop behind a primary `open_no_follow`**, not a primary guard. Real, but say so.

## Triage rule: enumerate by syntax, classify by consequence

Proposed by the CPE-1696 worker after conceding `snapshot_capture.rs:374`, sharpened by that PR's reviewer,
who tested it against its own independent sweep data before endorsing it.

**Step 1 â€” enumerate by syntax. Step 2 â€” classify by consequence.** And when this chain misses something,
say which step failed, because they need opposite fixes.

> **A type-check whose false branch discards state is an absence claim, not a type claim.**

**The enumeration was not the problem this time.** The worker's production `is_file()` count was 25, which
matched the reviewer's independent sweep **exactly** â€” so `snapshot_capture.rs:374` **was found, and then
mis-filed** into the excluded type-check family because nobody asked what its `else` branch did. It returns
an empty store, which is an absence claim wearing a type check's syntax.

That distinction matters more than it looks: *"widen the search again"* has been the reflex for four rounds
running, and it is **the wrong lesson this time.** The search was right; the sorting was wrong.

The reviewer validated the rule both ways against its own data. It fires on `snapshot_capture.rs:374` (false
branch discards the blob-store ledger), `content_index.rs:320` (discards the index) and `batch_media.rs:2054`
(accepts the name as free). It correctly stays **silent** on the genuinely-excluded family â€” `duplicates.rs:73`,
`checksum.rs:54`, `compare.rs:52` and the rest all read `meta.is_file()` off a `metadata()` that already
succeeded, so there is no failed stat and no fail-open branch to classify.

### What this rule costs â€” size it up front

**Applying it means the ~110 production `.is_dir()` hits can no longer be excluded wholesale.** CPE-1692
made a documented decision to skip that family *by syntax*, and this rule is precisely what overturns it.
Each one now needs its fail-open branch read.

Budget for that at the start rather than discovering it mid-ticket â€” that discovery is how this becomes a
seventh round. If the re-walk is too large for one ticket, split it deliberately and say so; do not quietly
narrow the scope and report the remainder clean.

## Scope

The twelve sites above. **Deliberately excluded, do not re-open:**

- The `.is_dir()` type-check family â€” CPE-1692 made an explicit documented decision.
- The 16 `.ok()?` scanner sites â€” CLAUDE.md's stated "skip entries we cannot read" contract for `list_dir`.
- Four `create_new`-backed sites, already atomically mitigated.

## Acceptance criteria

- [ ] **Start with `lib.rs:1786`.** It is the highest-traffic path and the clearest data-loss risk. A test
      must prove a rename onto an existing name **refuses** when the stat fails, and still proceeds when
      the destination is genuinely absent. Both directions â€” a fix that refuses everything is as broken as
      one that overwrites.
- [ ] `batch_media.rs:2054` is fixed in the same PR as, or before, anything that consumes its plans â€”
      CPE-1696 hardened the executor and this is the planner.
- [ ] Every Class A site distinguishes "the destination is absent" from "I could not tell", and never
      renames on the second. Reuse `dispatch::classify_path_error`'s taxonomy rather than re-deriving one.
- [ ] Consider whether `fs::rename`'s silent-replace semantics warrant a shared helper rather than twelve
      independent guards. Twelve copies of the same check is how the thirteenth gets missed. Decide and
      record.
- [ ] A genuinely missing path still behaves correctly at every site â€” the honest case must not regress.
- [ ] Tests drive the real entry points, not the helpers.
- [ ] Each guard broken **on its own** turns a **distinct** test red, real output pasted in the PR, per
      the Evidence Rules in `Ticketing/wiki.md`. Watch for the vacuous `expect_err` described above.
- [ ] **`unique_target`'s unprobed fallback.** After CPE-1696 bounded the unknown-run at 8, the fallback
      `dir.join(format!("{file_name}.{pid}"))` is returned **unprobed** and handed straight to
      `fs::copy`/`fs::rename`. It was always unprobed, but it used to sit behind 10,000 real collisions and
      now sits behind 8 unreadable stats â€” far more reachable. The residual risk is small (it needs a file
      named exactly `<name>.<our pid>`, and the dead mount that triggers it usually fails the write anyway),
      but it is the one path in that function that still writes to a name nothing proved empty. Decide and
      record.
- [ ] Re-run the sweep at the same scope and state it. If it comes back clean this time, that is worth
      saying explicitly â€” it would be the first time in six rounds.

## Work Log

**2026-08-13 â€” branch `cpe-1705-stat-collapse-sites`.**

### ROUND 2 â€” my headline finding was WRONG, and correction 4 is why

**Retracted in full.** Everything under "the headline finding" below is false, and I am leaving it in place
rather than deleting it because the shape of the error is the useful part.

I measured correctly and generalised one step past my evidence â€” the exact failure `Ticketing/wiki.md`'s
Evidence Rules were written about, committed inside a ticket whose subject is that failure, for the fourth
time in this chain. My setup denied **only the target**. On Windows `fs::metadata` falls back from a
refused `CreateFileW` open to `FindFirstFileW`, reading the entry out of the **parent** â€” which is why
`exists()` kept answering `true`. Deny `(RD)` on the parent and the fallback dies, while `RD â‰  DC` leaves
the rename's `FILE_DELETE_CHILD` route intact, so the stat fails *and* the rename lands.

Measured, this round, through the real `rename_entry_impl` with both guards removed:

```
assertion `left == right` failed: a rename target whose stat we were refused must NEVER be renamed over
  left: [82, 69, 78, 65, 77, 69, 68, 32, 83, 79, 85, 82, 67, 69]      // "RENAMED SOURCE" â€” victim destroyed
 right: [86, 73, 67, 84, 73, 77, 32, 79, 82, 73, 71, 73, 78, 65, 76]  // "VICTIM ORIGINAL"
```

And CPE-1696's `unique_target`, reverted to `!candidate.exists()`:

```
  left: [77, 79, 86, 69, 68, 32, 83, 79, 85, 82, 67, 69]              // "MOVED SOURCE" â€” victim destroyed
```

**That test's original "pre-fix this read back `MOVED SOURCE`" claim was exactly, literally correct.** I
rewrote correct documentation on already-merged code with an incorrect refutation, and deleted two
salvageable tests as "vacuous" when it was my *construction* that was incomplete. All three are restored,
and both restored tests are shown firing against unfixed code.

What I did in round 2:

1. **`fsutil::deny_stat_of` now also denies `(RD)` on the parent** â€” six lines, and it upgrades every ACL
   test in the repo at once. Never `(DC)`, which would cut both delete routes and make byte-loss
   assertions pass for the wrong reason.
2. **`undo_deny_stat_of` lifts the PARENT's deny first, target last.** New finding, and not cosmetic:
   `icacls <file>` cannot rewrite a file's ACL while its directory still denies list-directory. It fails
   silently, the target keeps its deny, and the caller's `fs::read` of the victim dies with
   `PermissionDenied` â€” which reads exactly like the test's own byte assertion failing. Target-first â†’
   parent-first turned six red tests green.
3. **Reverted all three doc-comment rewrites**, with the record of the wrong turn kept in place.
4. **Fixed a real user-facing bug I had introduced and not noticed**: `symlink_slot_refusal`'s `Err` arm
   rendered *"could not check what is at "â€¦\final.txt" is a link"* â€” my bulk rename of a sibling helper's
   wording had clipped it into nonsense. Nobody read it because the arm was *believed unreachable*. The
   parent-`RD` construction makes it reachable, so it is now covered by a test as well as fixed. An
   "unreachable" branch is exactly where an unread string hides.
5. **One genuinely missed site**, inside the sweep this PR claimed: `sidecar/host/src/bin/create_sidecar.rs`
   â€” `if root.exists()` guarding `File::create` + `write_all` over a whole crate tree. Dev-only CLI, low
   severity, now fixed rather than disclosed.
6. **Filed CPE-1711** for the ~110 `.is_dir()` re-walk, which this PR had promised and not done. (Filed
   first as CPE-1710 and renumbered â€” the Foreman had taken that id for the sibling follow-up on
   `copilot`'s missing `symlink_slot_refusal`, filed from the same review pass.)

### The headline finding â€” RETRACTED, see round 2 above. Preserved for the record.

The ticket says *"for a `try_exists`-guarded, rename-destructive site you can write a test that stages
actual byte loss."* Measured: **true only for a site already probing with `try_exists`.** Against the
`.exists()`/`.is_file()` code this ticket actually fixes, the ACL is invisible â€” the ticket's own "two
things that remain true" note says no deny refuses `Path::exists()`, and that fact runs one step further
than anyone carried it. Under `icacls /deny (R)`:

- the **unfixed** `target.exists()` sees `true`, calls the slot occupied, and **refuses**;
- the victim's bytes survive;
- so every byte-level assertion passes **against the bug**.

Three tests were vacuous because of this and were caught by measuring rather than reasoning:

1. **`cpe_1696_a_move_never_renames_over_a_target_it_cannot_stat` (PR #889, pre-existing).** Reverting
   `unique_target`'s probe to `!candidate.exists()` recompiled and the test passed **green**. Its
   "*the strongest test in this ticket: it stages REAL BYTE LOSS*" / "*pre-fix this read back
   `MOVED SOURCE`*" claims are false. Doc comment corrected in place; assertions kept (they guard a
   regression *within* the fixed design).
2. **This ticket's first `batch_media::plan` test** â€” reverting to `Path::is_file()` passed green.
3. **This ticket's first `fresh_manifest_id` test** â€” reverting to `.exists()` passed green.

(2) and (3) were **rewritten** to assert on the **bound** instead, which *is* distinguishable: past
8 consecutive unreadable candidates the fixed loop refuses while the old one walks past and returns a
guessed name. Both now red against the true pre-fix code (outputs in the PR).

What the ACL leg *can* prove at a `.exists()` site is that the refusal's **wording** changed from a
confident false claim ("already exists") to an honest "I could not tell". That is a real improvement â€”
CPE-1687 traced exactly that lie to users hunting for a file that was never gone â€” and it is what the
remaining site tests assert. The genuine overwrite is reachable only via the non-permission tail (`EIO`,
dead mount, stale handle), which no local ACL simulates, so the **pure classifier tests are the
load-bearing evidence** and the ACL legs are corroboration. `fsutil::deny_stat_of`'s doc comment now says
this.

### Second finding: `try_exists` â†’ unknown-as-occupied is a HANG risk, not a one-line swap

Two candidate-advancing loops (`batch_media::plan`, `snapshot_capture::fresh_manifest_id`) had **no
termination condition** other than finding a free name â€” safe only because the old probe answered `false`
for an unreadable slot and so always terminated on the first candidate. Skipping unknowns without a bound
turns an unreadable directory (where *every* candidate is unknown) into an infinite loop. Both are now
bounded at 8 and **refuse the item** (they return `Result`, so unlike `unique_target` they need no
pathological fallback name).

### Third finding: an earlier guard fires first at `batch_media::plan`

`classify_output_containment` (CPE-1623) refuses an output whose filesystem identity it cannot read, and
it runs on the first candidate *before* the disambiguation loop. So the collapse is reachable in the loop,
not on the first probe â€” the first draft of the test denied the first candidate and was measuring a
different guard entirely.

### Decisions recorded (ACs)

- **Shared helper: YES.** `fsutil::{TargetSlot, classify_target_slot, clobber_refusal,
  unknown_slot_message}` is now the single implementation; `src-tauri`'s CPE-1696 copy was deleted and
  repointed at it. Twelve open-coded copies is how the thirteenth gets missed â€” five rounds proved it.
- **`unique_target`'s unprobed fallback: left as is, deliberately.** It needs a file named exactly
  `<name>.<our pid>`, and the dead mount that reaches it fails the subsequent write anyway. The two loops
  fixed here return `Result` and so refuse instead â€” a strictly better shape, but retrofitting it onto
  `unique_target` changes a `PathBuf`-returning signature with many callers, which is its own ticket.
- **Dangling-symlink route: fixed as what it is.** `fsutil::symlink_slot_refusal` is a **separate**
  function called **separately** at the rename sites, precisely so a `try_exists` swap is never mistaken
  for having closed it. `try_exists` answers `Ok(false)` on a dangling link *correctly*; only
  `symlink_metadata` sees it.

### Sites fixed (14)

`src-tauri/src/lib.rs`: `rename_entry_impl` (the main rename, + symlink guard), `move_exact_impl`
(+ symlink guard â€” CPE-1692 hardened its *parent* check and left the destination's own collapsed),
`move_ticket_impl`, `restore_from_trash_impl`, `restore_trash_items_impl`.
`crates/server`: `copilot::apply_op` Rename + `copilot::transfer_entry`, `organize_apply::apply_proposals`,
`folder_template::stamp_nodes`, `split_join::split_file` (Ã—2) + `join_files`, `batch_media::plan`,
`snapshot_capture::load_store` / `fresh_manifest_id` / the blob-copy skip.

### Left undone, deliberately

- `content_index::load_at` (diagnosis severity), `thumb_source` (deliberate per its own comment; a
  memory-cap bypass, a different family), `links::link_status` / `suggest_repair` /
  `src-tauri` drive-symlink probe / `batch_media`'s `open_no_follow` backstop â€” all four are
  `symlink_metadata(..).unwrap_or(false)`, i.e. **CPE-1461 family, not stat-collapse**. Mixing them in
  would repeat the mis-sorting this ticket's own triage rule warns about.
- **The ~110 `.is_dir()` re-walk the triage rule overturns.** Sized up front as the ticket asked, and it
  is a ticket of its own â€” not quietly narrowed. Needs its own file.

## Notes

Filed by the Foreman from the PR #889 (CPE-1696) sweep, 2026-08-13. The worker flagged it unprompted:
*"the sweep's Class A/B/C findings are unfixed silent-overwrite bugs on `main` right now, including the
main rename command â€” I'd recommend filing that follow-up before this sprint's queue moves on."*

Related: **CPE-1678**, **CPE-1687**, **CPE-1692**, **CPE-1696** (the same bug, four times before this),
**CPE-1673** (the taxonomy), **CPE-1461** (the guard class C belongs to).

## Work Log

**Closed 2026-08-13, merged as PR #893 (`3de22192`).** Three rounds. The code was nearly right from round 1;
the three rounds were spent discovering that **the evidence proving it was hollow** â€” and that the guidance
in this very ticket was what made it hollow.

### The chain of wrong guidance, and how it broke

This ticket's ACL section was corrected **four times**. Each correction was a correct measurement of an
incomplete setup, generalised one step past its evidence:

1. "An ACL test can prove refusal but never byte loss." â€” measured against `fs::write`/`fs::copy`; false for
   `fs::rename`, which is what almost every site here does.
2. "Only `(F)` blocks the rename." â€” false; `(F)` is parent-dependent. Replacing a file needs `DELETE` on the
   target **or** `FILE_DELETE_CHILD` on the parent, and an ordinary parent grants the latter. Use `(R)`.
3. "No deny â€” not even `(F)` â€” refuses `Path::exists()`." â€” true for **target-only** denies, false in general.
4. **The actual mechanism**, found by the PR #893 UAT: on Windows `fs::metadata` falls back to
   `FindFirstFileW` when its `CreateFileW` open returns `ACCESS_DENIED`, reading the entry from the **parent
   directory**. Deny **`RD` on the parent** and the fallback dies â€” `exists()` answers `false` on a file that
   is really there. `RD` is not `DC`, so the rename's `FILE_DELETE_CHILD` route survives and still replaces.

Rounds 1 and 3 of the sibling CPE-1704, and rounds 1â€“2 here, are all the same failure: **a true narrow
observation written as a general law.**

### What the worker got right, including about their own work

Round 1's headline â€” "the ACL byte-loss construction doesn't catch a `.exists()` bug" â€” was **correct for
the construction they were told to use**, and they proved it by reverting PR #889's flagship test and
watching it pass against the bug. That was real and worth finding. The error was concluding *no* ACL could
do it, and then **relabelling** a merged test rather than **strengthening** it.

They accepted the correction without defending it, and improved on the instructions twice:

- **Found the `icacls` parents-first ordering independently**, before the Foreman's note arrived, and
  described the trap better than either reviewer: `icacls <file>` cannot rewrite a file's ACL while its
  directory denies list-directory. It fails silently, the target keeps its deny, and the caller's `fs::read`
  dies with `PermissionDenied` â€” **which reads exactly like the test's own byte assertion failing**, so the
  next person debugs a guard that was never broken.
- Asked to write tests for three `unknown_run` reset arms, they found one **unreachable by any reasonable
  test** and deleted it instead, folding both collision cases into the arm the interleaved test exercises.
  Verified behaviour-preserving case-by-case by the reviewer, including that the syscall count is unchanged.

### The fix that made the evidence real

`fsutil::deny_stat_of` now denies the target **and** `RD` on its parent; `undo_deny_stat_of` lifts
**parents before the target**. Two further gaps, both found by the reviewer by implementing rather than
reading:

- The helper is `pub(crate)` in `cpe-server` and **never reaches `src-tauri`** â€” so both inline `icacls`
  sites, including the **main rename command's** test, stayed vacuous after a helper-only fix.
- **The premise is now asserted.** If the staging fails, the test *announces* rather than passing:
  `NOTHING in this test covered CPE-1705's overwrite route on this run.` Silent degradation to vacuous â€”
  the failure mode behind this entire chain â€” is now impossible at these sites.

Two independent constructions exist: parent-`RD` (defeats the `FindFirstFileW` fallback) and a **symlink
whose resolution target is denied** (the reparse path), found separately by the reviewer.

### Evidence

Both guards neutralised at `rename_entry_impl` reds on the **byte assertion**: `"RENAMED SOURCE"` vs
`"VICTIM ORIGINAL"`. Breaking `clobber_refusal` *alone* leaves the victim intact because
`symlink_slot_refusal` catches it â€” defence in depth, and it retro-validated a dead arm the reviewer had
flagged as unreachable, which is now live and load-bearing.

PR #889's `cpe_1696` doc comment was **restored verbatim**: its "pre-fix this read back `MOVED SOURCE`"
claim was literally correct, and reverting the wrong "correction" was the right call.

### Scope

18 reported locations / 14 functions changed / 16 guard probes replaced, plus 2 `symlink_slot_refusal`
calls and 1 in `create_sidecar`. Those three numbers count different things and the PR now says so â€” the
mismatch was itself an instance of this ticket's disease.

**Cost:** CI Windows/macOS legs went ~40 â†’ ~55 min from the extra `icacls` spawns. Accepted: it buys
two-sided tests where we had wording-only ones.

### Foreman follow-up applied at close

One stale phrase survived â€” `deny_stat_of`'s Windows mechanism "denies **one file**", which stopped being
true when the parent `RD` deny was added. Substance was unaffected (the parent stays *writable*; `RD` is not
`DC`), but leaving an imprecise mechanism description in place is exactly what started this chain. Rewritten
to name the asymmetry explicitly.

### Standing rule now in the ticket

**Never write "byte loss is not stageable via ACLs" again.** If a future round cannot stage it, scope the
null result to the exact denies applied â€” target *and* parent â€” rather than stating it as a property of ACLs.

Verdicts: Reviewer **APPROVE**, UAT **PASS**. CI green â€” 12 pass, 1 expected skip, including both Windows
legs that actually execute the ACL tests.

Deferred with tickets: **CPE-1710** (copilot rename sites destroy a dangling symlink), **CPE-1711** (re-walk
the ~110 `.is_dir()` checks), and `unique_target`'s unprobed `<name>.<pid>` fallback.

