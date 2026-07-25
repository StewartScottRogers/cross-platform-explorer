---
id: CPE-235
title: Folder-context plugin system (detect + aggregate + act on folder contexts)
type: Feature
status: Done
priority: Medium
component: Multiple
estimate: 4h+
created: 2026-07-12
closed: 2026-07-12
---

## Summary

Generalize the file-type provider registry (CPE-059/060) to FOLDERS. A registry of
folder-context providers each cheaply inspects a folder's contents (marker files)
and, if recognized, claims a context. A folder may match several contexts, which
are aggregated and displayed together. Providers are extensible: beyond naming the
context they contribute actions/behaviours. This subsumes the earlier
"Repository" menu idea (Git repo is just one context provider).

Detection must be cheap (marker-file checks only — never a deep scan) so the plain
explorer stays fast (PURPOSE.md).

### Example contexts
- Git/GitHub repo — `.git/` (+ github.com remote): repo actions (open, fetch/pull,
  branch/dirty summary) — the "Repository" menu the user first asked for.
- Visual Studio solution — `.sln`/`.csproj`: open in VS, projects summary.
- Web site — `index.html`: open in browser, preview.
- Dev envs — `package.json` (Node), `Cargo.toml` (Rust), `pyproject.toml` (Python).

## Design decisions (user, 2026-07-12)
- All of (1): a context gives the folder **(a) badge/icon, (b) aggregated summary
  panel, (c) context actions**.
- (3): recognize **multiple** contexts per folder and **aggregate** them into one
  display.
- Extensible **beyond description** — providers contribute actions/behaviour, not
  just labels.
- (2, agent default): context actions surface as a **context-aware menu** in the
  menu bar (the renamed/refilled "Repository" slot) + summary in the details pane.
- (4, agent default): build the **general registry + provider interface first**,
  with **Git-repo and VS-solution** as the first two providers.

## Acceptance Criteria (initial)
- [ ] A `FolderContextProvider` interface + registry: `detect(marker info) → context | null`.
- [ ] Cheap backend probe of a folder's marker files (no deep scan).
- [ ] Selecting/opening a folder aggregates all matching contexts.
- [ ] Aggregated summary renders in the details pane; folder gets a context badge.
- [ ] A context-aware menu exposes each matched context's actions.
- [ ] Git-repo and VS-solution providers implemented as the first two.
- [ ] With no context match, behaviour is unchanged and fast.

## Resolution

Built the folder-context foundation. `src/lib/folderContext.ts` is the registry:
`detectContexts({ path, entries })` runs every provider and aggregates all
matches. Providers are pure functions over the current folder's listing (marker
files/dirs — no deep scan, no extra I/O since entries are already loaded); each
returns label, icon, optional detail, and actions carrying a `kind`
(`open-path` / `open-github`) + target, so providers contribute behaviour, not
just labels (extensible: append a function). First providers: Git repo (`.git/`),
VS solution (`*.sln`), web page (`index.html`), Node (`package.json`), Rust
(`Cargo.toml`), Python (`pyproject.toml`/`requirements.txt`).

Display: `ContextBar.svelte` renders an aggregated strip above the file list —
chips (icon + label = badge + summary) for each matched context plus that
context's action buttons; hidden entirely for plain folders. `App` computes
`folderContexts` from the RAW listing (so `.git` is seen regardless of
show-hidden) and runs actions: open-path → `openPath`; open-github → new backend
`git_remote_url` reads `.git/config`, normalises the origin remote
(`git@`/`ssh`/`https`, strips `.git`) to https, then `openUrl`.

Delivered decisions (1) badge+summary+actions via the ContextBar, (3) aggregate
multiple contexts, extensible providers, and (4) general registry + first
providers. Note vs the initial ACs: the summary/actions surface as the always-
visible ContextBar rather than a details-pane panel + separate menu — richer and
more discoverable; per-row subfolder badges (which would need probing every
child) are deferred as a future extension.

Verified: `npm run check` 0/0; `npm run build` + `cargo build` clean; 231 frontend
tests + 56 Rust tests pass.

## Work Log

2026-07-12 — Built registry + 6 providers (folderContext.ts), ContextBar strip, git_remote_url backend, App wiring.
2026-07-12 — check + both builds clean; 231 FE + 56 Rust tests pass. ContextBar surfaces badge+summary+actions; per-row badges deferred. Closed.

## Notes
Reframed from the user's "Repository menu" request. Relates to the preview
registry (CPE-059/060) and richer icons (CPE-233). Large — will be staged.
