---
id: CPE-1729
title: Splitting into a linked output directory fails with a message that names neither the path nor the real problem
type: task
priority: Low
status: Backlog
tags: ready
estimate: 30m
created: 2026-08-14
closed:
---

## This ticket was rewritten before it was ever worked — read why

**The original version described a bug that does not happen.** It was filed during CPE-1718 by reasoning
from a true observation — *"`create_dir_all` follows a link"* — one step past its evidence, and it claimed:

> a **dangling** link at `out_dir` … `create_dir_all` then creates the missing target directory and writes
> the manifest and the entire numbered part series into it … a split that reports success with its whole
> output in a directory the user never named.

**The CPE-1718 UAT measured it twice, on two dangling-link shapes** (`make_dangling_link`, and `mklink /D`
pointing at a missing target):

```
[T20] split -> Err("Cannot create a file when that file already exists. (os error 183)")
[T20] post: is_link=Ok(true)  missing_dir_created=Ok(false)  missing_census=[]
```

**The target directory is not created, nothing is written, and the split fails rather than reporting
success.** `std::fs::create_dir_all` tests `is_dir()` — which follows the link and answers `false` for a
dangling one — then calls `create_dir`, gets `AlreadyExists` because the *name* is held by the reparse
point, and returns the error. **It never walks through the link.**

*Scope of that measurement: Windows 11, two link shapes, on the CPE-1718 PR head. Not measured on Linux or
macOS.*

This rewrite is kept rather than the ticket being deleted, because "we thought this was a bug and it isn't,
here is the measurement" is worth more to the next reader than silence — and because the module doc **at
the site** was careful and correct all along. Only the ticket overstated.

## The real residual, which the original missed

`create_dir_all(out_dir).map_err(|e| e.to_string())` **discards the path**. So the user gets:

```
Cannot create a file when that file already exists. (os error 183)
```

No path. And **"file"** for what is a directory problem — the OS's wording, passed through unexamined. A
user who has pointed their output at a symlinked drive gets an error that names neither what failed nor
where.

This is a **message-quality chore, not a bug**. Nothing is lost, nothing is written, and the refusal is
correct — it just does not explain itself. Byte-identical on `main`, so it is pre-existing rather than
introduced by CPE-1718.

## Scope

`crates/server/src/split_join.rs` — the `create_dir_all(out_dir)` call and its `map_err`.

## Acceptance criteria

- [ ] The error names the **path** and describes the situation in terms the user can act on. "Cannot create
      a file" for a directory that is actually a link is three kinds of unhelpful at once.
- [ ] Distinguish the cases if it is cheap: the name is held by a **dangling link** (the measured case),
      by a **file**, or the parent is unwritable. They need different advice.
- [ ] A test pins the message, and breaking it turns a **distinct** test red, per the Evidence Rules in
      `Ticketing/wiki.md`.
- [ ] **Do not add a guard here.** CPE-1718 deliberately left this site alone and the reasoning holds:
      `create_dir_all` cannot truncate and cannot delete, and a live directory link is an ordinary way to
      name a drive — refusing would break a real use. This ticket is about the wording of an existing,
      correct refusal.
- [ ] Confirm the Linux/macOS behaviour, which nobody has measured. If `create_dir_all` there *does* walk
      through a dangling link, the original ticket was right on those platforms and this rewrite needs
      re-opening — say so loudly rather than assuming Windows generalises.

## Notes

Filed during CPE-1718 (2026-08-14), rewritten the same day by the Foreman after that PR's UAT falsified its
premise. The pattern is the one this sprint keeps hitting: **a correct observation, generalised one step
past what was measured.** It is recorded here rather than quietly corrected because the failure mode is
worth more than the ticket.

Related: **CPE-1718** (which guarded the module's four destructive primitives and deliberately left this
fifth alone), **CPE-1719** (`fs::write` follows a link and writes *through* it — the class this was assumed
to belong to), **CPE-1687** (refusals that name the wrong thing).

## Work Log (2026-08-17/18)

### Refuse vs. accept — accept, unchanged

