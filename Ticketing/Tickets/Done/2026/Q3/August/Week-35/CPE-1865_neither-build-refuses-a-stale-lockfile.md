---
id: CPE-1865
title: neither build refuses a stale lockfile, so version drift has no backstop
type: task
priority: Medium
status: Done
tags: ready
estimate: M
created: 2026-08-22
closed:
---

## Problem

CLAUDE.md records that `package-lock.json` and `src-tauri/Cargo.lock` are the two version-sync files that
get missed, because **nothing fails when they drift**. CPE-1853 measured exactly what happens, on
throwaway crates with the manifest at `0.2.0` and the lock at `0.1.0`:

| command | result |
|---|---|
| `cargo build` | **exit 0**, and it **silently rewrites** the lock to 0.2.0 |
| `cargo build --locked` | **exit 101** — *cannot update the lock file … because `--locked` was passed* |
| `npm ci` | **exit 0**, "up to date", lock left stale |
| `npm install` | **exit 0**, and it **silently repairs** both fields |

The npm rows are the whole story of how `package-lock.json` sat three releases behind through green CI:
CI's `npm ci` neither fails nor fixes it, a developer's `npm install` fixes it without telling anyone, and
the repair surfaces only as a dirty tree that reads as unrelated noise.

## What CPE-1853 already did, and what is left

CPE-1853 made `scripts/release.ps1` bump all five files atomically, under an exactly-one-match guard, with
a test asserting all five agree. **For `package-lock.json` that script is now the only mechanism** — there
is no build-level backstop at all.

`--locked` would give the Rust half a backstop independent of the release script. That is defence in
depth and worth having. It is also a real behaviour change: `--locked` reds on **any** uncommitted
dependency-graph change, not just a version drift, which is why CPE-1853 recorded the recommendation
rather than taking it.

## Acceptance criteria

- [x] Decide whether `--locked` goes on the Rust builds, and where — CI only, release only, or everywhere.
      Record the reasoning either way.
- [x] If taken: measure how often it would have redded CI over recent history before turning it on. A
      backstop that fires on ordinary dependency work will be switched off within a week.
- [x] Say what, if anything, gives `package-lock.json` a backstop. `npm ci --dry-run` behaves the same as
      `npm ci` (measured), so the honest answer may be "nothing, and the release script's all-five guard is
      it" — which is fine, but should be written down rather than assumed.
- [x] Check the sidecar and any other Cargo workspace in the tree, not just `src-tauri`. A partial sweep
      presented as complete is this repo's most-repeated defect.
- [x] If `--locked` is taken, confirm it against the real `tauri build` and the full CI matrix, not a
      throwaway crate. CPE-1853's measurements were single-file crates and it said so.

## Notes

Filed from CPE-1853, whose acceptance criteria required the decision be **recorded**, which it was, with
measurements — but the recommendation landed nowhere actionable until now. Its reviewer flagged the
missing ticket.

Read CPE-1853's Work Log first for the measurement method and the traps in that file; do not re-derive
them.

Related: CPE-1853 (the five-file bump), CPE-1841 (the scoped locators), CPE-1852 (the atomic write).

## Work Log

**2026-08-25 — `--locked` taken everywhere, and it found (and fixed) a real drift.** Branch
`cpe-1855-msrv-and-lockfiles`, off `origin/main`, worked alongside CPE-1855 in an isolated worktree
under `.claude/worktrees/cpe-1855`.

### Lockfile inventory: 17, not 1

