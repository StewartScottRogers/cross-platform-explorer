---
id: CPE-1718
title: join_files follows a symlink at the output path — writing through it on success, deleting it on failure
type: bug
priority: Medium
status: Doing
tags: ready
estimate: S
created: 2026-08-13
closed:
---

Related: **CPE-1710** (the `fs::rename`-destructive slots and the `rename_slot_refusal` pairing) and
**CPE-1715** (the name-picking probes). Same root hazard — a probe that *follows* a link standing in front
of an operation that does not — at a third shape: a `File::create` output path rather than a rename
destination or an auto-renamed candidate.

## Problem

Found by the PR #895 (CPE-1710) UAT, 2026-08-13, while re-checking the sites that PR classified as
out-of-class.

`crates/server/src/split_join.rs` (`join_files`, the `clobber_refusal` at :327) guards its `out_path` with
`clobber_refusal` alone. That check **follows links**, so a link at `out_path` whose target does not exist
reads as a free name, and the operation proceeds. Two distinct wrong outcomes, both measured by the UAT:

- **Failure path — the link is deleted.** Any error past the guard (missing part, short part, checksum
  mismatch, I/O error) runs `let _ = std::fs::remove_file(out_path)` to clear the partial output. That
  removes the user's **link**, not a partial file this operation created:

  ```
  [UAT] join_files (failing) -> Err("part 4 missing: ...")
  [UAT] AFTER: the user's link still exists = false
  ```

