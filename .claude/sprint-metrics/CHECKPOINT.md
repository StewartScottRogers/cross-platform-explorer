# Sprint checkpoint — 2026-08-14 18:00 MST (reboot)

**Batches: 20 of 40.** Counter at `.claude/sprint-metrics/BATCH-COUNTER` (`completed: 20`).

Nothing is unpushed. `main` is clean at `32d50ac7`; every merged ticket is closed to
`Ticketing/Tickets/Done/2026/Q3/August/Week-33/`.

## To resume

Start a fresh session and say **"resume the batched sprint"**. Read this file first.

## Merged this session (batches 16–20)

| Batch | Ticket | PR | What |
|---|---|---|---|
| 16 | CPE-1718 | #901 | `join_files` refused a link at every split/join output slot |
| 17 | CPE-1726 | #902 | WebDAV `MOVE` onto the served root — 5 rounds, `same_place` |
| 18 | CPE-1725 | #904 | one answer for a dangling link on both save paths |
| 19 | CPE-1727 | #903 | S3 delete for a GetObject credential + `start-after` belt — 6 rounds |
| 20 | CPE-1731 | #905 | FTP/SFTP rig rename-onto-root + `RMD`/`rmdir` empty-only |

## In flight at reboot — both will need re-dispatching

**PR #906 — CPE-1733 (`archive.rs` create/write sweep).** Branch
`cpe-1733-archive-create-sweep`, head `f68e7fcb` **pushed**. Reviewer **REQUEST CHANGES** (3
findings), UAT **BLOCKER** (6 findings); the worker was mid-round on all nine when the session
ended. Its worktree is `.claude/worktrees/cpe1733`. **Re-dispatch a worker** pointing at the
existing branch — do not start over, `f68e7fcb` already carries the enumeration and 11 guards.

The nine open findings, in one list:

1. Rows 1–5's *"unreachable by the hazard"* is false — `create_dir_all` silently accepts a
   pre-existing directory, so a link planted at the leaf is written through. What protects them is
   `%TEMP%` being per-user **on Windows**, a platform fact. On Linux `/tmp` is world-writable,
   `<pid>` is public, `<seq>` restarts at 0, nothing cleans up (measured: **1,054,930 leftover dirs;
   13% of fresh processes collide with an existing `<pid>-0`**), and the leaf name is
   archive-controlled. CWE-377/CWE-59. Not claiming rows 2–5 need guards — claiming the wording is
   wider than the evidence.
2. *"`guarded_join` does not need to be added"* is wider than its search. True for **traversal**;
   but `guarded_join` also carries CPE-1709's `local_safe_segment`, which `entry_name_is_safe`
   lacks — `entry_name_is_safe("file:stream") == true`, so bytes vanish into an NTFS alternate data
   stream leaving no visible file.
3. The rows 15/16 guard is **leaf-only**, and the `create_dir_all(parent)` above it follows a
   directory symlink — `entry_name_is_safe("sub/leaf.txt")` is true, so an entry escapes `dest`
   through a junction (no privilege needed). Row 17's *"no measured hazard"* overstates CPE-1729:
   that measured `create_dir_all` is not **destructive**; a live directory link **redirects**.
4. **F1 — the unpinned-gap note is false for 2 of the 3 paths it names.** tar (one-shot and
   streamed) **unlinks the symlink and writes a regular file** — victim safe, *link destroyed*,
   recorded nowhere. One-shot zip **aborts the whole extraction** (`Err("invalid Zip archive:
   Invalid symlink target path")`, nothing extracted). Only **7z** follows, as claimed.
5. **F2** — `src/docs/explorer-archives.md` says an entry landing on a link is *"skipped and the
   rest still extracts"*. True for streamed, false for one-shot. `extract_archive` is a registered
   Tauri command with **no current Svelte caller**, so API/doc inconsistency, not a live regression.
6. **F3** — the 7z gap is **live on the path the UI uses** (`start_archive_extract` →
   `extract_archive_streamed` → `extract_7z_stream` → `sevenz_rust::default_entry_extract_fn`,
   `archive.rs:1212`). Returns `Ok`, bytes land where nobody named. **Needs its own ticket.**
7. **F5** — row 17's rationale is inconsistent with row 7's (which got a guard purely because
   `AlreadyExists` stringifies misleadingly). Also *"does nothing at all"* is wrong: it **errors**,
   and the whole extraction fails with that wording.
8. **F6** — row 15 swallows `classify_create_slot`'s `Err` arm (*"could not check… refusing to
   guess"*) identically to a confirmed link, drops the entry silently, returns `Ok`.
9. Non-blocking: 4 un-rowed `create_dir_all` sites; two figures missing their platform boundary
   (`archive.rs:250-253` and the `every_guarded_row_…` doc); rows 15/16 have no live-link leg.

Ticket IDs **1734–1743 are taken.** Allocate against `main` after a `git pull`, verified across all
branches.

**CPE-1730 (protocol rig containment).** Branch `cpe-1730-rig-containment`, worktree
`.claude/worktrees/cpe-1730`, head **`ccdc27d8` — pushed**, no PR yet.

Two commits, and **neither is trustworthy yet**: `7d592bd1` confines `cpe-ftp`'s rig resolver to its
served root; `ccdc27d8` is a mid-edit snapshot of the SFTP call sites, committed only so the work
survived the reboot. **Not reviewed, not tested, not verified — read the diff before trusting any of
it.** Re-dispatch a worker onto that branch and have it start by checking what is actually there.

Its brief: three escape shapes, all measured —
(a) `..`-shaped (`/../<sibling>` moves the **served root itself** out and answers `250 Renamed` /
`Ok(())`, both rigs, both platforms); (b) **absolute** destinations, since `Path::join` discards the
base; (c) through a **symlinked intermediate directory**, needing neither of the first two. The task
is *containment*, not equality — `same_place` answers the wrong question, and lexical `..` popping
is unsound in the containment direction in a way it is not for equality.

## Standing lessons this sprint keeps re-proving

- **A red that is not the red you aimed at proves nothing** — a test can be saved by an errno
  rather than by the guard. Pin a *distinctive* refusal.
- **Assert the effect before unwrapping the `Result`**, or the assertion naming the damage is
  unreachable when the guard fails by returning `Ok` — which is how these bugs behave.
- **An assertion can discriminate by luck** — one matched a marker anywhere in a string built partly
  from caller-supplied paths, so a path could forge it.
- **A lenient test double can *conceal* a defect in the code under test**, not merely permit one
  (CPE-1741, found by making the rigs honest).
- **"No test" and "no record" are different questions.** Ask what holds *each* way a known defect
  could bite.
- CI: select the run by **head sha**, require `conclusion == "success"` by **equality**. A
  `cancelled` run and a **null** conclusion both read as "not failed" to a careless check; every run
  on one branch today ended cancelled, superseded by the next push.
