---
id: CPE-1687
title: join_files tells you a part is "missing" when the part is right there and merely unreadable
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-12
closed:
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

- [ ] A genuinely absent part still reports *missing*.
- [ ] An unreadable-but-present part reports the OS's real cause and does **not** claim it is missing.
- [ ] A test covers both, driven through `join_files`, with the unreadable part constructed for real
      (`icacls /deny` on Windows, `chmod` on Unix) — and, per CPE-1678's lesson, **the test announces
      itself when it cannot construct that condition** rather than skipping in silence.
- [ ] That announcement uses `writeln!(std::io::stderr(), ..)`, **not `eprintln!`**, and you have
      confirmed it appears under plain `cargo test` with no `--nocapture`. This is not a style
      preference. libtest captures stdout/stderr per test and replays it only for *failing* tests; a
      skip is a pass, so an `eprintln!` is swallowed and the notice reaches nobody. CI runs plain
      `cargo test`. CPE-1678 shipped this exact bug once — the fix was verified under `--nocapture`
      and assumed to hold everywhere — so verify through the channel that will actually carry it.
- [ ] Reverting the fix turns that test red; the actual output is pasted in the PR.
- [ ] The PR body reports what the wider `map_err(|_| ..)` sweep covered and what it found, scope stated.

## Notes

Filed by the Foreman from the PR #865 review, 2026-08-12, and independently confirmed before filing.

Related: **CPE-1678** (the same bug in `text_stats`), **CPE-1673** (the error taxonomy this sits on), and
the rule the pair of them keep proving — *a confident wrong answer is worse than an honest "I don't know"*.
