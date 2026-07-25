---
id: CPE-329
title: "Install: detect/bootstrap Node + winget like the reference"
type: Feature
status: Done
closed: 2026-07-13
priority: Medium
component: Backend
created: 2026-07-13
---

## Summary

Reference installers `ensure_node` (winget-install Node.js LTS if npm missing),
`ensure_winget`, and refresh PATH before `npm i -g`. Ours runs the manifest's `npm i -g`
directly, so install FAILS on a machine without Node/npm. Steal the bootstrap: before an
`npm`-based install, detect npm; if missing, install Node LTS via winget (or surface a
clear, actionable error with the winget command). Consider a shared "dependency" step in
lifecycle::install driven by manifest-declared prerequisites (node/uv/git), matching
Install-All.cmd's shared-deps phase.

## Acceptance
- Installing an npm-based agent on a machine without Node either bootstraps Node or fails
  with a precise, actionable message (not a raw spawn error).

## Work Log
2026-07-13 (Dayshift) — Implemented in `sidecar/ai-console/src/lifecycle.rs` on branch
`CPE-329-install-bootstrap`.

- Added a `Prerequisite` enum (currently `Node`) inferred from the install recipe's command
  (`npm`/`npx`/`pnpm`/`yarn` → Node). `install()` now calls `ensure_prerequisites(cmd, runner)`
  before running the recipe.
- `ensure_prerequisites`: probes `npm --version`; if present, proceeds. If missing:
  - no winget → returns a precise, actionable error naming Node.js LTS, nodejs.org, and the
    exact `winget install -e --id OpenJS.NodeJS.LTS` command (never a raw spawn error);
  - winget present → runs the winget Node LTS install (with `--accept-*-agreements`); on
    failure returns an actionable "install manually" error incl. winget stderr;
  - on success, re-probes npm; if still not on PATH (a running process won't see the new PATH),
    returns "Installed Node.js. Restart the app so npm is on PATH, then install again."
- Non-npm recipes (e.g. pipx/brew) are never gated.

Tests: closure-based `FnRunner` distinguishes the `npm --version` probe from the `npm i -g`
install. Added 6 cases (present / advise-when-both-missing / bootstrap-then-succeed /
restart-needed / winget-bootstrap-fails / non-npm-skips-gate); updated the existing
nonzero-stderr test to keep npm "present" so it still exercises the install-failure path.
`cargo test` 83 lib + 7 integration pass; `cargo clippy --all-targets` clean.

Assumptions (Dayshift, user away): inferred the prerequisite from the recipe command rather
than adding a new manifest `prerequisites` field (smaller change; the enum extends to uv/git
the same way when needed). PATH cannot be refreshed inside a running process, so post-bootstrap
we ask for a restart instead of faking a same-session success — verified only via the runner
abstraction (winget isn't actually invoked in tests). Full end-to-end bootstrap on a real
Node-less machine still warrants a manual check.
