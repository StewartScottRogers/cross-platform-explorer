---
id: CPE-1705
title: Eighteen more stat-collapse sites — incl. the main rename command and a snapshot-index wipe
type: bug
priority: High
status: Backlog
tags: ready
estimate: XL
created: 2026-08-13
closed:
---

## Problem

The **sixth** round of this bug class (CPE-1678 → 1687 → 1692 → 1696 → this), and the first where the
sweep behind it was genuinely exhaustive: **all 341 tracked `.rs` files, nothing excluded** — every file
under `tests/`, `examples/`, `benches/`, `src/bin/`, `build.rs`, and `sidecar/` in full, brace-matched to
separate production code from `#[cfg(test)]`. Found by the CPE-1696 worker, which reported rather than
fixed them, following the precedent CPE-1692 set — then **six more** by that PR's reviewer, running the
same sweep independently at seventeen patterns. Eighteen sites in total, and **all of them are unfixed on
`main` right now.**

That the sixth round's exhaustive sweep still missed six sites — including the most dangerous one in the
whole chain — is itself the lesson. Two people searched the same 341 files with overlapping patterns and
got different answers. Assume this list is still incomplete.

### Class A — refuse-to-overwrite via `.exists()`, then `fs::rename`, which replaces silently

The shape: the code checks `.exists()` to refuse clobbering an existing file, then calls `fs::rename`,
which **replaces the destination silently on both Windows and Unix**. A denied or failed stat makes
`.exists()` return `false`, the guard passes, and the rename destroys the file that was there.

- **`src-tauri/src/lib.rs:1786` — the main rename command.** This is the most-used operation in a file
  explorer. Rename a file to a name that already exists in a folder you cannot fully stat, and the
  existing file is gone with no warning and no error.
- `src-tauri/src/lib.rs:3317`
- `crates/server/src/copilot.rs:226`, `:255`
- `crates/server/src/organize_apply.rs:85`
- `crates/server/src/folder_template.rs:173`
- `crates/server/src/split_join.rs:111`, `:118`, `:314`
- `src-tauri/src/lib.rs:1869`, `:2102` — both carrying a "Never clobber" comment
- `src-tauri/src/lib.rs:156`

### Class B — unique-name loop, the exact `unique_target` shape CPE-1696 fixed

- **`crates/server/src/batch_media.rs:2054`** — the **planner** feeding the very executor CPE-1696
  hardened. Fixing the executor and leaving the planner means the plan is built on a false premise.
- `crates/server/src/snapshot_capture.rs:444`

### Class C — a security guard, same class as `transfer.rs:109`

- `crates/server/src/batch_media.rs:1736`

## CORRECTED — an ACL test CAN stage real byte loss on a rename site

**Read this instead of the earlier version of this section**, which said an ACL test can prove refusal but
never byte loss. That was measured against `fs::write`/`fs::copy` and is **false for `fs::rename`** — which
is what almost every site in this ticket does. Re-measured independently by the PR #889 reviewer,
non-elevated, local NTFS:

| `icacls <t> /deny <user>:(…)` | `exists()` | `try_exists()` | `fs::write` | **`fs::rename` onto it** |
|---|---|---|---|---|
| `RA`, `RC` | true | `Ok(true)` | Ok | **Ok — bytes replaced** |
| `REA`, `RD` | true | `Ok(true)` | Ok | **Ok — bytes replaced** |
| **`S`**, `R`, `RX`, `W` | true | **Err(PermissionDenied)** | Err | **Ok — bytes replaced** |
| **`F`** | true | Err(PermissionDenied) | Err | **Ok — bytes replaced** |
| **`F`** + `(DC)` denied on the PARENT | true | Err | Err | Err |
| `S`/`R`/`RX`/`W` + `(DC)` on the parent | true | Err | Err | **Ok — still destroyed** |

So under **any** deny that refuses `try_exists`, including `(F)`: **`try_exists` fails while `fs::rename`
succeeds and destroys the bytes** (measured, `"ORIGINAL"` → `"NEWDATA"`).

