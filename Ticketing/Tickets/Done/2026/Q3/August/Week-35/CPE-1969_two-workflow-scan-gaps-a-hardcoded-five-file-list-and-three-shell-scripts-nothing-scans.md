---
id: CPE-1969
title: two workflow-scan enumeration gaps — `lockfileLockedGuard` hard-codes five files, and **no consumer scans `.github/workflows/scripts/*.sh` at all**
type: task
priority: Medium
status: In Progress
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

Found by PR #1080's Reviewer while verifying CPE-1936's heredoc fixes. Both are **CPE-1932** — *enumerate,
don't recall* — and both were invisible until someone swept the directory instead of reading a list.

**Gap 1 — `src/lib/lockfileLockedGuard.test.ts`'s `WORKFLOW_FILES` is a hard-coded five-file list.**
There are **eight** workflow files. A sixth workflow that builds Tauri without `--locked` is simply not
looked at. CLAUDE.md's own rule: *"Any guard over 'all the X in this repo' derives its list at run time
(`git ls-files`, a tree walk) and fails loudly when the list comes back near-empty — a hard-coded list
of the instances someone remembered is how seventeen Cargo.lock files became two."*

**Gap 2 — nothing scans `.github/workflows/scripts/*.sh`.** Three scripts (39 / 50 / 20 lines,
measured), invoked *by* the workflows, and **no consumer reads them**: not the hang-hardening scan, not
the lockfile guard, not channel purity, not either Rust consumer. Every guard built on
`shellScriptLines` stops at the `run:` block boundary, and shell that has been *moved out* of a `run:`
into a file is out of scope by construction.

Gap 2 is the more interesting of the two, because **extracting shell into a script is normal, good
refactoring** — and here it silently removes that shell from every guard. Nobody did anything wrong;
the guards' scope just never followed the code.

## Why now

