---
id: CPE-1619
title: "Docs depth pass — Agent Deck, Workbench, Repositories pages (Tier-2 thin per the audit spike)"
type: Task
status: Backlog
priority: Medium
component: Frontend
epic: CPE-1569
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Epic CPE-1569 (docs completeness) slice 10, named explicitly in the epic's decomposition list. The docs
audit spike (Library entry `docs-completeness-audit-2026-08-10`) flagged these as **Tier-2 thin**, and a
direct word-count check on the live files confirms it against every other page's depth (`explorer-agent-
watch.md` alone runs 1,465 words after its own recent depth pass; `06-agent-board.md` already runs 906
words):

| Page | Section | Words today |
|---|---|---|
| `src/docs/04-ai-console.md` | "Agent Deck" (`ai-console`) | 268 |
| `src/docs/07-workbench.md` | "Workbench" (`workbench`) | 161 |
| `src/docs/08-repositories.md` | "Repositories" (`repositories`) | 180 |

All three are real, already-shipped, already-mapped Sections (`src/lib/sectionDocs.ts` lines 46/49/50) —
this is depth expansion of existing pages, not new IA work (CPE-1571's IA groundwork already landed).

## Goal
Each of the three pages gets genuine multi-section depth — every option, every action, every edge case a
user would hit — matching the quality bar the spike's 10-point rubric and the "Honest limits" closing-
section house style set, and matching the depth `explorer-agent-watch.md` and `06-agent-board.md` already
demonstrate for this same doc corpus.

## Scope
**Conflict surface:** `src/docs/04-ai-console.md`, `src/docs/07-workbench.md`,
`src/docs/08-repositories.md` only. No `sectionDocs.ts` change needed (all three already map correctly) —
verify the guard test (`src/lib/sectionDocs.test.ts`) stays green, don't need to edit the registry itself.
Zero overlap with any other ticket on this bench or any in-flight worker — pure Markdown content, no
shared file, fully parallel-safe.

- **Agent Deck** (`04-ai-console.md`): verify against `src/App.svelte`'s actual Agent Deck code (grep
  "Agent Deck" — the launch flow, Keys/Save-setup, session reattach, the "Work on this" scoped-launch
  entry point (CPE-313), the per-session close vs "close entirely" distinction, the toolbar button's
  sidecar-platform gating) and document what's currently only mentioned in passing or not at all: provider/
  model selection specifics, what "Save setup" actually persists vs. the key, multi-tab behavior, what
  happens on a Repair/missing-sidecar state.
- **Workbench** (`07-workbench.md`): verify against the real Workbench component/commands (diff rendering,
  the address-bar browser window, all the edge-case messages already listed) and go deeper: what actions
  are available per changed file (if any beyond Edit), how large diffs are handled, refresh behavior,
  relationship to the main explorer pane it's reading from.
- **Repositories** (`08-repositories.md`): verify against the real Repos-page component/commands and
  document its actual feature set in full (currently 180 words — almost certainly thinner than the real
  surface; read the component before writing, per CPE-1569's "verify against `src/lib/components/`, NOT
  epic titles" lesson from the audit spike).
- Apply the spike's 10-point quality rubric and "Honest limits" closing section to all three (see the
  audit spike / other already-expanded pages like `explorer-agent-watch.md` for the house style to match).
- Accuracy over volume (epic constraint, restated): every claim must be verified against the actual
  component/command, not invented or copied from a stale work-log.

## Explicitly NOT in scope
- No changes to `06-agent-board.md` (already deep at 906 words — not part of this slice) or `05-agent-
  grid.md` (separate page, not named in the epic's slice-10 scope).
- No screenshot embedding — that's the epic's separate, later slice 12 ("screenshot pass"), not this one.
- No code changes anywhere — docs-only ticket.

## Acceptance criteria
- All three pages pass the 10-point quality rubric and expand well beyond their current 161–268-word
  stubs, with real depth (options, actions, edge cases, workflow) verified against the live components.
- `src/lib/sectionDocs.test.ts` stays green (no registry drift).
- No new page/slug added — this expands the three existing pages in place.

## Notes
Model: sonnet. Library entry: `docs-completeness-audit-2026-08-10` (rubric + house style + the
verify-against-code lesson).

## Work Log

**2026-08-11 — depth pass done, three bugs found and filed.**

Expanded all three pages, verifying every claim against the live source before writing it:

- `src/docs/04-ai-console.md` (Agent Deck): 268 → 1,914 words. Read `sidecar/ai-console/src/launcher.html`
  in full plus `agents.rs`, `presets.rs`, `vault.rs`, `swarm_team.rs`, `console.rs`, `SidecarManager.svelte`,
  and the relevant `App.svelte` ranges (launch/close/Repair/toolbar-gating/CPE-313). Added real coverage of
  the Account picker, Manage agents ▾ (update/pin/reset/rollback), Recent… (relaunch + redacted transcript),
  "Work on this in Agent Deck" (CPE-313), and Settings → Platform's Repair flow — all shipped, none
  previously documented anywhere.
- `src/docs/07-workbench.md`: 161 → 1,101 words. Read `WorkbenchView.svelte`, `workbench.ts`, `diff.ts`, and
  the `workbench_diff` Rust command in full.
- `src/docs/08-repositories.md`: 180 → 1,200 words. Read `RepoBrowser.svelte`, `SyncDialog.svelte`,
  `ConflictDialog.svelte`, `syncPolicy.ts`, `autoMirror.ts`, `StatusBar.svelte`'s git section, and the
  `forge_*` Rust commands in full.

**Bugs found while verifying (not fixed here — docs-only ticket):**

1. **CPE-1621** (High) — "Close all consoles" from the sidebar/toolbar calls `sidecar_stop("ai-console")`,
   which only drops the host's connection to the console UI sidecar (`src-tauri/src/lib.rs:7972-7979`) — it
   never touches `state.daemon`, the separate host-owned process that actually holds every agent's PTY
   (`HostSessionDaemon`, reaped only on app exit per its own doc comment). The function's own comment claims
   "all running agents" are closed; they aren't. Contrast with the Agent Deck's own in-console "Close all"
   button, which really does terminate agents. Documented as a caveat in the new "Limits / notes" section
   rather than silently written up as working.
2. **CPE-1622** (Low) — the Model dropdown's `populateModels()` (`launcher.html:1948-1949`) always queries
   `/api/models?reseller=<Provider value>`; for a tool's built-in ("native") login — which has no model
   list at all — this hits the live-fetch fallback, fails, and shows an alarming "Couldn't load models."
   with a Refresh button that can never succeed. The Model field itself still works as free text.
3. **CPE-1620** (Medium) — `RepoBrowser.svelte`'s URL-paste convenience (`browse()`,
   `src/lib/components/RepoBrowser.svelte:29`) strips a `github.com` host prefix unconditionally, regardless
   of the selected Provider — so pasting a GitLab/Bitbucket/Codeberg URL isn't recognized as a URL, isn't
   stripped, still passes the "contains a slash" owner/name check, and gets sent to the backend as a fake
   repo name, producing a confusing "not found" instead of the existing friendly "Enter a repository as
   owner/name." guidance.

**Verification:**
- `npm run check` → `svelte-check found 0 errors and 0 warnings`.
- `npm run test` (full vitest run) → **272 test files passed, 3311 tests passed**, 0 failed. Includes
  `src/lib/docs.test.ts` (9/9, the ordering guard) and `src/lib/sectionDocs.test.ts` (2/2, the section→doc
  registry guard) — reconfirmed in isolation: both green.
- No `sectionDocs.ts` change needed (all three sections already mapped); no app code touched.