**This table has been revised three times; the last revision is the one that matters for writing a test.**
An earlier version said only `(F)` blocks the rename. It does not. Replacing a file needs `DELETE` on the
**target** *or* `FILE_DELETE_CHILD` on its **parent**, and an ordinary scratch parent grants the latter —
so **the protective factor is a property of the parent directory, not of the ACE you put on the target.**
Verified as a pair: `(F)` alone → bytes replaced; `(F)` **plus** parent `(DC)` → `PermissionDenied`.

For you that cuts both ways, and both matter:

- **Good:** there is no deny-on-target you could accidentally pick and be silently protected by. Any of
  them lets you stage the real byte loss.
- **Trap:** if you stage it with `(F)`, do **not** also deny `(DC)` on the parent — that cuts both routes,
  the rename is blocked, and your assertion passes for the wrong reason. A vacuous pass of exactly the kind
  this chain keeps producing.

**Use `(R)`, not `(F)`.** Measured across three different parent directories, `(R)` destroys the bytes in
every one; `(F)` is parent-dependent. Replacing a file needs `DELETE` on the target **or**
`FILE_DELETE_CHILD` on the parent, and only `(F)` denies the target's own `DELETE` — so `(F)` is the one
spec where the parent's ACL can rescue the victim and make your test lie.

**A cautionary note on how this table was arrived at.** It went through three revisions across two
reviewers and the Foreman. The version that said "only `(F)` blocks the rename" was an artifact of one
drive's inherited ACL — the same ACE gave opposite answers in two directories on the same machine. The
reviewer found that by **varying the parent**, which nobody had thought to vary, and reported it against
its own earlier finding. If you re-measure anything here, vary the directory too.

`crates/server/src/fsutil.rs`'s doc comment currently carries the second-to-last version of this table and
says "any of the above + parent `(DC)`". **Correct it to the row above as part of this ticket** — you will
be editing that comment anyway.

**What this means for you: for a `try_exists`-guarded, rename-destructive site you can write a test that
stages actual byte loss, not merely a refusal message.** Do that — it is a strictly stronger test.

- **A bare `expect_err` can still pass vacuously** on `write`/`copy` sites: the neutralised code still
  errors, just from the write rather than the guard. Assert on *which* error, or test the pure classifier.

## CORRECTION 4 — deny `RD` on the PARENT. `.exists()` IS reachable after all.

**This supersedes the "no deny refuses `Path::exists()`" line that stood in every earlier version of this
section.** That statement was measured four separate times — by me twice, by the PR #889 reviewer, and by
the PR #893 worker — and every one of those measurements **denied only the target**. It is true for
target-only denies and false in general. The PR #893 UAT found the missing step and the mechanism behind it.

**Mechanism.** On Windows `std::fs::metadata` does not give up when its `CreateFileW` open returns
`ACCESS_DENIED`. It **falls back to `FindFirstFileW`**, which reads the entry from the **parent directory**
instead of opening the file. That fallback is the entire reason a deny on the target leaves `exists()`
answering `true`. Deny **list-directory (`RD`) on the parent** and the fallback dies.

Crucially **`RD` is not `DC`**: the rename's `FILE_DELETE_CHILD` route on the parent is untouched, so the
rename still replaces the target. That is the combination nobody had tried — the stat fails *and* the
destructive operation succeeds.

| deny | `exists()` | `metadata()` | `try_exists()` | `rename` | unfixed `.exists()` guard clobbers? |
|---|---|---|---|---|---|
| `(R)` on target only | true | Ok | Err | Ok | **no** — the case we kept measuring |
| `(F)` on target only | true | Ok | Err | Err | no |
| `(S)` on target only | true | Ok | Err | Ok | no |
| **`(R)` target + `(RD)` parent** | **false** | **Err** | Err | **Ok** | **YES — bytes destroyed** |
| **`(S)` target + `(RD)` parent** | **false** | Err | Err | Ok | **YES** |
| `(RD)` parent only | true | Ok | Ok(true) | Ok | no |

