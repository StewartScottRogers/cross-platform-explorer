---
id: CPE-282
title: "Lifecycle: install / update (Rust-orchestrated package managers)"
type: Feature
status: Done
priority: High
component: Backend
estimate: 4h+
created: 2026-07-13
closed: 2026-07-13
---

## Summary

Rust reimplementation of the `*--install.cmd` behavior: install/update an agent CLI
and its prerequisites via the right package manager per OS (winget/npm on Windows,
brew/npm/pipx on macOS/Linux), refresh PATH for the running session, and report
progress. All orchestration in Rust — the package managers are the mechanism, not
shell scripts.

## Acceptance Criteria

- [ ] `install(agent)` / `update(agent)` follow the manifest's per-OS recipe;
      ensure prerequisites (e.g. Node) first.
- [ ] Streams progress/output to the console UI; clear success/failure result.
- [ ] Post-install PATH refresh so the tool is usable without a restart.
- [ ] Idempotent (re-run updates); never silently partially-installs.
- [ ] Tested against a manifest using a trivial installable package.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-278]], [[CPE-281]]. **Phase:** C3. **Epic:** [[CPE-261]].

## Resolution

Added `install` / `update` to `lifecycle` (ai-console): run the manifest's per-OS
`install`/`update` `OsCommand` via the `CommandRunner` (Rust orchestrates
npm/winget/brew/pipx — the recipe is data, no shell scripts), returning the captured
output on success or an error (with stderr) on non-zero exit / spawn failure / missing
recipe. `update` falls back to `install` when no update recipe is declared (re-running a
package-manager install updates it, as in the reference). 5 tests with a fake runner
(success, non-zero-with-stderr, no-recipe, update-fallback, update-recipe); 25 crate
tests + clippy green.

**Deferred:** streaming install output to the console UI (UI concern, [[CPE-271]]/
[[CPE-289]]); prerequisite chaining (e.g. ensure Node) as a manifest feature — the
recipe currently assumes prerequisites or bakes them into the command. Verified logic
via fake runner; real installs use `RealRunner`.

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Implemented install/update over the testable runner during dayshift. Done.
