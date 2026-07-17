---
id: CPE-534
title: "EPIC: Application → Documents — an in-app professional documentation library, built in"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic, big-design]
estimate: 4h+
created: 2026-07-16
---

## Summary
Add an **Application → Documents** menu item that opens a **complete, professional, comprehensive**
set of documents **inside the app**. The documents live as a **library in the repository**, are
**bundled into the application at build time** (available offline, no network), and are **kept updated
and extended as tickets and epics are worked** (a standing discipline — see [[maintain-in-app-docs-library]]).
A brief only until activated.

## Goal
A user opens **Application → Documents** and gets a real product manual — overview, getting-started,
a page per feature, reference, shortcuts, FAQ, troubleshooting — rendered in-app, navigable and
searchable. The library is the single source of truth, versioned in git, compiled into every build.

## Rough scope (NOT decomposed — for sizing only)
- **The library** — a `docs/` (or `app-docs/`) tree of professional markdown: Overview · Getting
  started · a page for **every** major feature (Explorer, Preview/Edit, Tabs, Search, AI Console,
  Agent Grid, Swarm, Agent Board, Workbench, Forge/Repositories, Shared memory, Settings, …) ·
  Keyboard shortcuts · Reference · FAQ · Troubleshooting · Changelog. Comprehensive, not stubs.
- **Build-in step** — bundle the library into the app at build time (compile-time include / a generated
  manifest + assets), so Documents work **offline** and ship with the installer. Keep it lean (PURPOSE).
- **In-app viewer** — opened from **Application → Documents** (new `MenuBar` item under the Application
  menu): a documents pane/window with a **table of contents / sidebar**, markdown rendering (reuse the
  preview-pane markdown renderer, CPE), in-doc links, and **search** across the set.
- **Authoring pipeline** — how docs are written/organised so they stay coherent (index/manifest,
  ordering, cross-links, images bundled as assets).
- **Maintenance gate** — the standing rule that every ticket/epic updates the relevant doc; optionally a
  light CI check that flags features without docs.

## Open questions (resolve at activation, with the user)
- **Location + format:** `docs/` tree of markdown vs a dedicated `app-docs/`; one manifest/index driving
  order + TOC.
- **Bundling:** compile-time `include_str!`/import of the markdown, or a build script that generates a
  bundle the frontend loads. How to keep the default build small.
- **Viewer:** reuse the existing markdown preview renderer, or a purpose-built docs reader (TOC + search +
  breadcrumbs)? In a pane, a tab, or its own window?
- **Initial coverage:** the first wave of docs to write (which features first — likely Overview +
  Getting started + the Agent Workspace suite, since it's newest).
- **Search:** simple client-side text search vs a small prebuilt index.
- **i18n:** English-only first, or hook into the language system ([[CPE-533]]) later?

## Definition of Done (epic-level — refined at activation)
- [ ] **Application → Documents** menu item opens the in-app documentation viewer.
- [ ] A comprehensive, professional library exists in the repo and is **built into** the app (offline).
- [ ] The viewer has navigation (TOC/sidebar), rendered markdown, and search.
- [ ] Every current major feature has a real doc page (not a stub).
- [ ] The maintenance discipline is written into the project guidance (CLAUDE.md) so docs stay current.
- [ ] Child tickets all Done.

## Notes
`big-design` — the weight is authoring a genuinely comprehensive doc set + the build-in pipeline + the
maintenance discipline. **Standing rule (user, 2026-07-16):** *always update and extend this documents
library while doing tickets and epics* — captured in memory ([[maintain-in-app-docs-library]]); to be
folded into CLAUDE.md when this epic is activated. Filed as a dormant brief per the just-in-time epic
workflow ([[CPE-487]]).