`symlink_metadata` also fails under the parent-`RD` constructions, so a symlink-slot `Unknown` arm is
reachable too — another thing an earlier round declared untestable.

**Consequences, all measured:**

- Real byte loss is stageable locally against an **unfixed `.exists()` guard**, driven through
  `rename_entry_impl` (the real command, not a helper). Pre-fix the victim reads back `RENAMED SOURCE`;
  with the fix it reads `VICTIM ORIGINAL` and the command returns *"could not check what is at … nothing
  was written"*.
- The claim that the genuine overwrite is reachable **only** through the non-permission tail (`EIO`, dead
  mount, stale handle) is **false**. That tail is still real, but it is not the only route.
- **Fix it once, in the shared helper.** `fsutil::deny_stat_of`'s Windows leg denies the target only;
  `undo_deny_stat_of` already takes `parent` and ignores it on Windows. Adding the parent `RD` deny is
  ~6 lines and upgrades **every** ACL test in the repo at once. Verified: full suite still
  **2093 passed, 0 failed** with the upgrade in place.

**Do not write "byte loss is not stageable via ACLs" into any doc comment again.** It has now been the
wrong conclusion four times, each time from a correct measurement of an incomplete setup. If a future round
cannot stage it, the null result must be scoped to the exact denies applied — target *and* parent — rather
than stated as a property of ACLs.

### Two traps in implementing the helper fix — both measured

Found by the PR #893 reviewer when they actually implemented the "~6 line" change:

1. **`undo_deny_stat_of` must lift the PARENT before the TARGET.** `icacls` resolves its own path argument
   through `FindFirstFile`, so while the parent's `RD` deny stands, `icacls <target> /remove:d` **itself
   fails** and the target's ACE is never lifted — leaving the file denied for the rest of the run. With the
   naive ordering, **4 tests fail** at read-back with `Os { code: 5, PermissionDenied }`. With parents-first
   ordering: `2093 passed; 0 failed; 4 ignored`.
2. **The shared helper does not reach the tests that matter most.** `fsutil::deny_stat_of` is `pub(crate)`
   in `cpe-server` and is **never called from `src-tauri`**. Both src-tauri ACL tests inline their own
   `icacls` — including the one covering the **main rename command**. Fixing only the helper leaves those
   vacuous; the parent `RD` deny has to be added at the inline sites too.

Cost to expect: the extra `icacls` invocations take the suite from **17 s to 133 s**.

### A second, independent construction

Deny `(R)` on the **resolution target of a symlink** placed in the slot. That also yields
`exists() = false, try_exists() = Err`, and `fs::rename` destroys the link. It exercises the **reparse**
path rather than `fs::metadata`'s `FindFirstFileW` fallback, so it corroborates the parent-`RD` result
independently rather than restating it. Verified through the real `organize_apply` entry point.

## The `lib.rs:1786` severity claim — corrected, and it is subtler than first filed

The structural claim is confirmed: `Path::exists()` is `metadata().is_ok()` and collapses, and `fs::rename`
onto an existing readable file silently replaces it (measured: `Ok(())`, no warning, target's contents
replaced). A denied stat *would* pass the guard and clobber.

**But the reviewer could not construct the denied stat**, and said so rather than letting the framing stand:

- **Windows: unreachable via ACLs.** Every deny including `(F)` leaves `Path::exists()` returning `true`.
- **Unix: mitigated by a coincidence.** `target = parent.join(new_name)` and the source is in that same
  parent, so a `chmod` that makes `stat` fail with EACCES also denies `rename(2)`, which needs write+execute
  on that same directory.

So the reachable routes are the **non-permission tail** — `EIO`, a stale handle, a dead mount, `ELOOP` —
plus one the reviewer demonstrated end to end:

