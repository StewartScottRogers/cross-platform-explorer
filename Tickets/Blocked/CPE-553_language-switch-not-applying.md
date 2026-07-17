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
