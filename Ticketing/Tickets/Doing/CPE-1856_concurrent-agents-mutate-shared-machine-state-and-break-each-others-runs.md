---
id: CPE-1856
title: concurrent agents mutate shared machine state and silently break each other's test runs
type: bug
priority: High
status: Doing
tags: ready
estimate: M
created: 2026-08-22
closed:
---

## Problem

Worktrees isolate the **filesystem**. They do not isolate the **machine**. Two agents working different
tickets share one PATH, one user profile, one tool store — and one of them installing or removing a tool
changes the environment underneath the other, mid-run.

Measured, with timestamps, on 2026-08-21:

- A worker on CPE-1842 needed PowerShell 7 to measure a codec claim, installed it via
  `dotnet tool install --tool-path` into `~/.dotnet/tools`, took its measurements, and **uninstalled it**
  — correctly, by its own lights, cleaning up after itself.
- A worker on CPE-1841 had been running its suite against **that same shim**. Its runs at 22:05, 22:08,
  22:10 and 22:12 were green on PowerShell 7.6.5. Its run starting **22:14:04** went red.
- The removal window: `~/.dotnet/tools` mtime **22:14:12**, `.store` **22:14:13**, `.store/.stage`
  **22:14:14**. `findPowerShellHost()` probed `pwsh` successfully at suite start; individual test spawns
  then hit a shim being deleted underneath them.
- Every run from 22:15:30 onward was green again — silently on a **different host** (Windows PowerShell
  5.1), because the probe fell through.

Two hours of wrong diagnosis followed. The red was blamed on Defender, then on a vitest timeout default.
Both were plausible, neither was the cause, and one of them was written into a PR body as fact.

## Why High

Three distinct harms, and the third is the worst:

1. **A test run went red for a reason nothing in the repo could explain.** Unreproducible by design — the
   evidence deleted itself.
2. **A silent host switch.** The suite kept passing on a *different* interpreter with no notice, so
   "measured on PowerShell 7" became false without any run turning red.
3. **A published claim became unverifiable.** A later reviewer searched the machine, found no `pwsh`, and
   correctly reported the worker had claimed measurements it could not have made. The worker was telling
   the truth; the machine had changed.

This is the same failure family the batch keeps closing — **evidence that quietly stops meaning what it
says** — but at the harness level, where no test can catch it.

## Acceptance criteria

- [x] Dispatch instructions state that installing, uninstalling or upgrading anything machine-global
      (`dotnet tool`, `winget`, `choco`, `npm -g`, `cargo install`, PATH edits) affects every concurrent
      agent, and must be treated as a shared-resource change rather than local setup.
- [x] An agent that needs a tool another agent may be using either scopes it somewhere private, or leaves
      it installed and says so, rather than removing it on the way out. Removal is the harmful half here —
      the install was harmless.
- [x] Any measurement claim records **which host/tool version produced it and how that was determined**, so
      a later reader can tell "measured on X" from "measured on whatever was on PATH". CPE-1841's fix — a
      provenance note per claim — is the shape.
- [x] Where a harness probes for a tool (`findPowerShellHost()` and its equivalents), a mid-run
      disappearance should fail loudly rather than silently falling through to a different implementation.
      Decide whether to pin the host at suite start and assert it stays, and say why either way.
- [x] Sweep for other machine-global state agents touch: environment variables, global git config, ports,
      the cargo registry cache, `%TEMP%`. Enumerate rather than fixing only the tool-install case.

## Work Log

**Both halves addressed.**

### 1. Dispatch instructions (`.claude/commands/sprint.md`)

Added a new `### Shared machine state — a tool install is a shared-resource change, not local setup
(CPE-1856)` subsection immediately after the existing `### Dispatch contract` (CPE-1848), following its
established shape (rationale + a verbatim `>` block for every Worker/Reviewer/UAT dispatch prompt) rather
than inventing a second convention. It:

- names the 2026-08-21 incident with its real timestamps, for teeth;
- states machine-global installs/uninstalls/upgrades affect every concurrent agent and must be treated as
  a shared-resource change, with **removal, not install, named as the harmful half** — leave a shared
  install in place and say so in the Work Log rather than uninstalling on the way out;
- requires a provenance note per measurement/benchmark claim (which host/tool version, how determined) —
  the CPE-1841 shape, referenced explicitly;
- requires a harness tool probe (naming `findPowerShellHost()`) to resolve once, pin for the whole run,
  and **announce the resolved host+version in the run's own output**, and states a pinned host that
  vanishes mid-run must fail loudly rather than the harness silently retrying a different implementation;