> **`exists()` follows symlinks and `rename` does not.** On a dangling symlink: `exists() = false`,
> `symlink_metadata = Ok`, and `fs::rename` onto it returns `Ok`, silently destroying the link.

**Note carefully: `try_exists()` also returns `Ok(false)` there, so this ticket's remedy would NOT fix that
route.** It is a CPE-1461-family symlink-following issue, not a stat-collapse one. Fix it, but fix it as
what it is, and do not let a `try_exists` swap be mistaken for having closed it.

Treat `lib.rs:1786` as priority — the structure is genuinely wrong and the tail is genuinely reachable —
but do not repeat "demonstrated byte loss in the most-used operation" as though the permission model did
not mitigate it on both platforms. Over-claiming is what produced rounds one through four of this chain.

`batch_media.rs:2054` is confirmed **without reservation**: it is the `plan()` producing the `PlannedItem`s
that `execute_plan` consumes, so now that CPE-1696 has hardened the executor, the collapsed planner will
produce plans the executor **refuses** — a visible symptom, not merely a latent one.

## Six more sites, found by the PR #889 reviewer's own sweep

Its scope: all 341 tracked `.rs` files, seventeen patterns, production/test split by brace-matching. It
reproduced CPE-1696's sweep line-for-line — and then found six sites in neither that triage nor its
excluded list. **One is worse than anything either of us named:**

- **`crates/server/src/snapshot_capture.rs:374` — `load_store`.** `if !path.is_file() { return
  Ok(BlobStore::new()) }`. A stat failure on `index.json` reads as *"first capture ever"*. `capture()`
  (`:142`) then loads that empty store, mutates it, and `save_store`s it back (`:167`) — **overwriting the
  real index with one containing only this capture's blobs, erasing every other snapshot's refcounts.** The
  next `delete_snapshot`/GC then frees blobs older snapshots still reference. Permanent cross-snapshot data
  loss from a single transient stat, and a plausible route on a network-backed store dir — the QNAP is an
  explicitly supported target. Note `:444` in the same file was already triaged; **`:374` is the more
  dangerous one and was missed**, which is the "file looks fully triaged, wasn't" shape.
- `snapshot_capture.rs:156` — `if dest.exists() { continue }` then `fs::copy` onto a blob. Benign because
  content-addressed, but the same fail-open-into-overwrite shape.
- `content_index.rs:320` — `load_at`; a stat failure reads as "needs build". Diagnosis severity.
- `thumb_source.rs:107` — `if let Ok(meta) = metadata(path) { size gate }` **with no `else`**. Structurally
  identical to the `transfer.rs` leaf CPE-1696 fixed for being the same hole; here a stat failure bypasses
  the CPE-1447/1449 memory cap and falls into an unbounded `fs::read`. Deliberate per its comment — but
  name the inconsistency.
- `links.rs:98`, `:120` and `src-tauri/src/lib.rs:647` — `symlink_metadata(..).map(is_symlink).unwrap_or(false)`;
  a denied `lstat` reads as "not a symlink".

One accuracy note on a site already listed: **Class C's `batch_media.rs:1736` is a self-described "belt and
braces" backstop behind a primary `open_no_follow`**, not a primary guard. Real, but say so.

## Triage rule: enumerate by syntax, classify by consequence

Proposed by the CPE-1696 worker after conceding `snapshot_capture.rs:374`, sharpened by that PR's reviewer,
who tested it against its own independent sweep data before endorsing it.

**Step 1 — enumerate by syntax. Step 2 — classify by consequence.** And when this chain misses something,
say which step failed, because they need opposite fixes.

> **A type-check whose false branch discards state is an absence claim, not a type claim.**

**The enumeration was not the problem this time.** The worker's production `is_file()` count was 25, which
matched the reviewer's independent sweep **exactly** — so `snapshot_capture.rs:374` **was found, and then
mis-filed** into the excluded type-check family because nobody asked what its `else` branch did. It returns
an empty store, which is an absence claim wearing a type check's syntax.

