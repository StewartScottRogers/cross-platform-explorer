---
id: CPE-1952
title: the catalog staging dir is a predictable path outside the project, and `create_dir_all` succeeds straight onto a pre-existing junction
type: bug
priority: Medium
status: In Progress
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

`do_fetch_catalog` stages downloaded catalog content in `temp_dir()/cpe-catalog-stage-<pid>`. Three
properties compound:

- **predictable** — `temp_dir()` plus a pid, both guessable by a local process;
- **outside the project**, so none of the containment work of CPE-1896 / CPE-1913 / CPE-1937 covers it;
- **`create_dir_all` succeeds onto a pre-existing junction**, so a local attacker who plants one at
  that path before the fetch has the staged writes land wherever it points.

Raised by PR #1058's Security Auditor as pre-existing and untouched by that PR, and re-raised by PR
#1063's worker, which judged it out of scope there because it is a **different attacker model** —
local filesystem write, no signing key — and the remedy is containment machinery rather than a
validation rule.

## Two corrections to the obvious plan

The Foreman initially suggested reaching for `open_beneath`. PR #1063's worker checked and both halves
of that suggestion were wrong at the time:

1. **`open_beneath` is `pub(crate)` inside `cpe-server`.** `do_fetch_catalog` lives in `src-tauri`, so
   it **cannot reach it at all** without an export decision that is its own piece of design.
2. **`remove_file_beneath` did not exist** when that was suggested — it lands with CPE-1937 / PR #1059.

So this is not a five-line change. Whoever takes it should decide the seam first: export a narrow
containment API from `cpe-server`, move the staging logic into `cpe-server`, or contain it without
`open_beneath` at all.

## Acceptance criteria

- [x] **Demonstrate it first.** Plant a junction at `temp_dir()/cpe-catalog-stage-<pid>` before a
      fetch and show the staged bytes landing outside. Assert on the **filesystem** — where the bytes
      ended up — not on a verdict. If it turns out something upstream already prevents this, record
      that and close the ticket honestly rather than fixing an imaginary bug.
- [x] Decide the seam, and record it: does `src-tauri` get a narrow containment API exported from
      `cpe-server`, does the staging move into `cpe-server`, or is it contained another way? This
      decision is most of the ticket.
- [x] Prefer a **freshly created, exclusively owned** staging directory over a predictable one —
      create-new-or-fail rather than `create_dir_all`, so an existing entry at that path is a refusal
      rather than something to write through.
- [x] Clean up on every exit path, including refusal. The current code `remove_dir_all`s staging on a
      verify failure; keep that property, and make sure the cleanup cannot itself follow a link out
      (`remove_dir_all` on a junction is exactly the CPE-1937 family).
- [x] **Red-proof by racing it**, not by reading it. The containment work this belongs beside
      (CPE-1896, CPE-1913, CPE-1937) all found that static fixtures understate the problem by one to
      two orders of magnitude — CPE-1937's static case showed 1 destroyed file where the race showed
      141 per 200 trials.
- [x] Check whether any **other** temp-dir staging path in the app has the same shape. Enumerate
      rather than recall (CPE-1932).

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1058's Security Auditor (residual 3) and PR #1063's
worker, which supplied both corrections above.

Family: **CPE-1896** (the handle-gate origin), **CPE-1913** (the containment gates), **CPE-1937**
(`remove_file_beneath`, PR #1059), **CPE-1940** / **CPE-1949** (the catalog trust engine this staging
serves).

## Work Log

### 2026-08-27 — reproduced, then removed the staging directory entirely

**1. Demonstrated first, on the filesystem, on both platforms.** A standalone repro ran the three
lines transcribed out of `do_fetch_catalog` after planting a link at
`temp_dir()/cpe-catalog-stage-<pid>` — a **junction** on Windows (`mklink /J` semantics, no admin
rights needed) and a **symlink** on Linux, on a real **ext4** path, not `/mnt/z`:

```
ARM A create_dir_all: OK, staging = .../temp/cpe-catalog-stage-34500
ARM A ON-DISK: .../victim/index.json exists = true
ARM A ON-DISK bytes: "STAGED-CATALOG-INDEX"
```

Same result on ext4 (pid 9784). `create_dir_all` walked the reparse point, the staged bundle landed
in the attacker's directory, and the code returned `Ok`. Assertions are on where the bytes ended up,
never on a verdict.

**Field evidence that the path really is predictable:** the developer machine's real `%TEMP%` holds
**8 leftover `cpe-catalog-stage-<pid>` directories**, dated 2026-07-14 to 2026-07-19 — the shipped
app leaking staging on the `?` early-return paths. The attacker never had to guess anything; the
names are sitting there.

**2. Two things the ticket expected that turned out not to be true.** Recorded rather than "fixed":

* **The cleanup leg is not a CPE-1937 destroyer.** `remove_dir_all` on the planted link deleted **the
  link**, leaving the victim directory and both its files intact (`victim entries 2 -> 2`,
  `victim dir still there = true`) on Windows *and* on ext4. Rust's std has refused to recurse into a
  reparse point / symlink at the top since 1.77.
