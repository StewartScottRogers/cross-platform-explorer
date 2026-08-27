---
id: CPE-1916
title: `apt-get update` fails a whole CI job when a third-party repo 403s — repos we never install anything from
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-26
---

## Summary

The `MSRV (1.88.0) compiles on every manifest` job failed after 20 seconds on PR #1035, with nothing
to do with the code under test:

    E: Failed to fetch https://packages.microsoft.com/repos/azure-cli/dists/noble/InRelease  403  Forbidden
    E: Failed to fetch https://packages.microsoft.com/ubuntu/24.04/prod/dists/noble/InRelease  403  Forbidden
    ##[error]Process completed with exit code 100.

Both failing repos are **`packages.microsoft.com`** — the azure-cli and Microsoft-prod feeds that ship
preinstalled on GitHub's `ubuntu-latest` image. This project installs **nothing** from either. The job
needs `dbus-1` and `glib-2.0` from Ubuntu's own archives, and it never got to ask for them, because
`apt-get update` returns a hard failure when *any* configured source fails.

So a transient outage on a Microsoft feed we do not use fails our build, on a job whose purpose is to
check that Rust compiles.

**This is not a regression of CPE-1787 — it is that ticket working correctly.** CPE-1787 hardened five
unhardened `apt-get` sites that could silently hang. This one failed **loudly and fast**, which is the
improvement. The remaining question is different: it should not fail *at all* for a repo we do not need.

## Acceptance criteria

- [ ] Stop a third-party source's failure from failing the job. Options, in rough order of preference:
      scope `apt-get update` to the sources we actually use; remove the unused Microsoft lists before
      updating (a common, well-understood one-liner on GitHub runners); or tolerate per-source failures
      while still failing hard if a package we genuinely need cannot be found.
- [ ] Whatever you choose, **a missing package we do need must still fail loudly.** The value CPE-1787
      added was that these steps stop hanging and start reporting; do not trade that away for quiet.
      Red-proof it: ask for a package that does not exist and confirm the job still fails clearly.
- [ ] Apply the fix at every `apt-get update` site, not only the MSRV job's. CPE-1787 found five; check
      whether they all carry this exposure.
- [ ] Record which sources the runner image ships preinstalled and which this project actually needs.
      That list is the justification for the fix and the thing a future reader will want.

## Notes

Filed 2026-08-26 by the sprint Foreman after the failure above. This is the **fourth** distinct
environmental CI-flake class observed in a single day, alongside:

- **CPE-1910** — GUI-smoke shard 2 dying on a WebDriver socket failure (three occurrences).
- **CPE-1914** — the layout guard's 15 s CDP timeout being too tight for a cold navigate on a loaded
  machine.
- The GitHub Actions outage itself (a database failure on their side, several hours, unfixable here).

Each is small on its own; together they are the reason a merge queue needs manual re-runs, and on an
unattended overnight run each one is a stall rather than an inconvenience. Worth someone looking at the
set rather than only at the members — the common shape is *an external dependency's bad day failing a
job that did not need it*.

Related: **CPE-1787** (the apt-get hardening that made this fail fast instead of hanging — read it
first), **CPE-1910**, **CPE-1914**.

## Work Log

- Found and fixed all **5** `apt-get update` sites in `.github/workflows/ci.yml` (the same 5 CPE-1787
  hardened) — not just the msrv job's, per the AC:
  1. `backend` job / "Install Linux system dependencies" (webkit2gtk etc, line ~211)
  2. `crates` job / "Install attr (getfattr) for xattr interop test" (line ~549)
  3. `crates` job / "Install ffmpeg (video-thumb real-render test, Linux)" — already
     `continue-on-error: true`, fixed anyway so an unrelated repo outage doesn't needlessly degrade a
     real-render test to a skip
  4. `sidecar` job / "Install Linux system dependencies (libdbus for the keyring)" (line ~1401)
  5. `msrv` job / "Install Linux system dependencies (src-tauri's WebKitGTK + sidecar/host's D-Bus
     keyring)" (line ~1670) — the one that actually failed on PR #1035