That distinction matters more than it looks: *"widen the search again"* has been the reflex for four rounds
running, and it is **the wrong lesson this time.** The search was right; the sorting was wrong.

The reviewer validated the rule both ways against its own data. It fires on `snapshot_capture.rs:374` (false
branch discards the blob-store ledger), `content_index.rs:320` (discards the index) and `batch_media.rs:2054`
(accepts the name as free). It correctly stays **silent** on the genuinely-excluded family — `duplicates.rs:73`,
`checksum.rs:54`, `compare.rs:52` and the rest all read `meta.is_file()` off a `metadata()` that already
succeeded, so there is no failed stat and no fail-open branch to classify.

### What this rule costs — size it up front

**Applying it means the ~110 production `.is_dir()` hits can no longer be excluded wholesale.** CPE-1692
made a documented decision to skip that family *by syntax*, and this rule is precisely what overturns it.
Each one now needs its fail-open branch read.

Budget for that at the start rather than discovering it mid-ticket — that discovery is how this becomes a
seventh round. If the re-walk is too large for one ticket, split it deliberately and say so; do not quietly
narrow the scope and report the remainder clean.

## Scope

The twelve sites above. **Deliberately excluded, do not re-open:**

- The `.is_dir()` type-check family — CPE-1692 made an explicit documented decision.
- The 16 `.ok()?` scanner sites — CLAUDE.md's stated "skip entries we cannot read" contract for `list_dir`.
- Four `create_new`-backed sites, already atomically mitigated.

## Acceptance criteria

- [ ] **Start with `lib.rs:1786`.** It is the highest-traffic path and the clearest data-loss risk. A test
      must prove a rename onto an existing name **refuses** when the stat fails, and still proceeds when
      the destination is genuinely absent. Both directions — a fix that refuses everything is as broken as
      one that overwrites.
- [ ] `batch_media.rs:2054` is fixed in the same PR as, or before, anything that consumes its plans —
      CPE-1696 hardened the executor and this is the planner.
- [ ] Every Class A site distinguishes "the destination is absent" from "I could not tell", and never
      renames on the second. Reuse `dispatch::classify_path_error`'s taxonomy rather than re-deriving one.
- [ ] Consider whether `fs::rename`'s silent-replace semantics warrant a shared helper rather than twelve
      independent guards. Twelve copies of the same check is how the thirteenth gets missed. Decide and
      record.
- [ ] A genuinely missing path still behaves correctly at every site — the honest case must not regress.
- [ ] Tests drive the real entry points, not the helpers.
- [ ] Each guard broken **on its own** turns a **distinct** test red, real output pasted in the PR, per
      the Evidence Rules in `Ticketing/wiki.md`. Watch for the vacuous `expect_err` described above.
- [ ] **`unique_target`'s unprobed fallback.** After CPE-1696 bounded the unknown-run at 8, the fallback
      `dir.join(format!("{file_name}.{pid}"))` is returned **unprobed** and handed straight to
      `fs::copy`/`fs::rename`. It was always unprobed, but it used to sit behind 10,000 real collisions and
      now sits behind 8 unreadable stats — far more reachable. The residual risk is small (it needs a file
      named exactly `<name>.<our pid>`, and the dead mount that triggers it usually fails the write anyway),
      but it is the one path in that function that still writes to a name nothing proved empty. Decide and
      record.
- [ ] Re-run the sweep at the same scope and state it. If it comes back clean this time, that is worth
      saying explicitly — it would be the first time in six rounds.

## Notes

Filed by the Foreman from the PR #889 (CPE-1696) sweep, 2026-08-13. The worker flagged it unprompted:
*"the sweep's Class A/B/C findings are unfixed silent-overwrite bugs on `main` right now, including the
main rename command — I'd recommend filing that follow-up before this sprint's queue moves on."*

Related: **CPE-1678**, **CPE-1687**, **CPE-1692**, **CPE-1696** (the same bug, four times before this),
**CPE-1673** (the taxonomy), **CPE-1461** (the guard class C belongs to).