- **Success path — the bytes land somewhere the user never named.** `join_into` opens `out_path` with
  `File::create`, which **follows** the final component, so the reconstructed file is written **through**
  the link to its target. `Ok(())` is returned and the user is told the join succeeded, while the bytes
  are at a path they did not choose (and whatever was at the link's target is truncated).

CPE-1710 guards exactly this follow-through-link case at `copilot::transfer_entry`'s copy branch and its
comment calls it "a different surprise, equally unasked-for" — then classified this site as safe three
modules over, on the strength of "it precedes `File::create`, not `fs::rename`". The `File::create` is
what makes it *worse*, not safer.

## Scope

`join_files` in `crates/server/src/split_join.rs`. While there, check `split_file`'s two sibling
`clobber_refusal` calls (the manifest path and each numbered part path) — they have the same
`File::create` follow-through-link exposure, and a split writes a whole numbered *series*, so one link in
the output directory is one file written somewhere unnamed per part.

## Acceptance criteria

- [ ] A link at `out_path` — dangling or live — is refused before anything is created or removed, with a
      message that names the link (`fsutil::symlink_slot_refusal`'s wording, or `rename_slot_refusal` if
      the refusal shape fits).
- [ ] The same for `split_file`'s manifest and part paths, or a written-down reason why they differ.
- [ ] A test proves the **failure** path no longer deletes the user's link, asserted on the slot
      (`symlink_metadata(..).is_symlink()`), not on the returned `Result`.
- [ ] A test proves the **success** path no longer writes through the link to a path the user never named.
- [ ] Platform-gated with `cpe_server::fsutil::make_dangling_link` (symlink, junction fallback on Windows)
      and a loud `writeln!(stderr)` skip if a link cannot be created; the Linux and macOS legs assert for
      real.
- [ ] Each guard broken on its own turns a distinct test red, real output pasted in the PR (Evidence
      Rules, `Ticketing/wiki.md`).

## Notes

Filed by the CPE-1710 worker from the PR #895 UAT's findings, as a separate ticket rather than folded into
CPE-1715: that one is about **name-picking** probes (`unique_target`, `resolve_conflict`), whose fix is
"treat a link slot as occupied and pick the next name". This one is a refusal-shaped site whose fix is a
refusal, and it also has a delete-on-failure path the name-picking sites do not.

## Work Log

2026-08-14 — Measured the bug on Windows before touching anything (dangling symlink at `out_path`):
`clobber_refusal = None`, `File::create -> Ok`, **4096 bytes at the link's target**, slot still a link;
and `remove_file` on the link removes **the link**. Both halves of the report reproduce.

2026-08-14 — Decision, via CPE-1716's question (*"am I claiming this name, or editing this file?"*): every
path this module writes is a name the user typed for a file that does not exist yet, so all of them
**claim**, and a link at any of them is **refused** — live or dangling, on the success path and on the
recovery path. The four cases:

| | success path | recovery path |
|---|---|---|
| **live link** | refuse. Already refused pre-fix, but as *"already exists"*, which sends the user to delete a file at a name that holds a link to somewhere else; now says it is a link. | never removed. Unreachable end-to-end (the front guard refuses first), so `remove_partial_output` is tested directly. |
| **dangling link** | refuse. **This was the bug** — read as a free name, bytes written through to the target, `Ok(())` returned. | never removed. **This was the sharper bug** — the link was deleted while the error talked about a missing *part*. |

2026-08-14 — Enumerated every destructive primitive in the module rather than fixing only the reported
one, and recorded the table in the module doc: (1) `join_into`'s `File::create`, (2) `join_files`'
recovery `remove_file`, (3) `split_file`'s per-part `File::create`, (4) `split_file`'s `fs::write` of the
manifest — all fixed; (5) `create_dir_all(out_dir)` — deliberately left, **not destructive** (cannot
truncate, cannot delete) and a live directory link is a legitimate way to name a drive, so it gets its own
argument as **CPE-1729**. There is no temp/staging in this module and `split_file` has no recovery path at
all; both written down so the absence reads as checked rather than overlooked.

2026-08-14 — Built `fsutil::create_slot_refusal` + `classify_create_slot` (the create-shaped sibling of
`rename_slot_refusal`) with the link half **first** — the opposite order — because at a create site a live
link is write-through too, and with its own wording because *"renaming onto a link destroys it"* is a
confident false statement about a site that **follows** the link. Made `stage_exclusive` a public
`create_exclusive` as the atomic belt; measured that `O_CREAT|O_EXCL` refuses a dangling link on Windows
too (`Err(AlreadyExists, os error 80)`, target not created), so the belt is not Unix-only.

2026-08-14 — Guard neutralisation, each guard broken **alone** and restored with `git checkout --`. G1
(the refusal's link arm) reds the classifier test, the join wording test (leaking `The file exists.
(os error 80)`), and the split census — which showed the split writing **all three parts** before failing
at the manifest slot, proving the refusal is load-bearing and not redundant with the exclusive open. G2
(the recovery's link arm) reds exactly the two recovery tests. G3 (`create_new`) reds exactly the
pre-existing `create_new_refuses_a_link_at_the_staging_name…` test. G1+G3 together reproduce the original
data loss: the census gains `rebuilt.bin-target-that-does-not-exist`. Full output in the PR.

2026-08-14 — Fixed two stale statements found in passing: this ticket's own frontmatter `id:` said
CPE-1717 (a different, closed ticket), and `rename_slot_refusal`'s doc credited a
`guards_are_paired_at_every_rename_destructive_site` scan that no longer exists — it was replaced by the
`clippy.toml` ban in round 3, so the comment named an enforcement mechanism the repo does not have.

2026-08-14 — `cargo test` 2143 pass (default) / 2191 pass (`--features index`), `cargo clippy
--all-targets -D warnings` clean in both modes. In-app docs updated (`src/docs/33-split-join.md`).

## Round 2 (Foreman-applied, from the review and UAT)

Two things happened that the round-1 log does not record, and both are the point of the ticket rather than
footnotes to it.

**A design decision argued for twenty lines and pinned by nothing.** `create_slot_refusal` checks the link
question *before* occupancy — the opposite of `rename_slot_refusal` — and the module doc defends that at
length. The reviewer swapped it back and **not one test out of 2143 redded.**

The reason is structural, and it is the interesting half: under **either** ordering a *dangling* link
reaches the link classifier, because `try_exists` answers `Ok(false)`, occupancy returns `Free`, and it
falls through. Every CPE-1718 test staged a dangling link. **The ordering only changes behaviour for a
*live* link, and there was no live-link leg anywhere** — so the single case the whole argument is about was
the single case untested.

Occupancy-first yields `rebuilt.bin: already exists — refusing to overwrite`: exactly what this module's own
table calls out as sending the user to delete a name that actually holds a link elsewhere, and exactly the
failure PR #899's reviewer already measured at the rename site. **The repo had been bitten by this ordering
once already**, and the fix for it was shipping undefended.

Pinned by `a_live_link_at_a_create_slot_is_reported_as_a_link_not_as_already_exists`, gated with
`require_staged` per CPE-1717 so a runner that loses symlink privilege goes red rather than green. The swap
now reds **exactly one test, the one aimed at** — verified independently by the reviewer, including that
the panic carries the occupancy wording, so it fails for the reason it exists for rather than incidentally.

The reviewer's framing of why this mattered is worth keeping: *"I am not asking because I doubt the
decision. The decision is right. I am asking because a confident sentence with nothing behind it is the
failure mode."*

**A follow-up ticket falsified by its own UAT.** CPE-1729 was filed claiming a dangling `out_dir` link would
make `create_dir_all` walk through and write the whole part series somewhere unnamed. Measured twice, two
link shapes:

```
split -> Err("Cannot create a file when that file already exists. (os error 183)")
post: is_link=Ok(true)  missing_dir_created=Ok(false)  missing_census=[]
```

`create_dir_all` tests `is_dir()` — which follows the link and answers `false` for a dangling one — calls
`create_dir`, gets `AlreadyExists` because the **name** is held by the reparse point, and returns. **It
never walks through.** A correct observation generalised one step past its evidence, the shape this sprint
kept hitting, this time in a ticket nobody had worked yet.

CPE-1729 was rewritten around the real residual the original missed — the error discards the path and calls
a directory a "file" — and kept rather than deleted, because *"we thought this was a bug and it is not,
here is the measurement"* is worth more than silence. The reviewer, who had approved the original reasoning,
noted plainly that the falsification came from the UAT and not from the review, and added that the
mechanism is **std-level rather than Windows-specific** (POSIX `mkdir(2)` returns `EEXIST` for an existing
symlink at the final component), so the Linux/macOS acceptance criterion will likely confirm rather than
reopen. The criterion stays: Windows was measured and said so.

**Also applied:** a skip notice on the new live-link leg, which returned silently green on an unprivileged
local machine while its four siblings all announced — CI red is the consequential half, but Evidence Rule 3
calls a visible notice the floor; and a clause scoping the `create_dir_all` prose to the *live* case, since
the sentence above the dangling measurement read as covering both.

**Split out on the reviewer's recommendation**, because they are different kinds of work: the `archive.rs`
sweep is *investigation* (~14 sites, and deciding which destinations are user-named comes first), while
adding `File::create`/`fs::write` to `clippy.toml`'s `disallowed-methods` is a *repo-wide policy decision*
whose allow-list will be long, since `File::create` is legitimate at app-owned paths far more often than
`rename` is. Bundled, the cheap decisive one would wait on the expensive exploratory one.

Final: `cargo test` 2144 default / 2192 `--features index`, clippy `-D warnings` clean in both modes.

**Verdicts, with what each was given against** — because the first version of this line simply read
"Reviewer APPROVE, UAT PASS", and the reviewer caught that it asserted a verdict on code it had not yet
seen and a CI result that did not exist. In a ticket whose whole subject is not asserting things ahead of
their evidence, that was worth naming rather than letting pass.

- **UAT PASS** — round 1, at `f96abf25`.
- **Reviewer CHANGES REQUESTED** at `f96abf25` (the ordering, argued for twenty lines and pinned by
  nothing), then **APPROVE** at `de6fd3a9`, explicitly gated on the `Server crates` jobs — which the next
  commit then discarded and restarted.
- **Reviewer APPROVE re-earned at `df28ef7e`**, re-running the ordering swap, both sabotage legs and both
  feature modes against that head rather than carrying the previous verdict forward.

If the head moves again, this line needs re-earning rather than carrying forward.

- **Reviewer APPROVE at `a2a010e9`** — code proven identical to `df28ef7e` by **tree hash**
  (`crates: 644762d6…` on both), not by eye, so the verdict carried across on proof rather than assumption.
  That distinction is the whole thread this paragraph exists for.
