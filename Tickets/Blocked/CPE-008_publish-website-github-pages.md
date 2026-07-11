---
id: CPE-008
title: Publish the project website via GitHub Pages
type: Task
status: Blocked
priority: Low
component: Docs
estimate: 30m
created: 2026-07-10
closed:
---

## Summary

The landing page exists at `docs/index.html` but is not hosted. GitHub Pages was rejected because
the repository is private on the free plan. Publish the page once that gate is resolved.

## Acceptance Criteria

- [ ] GitHub Pages enabled, serving from `main` branch `/docs`
- [ ] Site reachable at https://stewartscottrogers.github.io/cross-platform-explorer/
- [ ] README "Website" link verified working

## Resolution

*(Agent writes this when closing — do not fill in)*

## Work Log

2026-07-10 — Filed as Blocked. `gh api` to enable Pages returned 422: "Your current plan does not support GitHub Pages for this repository" (private repo on free plan).

## Notes

**Blocked on:** GitHub Pages is unavailable for a private repo on the current (free) plan.

**Unblocks when** one of these owner decisions is made:

### Next Actions — Owner (pick one)
- [ ] Make the repository public (then Pages works immediately on the free plan), OR
- [ ] Upgrade to GitHub Pro (Pages works on private repos), OR
- [ ] Host `docs/` elsewhere (Netlify / Cloudflare Pages) — no plan change needed.

Once decided, this returns to Backlog/ (or is worked directly) and takes ~5 minutes to enable.
