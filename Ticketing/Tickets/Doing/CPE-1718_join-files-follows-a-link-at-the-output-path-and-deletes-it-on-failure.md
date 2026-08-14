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