CPE-1936 (PR #1080) just fixed two `shellScriptLines` mis-parses, one of which (**N8**) had made **160
logical lines of `ffmpeg-pin-freshness.yml` invisible** to the hang-hardening scan — including an
`exit 1` and a whole error branch. Nothing was answering wrongly only because the blind window happened
to contain no `curl`, no `apt`, no `--expect-channel`, no `--locked`.

**These two gaps are the same defect one level up:** the parser now reads its input correctly, and the
input is still not everything it should be. A guard that parses perfectly over the wrong file set is
exactly as blind.

## Acceptance criteria

- [ ] **Derive both lists at run time** (`git ls-files '.github/workflows/*.yml'`,
      `git ls-files '.github/workflows/scripts/*'`) and **fail loudly when either comes back
      near-empty** — that near-empty check is the half CLAUDE.md singles out, and it is the half that
      gets left off.
- [ ] **Before changing anything, run each guard over the newly-included files and report what it
      finds.** If a sixth workflow or one of the three scripts contains an unhardened command, an
      unlocked Tauri build, or a `--expect-channel` invocation, **that is a live defect and needs its
      own ticket** — do not fold a real finding into the enumeration fix.
- [ ] **Decide what "a step" means for a standalone `.sh`.** The `step.run` consumers are built around
      YAML steps; a script file has no step. Say how you map it, and whether per-step and whole-file
      views both make sense there.
- [ ] **Check the other `shellScriptLines` consumers for the same scope gap** — `releaseHangHardening`,
      `channelPurityCoverage`, `release_workflow_wiring.rs`, `artifact_binding.rs`. The last two read
      only `release.yml` and `release-sidecar.yml`; say whether that is a deliberate scope or another
      remembered list.
- [ ] **Red-proof each**: add a sixth workflow (or a fourth script) containing exactly the thing the
      guard hunts, confirm it is now caught, and confirm the near-empty check fires when the glob
      matches nothing.
- [ ] Keep the shared oracle honest — if scanning `.sh` files needs any parser behaviour the YAML path
      does not, it belongs in `src/lib/shellScriptLines.cases.json` so **both** implementations get it.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1080's Reviewer, which swept the workflows directory
and the `scripts/` subdirectory rather than trusting either list, and flagged both as out of scope for
that PR — correctly.

Related: **CPE-1936** (the heredoc mis-parses, PR #1080 — where the 160-line blind window was
measured), **CPE-1932** (enumerate, don't recall — and the seventeen-Cargo.lock incident),
**CPE-1849** (which folded `ffmpeg-pin-freshness.yml` into `GUARDED`), **CPE-1933** (anchor on code,
never on prose — the rule `shellScriptLines` exists to serve).

## Work Log

### 2026-08-27 — implemented (rebased on `c1287d42`; CPE-1936 / PR #1080 already merged)

**Step 0: measured before changing anything (acceptance criterion 2).** A throwaway probe ran every
guard's own predicates over every file the guards were NOT reading — the three unscanned workflows
(`catalog-freshness.yml`, `ffmpeg-pin-freshness.yml`, `release-pipeline-watchdog.yml`) and all three
`.sh` scripts. **No live defect was found in CI**, so nothing is folded into this enumeration fix:

| scan | over the newly-included files | result |
|---|---|---|
| `cargo (build/test/check/clippy/run)` without `--locked` | 3 workflows + 3 scripts | **0 invocations at all** |
| `tauri-action` / `npm run tauri build` anchors | 3 workflows + 3 scripts | **0 anchors** |
| unhardened `apt`/`apt-get` | all 8 workflows + 3 scripts | **0 unhardened**; 4 sites in `gui-smoke.yml` were guarded by nothing but are correctly hardened |
| `curl --retry` + `--max-time` without `--retry-max-time` | all 8 workflows + 3 scripts | **0 offenders**; 2 new curl sites (`catalog-freshness.yml` already carries `--retry-max-time 20`; `model-snapshot.yml` passes no `--retry`, so the pairing rule does not apply) |
| `--expect-channel` / `verify-release-artifacts` | all 8 workflows + 3 scripts | **only the 2 known release-workflow sites** |

The scripts' logical-line counts confirm the ticket's figures exactly: 39 / 50 / 20.

**One real finding, but in a guard rather than in CI.** Widening the apt scan to `gui-smoke.yml`
exposed a false positive in the shared `APT_COMMAND_WORD`: `echo "waiting for background apt/dpkg
lock ..."` matched, because CPE-1916 excluded `/` from the LOOKBEHIND but not the lookahead. The
widened scan would have false-failed on its first run. Fixed symmetrically (`(?![\w\-/])`) with the
reasoning at the site, and red-proofed — reverting it turns 3 tests red. Not filed separately: it is
not a CI defect, it is this change's own prerequisite.

**What landed.**

* **New `src/lib/workflowShellSources.ts`** — the one enumeration. `discoverWorkflows()` and
  `discoverWorkflowScripts()` walk the directory at run time and **refuse a near-empty result**
  (`MIN_EXPECTED_WORKFLOWS = 8`, `MIN_EXPECTED_WORKFLOW_SCRIPTS = 3`, shaped after
  `MIN_EXPECTED_NPM_PROJECTS`). A tree walk rather than `git ls-files` on purpose: it sees an
  untracked workflow, and it takes a `root`, so the refusal is tested against a real fixture rather
  than argued for in a comment. A file in `scripts/` that is neither shell nor documentation is a
  **loud failure**, not a silent skip — that is gap 2's whole lesson.
* **"A step" for a standalone `.sh` = the WHOLE FILE, exactly one unit.** Reasoning recorded in the
  module header: a YAML step delimits one shell process, one `timeout-minutes` cap and one name, and
  all three coincide with the file; and `logicalLines` is a cross-line state machine (heredocs,
  continuations), so any finer split would cut that state mid-flight and manufacture exactly the
  blind window CPE-1936 measured. Per-step and whole-file are therefore the same view here. The one
  assertion that genuinely cannot be posed against a script is `releaseHangHardening`'s per-step
  ARITHMETIC (N curl calls under one cap) — a script has no cap of its own, so that check stays
  attached to the calling YAML step. `lockfileLockedGuard` refuses a Tauri build inside a script for
  the mirror-image reason: there is no preceding step to carry the `--locked` preflight.
* **`lockfileLockedGuard.test.ts`** — `WORKFLOW_FILES` deleted; both lists derived; the scripts
  scanned; `MIN_CARGO_INVOCATIONS` gains a staleness check so a floor cannot outlive its file.
  10 to 22 tests.
* **`releaseHangHardening.test.ts`** — `GUARDED` (4 remembered files) deleted; the curl-pairing scan
  now walks the derived enumeration, plus a new derived "no apt invocation anywhere in CI is left
  unhardened" backstop covering all 8 workflows + 3 scripts (previously three separate remembered
  lists covering 3 of them). 26 to 40 tests.
* **`channelPurityCoverage.test.ts`** — `BUILD_JOB_FOR_WORKFLOW` stays as-is (deliberate scope: only
  a release workflow can carry a channel gate, and widening it would let an unrelated
  `--expect-channel` read as coverage). What WAS prose is now derived: a new test asserts no workflow
  or script outside the mapped set really invokes `verify-release-artifacts`.
* **The two Rust consumers: deliberate scope, verdict written at the site**
  (`release_workflow_wiring.rs`). They do not sweep a file class — they read ONE named invocation's
  argv out of the workflow that states it, then execute the real binary with it. Deriving a file list
  there would only find files with no argv to read. The half that WAS a remembered list — "and
  nothing else invokes the verifier" — is now enforced on the TS side, where the enumeration and the
  YAML parser already live; a second Rust copy would be the very duplication this ticket exists to
  stop. Same verdict for `artifact_binding.rs`.
* **Shared oracle kept honest** — 4 cases added to `shellScriptLines.cases.json` for the shapes only
  a `.sh` brings: a `#!` shebang, a shell function body, an indented heredoc inside a function
  closing on a column-0 terminator, and a `case` pattern's empty `''` quote pair. The Rust port
  agrees with all four; red-proofed by editing one case and watching
  `the_port_matches_the_typescript_reference_on_every_shared_case` panic naming that exact case.

**Red-proofs — all run, all against real fixture directories under `.claude/tmp/`, all cleaned up.**
A sixth workflow with `cargo build --release` reported and named. A fourth script with a comment
decoy plus a backslash-continued `cargo clippy` reported as one joined line, comment ignored. A
fourth script with an offending `curl --retry ... --max-time` split over a continuation reported. A
sixth workflow with `sudo apt-get install -y foo` reported. A script heredoc body containing
`cargo build --release` still inert. Empty directory / missing `.github` / one-survivor partial
enumeration — **all three throw** rather than reporting clean. The `apt/dpkg` regex reverted:
3 tests red. One case-file entry edited: the Rust port panics.

**Gates.** `npm run check`: 0 errors / 0 warnings. `npm test`: 354 files, 5157 passed, 2 skipped.
`crates/updater-verify`: `cargo clippy --locked --all-targets -- -D warnings` clean,
`cargo test --locked` 147 passed (that crate declares no `[features]`, so one mode is all there is).

---

### Round 2 — Reviewer APPROVE with five notes; four fixed, one informational

Rebased onto `origin/main` @ `e275808e` (#1070 merged since the branch point); rebase clean, no
conflicts. The Reviewer's verification was exhaustive and independently reproduced every round-1
measurement, so none of it was re-run — only the four notes were worked.

**N1 — gap 2, one level down, inside the fix for gap 2. Decision: REFUSE, not recurse.**

Round 1's `fileNames()` filtered `statSync(...).isFile()` on a flat `readdirSync`. An unclassified
*file* was a loud failure — the whole point — but `.github/workflows/scripts/helpers/foo.sh` would
have produced no error, no report and no entry. Latent (no subdirectory exists today), which is
exactly how the original gap sat undetected.

Refusing beats recursing here for a reason this module already argues at length. The header's
**"one `.sh` file is exactly one unit"** mapping rests on a specific premise: a standalone script is
ONE executed process, with ONE inherited cap and ONE path to report against, so the three things a
YAML step delimits all coincide with the file. A `helpers/` fragment is precisely the case where that
premise is **false** — it is `source`d *into* another script, not separately executed. Recursing
would apply the mapping's conclusion where its stated reason does not hold, and would emit a "step"
that the per-step arithmetic carve-out cannot be posed against.

It is also not one decision but three wearing one coat: a `helpers/` of sourced fragments should be
scanned but attributed to its caller; a `fixtures/` of deliberately-broken shell must **not** be
scanned; a vendored tool tree must not either. Recursion silently picks "scan it as a standalone
unit" for all three, and silently picking a policy is the failure mode this ticket is about. The
refusal makes whoever adds the first subdirectory state its unit mapping instead of inheriting a
guess — and it is the cheap direction to be wrong in, since a refusal names the directory in a red
test while round 1's silence named nothing.

Closed in **both** walks, not just the one the note raised. `discoverWorkflows` now refuses any
subdirectory other than `scripts/` — and that exclusion is **derived**, sliced off
`WORKFLOW_SCRIPTS_DIR` rather than typed a second time, so renaming the constant moves it. GitHub
reads workflow YAML only from the top level, so a `.yml` in a new subdirectory is a file that does
not run and must not be mistaken for one that does.

*Red-proof (real subdirectory on disk, real offending script — `helpers/foo.sh` carrying
`cargo build --release` and `sudo /usr/bin/apt-get update`, i.e. one live defect for each of the two
guards that would have missed it):* `if (false) refuseSubdirectories(...)` → **2 red**
(`discoverWorkflowScripts` throws naming `helpers`; `allShellUnits` throws too). A third case pins
the defect itself as a measurement — a flat file-only read returns `a.sh b.sh c.sh` and nothing about
`foo.sh` — so the test states what the refusal is buying, not just that it fires. Restored: 22/22
green. (One self-inflicted correction: the `allShellUnits` case first failed on the *workflow* floor,
because `allShellUnits` enumerates workflows before scripts; the fixture now carries a full
complement of workflows so the refusal under test is the one actually reached.)

**N4 — a live hole in the apt guard, folded in on the Foreman's call.**

The Reviewer suggested a separate ticket; the Foreman overrode that, and the override is recorded
here as asked. The reasons: it is **the same regex in the same file** this PR is already editing, the
context is already loaded, and leaving a known hole in a hardening scan when the fix is one lookbehind
is worse than a slightly wider PR. The hole is also the exact mirror of the false positive round 1
fixed — pattern-wise they are one finding, and splitting them across two tickets would have left the
next reader with half the story at each site.

`sudo /usr/bin/apt-get update` matched **neither** the old regex nor round 1's, because CPE-1916's
lookbehind exclusion of `/` swallows any absolute-path invocation. Real, completely unhardened, and
invisible since CPE-1916. The left exclusion is gone; distinguishing a path from a command now rests
entirely on what **follows** — a path segment is followed by `/` or `.`, a command word by whitespace
or end of line.

*Sweep, old / current / new, 26 shapes* (the Reviewer's 22 plus four added for the path-prefixed and
`apt.conf` forms). Disagreements with intent: **OLD 7, CURRENT 5, NEW 0.** Cells that move, and only
these:

| shape | old | cur | new |
|---|---|---|---|
| `sudo /usr/bin/apt-get update` | . | . | **Y** |
| `/usr/bin/apt install -y foo` | . | . | **Y** |
| `exec /usr/bin/apt-get -o Acquire::Retries=3 update` | . | . | **Y** |
| `  /usr/bin/apt-get update` (path-prefixed, inside a script) | . | . | **Y** |
| `bash .github/workflows/scripts/x.sh && /usr/local/bin/apt-get -y clean` | . | . | **Y** |
| `echo "waiting for background apt/dpkg lock…"` | Y | . | . |
| `echo "apt/dpkg" && ls` | Y | . | . |
| `ls /etc/apt` *(ambiguous — see below)* | . | . | **Y** |
| `sudo rm -rf /var/cache/apt` *(ambiguous)* | . | . | **Y** |

Every real-invocation shape (13 of them) matches in all three. The five newly-matching cells are the
intended fix; the two `echo` cells are round 1's fix, unmoved.

**The first draft over-corrected and the sweep caught it**, which is the reason for running it: with
only the lookbehind dropped, `cat /etc/apt/apt.conf.d/99custom` matched on `apt.conf` — one
unintended cell. `.` joined `/` in the lookahead, and that cell went back to `.`. Both halves are
pinned as tests.

*Repo cross-check* (the direction a shape table cannot cover): all 29 `.yml`/`.sh`/`.mjs`/`.ps1`
files in the tree, line by line — **52 lines match under CURRENT, 52 under NEW, 0 lines match NEW but
not CURRENT.** No live behaviour change today; the fix is entirely about what a *future* line can
hide.

*The residue, stated at the site rather than papered over.* A path **tail** is genuinely undecidable
from the token alone: `/usr/bin/apt` (a command) and `/etc/apt` (a directory) are the same string
shape. The regex resolves that towards **matching**, deliberately — a false positive is a red test
naming the exact line and costs one reviewed exclusion, while a false negative is a six-hour IPv6 apt
hang with every guard green, which is what actually shipped between CPE-1916 and now. No line in the
repo hits the ambiguous shape today. The comment says explicitly: exclude at the **call site** if one
ever does, and do **not** put `/` back in the lookbehind — that is the widening that created this
hole, and it would take all five path-form invocations down with it.

*Red-proof, both directions:* restore `/` to the lookbehind → **1 red** (5 assertions). Drop `.` from
the lookahead → **1 red**. Restored: 47/47 green across both apt suites.

**N2 — one declaration, not two.** `/\bnpm run tauri build\b/` was written twice; the inline copy in
the script refusal is gone, replaced by `isTauriBuildAnchor({ run: unit.run })`. A script has no
`uses:`, so only the `run` branch can fire — but the point is that widening the anchor later now
follows into the refusal instead of silently not doing so. *Red-proof:* widen the one remaining
declaration to `npx tauri build` → the workflow-side anchor test reds (`anchorsSeen` 4 → 0), and
`grep` confirms exactly **one** occurrence of the pattern in the file.

**N3 — a matcher whose strictness flips sign in its new role.** `isRealInvocationLine`'s refusals are
fail-**safe** when crediting coverage (rejecting a decoy can only under-credit) and fail-**open** in
the new "nothing outside the map" role (every line it declines to call an invocation is a line it
declines to *report*). Two roles, two predicates: the stray check now uses
`mentionsVerifierInvocation`, widened in exactly the two ways the coverage predicate is narrow —
`cargo run` need only be a **command word** rather than start the line (which admits any prefix
without enumerating prefix words, the recall-list trap), and `|`/`;` are not disqualifying, since
exit-code laundering is a coverage question and a laundered invocation still needs mapping.

Named at the site, with the shapes spelled out: `exec …`, `sudo …`, `env … `, `timeout 60 …`,
`… | tee`, `… ; echo done`, `cd repo && …` — all seven are real invocations the coverage predicate
rejects, all seven now reported. The one deliberate over-report (`echo cargo run --bin …`, the
round-3 decoy) is pinned as a divergence test rather than left to be discovered: same fail-closed
trade as the apt path tail, and the comment cross-references it so the two decisions read as one
principle instead of two ad-hoc calls. *Red-proof:* point the stray check back at
`isRealInvocationLine` → **8 red**.

**N5 — informational, no action taken.** Scripts get no `MIN_CARGO_INVOCATIONS` floor. Correct today:
the true count is 0, so a floor would be vacuous, and the staleness check covers the other direction.

**Gates (round 2), and the delta.** `npm run check`: **0 errors / 0 warnings** (unchanged).
`npm test`: **356 files, 5205 passed, 2 skipped** — the merged baseline was 356 / 5189, so **+16
tests, +0 files, 0 failures**. `crates/updater-verify`: `cargo clippy --locked --all-targets -D
warnings` clean; `cargo test --locked` **147 passed** (79 + 31 + 21 + 13 + 2 + 1, unchanged — no Rust
file was touched this round, and no Rust copy of the apt vocabulary exists: the only `apt` mention in
that crate is one word inside a doc comment). `node scripts/ratchet-baselines.mjs compare
origin/main`: **exit 0**, all 12 baselines unchanged. `git diff --numstat` shows line-level diffs on
all six files (no whole-file CRLF rewrite).

**Tooling notes heeded.** Every repo-file write went through the `Edit` tool — no `sed -i`, no
PowerShell — and `--numstat` was checked afterwards to confirm no whole-file re-encode. The worktree's
`node_modules` is a **junction**, left in place rather than `rm -rf`'d.
