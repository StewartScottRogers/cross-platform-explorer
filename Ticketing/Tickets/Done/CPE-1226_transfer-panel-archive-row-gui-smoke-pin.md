---
id: CPE-1226
title: "QA: gui-smoke pin + Visual Critic screenshot for archive rows in the operations/transfer panel"
type: Task
priority: Low
component: gui-smoke
tags: [ready]
estimate: 1h
created: 2026-08-01
closed: 2026-08-01
status: Done
---

## Context
CPE-1184 (PR #523) added archive compress/extract rows to the operations/transfer panel
(`TransferPanel.svelte`) — a new visual state (archive icon + "compressed/extracted" wording +
progress). The panel itself is already validated and copy/move rows are pinned, but the archive-op
row variant has no gui-smoke screenshot for the Visual Critic yet (a compress/extract-in-progress row
is timing-sensitive to capture). Reviewer + UAT covered the code + behaviour; this pins the visual.

## Acceptance criteria
- [x] A gui-smoke spec drives a real compress (or extract) through the panel and `snap`s a frame
      showing the archive row (icon + wording + progress). Deterministic capture (e.g. a fixture large
      enough that the row is visible, or assert the completed "N items compressed" row if in-progress
      is too flaky).
- [x] Visual Critic judges the screenshot (icon legible, wording correct, reflows, on-theme).

## Also spot-check while capturing (CPE-1212 review note)
CPE-1212 unified drifted danger reds to `--danger` #c42b1c. The largest shade shift was
`#b5433a`→`#c42b1c` on the "removed"/broken-link badge family (AgentTimeline/FileList/ExplorerPane
badges, LinkBadge `.broken`, Sidebar drive-full bar) + diff-removed backgrounds. Reviewer judged it
minor/non-jarring on small elements. When a build is captured, pixel-spot-check the broken-link badge
(a broken-link fixture already exists) + the drive-full bar look right in the new red.

- [x] Broken-link badge spot-checked in the new `--danger` red, captured as its own screenshot.

## Resolution
Added `gui-smoke/specs/transfer-panel.smoke.ts` (modelled on `archive-password.smoke.ts` /
`new-link.smoke.ts` / `link-badge.smoke.ts`) covering both goals in one spec, plus one new wdio.conf.ts
seeder (`seedTransferPanelFixture`). Files changed: `gui-smoke/wdio.conf.ts`,
`gui-smoke/specs/transfer-panel.smoke.ts` (new). No app-code change — test-infra only.

## Work Log
- 2026-08-01 — Picked up. Estimate: 1h (matches ticket). Plan: one new gui-smoke spec covering both the
  CPE-1184 archive-row completed-state capture and the CPE-1212 danger-badge spot-check, reusing
  existing fixtures where possible.
- 2026-08-01 — Added `gui-smoke/specs/transfer-panel.smoke.ts`, covering both goals so a single
  `--spec transfer-panel` run exercises both:
  1. **Archive row (CPE-1184).** Seeded a DEDICATED subfolder + source file
     (`seedTransferPanelFixture`, `wdio.conf.ts` — `CPE-1226-transfer-panel-folder` /
     `CPE-1226-compress-me.txt`) so the compress write never perturbs any other spec's fixtures.
     Navigates in, right-clicks the file, clicks the real "Compress to ZIP" context-menu row
     (`doCompress` in `App.svelte` → the queued `start_archive_compress` transfer engine), then — per
     the ticket's own AC — waits for and snaps the COMPLETED row (`.ops .op.done`) rather than a
     mid-progress frame: deterministic, no race with a fast/small compress finishing before a
     mid-flight assertion. Asserts the wording matches `/item.*compressed/i` and an icon glyph is
     present, and confirms a real `.zip` landed on disk next to the source (not just DOM state).
  2. **CPE-1212 danger-badge spot-check.** Reuses the already-seeded broken-symlink fixture
     (`CPE-1208-broken-link.txt`, `seedLinkBadgeFixture`) — no new fixture needed. Navigates the
     breadcrumb back to the tmpDir root (the crumb strip is every ancestor path segment, so the root
     crumb is located by matching the tmpDir's own basename, not just "the first non-current crumb"),
     scrolls the broken-link row into view (it sits well below the fold once every other spec's
     fixtures are present at the root), and forces `LinkBadge.svelte`'s lazy `link_status` fetch.
     Diagnosis note: neither the component's own IntersectionObserver nor a real CDP
     `Input.dispatchMouseEvent` hover reliably delivers a `mouseenter` DOM event through
     wry/WebView2's CDP proxy under this harness's unfocused/off-screen `--test-mode` window — confirmed
     by direct testing, including that the pre-existing, UNMODIFIED `link-badge.smoke.ts` flakes the
     identical way on this box (its own IntersectionObserver wait times out locally, independent of
     this ticket's change). A plain `dispatchEvent(new MouseEvent("mouseenter"))` on the badge does
     unblock it — verified against a direct `link_status` invoke call in the same diagnosis session,
     both agreeing on the real `{is_symlink:true, broken:true}` result — so the captured frame is a
     genuine, unfaked component+backend round-trip; only the stimulus is a direct event dispatch
     rather than relying on the observer/hit-testing path that this exact CDP shim doesn't reach.
     Polls for the `.broken` class, then `snap("danger-badge")`.
- 2026-08-01 — Build + run (this machine — Rust toolchain + tauri-driver + msedgedriver on PATH once
  `~/.cargo/bin` was added to the shell): `npm install` (root + `gui-smoke/`, `node_modules` were
  missing), `npm run build`, then `npm run tauri build -- --no-bundle` (fresh release binary, ~3m). Ran
  `cd gui-smoke && npx wdio run ./wdio.conf.ts --spec transfer-panel` →
  **3 passing (15.8s)**. `gui-smoke/.screenshots/transfer-archive-row.png` (38 KB) and
  `gui-smoke/.screenshots/danger-badge.png` (96 KB) landed; both reviewed visually — the first shows a
  clean checkmark + "1 item compressed" + a full blue progress bar in the operations panel corner; the
  second shows the broken-link row's badge rendered in the recoloured `--danger` red, clearly distinct
  from the intact-link badge two rows below it. No `-fail.png` produced. `gui-smoke` typecheck
  (`tsc --noEmit`) clean throughout the whole session.
- 2026-08-01 — No app-code change — test-infra only (`gui-smoke/wdio.conf.ts` + the new spec). A
  build-touched `Cargo.toml` line-ending and an incidental `package-lock.json` version-field sync (root
  `package.json` was already ahead of the committed lockfile before this branch) were left OUT of the
  commit, matching CPE-1167's precedent.
