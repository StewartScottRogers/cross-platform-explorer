---
id: CPE-1787
title: ci.yml has five unhardened apt-get sites that can silently hang
type: task
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-19
closed:
---

## Problem

CPE-1772 (GUI-smoke shard hang) root-caused a class of silent, zero-output `apt-get` hang on
GitHub-hosted `ubuntu-latest` runners: `apt-get update` fetches every `InRelease` file, then stops —
no error, no further output — before fetching any `Packages`/`Translation` index. Reproduced live and
repeatedly on PR #935's own CI (`gui-smoke.yml`'s "Install Linux system dependencies", four separate
job instances across two different pushes, always at the exact same point). Current best account, not
conclusively proven: a GitHub-hosted-runner IPv6-connectivity stall (a runner advertises non-routing
IPv6, a client that tries it first hangs on the OS TCP timeout with zero output before ever falling
back to IPv4). Mitigated there with `Acquire::ForceIPv4=true` plus explicit `Acquire::Retries`/
`Acquire::http::Timeout`/`Acquire::https::Timeout`, and a `timeout-minutes` cap on every affected step
so a hang fails fast and named instead of riding to the job's full cap.

`ci.yml` has FIVE more `apt-get update && apt-get install` sites with none of that hardening — no
`ForceIPv4`, no explicit retries/timeouts, and (for four of the five) no step-level `timeout-minutes`
at all, so a hang here rides all the way to the job's default cap (no `timeout-minutes` is set on
either the `backend` or `crates` jobs, so GitHub's own 360-minute maximum is the only backstop):

1. `backend` job, "Install Linux system dependencies" (`ubuntu-latest` only) — installs the WebKitGTK
   dev packages the app compiles against.
2. `crates` job, "Install attr (getfattr) for xattr interop test" — required for the native-metadata
   OS-interop test to run for real instead of silently skipping.
3. `crates` job, "Install ffmpeg (video-thumb real-render test, Linux)" — `continue-on-error: true`,
   but that only helps once the step reaches a terminal state; with no `timeout-minutes` a genuine
   hang here never reaches one. **This is not hypothetical**: this exact step hung with zero output
   for 1.5+ hours on PR #935's own `Server crates (ubuntu-latest)` job while CPE-1772 was being
   investigated (observed live via `gh api .../jobs/<id>` step polling, `started_at` vs. wall clock),
   and had to be cancelled and rerun manually to get a verdict.
