---
id: CPE-765
title: Rework the GitHub user-facing presentation — simple landing page, working one-click download, user overview + developer link
type: feature
component: Website/Release
tags: big-design
created: 2026-07-19
priority: high
estimate: 4h+
status: Open
---

## Summary
The GitHub-facing presentation is inconsistent and confusing ("irrational"): the site's **one-click
download is broken**, and the README mixes a thin user intro with a wall of developer setup. Rework it into
a clean, **user-first** presentation — a simple landing page with a **working one-click download** and a
short "get up and working" overview, then **one clear link to everything a developer needs**. Cover the
**full** current feature set (see inventory below) so nothing is missed.

## Problem (grounded)
- **Download is dead.** `docs/index.html`'s "⬇ Download latest" button points at
  `…/releases/latest`, which GitHub only resolves to the newest **non-draft, non-prerelease** release.
  Every release we have is a **draft or a `-sidecar` prerelease**, so `/releases/latest` **404s** — a
  visitor cannot install the app. (`gh release list --exclude-drafts` for the plain channel is empty.)
- **README is developer-heavy / mixed.** A short intro, then ~12 dev sections (Prerequisites, Launch
  options, Develop, Build, Icons, Auto-updates, Agent catalog, Releasing, Code signing, Project layout).
  No clean user-vs-developer separation.
- **Feature list is stale.** It predates Agent Watch, the AI Console/sidecar, the Space analyzer, the
  in-app Docs library, Diagnostics, batch rename, tags, dual-pane, and more.

## Scope

### A · One-click download that actually works
- **Decide the headline build** (open question below) and make its installer reachable in one click.
- **OS-aware download** — detect Windows/macOS/Linux and link straight to the matching installer
  (`.msi`/`.exe`, `.dmg`, `.AppImage`/`.deb`). Either publish a real (non-prerelease) release so
  `/releases/latest` resolves, **or** have the page fetch the latest release's assets via the GitHub API
  and wire per-OS buttons. No dead links.
- Confirm the **signed auto-updater** path is complete (pubkey/endpoints) so "automatic updates" is true.

### B · Simple landing page (`docs/index.html`, GitHub Pages)
- **Hero:** one-line what-it-is + a prominent, working **Download** CTA + a real screenshot.
- **"Get up and working"**: a 30-second quickstart (download → install → open a folder → the few things
  that make it shine).
- **Scannable feature overview** covering the real feature set (inventory below) — grouped, not a wall.
- **One clear "For developers →" link** to the dev docs (build/architecture/contributing), so users are
  never dropped into build instructions.

### C · README restructure (user-first, dev-second)
- Top: what it is, who it's for, **download**, quickstart, screenshots.
- Then a single, well-organized **"For developers"** section (or link to `docs/`): build, run, versioning,
  updater setup, agent catalog, releasing, code signing, project layout. Keep it, just corral it.

### D · Feature inventory — the presentation must represent (from project notes; verify each still ships)
- **Core explorer:** fast/small/predictable Win/macOS/Linux file manager; tabs; **dual-pane commander**
  mode; breadcrumb + keyboard nav + back/forward/up/history.
- **Find:** content search, filename search (glob/brace-expansion), duplicates finder, smart folders,
  filter.
- **File ops:** copy/move/cut/paste, **batch rename**, duplicate, delete/trash, new file/folder,
  copy-as-path.
- **Archives:** extract, compress (zip), extract-on-drag-out.
- **Preview:** images, PDF, markdown, code (syntax highlight), audio/video, + many types; **universal
  thumbnails**.
- **Organize:** **tags** (+ import/export), smart folders, pin/favorites.
- **Insight:** **Disk-usage / Space analyzer** treemap; properties; attributes/permissions;
  **Compare studio**.
- **Feel:** light/dark themes with cross-platform-consistent menus; **streaming liveness** (instant
  paint); async everywhere (snappy); **Diagnostics** mode; in-app **Documents** library (contextual,
  now with expandable sections + per-section help).
- **AI (the differentiator):** **Agent Watch** (live view of an AI coding agent's filesystem activity);
  **AI Console** (run Claude Code / aider against a folder); **Agent Grid**; **Agent Board** (ticket
  kanban); **Swarms** (multi-agent); **Workbench/Repositories** (git diff).
- **Delivery:** native installers, **signed auto-updates**, tiny Tauri footprint.

## Decisions needed (resolve when worked)
1. **Headline download:** the plain **stable explorer** (sidecar-free, the general-purpose default per
   PURPOSE.md) or the **AI-enabled build** (currently the `-sidecar` preview channel)? This drives which
   release channel must produce the one-click installer. (Note: the plain channel has **no published
   release yet** — that likely must be cut.)
2. Screenshot source (capture from the running app) + whether to add a couple of GIFs.
3. Custom domain / Pages settings — leave as-is or set a CNAME.
4. Is this one ticket or an **epic**? If it grows, promote and decompose into: (i) release-channel so
   one-click works, (ii) landing page, (iii) README split, (iv) feature inventory + screenshots.

## Acceptance
- [ ] From the landing page, a visitor can download and install in **one click** on their OS (no 404).
- [ ] Landing page: hero + working download + screenshot + 30-second quickstart + scannable feature
      overview + a clear "For developers" link.
- [ ] Feature overview covers the inventory above (nothing major missing).
- [ ] README is user-first with developer content corralled into one clear section/link.
- [ ] Headline-build decision recorded; if a release channel change is needed, it's done and verified.
- [ ] Auto-updater claim on the site is actually true (or the claim is removed).

## Notes
Filed 2026-07-19 at the user's request ("rework the entire GitHub user-facing presentation… simple landing
page with one-click download and an overview to get a user up and working, then a link to everything a
developer needs"). Feature inventory assembled from project memory to avoid missing shipped features.
