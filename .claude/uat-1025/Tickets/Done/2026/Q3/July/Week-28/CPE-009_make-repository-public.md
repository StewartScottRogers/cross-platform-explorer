---
id: CPE-009
title: Make the repository public
type: Task
status: Done
priority: Medium
component: Docs
estimate: 15m
created: 2026-07-10
closed: 2026-07-10
---

## Summary

Change `StewartScottRogers/cross-platform-explorer` from private to public. This is the chosen path
to unblock CPE-008 (GitHub Pages is unavailable for private repos on the free plan), and it makes
the release installers downloadable without a GitHub login.

Before flipping visibility, the git history must be audited — going public exposes every commit ever
pushed, not just the current tree.

## Acceptance Criteria

- [x] Audit full git history for secrets: `updater.key`, `updater.pw`, `updater.key.pub`, `.env` never appear in any commit
- [x] Confirm the only credentials in the repo are the updater **public** key in `tauri.conf.json` and GitHub Actions secret *references*
- [x] Repository visibility changed to public
- [x] Verify the repo loads when logged out, and release assets are downloadable anonymously
- [x] CPE-008 moved out of `Blocked/` (its gate is now cleared)

## Resolution

Audited the full git history before publishing, using three independent checks:

1. `git log --all --full-history -- updater.key updater.pw updater.key.pub .env "*.key" "*.pw"` —
   returned nothing; none of these files were ever committed.
2. Enumerated every path ever added across all commits and filtered for
   `key|secret|env|pw|cred|token|pem|p12|pfx`. Only two matches, both false positives:
   `src/vite-env.d.ts` (matched "env") and `CPE-005_keyboard-navigation.md` (matched "key").
3. `git grep` across every commit in `git rev-list --all` for the minisign encrypted-secret-key
   header — no private-key material in any commit.

With the history clean, flipped visibility to public via
`gh repo edit --visibility public --accept-visibility-change-consequences`. Verified `visibility: PUBLIC`.

The only key material in the repo is the updater **public** key in `src-tauri/tauri.conf.json`, which
is designed to be published. The private key lives only in the GitHub Actions secret and the local
gitignored `updater.key`.

This cleared the gate on CPE-008, which was completed immediately afterwards.

## Work Log

2026-07-10 — Picked up. Estimate: 15m. Plan: audit history for secrets, then flip visibility, then unblock CPE-008.
2026-07-10 — History audit check 1: no secret file ever committed (git log --all --full-history over all secret paths returned empty).
2026-07-10 — History audit check 2: scanned every path ever added; only false positives (vite-env.d.ts, keyboard-navigation.md).
2026-07-10 — History audit check 3: git grep for minisign private-key header across all commits — no hits.
2026-07-10 — Audit clean. Flipped repo to public. Verified visibility: PUBLIC.
2026-07-10 — CPE-008 unblocked. All acceptance criteria met. Closing as Done.

## Notes

Unblocked CPE-008 (GitHub Pages), which was closed the same day.
