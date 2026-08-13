---
id: CPE-1692
title: Six more sites decide "not found" from a collapsed stat, so a denied path is reported as absent
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-12
closed:
---

## Problem

**Six** more live instances of the bug CPE-1678 and CPE-1687 have each closed once: **an unknown answered as
a confident one.** The first two were found by the CPE-1687 worker's semantic sweep; the other four by the
PR #869 reviewer, who re-ran that sweep and found it had missed four sites — two of them inside its own
stated scope.

- `crates/server/src/disk_usage.rs:40` — `!path.exists()` → *"not found"*
- `crates/server/src/native_meta.rs:112`, `:125`, `:144` — same
- `crates/server/src/links.rs:75` — `fs::metadata(p).is_err()`, comment literally reads *"err ⇒ target
  gone"* → the link is reported **broken** on any stat failure
- `crates/server/src/dangling_links_scan.rs:128` — `fs::metadata(&resolved).is_ok()` → the link is
  reported **dangling**; its own comment notes ELOOP arrives here and then folds it into "doesn't exist"
- `src-tauri/src/lib.rs:3274` — `!parent.exists()` → *"The original folder no longer exists"*
- `crates/sftp/src/lib.rs:460`, `:509` — `!is_dir()` / `!exists()` → **`StatusCode::NoSuchFile` returned
  over the wire to a remote SFTP client.** The most externally visible of the set.

*(A seventh and eighth — `split_join.rs:184`/`:208`, `!Path::is_file()` → "manifest not found" — were
found at the same time and **fixed in PR #869** rather than deferred here, since that PR was already in
that file.)*

All of them collapse a `stat` outcome into an existence claim. `Path::exists()` swallows **every** `stat` failure into
`false`, so a permission-denied path — or a dead network mount, or any transient I/O error — is reported as
a path that isn't there.

`dispatch.rs`'s own doc comment already warns against exactly this, and `classify_path_error` exists to do
it correctly:

> Deliberately does **not** collapse to `Path::exists()`, which swallows every `stat` failure — missing path
> AND permission-denied parent-directory traversal (EACCES) alike — into the same `false`. That would report
> "we don't know" as "it isn't there".

So the rule is written down and the helper is written; every one of these call sites predates both.

## Why this was missed twice before

CPE-1678 swept for `read_to_string`/`fs::read` and missed an `fs::metadata` collapse (CPE-1687). CPE-1687's
brief then told the worker to sweep `map_err(|_| ..)` — and **that search could not have found these
either**, because `!path.exists()` contains no `map_err` at all.

That worker ran the syntax sweep as instructed (44 hits over 340 tracked `.rs` files, all triaged, one
genuine hit) and then ran a **second, semantic sweep**: *any* `stat` outcome answered with an existence
claim, however spelled. That found `disk_usage.rs` and `native_meta.rs`.

**Then the PR #869 reviewer re-ran the semantic sweep and found four more the semantic sweep had missed** —
including `links.rs:75` and `dangling_links_scan.rs:128`, which were inside its own declared scope, and the
`split_join.rs` pair in the very file being edited. The structural cause is worth keeping: search 1 spanned
every tree, and search 2 — the one whose entire purpose is catching what syntax misses — was narrowed to
`crates/server/src/*.rs`. **The broader search had the narrower scope.**

So this is now the *fourth* time in this chain a search has under-covered its own conclusion, and the second
time the fix was to re-run someone else's sweep rather than trust it. Sweeping is not a step you do once.

## Scope

All the files listed above — replace the collapsing checks
with a real `fs::metadata` call whose error kind is classified, reusing `classify_path_error`'s taxonomy
rather than re-deriving it: `ErrorKind::NotFound` is genuinely not-found; anything else says what actually
went wrong.

Each module has its own message contract, so read what its callers expect before changing the strings.

### `Path::try_exists()` is the API nobody here is using

`Path::try_exists()` returns `io::Result<bool>` instead of collapsing every failure into `false`. The PR
#869 reviewer checked: it appears **zero** times across the repo's 340 tracked `.rs` files. For most of
these sites it is the natural fix, and where the *type* matters too (file vs directory) a `metadata()` call
classified through `dispatch::classify_path_error` is the right shape. Prefer either over `exists()`.

### One adjacent family — decide, do not silently pass

The same reviewer counted **~20** sites of the shape `if !root_path.is_dir() { Err("… not a folder") }`
(`checksum.rs:36`, `compare.rs:32`, and many more). Those are *type* claims rather than existence claims, so
they are arguably not this bug — but they collapse through the identical mechanism and will mislead a user
on a denied path in the identical way. Make a call on them and write it down; do not leave them
unmentioned.

## Acceptance criteria

- [ ] A permission-denied path through `disk_usage` reports the access failure, not "not found".
- [ ] The same for each of `native_meta`'s three sites, `links.rs`, `dangling_links_scan.rs`, the
      `src-tauri` folder-moved message, and **both** `crates/sftp` sites.
- [ ] The SFTP pair no longer answers `NoSuchFile` over the wire for a stat failure that is not an absence —
      this one is visible to a remote client, so it is the one to get right first.
- [ ] A genuinely missing path still reports not-found from **every** site — the honest case must not
      regress anywhere.
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