* **`open_beneath` cannot fix this**, and not only for the two reasons the ticket names (it is
  `pub(crate)` in `cpe-server`; `remove_file_beneath` was new). Its own module doc disqualifies it:
  *"It does not defend the root itself. The caller resolves the root once and passes it in; if the
  root was already a link to somewhere unexpected, every write goes there and this module agrees."*
  The component under attack **is** the root. `create_beneath` contains what is below an
  already-trusted root; the defect is the root's own creation.

**3. The seam decision — none of the three options in the ticket. The staging directory is deleted,
not defended.** `sidecar_host::catalog` grows a `BundleSource` trait with two implementations:
`MemBundle` (a `BTreeMap` — no filesystem at all) and a private `DirBundle` (the old on-disk shape,
still backing the published `apply_bundle_at`, so `catalog_republish_downgrade.rs` and the
release-side round-trips are untouched). `apply_bundle_with` now reads from a `&dyn BundleSource`;
the new public `apply_bundle_source_at` is the memory entry point. `do_fetch_catalog` assembles the
bundle in memory — every `catalog_http_get` already returned a `Vec<u8>`; the only reason the bytes
ever hit the disk was that the apply engine could not read them from anywhere else — and calls
`apply_bundle_source_at`. No `temp_dir`, no `create_dir_all`, no `create_dir`, no `remove_dir_all`,
no `fs::write`.

*Why this rather than a hardened staging directory:* an unguessable name plus create-new-or-fail
(which was the drafted fix, and which measures correctly — `create_dir` onto the planted junction is
`AlreadyExists`, code 183 on Windows / 17 on Linux, with zero bytes escaping) still leaves the leaf
writes resolving **by path** inside a directory a local process can watch appear via `readdir`. **A
directory that is never created cannot be redirected**, and that needed no new containment machinery
at all. It also stops unverified bytes off the wire being written to a world-listable directory
before the trust gate decides about them, and it ends the leak the 8 stale directories evidence.

**4. Properties the fix actually provides**, stated plainly:

| property | provided? |
|---|---|
| No predictable path in a shared namespace | **yes** — there is no path |
| No `create_dir_all`, so nothing to write through | **yes** — no directory is created |
| No cleanup that could follow a link out | **yes** — nothing to clean up; the `MemBundle` drops with its scope |
| Unverified bytes never reach the disk | **yes**, new |
| The *destination* `catalog_dir(app)` is contained | **no** — see the enumeration; it is still resolved and `create_dir_all`'d by path, which is exactly the question `open_beneath` declines about its own root |

**5. Enumeration, derived not recalled (CPE-1932).** `git ls-files '*.rs'`, excluding `tests/` and
everything after a file's first `#[cfg(test)]`, grepped for
`temp_dir()` / `env::var("TEMP"|"TMP"|"TMPDIR")` — 15 non-test sites — plus `.github/workflows/*` for
`mktemp` / `$RUNNER_TEMP`, and `sidecar/host/src/bin/catalog_sign.rs`:

| site | verdict |
|---|---|
| `src-tauri/src/lib.rs:10313` catalog staging | **the bug — FIXED** (staging removed) |
| `src-tauri/src/lib.rs:10155` `catalog_dir` fallback `temp_dir()/cpe-ai-console-catalog` | **same shape, NOT changed.** It is the *destination*, not staging, and only reached when `app_data_dir()` errors. Making it fail closed changes the catalog subsystem's behaviour on a path nothing here exercises; that deserves its own ticket rather than a drive-by. Recorded as CPE-1952's residual. |
| `src-tauri/src/lib.rs:11837` `temp_dir()/cpe-sidecar-storage` fallback | same shape as the above, same reasoning, same residual |
| `src-tauri/src/lib.rs:14349` `cpe_bindings_export.ts` | **build-time only** (`export_bindings`, dev/CI): writes a generated TS file it immediately re-reads and deletes. Not in the shipped runtime. No change |
| `crates/server/src/archive.rs:1672` extraction session root | **already contained (CPE-1786)**: refuses a link at the shared root on every call, then claims a per-session directory with an exclusive `create_dir` |
| `crates/server/src/ffmpeg_util.rs:113` `create_scratch_dir` | **already contained**: `fs::create_dir` (create-new-or-fail) with a bounded retry — never `create_dir_all` |
| `crates/server/src/fsutil.rs:4457` `ScratchDir::adopt` | test-fixture guard; canonicalises before arming a recursive delete. No change |
| `crates/server/src/fsutil.rs:4606` `scratch_dir(prefix)` | `create_dir_all` on a predictable name, but it is the **test-fixture** helper (`pub` only because `#[cfg(test)]` is per-crate and downstream crates need it from their own test builds). No production caller. No change |
| `sidecar/ai-console/src/console.rs:733,796` swarm mission dir | predictable (`cpe-swarm-<millis>`), `create_dir_all`. **Same shape, different subsystem** — recorded, not changed here |
| `sidecar/ai-console/src/session_diag.rs:33`, `session_supervisor.rs:151`, `sidecar/host/src/reaper.rs:61` | a diagnostic log and a port file under `temp_dir()/cpe-ai-console`, both best-effort and neither security-bearing. No change |
| `crates/net/examples/security_demo.rs:22`, `crates/net/src/bin/cpe-server-ref.rs:27` | example / reference binaries, not shipped. No change |
| `sidecar/host/src/bin/catalog_sign.rs:141` | `create_dir_all(out)` where `out` is the operator-supplied output directory on the release runner — not a temp path, not predictable-outside-project. No change |
| `.github/workflows/*` (`mktemp` x5, `$RUNNER_TEMP` x~25) | ephemeral single-tenant runners — no second local principal — and `mktemp` is unpredictable by construction. No change |