- enumerates the sweep the ticket asked for — env vars, global git config, listening ports, the cargo
  registry/build cache, `%TEMP%` — as prose, not code changes to each (per the AC's own "enumerate rather
  than fixing only the tool-install case").
- `sprint-batched.md` already says "everything in sprint.md applies unchanged" for the CPE-1848 dispatch
  contract, so this new subsection is picked up there automatically; no separate edit needed.

**Guard**: extended `src/lib/sprintDispatchAndCiLogGuards.test.ts` (per the crew's own instruction not to
add a near-duplicate file — that file already guards CPE-1848/CPE-1868 prose the identical way) with a new
`describe("sprint.md treats machine-global tool installs as a shared-resource change (CPE-1856)")` block,
6 assertions reading the real `sprint.md` text.

### 2. Observability — the more valuable half (`src/lib/releaseVersionBump.test.ts`)

`findPowerShellHost()` already pinned the host once in `beforeAll` (this file, pre-existing) rather than
re-resolving per spawn — that design decision is kept and now stated explicitly in comments, because
re-probing per call is exactly the shape that let a mid-run disappearance masquerade as "used pwsh" on one
spawn and "quietly used powershell" on the next, *within the same run*. What was missing was (a) any
record in the suite's own output of which host got pinned, and (b) proof a disappearance after pinning is
fatal rather than silently absorbed.

- Added `hostVersion(exe)` (reads `$PSVersionTable.PSVersion.ToString()` back from the resolved host) and
  an unconditional `console.log` in `beforeAll` announcing
  `[CPE-1856] release.ps1 suite pinned to PowerShell host="…" version="…"; resolved once … (never
  re-probed per call).` — **evidence**: this line actually printed on this machine, `powershell` /
  `5.1.26100.9168` (no `pwsh` on PATH here, consistent with the post-incident state CPE-1841 documented).
- Added a `host` test-only override to `BumpOptions`/`runBump` (defaults to the pinned `psHost`; every
  real case in the file is unaffected) so the fail-loud path can be exercised with a **fixture** host name
  instead of touching any real machine-global tool — required by this ticket's own working rules, since
  other agents share this machine's PATH right now.
- Added two `describe` blocks (4 new tests) proving, not asserting:
  - the resolved host/version are real and well-formed, and every ordinary `runBump()` call spawns the
    same pinned host by default;
  - `spawnSync("cpe-1856-host-that-does-not-exist", …)` surfaces `.error.code === "ENOENT"` directly (the
    disappearance mechanism, isolated);
  - `runBump(ALL_DECOYS, NEW, { host: VANISHED_HOST })` **throws** that same `ENOENT`, i.e. the suite reds
    rather than passing, on a vanished host.

**Break-it-and-show-it-red proof** (done by hand, then reverted — see PR): temporarily commented out
`runBump`'s `if (run.error) throw run.error;` line and re-ran
`npx vitest run src/lib/releaseVersionBump.test.ts -t "vanishes mid-run"` — the "DEMONSTRATION: runBump()
with a vanished host THROWS" test went **red** (`expected [Function] to throw an error`), the sibling
"REAL pinned host … unaffected" test stayed green. Restored the line, re-ran full file: 65/65 green again,
including the announcement line. Confirms the new tests actually guard the fail-loud property rather than
passing vacuously.

### Assumptions / judgment calls (logged, not asked)

- Placed the new sprint.md subsection as a sibling of "Dispatch contract" (CPE-1848) rather than folding it
  into that section's own verbatim block, since it's an additive, separately-quotable dispatch instruction
  covering a different failure family (shared machine state vs. background notifications) — matches how
  CPE-1868 also got its own sibling material rather than being merged into 1848's paragraph.
- The AC's "sweep for other machine-global state" is addressed as an **enumerated prose list** in
  sprint.md, not as code fixes to each item (env vars / global git config / ports / cargo cache / `%TEMP%`)
  — the AC itself says "Enumerate rather than fixing only the tool-install case," so no further code
  changes were made for those.
- `findPowerShellHost()`'s **pin-at-suite-start-and-never-re-resolve** design (pre-existing) is kept as the
  answer to "decide whether to pin the host at suite start" — reasoned above and now stated in-file.
- Only `.ts` files and `sprint.md` were touched; no Rust changed, so `cargo clippy`/`cargo test` were not
  run for this ticket (nothing in `crates/server` or `src-tauri` was edited).
- Ticket left in `Ticketing/Tickets/Doing/` with a PR open; not moved to `Done/` from here — that's the
  Foreman's call on merge, per this repo's status-flow convention.

### Gates

- `npm run check` (svelte-check + tsc): 0 errors, 0 warnings.
- `npx vitest run src/lib/releaseVersionBump.test.ts`: 65/65 passed (61 pre-existing + 4 new CPE-1856
  tests), including the printed host/version announcement.
- `npx vitest run src/lib/sprintDispatchAndCiLogGuards.test.ts`: 20/20 passed (14 pre-existing + 6 new).
- Full `npx vitest run` (whole frontend suite): **329 files / 4416 tests passed, 0 failed** — run
  synchronously to completion as part of this ticket, printing the CPE-1856 host-announcement line
  (`host="powershell" version="5.1.26100.9168"`) along the way.

## Notes

Found by the CPE-1841 worker when challenged on a claim it could no longer reproduce. It reconstructed the
timeline from directory mtimes rather than guessing, and retracted its own two earlier hypotheses
(Defender, then the vitest timeout default) once the real cause was in hand.

The vitest 5000ms default it had blamed second was a genuine latent hazard — 19 tests each spawning a
PowerShell process — and was fixed anyway. That is worth noting: a wrong diagnosis surfaced a real
problem, and the fix stands on its own merits.

Related: CPE-1848 (workers stalling on notifications they cannot receive — the other harness-level defect
found this batch), and the concurrent-nightshift coordination hazard already recorded in project memory.
