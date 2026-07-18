---
id: CPE-660
title: "EPIC: Collapsible left-panel sections"
type: Task
status: In Progress
priority: Medium
component: Frontend
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal

Make the left navigation pane's **primary sections collapsible** so the user can fold away the ones
they aren't using and give more vertical room to the rest. Today the three main groups —

1. **Pinned destinations** — Home / Gallery / Repositories / Agent Board / Workbench
2. **Quick-access folders** — Desktop / Documents / Downloads / Pictures / Music / Videos (`places`)
3. **Drives** — Local Disk (C:) and any other volumes (`drives`)

— are always fully expanded, stacked flat and separated only by divider lines. On a machine with many
user folders and a long drive tree, they consume the whole sidebar and push everything else off-screen.
Each group should get a section header with a twisty that folds the whole group, and the collapse state
should persist.

## Why

The two *optional* sidebar sections that already exist — **Agents** (Agent Watch) and **Favorites** —
each have a header + twisty (`agentsOpen` / `favOpen`) and can be folded. The three core sections don't:
they render as a flat list of `nav-item`s with `navigation-pane-sep` dividers between them
(`Sidebar.svelte` ~L230–L264), so there's no way to reclaim their space. When the drive tree or the
quick-access list is long, the user is stuck scrolling and can't keep, say, the pinned destinations and
the drives both in view at once. Letting each group collapse is the standard file-explorer affordance
(Windows Explorer, Finder, VS Code) and directly serves the "more working space for the remaining open
panels" ask. It's purely additive: default state stays fully expanded, so nothing changes for users who
don't touch it.

## Rough scope (areas, not child tickets)

- Give each of the three core groups a **section header row** with a collapse twisty, matching the
  existing `agents-head` / `fav-head` pattern already in `Sidebar.svelte` (reuse `.twisty`,
  `.nav-children`, the header styling) so all five sidebar sections behave and look identical.
- Wrap each group's items in a collapsible container driven by a per-section `open` boolean, mirroring
  `agentsOpen` / `favOpen`.
- **Persist** each section's collapsed/expanded state across sessions (reuse whatever store the sidebar
  already uses for its transient twisties / app settings) so the layout the user sets sticks.
- Decide and apply **section labels** for the three groups (the pinned group is currently unlabelled).
- Keep individual node expansion (drive/folder tree twisties) working independently of the new
  group-level collapse — collapsing a group hides it wholesale; expanding it restores prior per-node
  state.
- Accessibility + theming: header rows keyboard-focusable, `aria-expanded`, and colours from theme
  variables (menu/tab conventions), consistent light/dark.

## Open questions (resolve at activation)

- **Labels for the pinned group.** The Desktop/Documents block and the drives block have implicit
  identities, but the top Home/Gallery/… block has no header today — what do we call it? ("Places",
  "Explore", none?) And do we relabel the others?
- **Persistence granularity.** Per-section booleans in app settings vs. the existing transient twisty
  store — which store, and is collapse state global or per-window/per-tab?
- **Default state.** Ship all-expanded (today's behaviour, zero surprise) — confirm we don't want any
  group collapsed by default on first run.
- **Header affordance placement.** Twisty-only header (like Favorites) vs. a header that's also a
  clickable target; and whether the existing divider lines stay or are replaced by the headers.
- **Interaction with the existing Agents/Favorites sections** — do we want a single consistent ordering
  and one shared section-header component extracted, or just match the pattern inline?

## Definition of Done

- Each of the three core sidebar groups (pinned destinations, quick-access folders, drives) has a
  header with a working collapse twisty that folds/unfolds the whole group.
- All five sidebar sections (the three new + Agents + Favorites) look and behave consistently.
- Collapse state persists across app restarts.
- Default (fresh) state is fully expanded — no behavioural change for users who never collapse anything.
- Per-node tree expansion still works and is independent of group collapse.
- Headers are keyboard-accessible, expose `aria-expanded`, and are themed from variables (identical
  light/dark).

## Decisions (activated 2026-07-18, nightshift — user away, no-questions; best-guess logged)
- **Labels:** pinned nav group = **"Explore"** (Home/Gallery/Repos/Board/Workbench); quick-access
  `places` = **"Quick access"**; `drives` = **"Drives"**. Existing Favorites/Agents/Tags/Smart keep their
  labels.
- **Persistence:** one small localStorage-backed store (`src/lib/sidebarSections.ts`, keyed by section
  id) — global (not per-window/tab), reusing the shared `persist` layer like `smartFolders`.
- **Default:** all sections expanded (unset = open) — zero change on first run.
- **Header affordance:** the whole header row toggles (matches `fav-head`), keyboard-focusable with
  `aria-expanded`; dividers between groups kept.
- **Consistency:** convert the existing transient twisties (Favorites/Agents/Tags/Smart) to the same
  persisted store so **all** sidebar sections behave + persist identically (serves the DoD's
  "consistent" + "persists" gates), rather than extracting a new shared component (lower risk).

## Child tickets
1. **CPE-675** — Collapsible core sidebar sections: add "Explore"/"Quick access"/"Drives" headers with
   persisted twisties, convert all existing section twisties to the shared persisted store, labels + i18n
   ×12, a11y (`aria-expanded`, focusable) + theming. Covers the epic's DoD in one cohesive change.
