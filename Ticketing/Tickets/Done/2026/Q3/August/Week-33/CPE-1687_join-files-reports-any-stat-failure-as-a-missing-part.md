---
id: CPE-1687
title: join_files tells you a part is "missing" when the part is right there and merely unreadable
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-12
closed: 2026-08-12
---

## Problem

The same bug CPE-1678 just fixed in `text_stats`, in a second place — found by the independent reviewer on
PR #865, who re-ran the sweep instead of accepting the author's "there are no siblings" claim.

`crates/server/src/split_join.rs:309`:

```rust
let part_meta = std::fs::metadata(&p).map_err(|_| format!("part {i} missing: {}", p.display()))?;
```

The `io::Error` is discarded, so **every** stat failure is answered with an existence verdict. Permission
denied, a dead network mount, a transient I/O error, a path too long — all of them come back as *"part 3
missing"* about a file the user can see sitting in the folder. They go looking for a part that was never
gone.

It is wrong *in place*, which is the tell: the two lines immediately below it do the right thing.

```rust
let mut part_file = File::open(&p).map_err(|e| format!("part {i} ({}): {e}", p.display()))?;
let n = part_file.read(&mut buf).map_err(|e| format!("part {i} ({}): {e}", p.display()))?;
```

## Why it was missed

CPE-1678's author swept for `read_to_string` and `fs::read`. This is `fs::metadata`, so the search
**structurally could not find it** — not carelessness, a scope that was narrower than the conclusion drawn
from it. The PR body asserted in bold that `text_stats` was "the only collapse". Worth remembering: a
negative result is only as wide as the search that produced it, and it should be stated with its scope
attached.

## Reachability — this is live, not dead code

`join_files` is a registered Tauri command (`src-tauri/src/lib.rs:3715`, in `generate_handler!` at :11802
and :12658) → `cpe_server::split_join::join_files` → `join_into`. A user rejoining a split file on a network
share, or from a folder they have partial rights to, hits this today.

## Scope

`crates/server/src/split_join.rs` — distinguish "the part is not there" from "the part is there and the
stat failed". `ErrorKind::NotFound` is the only case that should say *missing*; everything else names the
OS's own cause, the way the two lines below it already do.

Sweep the rest of the file and the crate for `fs::metadata(..).map_err(|_| ..)` and
`.map_err(|_| ..)` generally while you are there — this is now the **second** confirmed instance of the
pattern, so treat it as a class rather than an incident, and **state the scope of whatever search you run**
in the PR body.

## Acceptance criteria

- [x] A genuinely absent part still reports *missing*.
- [x] An unreadable-but-present part reports the OS's real cause and does **not** claim it is missing.
- [x] A test covers both, driven through `join_files`, with the unreadable part constructed for real
      (`icacls /deny` on Windows, `chmod` on Unix) — and, per CPE-1678's lesson, **the test announces
      itself when it cannot construct that condition** rather than skipping in silence.
      *Constructed for real, but by a different mechanism than the AC names — see the Work Log: neither
      `icacls /deny` nor `chmod` can make `fs::metadata` fail on a **file**, so the test denies first
      (probing, never assuming) and then falls back to a self-referential symlink, which is what
      actually reproduces the bug.*
- [x] That announcement uses `writeln!(std::io::stderr(), ..)`, **not `eprintln!`**, and you have
      confirmed it appears under plain `cargo test` with no `--nocapture`. This is not a style
      preference. libtest captures stdout/stderr per test and replays it only for *failing* tests; a
      skip is a pass, so an `eprintln!` is swallowed and the notice reaches nobody. CI runs plain
      `cargo test`. CPE-1678 shipped this exact bug once — the fix was verified under `--nocapture`
      and assumed to hold everywhere — so verify through the channel that will actually carry it.
- [x] Reverting the fix turns that test red; the actual output is pasted in the PR.
- [x] The PR body reports what the wider `map_err(|_| ..)` sweep covered and what it found, scope stated.

## Work Log

**2026-08-12 — fixed, PR #869.**

`crates/server/src/split_join.rs:309` now classifies instead of collapsing, via a small pure
`part_stat_error(i, p, &io::Error)`: `ErrorKind::NotFound` keeps `part {i} missing: {path}`, every other
cause gets `part {i} ({path}): {e}` — the exact shape the `File::open`/`read` lines below it already use,
so the same part failing at `stat` and failing at `open` now read identically.

**The AC's construction mechanism does not work, and that is worth recording.** The ticket (and CPE-1678
before it) says to build the unreadable part with `icacls /deny` / `chmod`. That fixed the *sibling*
bug, where the code under test was a `read` that ran *after* a successful `metadata`. Here the code under
test **is** the `metadata` call, so the denial has to break `stat` itself, and a per-file permission
denial cannot do that on either platform:

- **Unix:** `stat()` on a file needs no permission on the file at all — only `+x` on its parent
  directories. `chmod 000` leaves `fs::metadata` succeeding.
- **Windows:** `fs::metadata` opens with a desired-access mask of 0, which a per-file deny ACE does not
  refuse. Measured, not assumed: with `icacls /deny "<user>:(RA,RD)"` applied, the probe still reported
  the file stattable and the test fell through to mechanism 2.

Denying the *parent directory* would work on both, but `join_files` reads the manifest out of that same
directory before it reaches any part, so the run would fail above the code under test.

What does reproduce it, with no privilege on Unix and Developer Mode on Windows: a **self-referential
symlink** at the part's path. The entry is listed in the folder — which is the user's actual complaint,
"it is right there" — `symlink_metadata` sees it, and `fs::metadata` fails with `FilesystemLoop`/ELOOP or
ERROR_CANT_RESOLVE_FILENAME. The test tries the permission denial first anyway and *probes* the result
(`symlink_metadata` Ok **and** `stat` fails with something other than `NotFound`), so if a future OS
starts honouring the deny, the test uses it and the recorded claim above self-corrects.

Two guards, so the deterministic half never depends on machine privileges:
`part_stat_error_says_missing_only_for_a_genuine_absence` (pure, runs everywhere) and
`a_present_but_unstattable_part_names_the_cause_instead_of_calling_itself_missing` (end-to-end through
`join_files`, skips loudly via `writeln!(std::io::stderr(), ..)` when the condition can't be built —
verified visible under plain `cargo test`, no `--nocapture`, on a *passing* test).

**Sweep result (scope stated).** `map_err(|_| ..)` across all 570 tracked `.rs` files: 44 hits, all
triaged; `split_join.rs:309` was the only I/O-error-to-existence-verdict collapse among them. But that
pattern is itself too narrow — the same search shape is what missed this bug from CPE-1678. A second,
structural sweep for the *class* (a `stat` outcome answered with an existence claim, regardless of
syntax) found two more live instances that no `map_err` search could reach:
`crates/server/src/disk_usage.rs:40` and `crates/server/src/native_meta.rs:112/125/144`, both using
`!path.exists()` — which folds ENOENT and EACCES into the same `false`, exactly what
`dispatch.rs`'s own doc comment warns against. Reported in the PR for filing rather than fixed here:
they are separate modules with their own message contracts, and testing them needs a parent-directory
traversal denial, which is a different (and harder) rig than this ticket's.

## Notes

Filed by the Foreman from the PR #865 review, 2026-08-12, and independently confirmed before filing.

Related: **CPE-1678** (the same bug in `text_stats`), **CPE-1673** (the error taxonomy this sits on), and
the rule the pair of them keep proving — *a confident wrong answer is worse than an honest "I don't know"*.
