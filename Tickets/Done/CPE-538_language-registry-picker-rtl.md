---
id: CPE-538
title: "Languages — locale registry + searchable RTL picker + incremental English fallback"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-533
sprint: SPR-08
estimate: 3-4h
created: 2026-07-16
closed: 2026-07-16
---

## Summary
Wave 1 of [[CPE-533]]: make the 🌐 picker offer **dozens** of languages. A **locale registry** (native +
English names + RTL flag) for 31 languages, a **searchable** picker that scales, **RTL layout** for
Arabic/Hebrew/Persian/Urdu, and graceful **English fallback** per key so coverage fills in incrementally.

## Acceptance Criteria
- [x] A `LOCALES` registry of 31 languages (native name, English name, RTL flag); `Locale` widened to a
      code; `SUPPORTED_LOCALES` = the registry.
- [x] The 🌐 menu lists all languages by **native name** with a **search** box (native/English/code) + a
      scrolling list; the active one is checked.
- [x] **RTL** languages set `document.dir = rtl` on selection (layout mirrors).
- [x] Any language without a full catalog **falls back to English** per key (existing `translate`
      fallback) — no broken UI; en/es/de/fr stay fully translated.
- [x] Tests: registry + `filterLocales` + `isRtl` + the fallback.

## Resolution
`i18n.ts`: widened `Locale` to a string code; added the `LOCALES` registry (31 languages, native names,
RTL flags for ar/he/fa/ur), `filterLocales(query)` + `isRtl(code)` (pure, tested); `locale.subscribe`
now sets `document.documentElement.dir`. `MenuBar.svelte`: the language dropdown gained a **search box**
+ a scrolling list showing native + English names. The picker now offers **dozens** of languages;
en/es/de/fr are fully translated and every other language gracefully shows English until its catalog is
filled in ([[CPE-539]]). `npm run check` clean; 557 frontend tests (5 new); the CPE-481 coverage gate
still passes (it checks es/de/fr completeness, unaffected). First ticket of SPR-08.

## Work Log
2026-07-16 — Built the locale registry + searchable/RTL picker + incremental fallback. 5 new tests. npm check clean; 557 tests. All ACs met.