- Chose the AC's PREFERRED option: remove the two unused third-party source lists before `update`
  (`sudo rm -f /etc/apt/sources.list.d/azure-cli.list /etc/apt/sources.list.d/microsoft-prod.list`),
  not "tolerate any per-source failure" — that alternative would also swallow a real failure on a
  source we DO care about, trading away exactly what CPE-1787 fought to get (loud, fast failure
  instead of silence/hangs). `apt-get update` itself has no flag to scope which sources it checks, so
  that AC alternative wasn't available. `rm -f` is a no-op if a runner image ever renames/drops either
  file, so this can't become a new failure mode of its own.
- Sources recorded (AC requirement): `ubuntu-latest` ships `archive.ubuntu.com`/`security.ubuntu.com`
  (needed — every package installed above comes from here, left untouched) plus the `azure-cli`
  (`/etc/apt/sources.list.d/azure-cli.list`) and Microsoft-prod
  (`/etc/apt/sources.list.d/microsoft-prod.list`) feeds preinstalled for the `az` CLI and .NET tooling
  (not needed — confirmed no `az`/`azure-cli`/`dotnet`/`microsoft` package referenced anywhere in this
  workflow or any other). These two are exactly the hosts that 403'd in the PR #1035 failure quoted in
  the ticket (`packages.microsoft.com/repos/azure-cli/...` and
  `packages.microsoft.com/ubuntu/24.04/prod/...`).
- Did NOT touch `install` lines, `continue-on-error`, or `timeout-minutes` on any of the 5 steps — a
  package we genuinely need still fails exactly as before.
- Red-proof (AC requirement — "ask for a package that does not exist and confirm the job still fails
  clearly"): this machine has no `apt-get` (Windows), so a live GitHub-runner run isn't possible here.
  Instead verified the actual mechanism that makes a missing-package failure fatal: GitHub Actions runs
  a `run:` block with no explicit `shell:` override under `bash --noprofile --norc -eo pipefail {0}` by
  default on a Linux runner (documented behaviour, not assumed) — `-e` means the FIRST non-zero exit in
  the script kills the step. Built a stand-in script with the exact 3-line shape every fixed site now
  has (`rm -f` a harmless path, a faked `apt-get update` that succeeds, a faked `apt-get install` for a
  nonexistent package that fails with exit 100) and ran it under the identical `bash -eo pipefail`
  flags: it printed the `apt-get`-style error, exited 100, and never reached the following
  "UNREACHABLE" echo — confirming the shell mechanics genuinely propagate a real install failure,
  unaffected by the `rm -f` addition (which itself never fails, by `-f`'s own design, so it can't be
  the thing that goes silent).
- ci.yml still parses as valid YAML after all 5 edits (`python3 -c "import yaml; yaml.safe_load(...)"`,
  since no `js-yaml`/`actionlint` is available locally and no new dependency was added) — same 7
  top-level jobs present, none renamed/reordered/broken.
- Fixing this surfaced a real bug in the CPE-1787 regression guard, `src/lib/ciAptGetHardening.test.ts`:
  its `APT_COMMAND_WORD` regex matched `apt` as an isolated "command word" whenever it wasn't
  surrounded by `[\w-]` — but `/etc/apt/sources.list.d/...` (the path my `rm -f` line references) has
  `/` on both sides of `apt`, which the old regex didn't exclude, so it misread that PATH SEGMENT as a
  sixth unhardened apt-get invocation and false-failed "no apt/apt-get invocation anywhere in ci.yml is
  left unhardened". Fixed by adding `/` to the excluded lookbehind
  (`/(?<![\w\-/])apt(?:-get)?(?![\w-])/`), confirmed red (all 5 hardening tests failed with the false
  positive before the regex fix) then green (all 6 pass after).
- `npm run check`: 0 errors, 0 warnings. Full frontend suite (`npx vitest run`): 335 files / 4605 tests,
  all green, including every test file that reads `ci.yml`
  (`ciAptGetHardening`, `msrvSync`, `lockfileLockedGuard`, `ffmpegOverrideAutoDispatch`,
  `releaseHangHardening`, `sprintDispatchAndCiLogGuards`, `sprintStallControls`).
- **Out of scope, flagged for a follow-up ticket, NOT touched here** (kept surgical to `ci.yml` per this
  sprint's file-ownership note — a sibling worker is on the release-channel guard scripts and
  `release-sidecar.yml`): `gui-smoke.yml` and `release.yml` also run `apt-get update` and carry the
  identical exposure to the same two unused Microsoft feeds. Same fix (the `rm -f` one-liner) should
  apply there too, but is left for a separate ticket rather than widening this PR's footprint.
