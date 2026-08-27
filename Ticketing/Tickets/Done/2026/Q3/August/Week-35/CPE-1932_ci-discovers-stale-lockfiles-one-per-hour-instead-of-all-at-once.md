---
id: CPE-1932
title: CI discovers stale `Cargo.lock` files one per hour-long run instead of all at once — a seconds-long pre-flight would catch every one
type: task
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

This repo has **seventeen independent `Cargo.lock` files**. Nine of them pin `cpe-server`. A
dependency change in `cpe-server` therefore needs up to nine lockfiles regenerated, and CI builds
each crate with `--locked`, so a stale one is a hard failure.

The problem is not the rule — it is the **discovery cadence**. CI hits stale lockfiles **serially**,
one per job, and each discovery costs a full CI cycle (~1 hour on this repo). On CPE-1896 the matrix
named `crates/net` first; there were in fact **seven** stale:

    crates/ftp  crates/mdns  crates/net  crates/s3  crates/sftp  crates/vfs  crates/webdav

Had they been discovered one at a time, that is seven CI cycles to learn something that takes
seconds to check.

## The check already exists and is nearly free

`cargo metadata --locked` is resolve-only — **no compilation**. CPE-1896's worker ran it across all
seventeen lockfiles in seconds, including the eight that cannot be affected, precisely because
checking everything was cheaper than another round-trip to discover otherwise.

## The real lesson, worth carrying past the fix

From that worker, and it is sharper than "remember to update the lockfiles":

> Round 1 didn't break the rule, it got the **enumeration** wrong — it updated the two lockfiles it
> knew about and never asked how many existed.

Both reviewers independently verified `crates/server/Cargo.lock` and `src-tauri/Cargo.lock`, and both
were **correct**. The two everyone knew about were the two that were fine. Nothing in the process
asked *how many there are*, so nothing caught the other seven. A rule you follow from memory is a
rule you follow incompletely; make it complete by construction.

The mechanical form:

    git ls-files '*Cargo.lock'            # enumerate, do not recall
      -> grep each for the changed package
      -> cargo metadata --locked in each directory

## Acceptance criteria

- [x] Add a fast CI job that runs `cargo metadata --locked` over **every** `Cargo.lock` in the tree
      and fails naming **all** stale files at once, not the first one reached. It must not compile
      anything — the whole value is that it finishes in seconds and runs before the expensive matrix.
- [x] Have it enumerate lockfiles from `git ls-files` rather than a hand-maintained list, so a new
      crate is covered the day it lands. A hardcoded list would reproduce the exact defect this fixes.
- [x] Red-proof it: stale one lockfile deliberately and confirm the job fails naming that file; stale
      two and confirm it names **both** rather than stopping at the first.
- [x] Make the failure message say what to do — regenerate with `cargo metadata` (resolve-only), not
      `cargo update`, which would turn a build fix into an unreviewed dependency bump.
- [x] Note the format-version trap: regenerating can silently bump a lockfile's `version = 3` to `4`
      as a side effect. CPE-1896 reverted that by hand both times. Either the guard or the docs should
      warn, so a format bump does not ride in on a build fix.
- [x] Consider whether the same enumerate-don't-recall shape applies elsewhere — the repo already has
      a "keep five files in sync" release rule in CLAUDE.md that is maintained by memory and,
      per that document, is the one that gets missed.

## Notes

Filed 2026-08-27 by the sprint Foreman after PR #1043 spent a CI cycle discovering one stale lockfile
and would have spent six more discovering the rest. Related: **CPE-1896**, **CPE-1904**
(`package-lock.json` drift has no build-time backstop either), **CPE-1922** and **CPE-1931** (other
guards that measure less than they appear to).

## Work Log

Added a `lockfile-preflight` job to `.github/workflows/ci.yml`, first in the file, before every job
that runs `cargo ... --locked`. It enumerates lockfiles via `git ls-files '*Cargo.lock'` (17 files,
verified none hardcoded), runs `cargo metadata --locked --manifest-path <dir>/Cargo.toml` for each
(resolve-only, no compilation, `set -uo pipefail` with no `-e` so one failure doesn't stop the sweep),
and collects every stale path into an array before failing — so it names all of them, not just the
first. `backend`, `crates`, `sidecar`, and `msrv` now `needs: lockfile-preflight`, so a repo-wide
lockfile problem is caught before any of the expensive 3-OS matrix jobs start.

Local sweep of a clean tree: all 17 pass in ~7-9s (no network-heavy compilation, `cargo metadata`
only). Red-proofed by adding an unresolved dependency line to a leaf crate's `Cargo.toml` (no
lockfile edit) and running the exact script extracted from the YAML (not a hand-copy — verified the
heredoc terminator survives YAML's block-scalar dedent, since the indented `EOF` marker only breaks a
raw shell script, not the string PyYAML/GitHub Actions actually executes):

- One stale (`crates/updater-verify`): job exits 1, names exactly that one file.
- Two independent stale (`crates/s3` + `crates/updater-verify`, chosen specifically because nothing
  else path-depends on either): job exits 1, names **both**, not just the first reached.

Failure message tells the fixer to run `cargo metadata` (not `cargo update`) and warns explicitly
about the `version = 3` -> `4` format-version trap CPE-1896 hit twice.

Also noticed while red-proofing: editing `crates/mdns/Cargo.toml` staled **two** lockfiles at once
(`crates/mdns` itself and `src-tauri`, which path-depends on `cpe-mdns`) — a live example of exactly
the fan-out this ticket exists to catch in one pass instead of two CI cycles.

