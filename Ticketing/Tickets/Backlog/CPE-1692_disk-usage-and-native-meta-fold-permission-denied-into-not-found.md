---
id: CPE-1692
title: disk_usage and native_meta use path.exists() to decide "not found", so a denied path is reported as absent
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-12
closed:
---

## Problem

Two more live instances of the bug CPE-1678 and CPE-1687 have each closed once: **an unknown answered as a
confident one.** Found by the CPE-1687 worker, and found the right way — by sweeping the *class* rather than
the syntax.

- `crates/server/src/disk_usage.rs:40`
- `crates/server/src/native_meta.rs:112`, `:125`, `:144`

All four are `!path.exists()` → *"not found"*. `Path::exists()` swallows **every** `stat` failure into
`false`, so a permission-denied path — or a dead network mount, or any transient I/O error — is reported as
a path that isn't there.

`dispatch.rs`'s own doc comment already warns against exactly this, and `classify_path_error` exists to do
it correctly:

> Deliberately does **not** collapse to `Path::exists()`, which swallows every `stat` failure — missing path
> AND permission-denied parent-directory traversal (EACCES) alike — into the same `false`. That would report
> "we don't know" as "it isn't there".

So the rule is written down and the helper is written; these four call sites predate both.

## Why this was missed twice before

CPE-1678 swept for `read_to_string`/`fs::read` and missed an `fs::metadata` collapse (CPE-1687). CPE-1687's
brief then told the worker to sweep `map_err(|_| ..)` — and **that search could not have found these
either**, because `!path.exists()` contains no `map_err` at all.

That worker ran the syntax sweep as instructed (44 hits across all 570 tracked `.rs` files, all triaged,
one genuine hit) and then ran a **second, semantic sweep**: *any* `stat` outcome answered with an existence
claim, regardless of how it was spelled. That is the search that found these. Worth recording, because it is
the third time in this chain that a keyword search has under-covered its own conclusion.

## Scope

`crates/server/src/disk_usage.rs` and `crates/server/src/native_meta.rs` — replace the `exists()` checks
with a real `fs::metadata` call whose error kind is classified, reusing `classify_path_error`'s taxonomy
rather than re-deriving it: `ErrorKind::NotFound` is genuinely not-found; anything else says what actually
went wrong.

Each module has its own message contract, so read what its callers expect before changing the strings.

## Acceptance criteria

- [ ] A permission-denied path through `disk_usage` reports the access failure, not "not found".
- [ ] The same for each of `native_meta`'s three sites.
- [ ] A genuinely missing path still reports not-found from all four — the honest case must not regress.
- [ ] Tests drive the real entry points, not the helpers.
- [ ] **Construct the denied condition with a mechanism that actually works for the code under test.**
      CPE-1687 established, by measurement, that a per-file `icacls /deny` cannot make `fs::metadata` fail:
      on Unix `stat()` needs no permission on the file itself (only `+x` on its parents), and on Windows
      `fs::metadata` opens with desired-access `0`, which a deny ACE does not refuse. A **parent-directory**
      traversal denial is the mechanism that works here — check first whether the code under test reads
      anything else from that directory, which is what ruled it out for CPE-1687.
- [ ] If a test cannot construct the condition on some machine, it **announces the skip** with
      `writeln!(std::io::stderr(), ..)` — not `eprintln!`, which libtest swallows for passing tests — and
      you have confirmed the notice appears under plain `cargo test` with no `--nocapture`.
- [ ] Each guard broken **on its own** turns a **distinct** test red, real output pasted in the PR.

## Notes

Filed by the Foreman from the PR #869 work, 2026-08-12. The worker deliberately did not file these itself:
a concurrent sprint is allocating IDs and it judged an ID collision the worse risk. Correct call.

Related: **CPE-1678** and **CPE-1687** (the same bug, twice), **CPE-1673** (the taxonomy), and the Evidence
Rules in `Ticketing/wiki.md` — particularly that a negative result is only as wide as the search behind it.
This ticket is what the *semantic* sweep found after the syntactic one came back clean.