Read `split_join.rs`'s module doc (the link-policy table, row 5) and CPE-1718/CPE-1744 before touching
anything. **A symlinked `out_dir` is accepted, not refused, and this ticket does not change that.**
`create_dir_all` cannot truncate or delete, so for a **live** directory link the worst outcome is the
output landing in a directory the user did not literally name — a surprise, not a loss — and a live
directory link is an ordinary way to point at a USB stick or another drive; refusing it would break a
real use. `create_dir_all` already succeeds and redirects through a live link today, and nothing here
touches that path. The **dangling** case is the one that actually reaches an error, and CPE-1718's UAT
already proved `create_dir_all` refuses there too (measured, not assumed) — nothing is created, nothing
is written, the link survives. So per the ticket's own "do not add a guard" acceptance criterion: the
refusal (such as it is — dangling links only) was already correct. Only the wording was broken.

### The fix

Added `out_dir_error(out_dir: &Path, e: std::io::Error) -> String` in `crates/server/src/split_join.rs`
(private fn, just above `split_file`) and pointed the existing `create_dir_all(out_dir).map_err(...)` at
it instead of `|e| e.to_string()`. It runs **after** `create_dir_all` has already failed — one extra
`std::fs::symlink_metadata(out_dir)` read to classify what's occupying the name, never a pre-check that
changes behaviour:

- a **link** at `out_dir` (dangling — the measured case — or pointing at a non-directory): names the link,
  says it leads nowhere, and does not repeat the OS's "already exists" as if it meant a literal file.
- an ordinary **file** already at that name: says "file", not "link" — different advice (delete the file
  vs. repair/remove the link).
- anything else, most commonly an **unwritable parent**: path plus the OS's own message, via
  `e.kind() == PermissionDenied` for the friendlier wording, falling back to `{path}: {e}` otherwise.

Mirrors the convention CPE-1744 already established for the identical class of site in `archive.rs`
(`extraction_dest_error`, the four `dest`-level `create_dir_all` calls) — same one-extra-stat idea, same
"don't touch the refusal, only the message" scope.

### Before / after (measured on this machine, Windows 11)

Dangling link at `out_dir`:
- Before: `Cannot create a file when that file already exists. (os error 183)`
- After: `"C:\...\outs" is a link, and it leads nowhere — the output folder cannot be created there. The
  OS reports "Cannot create a file when that file already exists. (os error 183)", which sends you to
  delete a file that does not exist; what exists at that name is the link. Repair the link's target, or
  split into a different folder`

Ordinary file already at `out_dir`'s name:
- Before: `Cannot create a file when that file already exists. (os error 183)`
- After: `"C:\...\outs" already exists as a file, not a folder — split into a different folder, or remove
  the file at that name`

### Linux/macOS confirmation

Not independently measured here — this machine is Windows-only (no cargo toolchain access to a Linux/
macOS box in this session). `mkdir`'s refusal to walk through the final symlink component (the reason a
dangling link fails at all, rather than being silently walked through) is standard POSIX behaviour, not
a Windows quirk, so the module doc's existing caveat ("not independently measured on Linux or macOS") is
left in place and now says explicitly that CI's three-OS matrix is the actual confirmation mechanism.
`cpe_1729_dangling_out_dir_names_the_path_and_the_link_not_just_the_os_text` uses
`crate::fsutil::make_dangling_link`, which is cross-platform (symlink on Unix, junction fallback on
Windows), so it will run for real on all three CI legs rather than skipping there. **If CI's Linux or
macOS leg reports this test skipped rather than passing, that is new information this ticket did not
have and should be flagged, not silently accepted.**

### Sibling sites found, NOT fixed (out of this ticket's scope — reported per the run instructions)

Same anonymous-message shape (`.map_err(|e| e.to_string())` or equivalent, discarding the path) at
`create_dir_all`/`create_dir` sites for a **user-named** output location:

