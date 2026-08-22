---
id: CPE-1856
title: concurrent agents mutate shared machine state and silently break each other's test runs
type: bug
priority: High
status: Backlog
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

- [ ] Dispatch instructions state that installing, uninstalling or upgrading anything machine-global
      (`dotnet tool`, `winget`, `choco`, `npm -g`, `cargo install`, PATH edits) affects every concurrent
      agent, and must be treated as a shared-resource change rather than local setup.
- [ ] An agent that needs a tool another agent may be using either scopes it somewhere private, or leaves
      it installed and says so, rather than removing it on the way out. Removal is the harmful half here —
      the install was harmless.
- [ ] Any measurement claim records **which host/tool version produced it and how that was determined**, so
      a later reader can tell "measured on X" from "measured on whatever was on PATH". CPE-1841's fix — a
      provenance note per claim — is the shape.
- [ ] Where a harness probes for a tool (`findPowerShellHost()` and its equivalents), a mid-run
      disappearance should fail loudly rather than silently falling through to a different implementation.
      Decide whether to pin the host at suite start and assert it stays, and say why either way.
- [ ] Sweep for other machine-global state agents touch: environment variables, global git config, ports,
      the cargo registry cache, `%TEMP%`. Enumerate rather than fixing only the tool-install case.

## Notes

Found by the CPE-1841 worker when challenged on a claim it could no longer reproduce. It reconstructed the
timeline from directory mtimes rather than guessing, and retracted its own two earlier hypotheses
(Defender, then the vitest timeout default) once the real cause was in hand.

The vitest 5000ms default it had blamed second was a genuine latent hazard — 19 tests each spawning a
PowerShell process — and was fixed anyway. That is worth noting: a wrong diagnosis surfaced a real
problem, and the fix stands on its own merits.

Related: CPE-1848 (workers stalling on notifications they cannot receive — the other harness-level defect
found this batch), and the concurrent-nightshift coordination hazard already recorded in project memory.
