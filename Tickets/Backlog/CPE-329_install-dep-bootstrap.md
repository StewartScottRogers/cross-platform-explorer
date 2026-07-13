---
id: CPE-329
title: "Install: detect/bootstrap Node + winget like the reference"
type: Feature
status: Open
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