**Enumerate-don't-recall elsewhere:** CLAUDE.md's "Versioning — keep five files in sync" release
rule is the same shape and, per that document's own text, is "the one that gets missed" — items 4
(`package-lock.json`, two spots) and 5 (`src-tauri/Cargo.lock`) drift silently because neither build
runs `--locked`, so nothing fails; it only surfaces later as a dirty working tree that reads as noise.
Unlike the lockfile fleet, this rule isn't naturally enumerable from `git ls-files` (it's five specific
fields across five files, not "every file matching a glob"), so the fix shape is different: a small
script asserting the version string in all five locations agrees, run in CI (or as a pre-tag check in
`scripts/release.ps1`), would convert "maintained by memory" into "checked by construction" the same
way this ticket does for lockfiles. Filing that as a follow-up rather than fixing it here — out of
scope for CPE-1932, which is lockfiles specifically. Related also: CPE-1904 (`package-lock.json` drift
has no build-time backstop either), named in this ticket's own Notes.

## Work Log (round 2 — UAT + Reviewer follow-up)

UAT ran the real scenario (`itertools` added to `crates/server/Cargo.toml`) and confirmed all 9
`cpe-server`-pinning lockfiles named in one shot, 11.4s, and that the `version = 3` -> `4` format
trap fires on every one of them with cargo 1.98.0 (not hypothetical) and reverts cleanly by hand.
Reviewer independently re-verified the red-proof via PyYAML extraction and found three real gaps,
all fixed here:

**Gap 1 — `net-e2e` was ungated.** `backend`/`crates`/`sidecar`/`msrv` got `needs: lockfile-preflight`
in round 1; `net-e2e` (the Docker/real-SFTP/WebDAV/FTP interop job, which builds `crates/net` and
`crates/vfs` with `--locked`) was missed. Added `needs: lockfile-preflight` to it.

**Full enumeration of every `--locked`-running job (done via `grep -nE '^  [a-zA-Z0-9_-]+:$'` — the
first attempt at this enumeration used `[a-zA-Z_-]` without digits and silently skipped `net-e2e`
because "net-e2e" contains a digit; re-running with the corrected class is what surfaced it):
- `backend` — gated
- `crates` — gated
- `net-e2e` — gated (fixed this round)
- `sidecar` — gated
- `msrv` — gated
- `ffmpeg-pin-guard` — NOT gated, correctly: verified it has no `dtolnay/rust-toolchain` step and
  installs no Rust at all; its only `--locked` text is inside a long comment narrating a historical
  one-off manual probe (`cargo +1.88.0 check --locked ...` someone ran by hand outside CI), not a
  `run:` step. Confirmed via PyYAML that `ffmpeg-pin-guard.needs` is unset and no `cargo` command
  appears in any of its actual steps. Left ungated deliberately — do not "helpfully" add `needs:`
  here, there is nothing for it to wait on.
- `frontend`, `lockfile-preflight` — no cargo at all; `lockfile-preflight` itself carries no `needs:`
  so it runs in parallel with `frontend`, not serialized behind it.

**Gap 2 — stale vs. broken manifest were conflated.** The `2>&1` on the `cargo metadata` call
discarded stderr, so ANY failure (a genuine stale lockfile, or an unrelated Cargo.toml syntax error)
was reported as "STALE" with regenerate-the-lockfile advice — actively wrong advice for a syntax
error. Fixed: capture stderr, classify on cargo's own distinguishing text
(`--locked was passed to prevent this` = genuine staleness; anything else = broken manifest), keep
the real cargo error inside the `::group::` log instead of discarding it, and give two different
remediation messages. Red-proofed both paths against the actual YAML-extracted script:
  - Stale (2 independent lockfiles, `crates/s3` + `crates/updater-verify`): both correctly
    classified STALE, both named, exit 1.
  - Broken (`this is not valid toml [[[` appended to `crates/vfs/Cargo.toml`): correctly classified
    BROKEN (not STALE) for both `crates/vfs` (direct) and `src-tauri` (cascade, since it path-deps
    on `cpe-vfs`) — src-tauri's cargo error text differs ("failed to get `cpe-vfs` as a dependency
    of package...") but correctly falls to BROKEN since it also lacks the staleness string. Message
    correctly says "NOT a lockfile-staleness problem... fix the syntax error", not the lockfile
    regeneration advice. Real cargo error visible in the group log in both cases.

**Gap 3 — zero-enumeration false green.** If `git ls-files` returns nothing (git missing, wrong
cwd, a future refactor of this step), the job checked 0 files and reported "All 0 Cargo.lock files
are up to date" — a false green covering nothing. Fixed with a floor check: fewer than 10 lockfiles
found fails the job outright with a message explaining why (repo has 17 today; 10 is a deliberately
conservative floor that survives removing several crates without needing to be bumped, while still
catching near-total enumeration loss). Chose a floor over a bare non-zero check per the reviewer's
suggestion, specifically so a partially-broken enumeration (e.g. only 2-3 files found due to some
other bug) is caught too, not just a totally-broken one. Red-proofed by stripping `git` from PATH:
`git: command not found` -> `Found 0 Cargo.lock file(s)` -> `::error::only 0 Cargo.lock file(s)
found ... expected at least 10` -> exit 1 (previously this path exited 0).

All four scenarios (clean / 2-stale / 2-broken / zero-enumeration) re-verified against the script
extracted from the final committed YAML via PyYAML, matching how GitHub Actions itself parses the
block scalar.
