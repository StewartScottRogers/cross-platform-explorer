---
id: CPE-765
title: Rework the GitHub user-facing presentation — simple landing page, working one-click download, user overview + developer link
type: feature
component: Website/Release
tags: big-design
created: 2026-07-19
priority: high
estimate: 4h+
status: Done
closed: 2026-07-19
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
- [x] From the landing page, a visitor can download and install in **one click** on their OS (no 404).
- [x] Landing page: hero + working download + 30-second quickstart + scannable feature overview + a
      clear "For developers" link. *(Uses the app icon as hero art; a real app **screenshot** is a small
      follow-up — see below.)*
- [x] Feature overview covers the inventory above (nothing major missing).
- [x] README is user-first with developer content corralled under a single `#for-developers` section.
- [x] Headline-build decision recorded (see Resolution): the download serves the **latest published
      build** via the GitHub API — no release-channel change required to make one-click work.
- [x] Auto-updater claim retained — it's true (signed Tauri updater ships).

## Resolution (closed 2026-07-19)
Reworked the whole user-facing presentation.

**One-click download — fixed a different (better) way.** Rather than depend on `/releases/latest` (which
404s for a prerelease-only channel), the landing page now fetches the latest **published** release from the
GitHub API client-side, detects the visitor's OS, and points the button straight at the matching installer
(`x64-setup.exe`/`.msi`, `.AppImage`/`.deb`/`.rpm`, `.dmg`). Verified live in-browser: on Windows it
resolved to `…_x64-setup.exe` and showed `v0.53.11-sidecar (preview)`. It degrades gracefully to the
releases page when offline/rate-limited, and will automatically pick up a future stable release with no
code change.

**Landing page (`docs/index.html`)** — full rewrite: light/dark theme-aware, responsive, self-contained.
Hero + OS-aware download + version label; a 3-step "get up and working" quickstart; a 6-card feature
overview; a dedicated **AI** band (Agent Watch / AI Console / Grid-Board-Swarms / Repositories-Workbench);
and a "For developers →" link into the README.

**README** — restructured user-first: what-it-is, download/website links, get-started, a grouped feature
list (the full inventory), then **all** developer content corralled under one `## For developers` section
(prereqs, develop, build, icons, launch options, updater, agent catalog, releasing, code signing, layout).
No dev content lost — just reorganized so a user is never dropped into build instructions.

**Headline-build decision:** the public download serves whatever is the **latest published release**
(currently the `-sidecar` preview, labelled "(preview)"). Whether to additionally cut a **stable plain
release** for the public remains a product call, but is **no longer a blocker** — the button works today.

## Follow-ups (not blocking; noted)
- Add a real **app screenshot/GIF** to the landing page hero (currently the app icon). Needs a capture from
  the running app.
- Decide whether to publish a **stable, non-preview** release channel for the public; the download button
  will prefer it automatically once one exists (tweak the API filter to `!prerelease` if desired).
- GitHub **Pages redeploys via Actions** — during the current GitHub outage the live site update may lag;
  the committed source is correct and will publish when Actions recovers.

## Work Log
- 2026-07-19 — Picked up. Estimate 4h+. Grounded the ticket: the site's download 404s (no non-prerelease
  release), README is dev-heavy. Chose an API-driven OS-aware download so one-click works without waiting
  on a release (and survives the ongoing GitHub Actions outage).
- 2026-07-19 — Rewrote `docs/index.html` + `README.md`. Verified the landing page live in-browser (served
  locally): renders light/dark, download button auto-detected Windows and resolved to the real installer,
  version shown, all feature sections present. Closed and merged to main.

## Notes
Filed 2026-07-19 at the user's request ("rework the entire GitHub user-facing presentation… simple landing
page with one-click download and an overview to get a user up and working, then a link to everything a
developer needs"). Feature inventory assembled from project memory to avoid missing shipped features.
