---
id: CPE-553
title: "Language switch doesn't apply in the packaged app (even es/de/fr)"
type: Bug
status: Open
priority: High
component: Frontend
tags: [needs-info]
estimate: 2-3h
created: 2026-07-16
closed:
---

## Summary
User QA (2026-07-16, v0.32.0-sidecar): picking Spanish/German/French from the 🌐 picker changed nothing —
"even the menu bar stayed English." These three locales have full catalogs, so this is a real bug, not the
English-fallback behaviour of the untranslated locales.

## Investigation so far (2026-07-16)
- The reactive path is correct: components use `$t(...)`, `t` is `derived(locale, …)`, `pickLocale`
  calls `locale.set(code)`. Single store instance (one `writable<Locale>` in `src/lib/i18n.ts`).
- **Cannot reproduce headlessly.** Two component tests pass: (a) `locale.set("es")` re-renders
  `MenuBar` `File`→`Archivo`; (b) the **real picker-click path** (open 🌐, click "Español") switches too.
  So the Svelte-level logic works.
- Therefore the failure is **environment-specific to the packaged WebView2 build** — most likely a
  stale/cached frontend bundle, or a production-build difference the jsdom harness doesn't exercise.

## Acceptance Criteria
- [ ] Reproduce the failure against a real build (or prove it's a stale cache and add cache-busting so a
      fresh install always serves fresh JS).
- [ ] Picking es/de/fr visibly re-translates the UI in the packaged app.
- [ ] A regression guard: the MenuBar picker-click language-switch test lands in the suite.
- [ ] `npm run check` clean; verified in the installed app.

## Notes
`needs-info` / assumption (nightshift, user asleep): pursue the stale-cache / production-build hypothesis
first (add a build/version cache-bust; rebuild + reinstall + re-verify). Keep the passing picker-click
test as the regression guard regardless. If a fresh reinstall fixes it, the bug was cache — document +
add the cache-bust. If it persists on a verified-fresh build, escalate to a production-Svelte repro.

## Investigation 2026-07-18 (Nightshift) — stale-cache hypothesis substantiated
Analysis on the freshly-installed 0.46.0-sidecar build (no live GUI repro — Windows blocks a
background process from stealing foreground over the user's open browser windows, so menu-clicking the
🌐 picker couldn't be driven reliably without risking interference with the user's session):

- **Confirmed the WebView2 cache survives reinstalls.** `%LOCALAPPDATA%\com.example.crossplatformexplorer.sidecar\EBWebView\`
  exists and is NOT touched by the NSIS installer (NSIS replaces the install dir, not this per-user
  WebView2 profile). So "fresh install" does NOT equal "fresh frontend cache".
- **Where a stale bundle can hide.** On Windows Tauri serves the frontend over `http://tauri.localhost`,
  which WebView2 caches by URL. Vite hashes the JS asset filenames (`assets/index-<hash>.js`), so a new
  build's JS is a cache *miss* — good. **But `index.html` is NOT hashed** (always `index.html`). If
  WebView2 has `index.html` cached, it keeps referencing the OLD hashed JS filename → the whole app runs
  the old bundle, including old i18n wiring. This matches "even the menu bar stayed English" on an
  updated app.
- **Recommended fix (next session).** Serve `index.html` (at minimum) with `Cache-Control: no-cache`
  from the Tauri asset/response layer on Windows so a new build's HTML is always refetched and pulls the
  new hashed JS. Then reinstall + user-assisted 🌐-picker verification (es/de/fr must re-translate).
  Regression guard (picker-click test) already passes and should stay.
- A fresh 0.46.0 build is installed and ready for the user-assisted repro. Still Blocked on `needs-info`
  (a real interactive repro/confirmation), not an external gate.