4. `sidecar` job, apt-get update + `libdbus-1-dev pkg-config` install.
5. `sidecar` job, `gnome-keyring` install (shares site 4's `apt-get update`, separate `install -y`).

`ci.yml` runs on every push and PR — far more often than `gui-smoke.yml` — so if the IPv6-stall
diagnosis is right, it applies here at least as often, and currently has no fail-fast behavior at all:
a hang is indistinguishable from the job simply being slow until someone notices it has been "running"
for an hour.

## What to do

- Apply the same hardening `gui-smoke.yml` now has to all five sites: `Acquire::ForceIPv4=true` plus
  explicit `Acquire::Retries`/`Acquire::http::Timeout`/`Acquire::https::Timeout` on both `apt-get
  update` and `apt-get install` invocations, and a `timeout-minutes` on each step sized against its
  normal cost (site 3 in particular — `continue-on-error: true` with no timeout is the exact
  configuration that let PR #935's live hang run for 1.5+ hours instead of failing fast and being
  swallowed by `continue-on-error` as designed).
- Confirm whether `ForceIPv4` actually reduces the STALL RATE here (not just whether the step still
  completes — a step can pass while still occasionally hanging on a bad roll) by watching a run of
  `ci.yml` for a hang before/after, same honesty standard CPE-1772 held itself to: a fix like this
  cannot be proven by one green run.
- Consider whether the `crates` job (and `backend`, which also has no job-level cap) should get a
  job-level `timeout-minutes` as a backstop, independent of the per-step hardening — GitHub's 360-minute
  default is not a meaningful cap for anything that matters in practice.

## Acceptance criteria

- [ ] All five `apt-get` sites in `ci.yml` get the same `ForceIPv4`/retry/timeout treatment as
      `gui-smoke.yml`'s two sites.
- [ ] Site 3 (`Install ffmpeg`) specifically gets a `timeout-minutes` so `continue-on-error` can
      actually do its job on a hang, not just on a fast failure.
- [ ] State honestly whether the fix measurably reduces hang frequency, or only makes an unavoidable
      hang fail faster/louder — do not claim more than the evidence supports.

## Notes

Filed by the CPE-1781/CPE-1772 PR (#935) reviewer during that PR's review — scoped out of that PR
(which is `gui-smoke.yml` + `wdio.conf.ts` only, per its own brief) so the diagnosis it produced isn't
stranded. See that PR's commits and comments for the full CPE-1772 root-cause writeup this ticket
extends to `ci.yml`.

## Work Log

- 2026-08-20 — Applied gui-smoke.yml's `ForceIPv4`/retry/timeout hardening
  (`-o Acquire::ForceIPv4=true -o Acquire::Retries=3 -o Acquire::http::Timeout=20
  -o Acquire::https::Timeout=20`) to all five `apt-get` sites in `ci.yml`, on both the `update` and
  `install` invocation of each:
  1. `backend` job, "Install Linux system dependencies" — `timeout-minutes: 8` (new).
  2. `crates` job, "Install attr (getfattr) for xattr interop test" — `timeout-minutes: 5` (new).
  3. `crates` job, "Install ffmpeg (video-thumb real-render test, Linux)" — `timeout-minutes: 8`
     (new); `continue-on-error: true` unchanged. This is AC2 — the site that hung for 1.5+ hours on
     PR #935's own CI with no cap at all, so `continue-on-error` never got the chance to act.
  4. `sidecar` job, "Install Linux system dependencies (libdbus for the keyring)" —
     `timeout-minutes: 8` (new).
  5. `sidecar` job, the `gnome-keyring` install inside "host — real keychain round-trip (CPE-322)" —
     `timeout-minutes: 10` (new, sized to leave room for the round-trip test that follows in the
     same step); `continue-on-error: true` unchanged.

  Scope: `.github/workflows/ci.yml` only — the ffmpeg pin guard job and the CPE-1802
  override-dispatch step were left untouched, per the ticket's own scope and the Foreman's
  instruction.

  New guard test `src/lib/ciAptGetHardening.test.ts` (6 assertions) parses `ci.yml` structurally via
  the repo's bounded-subset YAML parser (`src/lib/preview/yaml.ts`) rather than regex-over-text, per
  the CPE-1802 review lesson (a text regex can be satisfied by a neighbouring comment). One test per
  site plus a repo-wide regression guard ("no apt-get invocation anywhere in ci.yml is left
  unhardened"). Every assertion was red-proofed: deleted the single line it protects, ran
  `npx vitest run src/lib/ciAptGetHardening.test.ts`, confirmed the named test (and, for sites 1/4,
  the regression-guard test too) failed, then reverted. Full list of what was deleted/observed red
  is in the PR description.

  **AC3, stated honestly**: this PR does not — cannot — measure whether `ForceIPv4` reduces the
  underlying stall rate; that requires watching real `ci.yml` runs over time, which is out of scope
  for a single PR's own CI (one green run proves nothing, the same standard CPE-1772 held itself
  to). What this change unambiguously does: turns a hang at any of the five sites into a fast,
  named, ~5-10 minute failure instead of one that could previously ride to the job's default
  360-minute cap (no `timeout-minutes` existed on any of these five steps before). Site 3
  specifically goes from "can swallow a 1.5+ hour wall-clock hang under `continue-on-error`" to
  "fails loud in 8 minutes, then `continue-on-error` swallows THAT". Job-level `timeout-minutes` on
  `backend`/`crates` (mentioned in "What to do" as something to "consider") was left out — it's not
  one of the three binding acceptance criteria, and the Foreman scoped this ticket to the per-step
  apt-get hardening only.

  Gates: YAML-parsed `ci.yml` (`python -c "import yaml; yaml.safe_load(...)"` — OK); `npm run check`
  (0 errors, 0 warnings); `npx vitest run` (318 files / 4195 tests passed, including the new guard
  file).

- 2026-08-20 — **Reviewer round on PR #970 (approved, one required fix before merge).** Reviewer
  confirmed no blocking defects against the three binding ACs, re-ran two red-proofs itself (matched),
  confirmed the parser fails loud rather than silently, confirmed the ffmpeg pin guard job and the
  CPE-1802 override-dispatch step were untouched, and reproduced the gates exactly. It also confirmed
  two things the PR raised honestly rather than glossed: `Acquire::http::Timeout` genuinely bounds a
  stalled mid-transfer (not just connection setup — `apt-transport-http(1)` documents it applying "to
  the connection as well as the data timeout"), matching the observed symptom; and
  `continue-on-error: true` does swallow a step killed by its own `timeout-minutes`, so sites 3 and 5
  behave exactly as intended (fail fast and loud, then get swallowed) with no unintended job-outcome
  change.

  **Required fix**: the Reviewer applied pressure by injecting a brand-new, fully unhardened step into
  the `backend` job using bare `apt update` / `apt install -y jq` (a common, functionally identical
  alias to `apt-get`) with no `timeout-minutes` — all 6 guard tests still passed, because
  `aptGetLines()` filtered lines on the literal substring `"apt-get"` only. Fixed by matching
  `apt`/`apt-get` as an isolated command word via
  `/(?<![\w-])apt(?:-get)?(?![\w-])/` (lookbehind/lookahead on non-word/non-hyphen boundaries), so it
  does not false-positive on `apt-transport-https`, `adapter`, or `apt-get-wrapper`. Added a doc
  comment on the new `APT_COMMAND_WORD` regex explaining which spellings it covers and why. Re-proved
  by injecting the Reviewer's exact repro step (bare `apt update` / `apt install -y jq` in the
  `backend` job) — regression guard failed, naming both injected lines by name
  (`backend / TEMP unhardened bare-apt injection (red-proof, must not survive): sudo apt update` and
  `... sudo apt install -y jq`) — then reverted with `git checkout -- .github/workflows/ci.yml`,
  confirmed all 6 tests green again.

  Scope held: did NOT touch the identical unhardened pattern the Reviewer also found in
  `release.yml:49-54,201`, `release-sidecar.yml:130-138`, `ci.yml`'s `brew`/`choco` ffmpeg installs, or
  the pdfium `curl` fetches missing `--max-time` — the Foreman is filing that as its own ticket.

  Gates re-run on the fix: `python -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"`
  — OK; `npm run check` — 0 errors, 0 warnings; `npx vitest run` — 318 files / 4195 tests passed.
  Pushed to `cpe-1787-apt-get-hardening` (`f65feb96`), PR #970 now MERGEABLE at that head.