- `crates/server/src/backup.rs:115` — `copy_one_verified`'s `create_dir_all(parent).map_err(|e|
  e.to_string())`, parent of a user-chosen backup destination file.
- `crates/server/src/batch_execute.rs:457` — `execute_one`'s `create_dir_all(parent).map_err(|e|
  format!("could not create output dir: {e}"))?` — has the cause, still missing the path.

Not touched: `archive.rs`'s **row-18** per-entry `create_dir_all(&out)`/`create_dir_all(parent)` calls
(lines ~1120, 1142, 1716, 1735) also use `e.to_string()`, but those are archive-controlled paths already
inside a confined `dest`, not a user-named top-level directory — a different risk/audience than this
ticket's `out_dir`, so left off the list above as arguably out of the same class; noted here so the next
reader doesn't have to re-derive that they were seen and set aside on purpose.

### Tests (in `crates/server/src/split_join.rs`, `mod tests`)

All three assert on the **message text** (path + cause named), not just `Result::is_err()`, per the run
instructions. A local `RemoveOnDrop` guard (same idiom as `archive.rs`'s CPE-1758 `RemoveOnDrop`, not
previously present in this file despite the run brief's assumption — the closest existing idiom here was
manual `remove_dir_all` at the end of each test, which does leak on panic) is armed before any assertion.

1. `cpe_1729_dangling_out_dir_names_the_path_and_the_link_not_just_the_os_text` — dangling link at
   `out_dir` via `make_dangling_link`; asserts the message contains the path and the word "link", and
   that the link itself still exists afterward. Loudly skips (via `skip_notice!`) if this machine can't
   stage a link at all.
2. `cpe_1729_file_at_out_dir_says_file_not_link` — plain file at `out_dir`'s name; asserts the message
   contains the path and "file", and does NOT contain "link".
3. `cpe_1729_unwritable_parent_names_the_path_too` — parent directory traversal denied via the existing
   `crate::fsutil::deny_dir_traversal`/`undo_deny_dir_traversal` pair (not hand-rolled — reused per the
   run instructions); asserts the message contains the path. **Skipped on this Windows machine** —
   `deny_dir_traversal`'s own doc already establishes this is Unix-only in practice
   (`SeChangeNotifyPrivilege` makes Windows `fs::metadata`-based calls, which is what `create_dir_all`
   uses, ignore a directory-level deny), so the skip here is expected and legitimate, not a gap this
   ticket introduced. CI's Linux/macOS legs are what actually exercise this test.

Red-proof — reverted line 203 to `.map_err(|e| e.to_string())`, ran just the CPE-1729 tests:

```
running 3 tests
test split_join::tests::cpe_1729_file_at_out_dir_says_file_not_link ... FAILED
test split_join::tests::cpe_1729_dangling_out_dir_names_the_path_and_the_link_not_just_the_os_text ... FAILED
[CPE-1729] SKIPPED the unwritable-parent wording leg: could not stage a denied parent at ...\locked on
this machine/account ... (legitimate, expected skip on Windows)
test split_join::tests::cpe_1729_unwritable_parent_names_the_path_too ... ok

---- cpe_1729_file_at_out_dir_says_file_not_link ----
panicked: the message must name the path the user pointed the split at: Cannot create a file when that
file already exists. (os error 183)

---- cpe_1729_dangling_out_dir_names_the_path_and_the_link_not_just_the_os_text ----
panicked: the message must name the path the user pointed the split at: Cannot create a file when that
file already exists. (os error 183)

test result: FAILED. 1 passed; 2 failed; 0 ignored
```

Restored the fix afterward; re-ran green (27/27 in `split_join`, `cpe_1729_unwritable_parent_names_the_path_too`
still legitimately skipped on Windows).

### Full verification (Windows, this machine)

- `cargo test -p cpe-server` (default features): 2205 lib tests + all integration suites, 0 failed, 4 ignored.
- `cargo test -p cpe-server --features index`: 2253 lib tests + integration suites; one FIRST-run failure,
  `sample_fixtures::zip_lists_real_tree_and_extracts_inner_file`, unrelated to this change (not in
  `split_join.rs`, passed in isolation, and passed on an immediate full re-run — pre-existing flake, not
  introduced here).
- `cargo clippy -p cpe-server --all-targets -- -D warnings`: clean.
- `cargo clippy -p cpe-server --all-targets --features index -- -D warnings`: clean.

### Assumptions / unverified

- Linux/macOS behaviour is inferred from POSIX `mkdir` semantics and CPE-1718's own doc caveat, not
  independently measured in this session — CI is the real confirmation, flagged above.
- The `unwritable-parent` test's positive assertion (message actually names the path when
  `PermissionDenied`) is unverified on any platform in this session, since it skipped here — will only be
  proven once CI's Unix legs run it.
- Sibling sites in `backup.rs`/`batch_execute.rs` are reported, not fixed — deliberately out of this
  ticket's stated scope (`crates/server/src/split_join.rs`).

## Work Log addendum (2026-08-18) — Reviewer/UAT finding, fixed

**Reviewer approved and UAT passed the rest** (fallback ordering, the path named in all four arms, the
loud Windows skip on `deny_dir_traversal`, refuse-vs-accept genuinely unchanged — a live directory link
still works end to end with parts landing through it). **One finding, squarely in scope:**

`out_dir_error`'s symlink arm matched on `meta.file_type().is_symlink()` alone and unconditionally used
the dangling-link wording. UAT measured a **live** symlink pointing at an existing *file*:

```
setup:  out_dir = symlink -> a real file
before: "<path>" is a link, and it leads nowhere — the output folder cannot be created there...
        Repair the link's target, or split into a different folder
