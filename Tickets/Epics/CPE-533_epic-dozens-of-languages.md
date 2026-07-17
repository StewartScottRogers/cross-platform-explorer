---
id: CPE-533
title: "EPIC: Dozens of languages in the main-menu language picker"
type: Task
status: Proposed
priority: Medium
component: Frontend
tags: [epic, big-design]
estimate: 4h+
created: 2026-07-16
---

## Summary
The app already has a live **🌐 Language** picker in the main menu ([[CPE-362]], `MenuBar.svelte`) backed
by `src/lib/i18n.ts` — but it ships only **4** locales (English, Español, Deutsch, Français). Bring
**dozens** of the world's major languages into the picker so users everywhere can run the app in their
own language. A brief only until activated.

## Goal
Open the 🌐 menu → pick from **dozens** of major languages (native names) → the whole UI switches live.
Full coverage of the message catalog per language, correct **right-to-left** layout where needed, and a
picker that stays usable at that scale.

## Rough scope (NOT decomposed — for sizing only)
- **Translation catalogs** for ~2–3 dozen major languages (add each to `messages` + `SUPPORTED_LOCALES`
  — the documented "add a language" path). Each catalog is the full key set (100s of keys).
- **RTL support** — layout mirroring for Arabic / Hebrew / Persian / Urdu (`dir="rtl"`, logical
  CSS properties, icon/chevron mirroring).
- **Picker UX at scale** — the 🌐 menu with dozens of entries needs **native names**, a **search/filter**,
  scrolling, and maybe grouping; the current flat list won't do for 30+.
- **Locale detection** — extend the OS/browser-language auto-pick to the new locales; sensible regional
  fallback (e.g. `pt-BR` → `pt`).
- **Catalog completeness + maintenance** — a check that every locale has every key (fill gaps, and keep
  them in sync as new UI strings land); how missing keys fall back (already: locale → English → key).
- **Bundle size** — dozens of catalogs add weight; the app is deliberately small (PURPOSE.md) — decide
  bundle-all vs lazy-load per locale.

## Open questions (resolve at activation, with the user)
- **Which languages + how many?** A concrete list (e.g. the top ~30 by speakers, or a named set).
- **Translation source + quality:** machine-translated seed (fast, needs review) vs human/community
  translations (quality, slower)? How is quality signalled / corrected?
- **Bundle strategy:** ship all catalogs (simple, larger) or **lazy-load** the chosen locale (keeps the
  default build small — fits PURPOSE) ?
- **RTL depth:** full mirrored layout, or start with text + defer complex mirroring?
- **Maintenance:** how do we keep dozens of catalogs from rotting as strings change (a key-coverage
  gate in CI? a "missing translation" fallback that's visibly flagged)?

## Definition of Done (epic-level — refined at activation)
- [ ] The 🌐 picker offers the agreed set of dozens of languages (native names) + a way to find one.
- [ ] Each shipped locale has a complete catalog (coverage gate green); missing keys fall back cleanly.
- [ ] RTL languages render correctly (at the agreed depth).
- [ ] Locale auto-detection covers the new set with regional fallback.
- [ ] Bundle strategy decided + implemented so the default build stays reasonable.
- [ ] Child tickets all Done.

## Notes
Extends [[CPE-362]] (the picker) + the i18n work ([[CPE-481]] and siblings). `big-design` — the real
weight is producing + maintaining dozens of complete catalogs, plus RTL and the picker UX at scale.
Filed as a dormant brief per the just-in-time epic workflow ([[CPE-487]]); activate to pick the language
set + translation approach with the user, then decompose.