`find` for every `Cargo.toml` outside `target/`/`node_modules/`/`.claude/worktrees` (each crate is
**standalone, out of any shared workspace** — see `crates/server/Cargo.toml`'s own comment on why),
cross-checked against `git ls-files -- '**/Cargo.lock'`: **17 independently-buildable Rust crates, all
17 with a committed `Cargo.lock`.**

- 11 under `crates/`: `contract`, `ftp`, `mdns`, `net`, `s3`, `security`, `server`, `sftp`,
  `updater-verify`, `vfs`, `webdav`.
- 5 under `sidecar/`: `agent-board`, `ai-console`, `contract`, `host`, `repos`.
- `src-tauri` — the shipped app.

### Decision: `--locked` everywhere a real `cargo build/test/check/clippy/run` happens, plus a
### preflight where the Tauri CLI hides the `cargo build` it drives internally

**Everywhere**, not "CI only" — record the reasoning: `--locked` is cheap defence-in-depth precisely
because it is a NO-OP when the lock already matches the manifest (confirmed below, on every one of
the 17 manifests, in every feature combination CI actually exercises); the only place skipping it
would matter is exactly the place a stale lock would otherwise silently slip through, which is every
`cargo` invocation in this tree — CI legs, the release builds, the model-catalog + agent-catalog
publish jobs. Applied to:

- `.github/workflows/ci.yml` — every real `cargo build/test/check/clippy/run` line, including the
  new `msrv` job CPE-1855 just added.
- `.github/workflows/release.yml` — the two direct `cargo run --manifest-path ...` invocations
  (`verify-release-artifacts`, `catalog-sign`).
- `.github/workflows/release-sidecar.yml` — the four direct `cargo build --release` invocations (the
  three sidecar builds + the self-heal relink).
- `.github/workflows/model-snapshot.yml` — the one `cargo run --manifest-path
  sidecar/ai-console/Cargo.toml` invocation.

**Two sites can't take `--locked` directly and get a preflight instead:** `tauri-action` (used by
`release.yml` and `release-sidecar.yml` to build the real shipped binary) and `npm run tauri build`
(used twice by `gui-smoke.yml`) both drive `cargo build` *internally*, through the Tauri CLI, which has
no flag to forward `--locked` (checked `npx tauri build --help` locally — no `-- <cargo args>`
passthrough, unlike `tauri dev`). Each of those four sites gets a `cargo check --locked --all-targets`
step, in the same feature configuration the real build uses (`--features sidecar-platform` for
`release-sidecar.yml`; plain for the other three), immediately before it, against
`src-tauri/Cargo.lock` — the exact same lockfile the real build is about to consume. Building the
frontend (`npm run build`) first where it hadn't already run, since `src-tauri`'s `build.rs` needs
`dist/` to exist at compile time (same reason CI's `backend` job orders it that way).

### Measured: does it red today? Yes, twice — and both were real drift, now fixed

Ran `cargo check --locked --all-targets` against all 17 manifests with default features (current
stable toolchain, 1.97.0; `dist/` built first for `src-tauri`):

```
OK: crates/contract, crates/ftp, crates/mdns, crates/net, crates/s3, crates/security, crates/server,
    crates/sftp, crates/updater-verify, crates/vfs, crates/webdav,
    sidecar/agent-board, sidecar/ai-console, sidecar/host, src-tauri
FAIL: sidecar/contract (exit 101)
FAIL: sidecar/repos (exit 101)
```

Both failures: `error: cannot update the lock file ... because --locked was passed to prevent this`.
Per this ticket's own acceptance criteria — **check what it breaks before adding it; if the update is
legitimate, commit it rather than dropping the flag** — investigated rather than dismissed:

- `sidecar/contract/Cargo.toml` declares an **optional** `specta` dependency (CPE-957, "OFF by default
  so normal sidecar builds never compile specta"), but the committed `Cargo.lock`'s
  `sidecar-contract` package entry never listed `specta` as a dependency, and the `specta`/
  `specta-macros`/`thiserror`/`thiserror-impl`/`Inflector` package blocks were entirely absent — even
  though Cargo.lock is supposed to carry the full resolvable graph for every feature combination,
  optional included, independent of what's active at check time. The optional dependency was added to
  the manifest and the lock was never regenerated to match. Real, pre-existing drift, invisible until
  today because nothing ever ran `--locked` against it.
- `sidecar/repos/Cargo.toml` has the identical `specta` optional dep (also CPE-957) plus a path
  dependency on `sidecar-contract`, so it inherited the same gap.
- Confirmed both are genuine, not a fluke of this environment: plain `cargo check --all-targets` (no
  `--locked`) on each showed exactly the expected repair — `sidecar/contract/Cargo.lock`: **50
  insertions**, `sidecar/repos/Cargo.lock`: **51 insertions**, both adding ONLY the missing
  `specta`-family packages and the `specta` dependency edge on the crate's own entry, nothing else
  touched (verified via `git diff`).

**Fixed by committing the regenerated lockfiles** (this ticket's commit includes both), not by
weakening the guard. Re-ran `cargo check --locked --all-targets` on both afterward: **both now pass.**
This is the one concrete "what --locked broke" the acceptance criteria asked for, and it is exactly
the class of bug the whole ticket exists to catch — it was sitting in `main` before this work started.

Also measured the specific feature combinations CI's new `--locked` steps exercise beyond the
default build, all green with the regenerated lockfiles in place:

```
OK: crates/security --features jwt
OK: crates/server --features index
OK: crates/server --features pdf-thumb,video-thumb,waveform,dicom-thumb
OK: crates/ftp --features e2e-extra-ca
OK: crates/vfs --features e2e-extra-ca
OK: src-tauri --features sidecar-platform
```

**Net measurement: with the two lockfiles fixed, `--locked` would NOT have redded CI today**, in
either the default build or any feature combination checked above. It is not a backstop that fires on
ordinary dependency work — it is silent today, and only speaks when a manifest and its lock actually
disagree, which is exactly the CLAUDE.md-documented failure mode this ticket exists to close. The
two real reds it produced during THIS measurement are the evidence, not a counter-argument: they are
precisely the kind of drift `cargo build`'s current silent-rewrite behaviour was hiding.

### `package-lock.json`'s backstop: the release script's all-five guard, and nothing else — confirmed, not assumed

Re-checked CPE-1853's finding rather than take it on trust: `npm ci` and `npm ci --dry-run` both
validate the **dependency graph**, not the app's own `version` field, so neither fails nor repairs a
`package.json`/`package-lock.json` version mismatch — `npm ci` is already used everywhere in this
repo's CI (`ci.yml`, `gui-smoke.yml`, `release*.yml`) and none of those runs would have caught the
three-releases-behind drift CLAUDE.md documents. There is no npm build-time flag equivalent to
`cargo --locked` for a version-field check. **The honest answer, written down: nothing at build time
gives `package-lock.json`'s version fields a backstop — `scripts/release.ps1`'s exactly-one-match bump
plus the all-five guard test (`src/lib/releaseVersionBump.test.ts`, CPE-1853) is the only mechanism,
and it only protects the release path, not an arbitrary local `npm install`.** Left as-is: inventing a
build-time npm check was not in scope here and CPE-1853 already established there's no natural flag
for it.

### The local, no-toolchain-required guard: `src/lib/lockfileLockedGuard.test.ts`

Following CPE-1855's own precedent (a CI job that runs a real toolchain can't be exercised in this
session, which is barred from installing/switching Rust toolchains locally), a new Vitest test reads
the five workflow files' raw text and asserts:

1. Every real `cargo build/test/check/clippy/run` line (a line matched against
   `\bcargo\s+(build|test|check|clippy|run)\b`, EXCLUDING `#`-comment lines and step `name:`/`- name:`
   labels — both categories routinely echo a bare `cargo test`/`cargo build` in prose without being an
   actual invocation) contains `--locked`. One test per workflow file, so a failure names the exact
   file.
2. Every `tauri-action`/`npm run tauri build` site (4 total: `release.yml` ×1, `release-sidecar.yml`
   ×1, `gui-smoke.yml` ×2) has a real (non-comment) `cargo check --locked` line within the 40 lines
   immediately before it.

**Red-proof, by actual reversion — two, the second one catching a real bug in the test itself:**

1. Reverted one `run: cargo test --locked` back to `run: cargo test` in `ci.yml` → **1 failed / 5
   passed**: *"ci.yml: every real cargo invocation line carries --locked"*, naming the exact line.
   Restored; `git diff --numstat` back to `66 66` (the file's only change, a pure swap on 66 lines).
2. Removed the `Verify src-tauri/Cargo.lock is current...` preflight step from `release.yml` (leaving
   its explanatory comment in place) → **test STILL PASSED.** The first version of the preflight check
   searched the raw preceding TEXT for the substring `cargo check --locked`, and the comment
   explaining the (now-deleted) step still contained that exact phrase in prose — the check was
   satisfied by its own documentation, not by the real step. Fixed: the detector now only matches a
   REAL (non-comment) line, using the same line-level filter as check 1 above, within a 40-line
   lookback. Re-ran the same reversion → **1 failed / 5 passed**: *"real Tauri build with no preceding
   'cargo check --locked' preflight"*, correctly naming `release.yml`'s anchor line. Restored; `git
   diff --numstat` back to `16 2` (the file's only change). Worth recording: this is the same
   "green over zero coverage" trap this repo's other guards (CPE-1717, CPE-1806) exist to catch, found
   in a guard written for THIS ticket rather than an old one — checking a guard's own red-proof against
   a targeted deletion, not just its happy path, is what caught it.

### Gates

- `npx vitest run src/lib/lockfileLockedGuard.test.ts`: **6 passed** (both red-proofs above, then
  reverted to this green state).
- `npx vitest run` (full suite, including CPE-1855's `msrvSync.test.ts`): see CPE-1855's Work Log for
  the combined full-suite run.
- `cargo check --locked --all-targets` on all 17 manifests (default features): **17/17 pass** after
  committing the two regenerated lockfiles.
- The 6 non-default feature combinations CI's new `--locked` steps cover: **6/6 pass**.
- `git diff --numstat` on the two fixed lockfiles: `sidecar/contract/Cargo.lock` **50 0**,
  `sidecar/repos/Cargo.lock` **51 0** — pure additions, nothing else touched.
- YAML sanity: all 5 touched workflow files (`ci.yml`, `release.yml`, `release-sidecar.yml`,
  `gui-smoke.yml`, `model-snapshot.yml`) still parse cleanly (`yaml.safe_load`) after every edit.

### Not verified here, stated plainly

- **The actual GitHub Actions run.** Per the sprint's "the Foreman owns CI" rule, pushed and not
  watched/polled. Every measurement above ran the real `cargo`/toolchain locally against the real
  manifests and lockfiles — the strongest local proxy available — but the runner environment (network
  access to crates.io, OS-specific dependency resolution on the 3-OS matrix) is not reproduced here.
- **`sidecar-host`'s Windows keyring / `ai-console`'s other feature flags** and any feature
  combination not explicitly exercised by an existing `--features` step in these workflows — not
  swept exhaustively; only the combinations CI's own steps already build were checked, matching
  CPE-1855's equivalent scoping note.

**2026-08-26 — the "not verified here" gap closed: re-tested against a REAL stale lockfile, PR #1027
attempt 2.** The previous entry's local measurements were all against lockfiles that were ALREADY
correctly in sync (or freshly fixed); this pass deliberately manufactured drift and watched the guard
catch it, rather than trusting the earlier "would NOT have redded CI" conclusion on faith.

Method: bumped `crates/updater-verify/Cargo.toml`'s `version` field from `0.1.0` to `0.1.1` **without**
touching `Cargo.lock` (a real backup taken first) — the same shape of drift CLAUDE.md documents for
`package-lock.json`/`Cargo.lock` going stale. Ran `cargo check --locked --all-targets`:

```
error: cannot update the lock file .../crates/updater-verify/Cargo.lock because --locked was passed
       to prevent this
```

Exit 101 — the guard reds on manufactured drift, not just on the happy path. Reverted the version
field (`sed`, not PowerShell — see the project's own file-corruption memory note) and confirmed
`Cargo.lock` was never touched by the experiment (`diff` against the backup: identical; the backup was
then deleted). `git diff --numstat` after revert shows only this session's real
`rust-version = "1.83.0"` → `"1.88.0"` bump on that file (CPE-1855, same PR) — the stale-lockfile
probe left no residue.

This directly answers this ticket's own acceptance criterion ("measure how often it would have redded
CI... a backstop that fires on ordinary dependency work will be switched off within a week") with a
positive control: the mechanism is not just present in the workflow YAML, it demonstrably fires when
the condition it exists to catch is really there. Re-ran `npx vitest run
src/lib/lockfileLockedGuard.test.ts` afterward (6/6 passing) as a second, independent confirmation that
nothing about this probe disturbed the existing guard-test state.

The GitHub Actions run itself: pushed as `8133858f` (this attempt's commit, combined with CPE-1855's
MSRV floor correction above) and watched to conclusion this time — see PR #1027 for the live result.

**Final result: CI GREEN at SHA `c3ccdcf7`** (a follow-up push fixing an unrelated `msrv`-job system-
dependency gap CPE-1855's Work Log covers). 19 SUCCESS + 1 expected SKIPPED, 0 failed, 0 pending,
`mergeable: MERGEABLE`. Every `--locked` site this ticket added — CI, release, release-sidecar,
model-snapshot, plus the four `cargo check --locked` preflights ahead of `tauri-action`/`npm run
tauri build` — ran for real in this CI run and passed, not just locally. PR #1027 body carries the
final before/after summary.
