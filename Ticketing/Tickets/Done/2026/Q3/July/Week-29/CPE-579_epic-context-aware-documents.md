---
id: CPE-579
title: "EPIC: Context-aware Documents — open the docs viewer to the page for the section you're in"
type: Task
status: Done
priority: Medium
component: Frontend
tags: [epic, big-design]
estimate: 4h+
created: 2026-07-17
closed: 2026-07-17
---

## Summary
The in-app **Documents** viewer ([[CPE-534]] / CPE-536 / CPE-537, `src/lib/components/DocsView.svelte`
over `src/lib/docs.ts` + `src/docs/*.md`) already looks great, but it always opens to the first doc
(`DOCS[0]`). Make it **context-aware**: opening Documents from a section should land on *that section's*
page, already selected and rendered. Open it from the **AI Console** and you get the AI Console page; from
the **Workbench**, the Workbench page; from **Swarms**, the Swarms page — and so on.

The second, standing ask: this must be **self-maintaining discipline** — every new feature that earns a
section must (a) ship its doc page and (b) register its section→doc mapping, so contextual help never
drifts behind the app. Fold that into `CLAUDE.md` and the docs-library rule ([[maintain-in-app-docs-library]]).

## Goal
From any major surface, a "?"/Help affordance (button in the section header and/or its menu) opens
Documents with the matching page pre-selected and scrolled to top. Opening Documents from the global
Application menu keeps today's behaviour (Overview / first doc, or the page for the current mode if one
is active). No section can point at a missing doc — an unmapped or stale slug is caught, not silently
shown as the wrong page.

## Design principles

**One mapping, one source of truth.** A single `section → doc slug` registry (e.g. keyed by the app's
existing mode/view identifiers) drives both the contextual open *and* the "is every section documented?"
check. Don't scatter slug strings across components.

**Orthogonal to the viewer.** `DocsView` gains one optional input — the initial slug to select — and
nothing else changes. Everything about *which* slug to pass lives in the caller/registry, so the viewer
stays a dumb, reusable panel. Current callers that pass nothing keep today's default.

**Fail loud in dev, degrade gracefully in prod.** If a section maps to a slug that isn't in `DOCS`
(typo, deleted doc, renamed file), a unit test fails in CI; at runtime it falls back to the default doc
rather than a blank pane.

**Self-maintaining.** The registry is *exhaustive over sections* — a test asserts every user-facing
mode/section has a mapping and every mapped slug resolves. Adding a section without its doc breaks the
build, which is the point.

## Rough scope (NOT decomposed)
- **Viewer input** — add an optional `initialSlug` (or `slug`) prop to `DocsView.svelte`; select that doc
  on mount when present and valid, else fall back to `DOCS[0]`. Wire `showDocs` in `App.svelte` to carry
  an optional slug so `<DocsView>` receives it.
- **Section→doc registry** — a small pure module mapping each mode/section id (AI Console, Workbench,
  Agent Board, Agent Grid, Swarms, Repositories, Explorer, Home…) to a doc slug from `src/docs/`
  (`04-ai-console`, `07-workbench`, `06-agent-board`, `05-agent-grid`, `09-swarms`, `08-repositories`,
  `03-explorer`, `01-overview`). Pure + unit-tested alongside `docs.test.ts`.
- **Contextual affordances** — a consistent Help/"?" entry point per section (header button and/or the
  section's menu) that opens Documents to the mapped page. Must follow the menu standard
  ([[menu-design-standard]], `docs/design/MENUS.md`) where it lands in a menu.
- **Global open stays sensible** — Application → Documents opens the current mode's page if a mode is
  active, otherwise Overview (decide at activation).
- **Guard test** — assert every section id has a registry entry and every mapped slug exists in `DOCS`
  (exhaustiveness + no dangling slugs).
- **Docs + process** — update the docs library itself, and amend `CLAUDE.md` so "new feature ⇒ new/updated
  doc page **and** registry entry" is an enforced rule ([[maintain-in-app-docs-library]]).

## Open questions (resolve at activation)
- **Entry-point shape:** a "?" icon in each section header, an item in each section's menu, a global
  shortcut (e.g. F1 opens the current section's doc), or all three?
- **Keying:** reuse the existing mode/view enum as the registry key, or introduce a dedicated section id?
- **Deep-linking within a doc:** just select the page (v1), or also scroll to an anchor/heading for
  sub-features later?
- **Global-open default:** current mode's page vs. always Overview when opened from the Application menu.
- **Home/Explorer:** do the non-mode default surfaces get contextual help too, or only the additive modes?

## Definition of Done (epic-level)
- Opening Documents from a section lands on that section's page, selected + rendered, in every major
  surface.
- `DocsView` takes an optional initial-slug input; existing callers are unaffected.
- A single registry is the one source of truth; a guard test proves every section is mapped and every
  mapped slug resolves.
- The contextual entry points follow the menu/UI standards and are theme-correct light/dark.
- `CLAUDE.md` + the docs-library rule updated so future features must add their doc page **and** registry
  entry (self-maintaining).

## Notes
Small, well-shaped epic: a one-prop change to a reusable viewer + a pure mapping module with an
exhaustiveness test + light UI entry points — mostly design (entry-point ergonomics + the
self-maintaining discipline), not plumbing, hence `big-design`. Fits the codebase's pure-core + thin-UI
shape. Dormant brief until activated with `/ticketing-epic activate CPE-579`.

## Decisions (activation 2026-07-17)
- **Entry point:** a **"?" button in each section header + F1** (opens the current section's doc). No
  per-section menu items in v1.
- **Coverage:** **every surface** — all modes (AI Console, Workbench, Board, Grid, Swarms, Repositories)
  and the base surfaces (Home, Explorer).
- **Keying:** reuse the existing mode/view id enum as the registry key.
- **Global open:** Application → Documents opens the active mode's page, else Overview.
- **Deep-linking:** page-only for v1 (scroll-to-anchor deferred).

## Child tickets (created at activation)
- [[CPE-594]] — DocsView takes an optional initial slug (30m)
- [[CPE-595]] — Section→doc registry + exhaustiveness guard test (1h) — the one source of truth
- [[CPE-596]] — Contextual Help entry points: "?" header button + F1 (1-2h) — needs 594 + 595
- [[CPE-597]] — Self-maintaining docs rule in CLAUDE.md + docs library (30m)

## Work Log
2026-07-17 — Filed as a dormant `Proposed` brief on request (user: "make Documents openable from the
section it addresses, with that section's page pre-selected; remember this as a standing rule that new
features add their documentation"). Not decomposed; activate to plan.
2026-07-17 — **Activated.** Resolved the open questions with the user (see Decisions) and decomposed into
CPE-594…CPE-597. Suggested order: 594 → 595 → 596 → 597.

## Resolution (Done — 2026-07-17)
Documents is now context-aware. [[CPE-594]] gave `DocsView` an optional `initialSlug`; [[CPE-595]] added
the one-source-of-truth `sectionDocs.ts` registry + an exhaustiveness guard test; [[CPE-596]] wired F1 +
a toolbar "?" (and Application→Documents) to open the current section's page; [[CPE-597]] recorded the
self-maintaining rule in CLAUDE.md (new section ⇒ doc page + registry entry, enforced by the guard test).
DoD met: opening from a section lands on its page; one registry drives both the open + the guard;
theme-correct entry points. Per-section header "?" for the additive modes that live in other surfaces
reuse the same registry as those headers are touched.
