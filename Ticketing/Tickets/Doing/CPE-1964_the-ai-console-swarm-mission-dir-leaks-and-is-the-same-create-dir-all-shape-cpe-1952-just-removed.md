---
id: CPE-1964
title: the AI Console's `cpe-swarm-<millis>` mission directory leaks — 55 on one machine — and is the same predictable-`create_dir_all` shape CPE-1952 just removed
type: bug
priority: Medium
status: In Progress
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

Found by PR #1075's Reviewer while confirming CPE-1952's field evidence. CPE-1952 deferred this site as
a table row with a stated reason; the Reviewer's measurement says it deserves a ticket instead, and the
argument is hard to dispute.

**On this machine's real `%TEMP%`, right now:**

| directory shape | count | note |
|---|---|---|
| `cpe-catalog-stage-<pid>` | **9** | eight from 2026-07-14→19, **plus one from today at 18:29** with a real 155-byte `index.json` — the shipped app was still leaking as the fix was being reviewed. **Closed by CPE-1952** (PR #1075): nothing is written at all now, so no exit path can leak. |
| **`cpe-swarm-<millis>`** | **55** | **still leaking, ~6× harder than the site that got the ticket** |
| `cpe-ai-console-catalog` | 0 | fallback branch, never reached on this machine |
| `cpe-sidecar-storage` | 0 | same |

All are plain directories, no reparse points — nobody's repro left a hazard behind.

**Of the three residuals CPE-1952 deferred, this is the only one with live evidence, and it is not the
one that ticket foregrounds.** The two `temp_dir()` fallbacks it discussed at length show **zero**
on-disk instances; the one it listed in a table is the one filling the disk.

## Why it is the same defect, not merely similar

`cpe-swarm-<millis>` is a **predictable path in a shared namespace**, created with **`create_dir_all`**
— the exact primitive CPE-1952 established will follow a junction/symlink into an attacker-chosen
directory. `<millis>` is a timestamp, so it is guessable within a narrow window rather than random.

Two things make it *worse* than the catalog case in one respect and better in another:

- **Worse:** it leaks constantly (55 vs 9), so an attacker watching `%TEMP%` has abundant signal about
  when and how the app creates these, and the leaked names publish the timestamp pattern outright.
- **Better:** the content is mission scaffolding rather than bytes off the wire, so the escape
  primitive is weaker than the pre-fix catalog bug's *"unverified download written to a location the
  attacker chose."*

**Threat-model caveat, from the same Reviewer (F2 on #1075) — do not over-inherit CPE-1952's framing.**
On **Windows**, `std::env::temp_dir()` resolves to the **per-user** `%LOCALAPPDATA%\Temp`, not a
machine-shared namespace, so the Windows attack needs a same-user process. *"Predictable path in a
shared namespace"* is fully true of **Unix `/tmp`**. Both halves are real; state them separately.

## Acceptance criteria

- [ ] **Reproduce the escape before fixing**, on both platforms, and **assert on the filesystem** —
      where the bytes actually landed — never on a returned verdict. Plant a junction (Windows,
      `junction::create`, no admin needed) / symlink (Unix, on a **real ext4** path, not `/mnt/z`).
- [ ] **Keep a sensitivity control**: with the fix disabled the escape must happen, as a normal CI test
      on all three OSes — **not `#[ignore]`d**. PR #1075's
      `the_old_staging_primitive_writes_through_a_planted_link` is the model. Note its Reviewer's F3:
      a `Scene::planted()` that silently returns is a **green** test, because `eprintln!` is captured
      by default — `panic!` on the platforms where the link must work.
- [ ] **Plant at the REAL path**, not a stand-in inside a `tempfile::tempdir()`. A stand-in is
      unreachable by any regression and every assertion about it is unfalsifiable (CPE-1929).
- [ ] **Fix the leak as well as the escape.** These are two defects sharing a site: the directory is
      created in a place an attacker can pre-empt, *and* it is never cleaned up on the error paths.
      **Prefer the shape CPE-1952 chose** — if the mission scaffolding need not exist on disk, deleting
      the directory beats defending it, and it closes both defects at once. If it genuinely must exist,
      say why, and then the leak needs its own answer (RAII guard, or a sweep with a stated retention).
- [ ] **Decide what to do with the 55 existing directories.** They are on a real user's machine. A
      startup sweep is the obvious answer and is also a new destructive operation over a shared
      namespace — argue it, and make it refuse anything that is not plainly ours.
- [ ] **Re-derive the `temp_dir()` enumeration** and re-check the two fallbacks CPE-1952 left
      (`catalog_dir`'s and sidecar-storage's). Use the **corrected** recipe: `git ls-files '*.rs'`,
      minus `tests/`, minus everything after each file's first **column-0** `#[cfg(test)]`. PR #1075's
      stated recipe said "first `#[cfg(test)]`", which matches indented in-function attributes and doc
      comments and amputates production code — run literally it yields **10** sites instead of **15**,
      dropping **both swarm sites**. A derivation nobody else can re-run is halfway back to recall
      (CPE-1932).

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1075's Reviewer (**APPROVE**, F6), which measured the
counts rather than accepting the deferral. It made the case plainly: *"the residual this ticket
deferred is leaking roughly six times harder than the one it fixed… it should not sit as a table row."*

Related: **CPE-1952** (the catalog staging fix, PR #1075 — the model for the fix shape and the test
shape), **CPE-1937** / **CPE-1929** (the containment and shadowed-guard families), **CPE-1932**
(enumerate, don't recall — and the corrected recipe above).

## ID note 2026-08-27

Filed as CPE-1963 and renumbered to **CPE-1964** within the hour: PR #1070's round-2 worker filed its
own **CPE-1963** (the staging rename's source being an enumerable attacker-writable path) at almost
the same moment, from a worktree that could not see this one. Theirs is referenced from `fsutil.rs`
comments, its PR body and CPE-1961; this one was referenced only by itself, so this is the cheaper
side to move. Standing hazard: two agents allocating the next free ID from different checkouts will
collide, and the tell is that neither can see the other's file.

## Work Log — 2026-08-27

### Reproduction, before the fix, on both platforms, asserted on the filesystem

`sidecar/ai-console/tests/swarm_mission_dir_containment.rs` plants a real directory link (junction on
Windows via `junction::create`, `symlink(2)` on Unix) at a real `<temp>/cpe-swarm-<n>` path and runs the
pre-fix primitive verbatim.

* **Windows** (`%LOCALAPPDATA%\Temp`): the control's first run, with a deliberately wrong expectation,
  printed the escape as an on-disk listing — *"the mission scaffolding landed inside the attacker's
  directory at `C:\Users\Stewart Rogers\AppData\Local\Temp\.tmpeksRPN\victim`"*, `left:
  ["mcp-claude-builder1.json", "members.json"]`.
* **Linux, real ext4** (`/dev/sdd`, with `TMPDIR` overridden to `$HOME/cpe-1964-tmp` — WSL's `/tmp` is
  tmpfs): a standalone repro of the same two primitives printed
  `victim after create_dir_all = ["mcp-claude-builder1.json", "members.json"]`,
  `escaped bytes at .../cpe-1964-victim/members.json: "MISSION-ROSTER"`, then
  `create_dir on the same planted path -> AlreadyExists (File exists (os error 17))`,
  `victim after create_dir = []`. The committed suite also runs green there (4/4).

### The fix

New module `sidecar/ai-console/src/swarm_mission_dir.rs`.

CPE-1952's shape — *delete the directory* — is **unavailable** here, and the module says why at the
top: the mission directory **is** the swarm's shared substrate. Each agent spawns its own
`ai-console --swarm-mcp --dir <mission>` host in a separate process, and they coordinate through
`members.json` / `mailbox.jsonl` / `memory/*.md` / `mcp-<agent>.json` / `task-*.txt`. So the two
defects get two answers:

* **Escape** — `create_mission_dir_at` uses `std::fs::create_dir` (one `mkdir(2)` /
  `CreateDirectoryW`), which fails `AlreadyExists` on *anything* already at the path including a
  reparse point, atomically with the create. The name became 32 hex characters of `RandomState`
  entropy instead of `now_millis()`. **No `exists()` pre-check** — that would be a shadowed guard
  (CPE-1929), and it is refused at the site with the reason.
* **Leak** — `sweep_stale_mission_dirs`, run once at console startup on a background thread with a
  **24-hour retention**. An RAII guard on the mission thread was designed first and rejected: the live
  coordination panel (CPE-592) reads that directory, and the moment you most want it is right after
  the mission ends.

### The 55 existing directories

Measured on this machine: **55** `cpe-swarm-*`, **0** reparse points, **54** carrying `members.json`,
**55** older than 24h (17 Jul to 26 Jul 2026). The sweep removes **54** and leaves **1** — the one
without a roster, which is `console.rs`'s own unit-test leftover. Every condition fails closed
(CPE-1972): wrong name, not a plain directory, no roster, unreadable mtime, future mtime are all
skips, never deletes. Another user's directory under `/tmp`'s sticky bit fails at `remove_dir_all` and
is counted, not hidden.

### CPE-1929 pairs, measured on Windows against `swarm_mission_dir_containment`

| refusal | disabled | predicate made to lie | verdict |
|---|---|---|---|
| `create_dir`'s `AlreadyExists` | `create_dir_all` = **RED** (1 failed) | `remove_dir_all` first = **RED** (1 failed) | live |
| sweep: `!meta.is_dir()` on a `symlink_metadata` | `if false && ...` = **RED** | `metadata()`, i.e. the following stat = **RED** | live |
| sweep: `meta.file_type().is_symlink()` | (with the above) RED | forced to lie = **GREEN** (4 passed) | **shadowed, deleted** |

The `is_symlink()` arm was written, its pair run, and then **deleted**: `!meta.is_dir()` answers the
same fact first on both platforms, because std reports a name-surrogate reparse point as a symlink and
never as a directory. Both numbers are recorded at the site.

### Re-derived `temp_dir()` enumeration

`src/lib/tempDirSites.test.ts` carries the corrected recipe as code: `git ls-files '*.rs'`, minus a
`tests/` path segment, comments stripped with the shared `stripRustComments`, then cut at the first
**column-0** `#[cfg(test)]`. Measured at `origin/main` (e275808e): **naive 10, corrected 14**. The −4 is
a net of two errors: the naive rule **misses five** real sites — `console.rs:733` and `:796`, i.e.
**both swarm sites**, plus `crates/server/src/fsutil.rs:5578`, `src-tauri/src/lib.rs:11904` and
`:14416` — and **adds one spurious** hit, `crates/server/src/archive.rs:2088`, a `///` doc-comment
line. The ticket's figure was 15; 14 is the honest number, and the file's header carried a stale 15
into round 1 (fixed in round 2, re-derived rather than copied).

A new finding worth its own ticket (**CPE-1975**): `session_diag.rs:33`, `session_supervisor.rs:151`
and `sidecar/host/src/reaper.rs:61` all build the **fixed** path `<temp>/cpe-ai-console/...`, and it
holds the session-daemon **port file** — a control channel, not just data. Two of the three create it
with `create_dir_all` (`session_diag.rs:52`, `session_supervisor.rs:144`); `reaper.rs` does **not** —
it only reads (`port_file.exists()`) and deletes (`remove_file`, `:79`). Corrected in round 2, in this
log and in CPE-1975 itself.

CPE-1952's two fallbacks re-checked and unchanged: `catalog_dir`'s `cpe-ai-console-catalog`
(`src-tauri/src/lib.rs:10155`) and `cpe-sidecar-storage` (`:11899`), both reached only when
`app_data_dir()` fails, both still zero on disk.

## Work Log — 2026-08-27, round 2 (review APPROVE, four fixes)

The reviewer approved and returned four small corrections. Two were numbers that had gone stale, two
were real defects in the sweep. All four are in; nothing from round 1's verification needed redoing.

### The two numbers

* **`tempDirSites.test.ts`'s header said "corrected recipe finds 15"** — in the one file that exists
  because a miscounted enumeration shipped. Re-derived here rather than copied from the PR body: both
  recipes were re-run against `origin/main` (e275808e) via `git ls-tree` + `git show`, giving **naive
  10, corrected 14**, and the header now states the −4 as what it actually is (five real sites missed,
  one doc-comment hit added) instead of "the four it dropped".
* **The residual `create_dir_all` enumeration was short by one.** It named `write_members`
  (`swarm_mcp_server.rs:195`), `seed_kickoff` (`:207`) and `write_mcp_config` (`swarm_plan.rs:139`);
  **`seed_memory` (`swarm_mcp_server.rs:237`) is a fourth**, called one line after `seed_kickoff` in
  `handle_swarm_run`. The **bound is unchanged** and was correctly stated: all four run *after* the
  hardened create, `write_members` is the first of them, so the residual race is still only
  create → `write_members`. The list now lives in the module header (round 1 had it only in the PR
  body, where it would have evaporated) with the `rg` that produces it and an explicit note that it is
  a point-in-time enumeration nothing tests — the bound is the load-bearing half.

### F3 — the sweep is a detached thread, so a torn delete could be permanent

The reviewer's find, and the sharper of the two: the sweep is `std::thread::spawn`ed and never joined,
so a console exiting mid-`remove_dir_all` leaves a **half-deleted** mission directory that may have
lost `members.json` — which condition 3 then refuses **forever**. The cleanup would have been
manufacturing more of exactly the litter it exists to remove (the one directory it already cannot
reclaim is rosterless).

**Decision: delete the marker last, not join on shutdown.** Joining narrows the window; ordering
closes it. Joining also loses to `taskkill /f` / `SIGKILL` / power loss, and it would put an unbounded
walk of `%TEMP%` (2,127 reparse points on this machine — CPE-1974) into the console's exit path. So
`remove_mission_dir` removes every other entry first, then the roster, then the now-empty directory;
any torn state still carries the ownership evidence and the next startup sweep finishes it.

Removing children individually meant handling nested links directly rather than delegating the whole
tree to std, so: a real subdirectory still goes to `remove_dir_all`, and a link goes to `remove_link`,
which on Windows picks `remove_dir` for a directory reparse point off the link's **own** attributes
(`symlink_metadata`, never following).

**CPE-1929 pair on the new ordering**, `cargo test --locked --lib`, `Compiling ai-console` confirmed in
both runs (a `/mnt/z` touch does not reliably force a rebuild — the reviewer's first WSL sabotage came
back falsely green for exactly that reason):

| sabotage | result |
|---|---|
| refusal disabled (`if false && …` on the marker skip — one-pass delete) | **RED**, 388 passed / **3 failed** |
| predicate made to lie (skip compares against `"mailbox.jsonl"`) | **RED**, 388 passed / **3 failed** |

### F4 — a count that read as something it was not

`SweepReport.skipped` counted **every** `%TEMP%` entry of any kind, so `"left {skipped}"` would have
printed thousands and read as thousands of mission directories. Narrowed: `skipped` now counts only
entries whose **name** matched `cpe-swarm-<alnum>` and which a later condition then refused; a
non-matching name is not a mission directory the sweep considered, so it is not counted. The log line
is reworded to `removed N, kept M cpe-swarm-* (in retention or not ours), could not remove K`, which
cannot be misread either way. `sweep_refuses_every_name_that_is_not_exactly_ours` now asserts
`skipped == 0` and gains a genuine mission alongside the five bogus names, so "removed nothing" still
cannot pass by the loop never running.

### A claim turned into a test

The module header claimed std would not recurse through a reparse point nested *inside* a genuine
mission directory. The reviewer verified that by hand on NTFS and ext4; round 2 **commits it** rather
than citing a review nobody can re-run (CLAUDE.md: derive provenance, don't claim it).
`the_sweep_does_not_walk_a_link_nested_inside_a_real_mission_directory` plants two links inside a real
stale mission — one directly inside (which exercises the new `remove_link`), one a level deeper inside
a real subdirectory (which exercises std's `remove_dir_all`) — each pointing at an attacker directory
holding `secret.txt`, runs the real sweep, and asserts the mission is removed while both secrets
survive byte-for-byte. Sensitivity control: a `canonicalize()` inserted before the delete, i.e. the
removal made to follow, → **RED**. This is the case round 1 never covered, because every earlier test
stops at the top of the tree where condition 2 refuses and the delete never runs.

### Not taken in this PR

* **F2 → CPE-1976** (filed): the **read** side is unhardened — `handle_swarm_activity` does
  `temp_dir().join(mission)` with no `symlink_metadata` check. Pre-existing; this PR widened the id
  space digits→alnum. Deliberately not widened into here.
* **F9 → CPE-1974**: the 9 pre-CPE-1952 `cpe-catalog-stage-*` directories, and the 2,127 reparse
  points, still on disk with nothing sweeping them.
* **F6**: `random_suffix()` uses `RandomState`, caveated at the site as not a CSPRNG. `getrandom` is
  already in the lock tree via `ed25519-dalek` if the name is ever asked to be load-bearing; it is not
  — the exclusive create is.

### Gates (round 2, Windows)

`cargo test --locked` (ai-console) **413 passed / 0 failed / 2 ignored** (round 1: 412 — +1 unit test
for the torn delete); containment suite **5 passed** (round 1: 4 — +1 nested-link test);
`cargo clippy --all-targets --locked -D warnings` clean.
