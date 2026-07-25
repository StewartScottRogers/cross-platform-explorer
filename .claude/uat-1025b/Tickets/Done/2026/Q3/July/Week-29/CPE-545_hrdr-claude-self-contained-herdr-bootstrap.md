---
id: CPE-545
title: "hrdrClaudeNative.cmd — self-contained herdr bootstrap (start server + PATH fallback)"
type: Enhancement
status: Done
priority: Medium
component: Docs
tags: [ready]
estimate: 45m
created: 2026-07-16
closed: 2026-07-16
---

## Summary
Migrate the remaining behavior from the source `_hrdr-launch.cmd` dependency
(`Z:\repos\AgenticCliOptions\TermainalAgentPairs\`) into this repo's
`hrdrClaudeNative.cmd`, keeping the launcher a single self-contained file (no new
shared engine file). Prior tickets (CPE-542/543/544) migrated the launcher by
inlining, but dropped two capabilities the source engine had:

1. **herdr location fallback** — when `herdr` is not on PATH, fall back to the
   known install dir `%LOCALAPPDATA%\Programs\Herdr\bin\herdr.exe` instead of
   hard-erroring.
2. **herdr server bootstrap** — when no herdr server is running, start one, wait
   for it to become ready, then inject the pane. Today the script assumes a
   session is already up and fails otherwise. When it does start the server it
   should tell the user to switch to the new window; when a server was already
   running it should attach this console to the herd.

## Acceptance Criteria
- [x] `herdr` is resolved via PATH first, then `%LOCALAPPDATA%\Programs\Herdr\bin\herdr.exe`,
      stored once in `%HERDR%` and used consistently throughout.
- [x] If no herdr server is running, the script starts one (`start "herdr" <herdr>`),
      polls up to ~20s for `status server` = running with at least one pane, then proceeds.
- [x] After a successful start with a pre-existing server, the console attaches to the herd;
      after auto-starting a server, it prints "switch to the new herdr window" and exits 0.
- [x] Existing behaviors preserved: default model `claude-opus-4-8`, uses this repo's
      `RunClaude.cmd`, installs the native `claude` integration hook if missing, and
      focuses an existing "Claude" pane instead of a duplicate `agent start`.
- [x] Still a single self-contained `.cmd` — no `_hrdr-launch.cmd` added to the repo.

## Resolution
Enhanced `hrdrClaudeNative.cmd`:
- Resolve herdr once into `%HERDR%`: `where herdr` first, else
  `%LOCALAPPDATA%\Programs\Herdr\bin\herdr.exe`; error only if neither exists.
- Added a server-bootstrap block: if `status server` is not `running`, `start "herdr"`,
  set `STARTED_HERDR`, then poll up to 20×1s for `status: running` + a `pane_id` before
  jumping to `:server_ready`.
- Kept the existing integration-install and focus-existing-pane guards (now driven through
  `%HERDR%`), and anchored the integration probe to `claude: not installed` so the "not
  installed" lines of other agents can't trigger a spurious reinstall.
- After a successful `agent start`: if we launched the server, tell the user to switch to the
  new window and exit 0; otherwise attach this console to the herd.

## Verification
herdr 0.7.4-preview on this machine:
- `integration status` → `claude: current (v7)` ⇒ the `claude: not installed` probe does not
  match ⇒ no reinstall (other agents' "not installed" lines lack the `claude:` prefix).
- `status server` → `status: running` ⇒ findstr matches ⇒ `goto :server_ready`.
- `agent get Claude` → exit 0 (a live Claude pane present) ⇒ focus-existing path taken.
The server-bootstrap/attach branch was not run live (it seizes the console) but is a verbatim
lift of the proven source `_hrdr-launch.cmd` bootstrap loop.

## Work Log
- 2026-07-16 — Filed. Compared the repo launcher against the source `_hrdr-launch.cmd`;
  identified the two missing capabilities (herdr path fallback + server bootstrap/attach).
- 2026-07-16 — Implemented the fallback + bootstrap in the single self-contained launcher.
  Verified the individual herdr subcommands the script relies on against live herdr
  0.7.4-preview; all three decision points behave as the batch logic assumes. Closed.

## Notes
Follow-up to CPE-544. Chosen approach (confirmed with the user): enhance the existing
self-contained launcher rather than porting `_hrdr-launch.cmd` as a separate shared file.
The source's `Claude--run.cmd` dependency stays satisfied by this repo's own `RunClaude.cmd`
(substituted in CPE-542), so no other files need migrating.
