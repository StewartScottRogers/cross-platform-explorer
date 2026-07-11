---
id: CPE-009
title: Make the repository public
type: Task
status: Open
priority: Medium
component: Docs
estimate: 15m
created: 2026-07-10
closed:
---

## Summary

Change `StewartScottRogers/cross-platform-explorer` from private to public. This is the chosen path
to unblock CPE-008 (GitHub Pages is unavailable for private repos on the free plan), and it makes
the release installers downloadable without a GitHub login.

Before flipping visibility, the git history must be audited — going public exposes every commit ever
pushed, not just the current tree. The updater private key (`updater.key`), its password
(`updater.pw`), and `STATUS.html` are gitignored and were never staged, but this must be verified
against history rather than assumed.

## Acceptance Criteria

- [ ] Audit full git history for secrets: confirm `updater.key`, `updater.pw`, `updater.key.pub`, and any `.env` never appear in any commit (`git log --all --full-history -- <paths>`)
- [ ] Confirm the only credentials in the repo are the updater **public** key in `tauri.conf.json` (safe to expose) and GitHub Actions secret *references* (not values)
- [ ] Repository visibility changed to public (`gh repo edit --visibility public`)
- [ ] Verify the repo loads when logged out, and release assets are downloadable anonymously
- [ ] CPE-008 moved from `Blocked/` back to `Backlog/` (its gate is now cleared)

## Resolution

*(Agent writes this when closing — do not fill in)*

## Work Log

*(Agent appends dated entries here throughout — do not fill in)*

## Notes

**Unblocks CPE-008** — once this is done, GitHub Pages can be enabled on the free plan and the
website at `docs/` goes live.

If the history audit finds a leaked secret, do NOT go public. Instead rotate the updater key
(generate a new keypair, update `tauri.conf.json` pubkey and the repo secrets) and purge the history,
or reconsider the alternatives in CPE-008 (GitHub Pro, or host `docs/` on Netlify/Cloudflare).
Note that rotating the updater key breaks auto-update for anyone already running an older install.