```

That is false — the link resolves fine, it just does not resolve to a folder. Telling the user to
"repair the link's target" points them at a target that is not broken.

**Fix:** split the symlink arm on a second, *resolving* `std::fs::metadata(out_dir)` read:

- resolve fails with `NotFound` → the existing dangling wording (unchanged);
- resolve succeeds but is not a directory (the new case) → `"<path>" is a link that points at a file, not
  a folder — the output folder cannot be created there. Repoint the link at a folder, or split into a
  different folder`;
- resolve succeeds as a directory (should be unreachable — `create_dir_all` would already have succeeded
  through it) or anything else this second read cannot classify → falls through to the same honest
  generic `path: {e}` / permission-denied wording used for every other unclassified cause, rather than
  asserting a third unverified claim.

**Before / after, live link → file** (measured on this machine, Windows 11):
- Before: `"C:\...\outs" is a link, and it leads nowhere — the output folder cannot be created there. The
  OS reports "Cannot create a file when that file already exists. (os error 183)", which sends you to
  delete a file that does not exist; what exists at that name is the link. Repair the link's target, or
  split into a different folder`
- After: `"C:\...\outs" is a link that points at a file, not a folder — the output folder cannot be
  created there. Repoint the link at a folder, or split into a different folder`

**New test:** `cpe_1729_live_link_to_a_file_says_file_not_leads_nowhere` — stages a live symlink at
`out_dir` pointing at a real file (bare `symlink_file`/`symlink`, no pre-existing cross-platform helper
covers a *live file* link the way `make_dir_link` covers a live *directory* link and `make_dangling_link`
covers dangling — noted in the code as a possible future `fsutil` extraction, not done here since one
call site doesn't earn a new shared helper). Ran for real on this machine (no skip — file symlinks work
here without extra privilege). Asserts the message names the path, says "file", and does **not** contain
"leads nowhere" or "Repair the link's target".

**Red-proof** — reverted the symlink arm to the single-branch pre-review version, ran the new test alone:

```
thread '...cpe_1729_live_link_to_a_file_says_file_not_leads_nowhere' panicked at src\split_join.rs:1085:9:
the link resolves fine — it must NOT be told it leads nowhere or to repair a target that isn't broken,
the bug the Reviewer caught in PR #930: "C:\...\outs" is a link, and it leads nowhere — the output folder
cannot be created there. The OS reports "Cannot create a file when that file already exists. (os error
183)", which sends you to delete a file that does not exist; what exists at that name is the link. Repair
the link's target, or split into a different folder
test result: FAILED. 0 passed; 1 failed
```

Restored the fix; re-ran green (29/29 in `split_join`, including the new test and the still-legitimate
Windows skip on the unwritable-parent test).

**Re-verification after the fix, this machine:**
- `cargo test -p cpe-server` (default): 2206 lib tests, 0 failed. One first-run failure in
  `sample_fixtures::zip_lists_real_tree_and_extracts_inner_file` again (same pre-existing flake noted
  above — passed in isolation on immediate re-run, still unrelated to `split_join.rs`).
- `cargo clippy -p cpe-server --all-targets -- -D warnings`: clean.
- `cargo clippy -p cpe-server --all-targets --features index -- -D warnings`: clean.

**25-site tree sweep, read-only-filesystem/PermissionDenied nuance, and every other out-of-scope item the
Reviewer raised are filed as CPE-1777** — not addressed here, per direct instruction to stay in
`split_join.rs` and not expand beyond this one finding.