**6. Guards.** `sidecar/host/tests/catalog_staging_containment.rs` — 3 deterministic tests, run by the
`sidecar` CI job on **all three OSes**:

* `the_old_staging_primitive_writes_through_a_planted_link` — the **sensitivity control**. Runs the
  pre-fix primitive against the planted link and asserts the bytes land in the attacker's directory.
  If it ever passes by *not* escaping, the answer is to find out what changed, not to delete it.
* `the_fetched_bundle_never_touches_the_filesystem` — the same scene, the real production entry
  point, a really-signed bundle; asserts the attacker's directory stays empty, the link still has
  nothing behind it, **and** that the catalog installs correctly (so "refuses everything" cannot
  pass).
* `the_memory_arm_still_enforces_anti_rollback` — the refactor did not soften the trust engine.

The link is planted at the **real** `temp_dir()/cpe-catalog-stage-<pid>`, not a stand-in inside a
`tempfile::tempdir()`, because a stand-in is unreachable by any regression of the code under test, so
every assertion about it would be unfalsifiable — safe-looking and worthless at once (CPE-1929). The
two tests share a `Mutex`, and a `Drop` guard cleans the link up on every exit path including a
panic.

`src/lib/catalogStagingContainment.test.ts` covers the caller, which needs a Tauri `AppHandle` and a
network fetch and so cannot be reached from a Rust test. It **reads `src-tauri/src/lib.rs`**
(CPE-1933: derive, do not claim), strips Rust comments first — the fix's own explanation names
`create_dir_all` three times, and a raw-text guard would fail on the fix it guards — and asserts that
`do_fetch_catalog` contains none of the five staging primitives *and* still contains
`MemBundle::new()` / `apply_bundle_source_at`, so deleting or renaming the function cannot pass by
vacuity. The stripper is `src/lib/rustSource.ts`'s `stripRustComments` — CPE-1950 had already lifted
it out of `MacroRunConfirm.test.ts` for exactly this reason, so this ticket is its second consumer
rather than a second copy of it. (Landed on main mid-flight; this branch's own extraction was
dropped on the rebase in favour of theirs.)

**7. Red-proof — by attacking, not by reading.**

* *The vitest guard*: reinserted the two deleted lines into `do_fetch_catalog` →
  `x does not call temp_dir`, `x does not call create_dir_all`; reverted → green.
* *The Rust containment test*: put the pre-fix staging back **inside `apply_bundle_source_at`** (in a
  throwaway Linux copy) → `the_fetched_bundle_never_touches_the_filesystem` FAILED with
  `the attacker's directory must stay empty; found ["index.json"]` — on-disk evidence, not a verdict.
  The other two stayed green, so the sabotage was specific.
* *No race harness*, and deliberately. The ticket asked for one because CPE-1937's static fixture
  understated a race. There is no race here to understate: the attack is a **pre-plant** — the path
  was computable before the fetch started, so the attacker never needed to win a window — and the fix
  removes the path rather than narrowing a window. A trial count would be decoration on a
  deterministic result.

**8. Verification.** `cargo clippy --locked --all-targets -- -D warnings` clean for `sidecar/host` on
Windows and on Linux/ext4; `src-tauri` clippy clean in **both** feature modes (`--features
sidecar-platform` and default). `cargo test --locked` green for `sidecar/host` (129 tests). Full
vitest: 4883 passed, with the 19 failures identical at the merge base (shell-script-executing suites
that need a POSIX toolchain this Windows box lacks). `npm run check`: 0 errors, 0 warnings. Every
planted junction/symlink cleaned up; `%TEMP%` and `/tmp` verified free of stray staging links
afterwards.

**Residual, stated so it is not mistaken for covered:** `catalog_dir(app)`'s and the sidecar-storage
`temp_dir()` fallbacks, and `ai-console`'s `cpe-swarm-<millis>` mission directory, are the same shape
and are unchanged. Each is a destination rather than staging, and each needs its own decision about
what failing closed costs.
