# Manual Test Burndown — the MVD ledger

The authoritative list of every app aspect that still needs a **human** to verify it, and the automation
that will retire each one. The QA Architect drives the **still-manual count (MVD) toward zero** and never
lets an automated row silently regress. Charter + rules: [README.md](README.md).

**MVD (still-manual surfaces): 7** · _baseline seeded 2026-07-25; row #8 flipped ✅ (CPE-1049); row #6's download/verify sub-surface automated (CPE-1058) — row stays in MVD for the still-attended in-place binary swap; row #9 added 2026-07-29 (CPE-1129 UAT deferred the standalone-board switcher's live-browser click-through) then flipped ✅ 2026-07-31 (CPE-1168 headless click-through)._

## Legend
`⛰ manual` = still needs human eyes · `🔧 in progress` = automation ticket open · `✅ automated` = retired,
pinned by a CI/guard job (must never regress).

## Ledger

| # | App aspect | Automated coverage today | Status | Automation to build (retires the manual step) | Ticket |
|---|-----------|--------------------------|--------|-----------------------------------------------|--------|
| 1 | **GUI end-to-end** (real Tauri/WebView2 app: navigate, click, dialogs, menus behave) | none headless — clicked by hand | 🔧 in progress | `tauri-driver` + WebDriver (Edge WebDriver on Win, WebKitWebDriver on Linux) driving the built app in CI; assert core flows. First slice: `--open <tmpdir>` → breadcrumb navigated | CPE-1045 |
| 2 | **Build → deploy → run smoke** (installer installs, app launches + responds) | done by hand each GUI verify | 🔧 in progress | CI smoke job: launch the built app, assert it answers a health/ping (folded into the CPE-1045 harness: window + `<body>` render check) | CPE-1045 |
| 3 | **Visual / theme regression** (light+dark, menus per MENUS.md, tabs per TABS.md, pill reflow) | none | ⛰ manual | Screenshot-diff harness over key screens in both themes; fail on unexpected pixel delta | _unfiled_ |
| 4 | **Cross-OS GUI** (macOS + Linux app behaviour, not just backend) | backend only (3-OS matrix) | ⛰ manual | macOS/Linux CI runners driving the headless GUI harness from #1 | _unfiled_ |
| 5 | **macOS Finder tag byte-interop** (Finder actually reads CPE's tag bytes) | codec round-trips in unit tests; real Finder unverified | ⛰ manual | macOS runner asserting via `mdls`/Finder that written tags are read back by the OS | CPE-828 (attended) |
| 6 | **Auto-update flow** (updater downloads, verifies signature, swaps in place) | manifest shape + minisign signature + version match automated by `crates/updater-verify` (CPE-1058, **merged** PR #376) | 🟡 partial — download/verify automated & pinned; binary-swap still attended | **Done for the download/verify/version sub-surface** — hermetic `crates/updater-verify` unit tests (manifest shape + minisign signature verify against the configured pubkey + version match) pinned on the 3-OS `Server crates` CI job, plus a `release.yml` guard (`verify-release-artifacts`, skips cleanly without signing secrets) re-checking the real built artifacts. **Residual still-manual: the in-place binary swap on each OS only** (needs a running app/GUI runner — kept in MVD) | CPE-1058 |
| 7 | **Real remote network run** (non-loopback client↔server over the wire) | loopback via `cpe-net` example + unit tests | ⛰ manual | Containerised two-host network E2E in CI (server container + client), asserting listing over a real socket | CPE-819/820 |
| 8 | **Native OS metadata interop** (ADS on Win, xattr on Linux) verified with OS tools | **self-asserting `cargo test`** (`native_meta_os_interop.rs`) reads back via the OS's own path (`file:stream` on Win / `getfattr` on Linux / `xattr` on macOS) and compares bytes | ✅ automated | Done — pinned by the `Backend` + `Server crates` 3-OS `cargo test` jobs (ubuntu leg now installs `attr` so `getfattr` always runs) | CPE-1049 |
| 9 | **Standalone agent-board sidecar UI** (Board/Epics/Sprints view-switcher click-to-swap in a live browser) | **live-browser click-through automated** (`sidecar/agent-board/clickthrough.mjs`): launches the built sidecar, does the ADR-0001 stdio handshake to reach `Ready`, drives the announced loopback URL with headless Edge (raw WebDriver via `msedgedriver`, zero-dep), clicks Board/Epics/Sprints and asserts each view's list renders + the others actually **hide** (computed `display`, not just the `hidden` prop) + snaps a screenshot per view; tears the sidecar + browser down. Caught & fixed a real swap bug (CPE-1168: `[hidden]` was overridden by `.cols`/`.list{display:flex}` so panes never hid) — pinned by the new `ui.rs` `board_html_is_valid` assertion. Plus the pre-existing agent-board `ui.rs` HTTP/HTML tests | ✅ automated | **Done** — local harness `node sidecar/agent-board/clickthrough.mjs` (msedgedriver + a `cargo build --release` of the sidecar). Not yet wired as a CI job (needs Edge+msedgedriver on the runner, like `gui-smoke`); a follow-up can add it to `gui-smoke.yml` | CPE-1168 |

## Already automated (the ratchet — must never regress to manual)

These are **not** MVD; listed so the QA Architect pins them and audits for regression each shift:

- Backend pure logic — extensive `cargo test` across `crates/server`, `crates/net`, `crates/security`,
  `crates/contract`.
- Frontend units — the large **vitest + jsdom** suite (`src/**/*.test.ts`), including the AI-Console
  launcher harness and component tests.
- Lint/format gate — `clippy --all-targets -D warnings`, both feature modes, in CI.
- 3-OS backend matrix (Linux + macOS + Windows).
- Guard tests — e.g. `sectionDocs.test.ts` (every section maps to a doc slug).

## Notes
- Rows 5 and 7 already have attended tickets (CPE-828, CPE-819/820); the QA Architect's job is to fold a
  **CI-automatable** slice out of each so they stop being purely hand/attended.
- Row 1 (headless GUI) is the **highest-leverage** item — it underlies most skip-to-user escalations and
  unblocks rows 3 and 4. Automate it first.
- When a row flips to ✅, decrement the MVD count in the header and name the pinning CI job in the row.

| CPE-1090 | Code-preview outline strip / pills reflow / breadcrumb-on-scroll / jump feel | render pinned by `gui-smoke` (CPE-1096): opens a seeded code fixture and asserts `.outline-bar` + `.outline-pill` render on a real `tauri build` binary; job is a non-blocking CI smoke signal (`continue-on-error`, CPE-1048, WebView2-flakiness caveat), not a hard gate; fold animation/jump *feel* still worth an occasional human glance | automated — pinned by `gui-smoke` (CPE-1096; non-blocking per CPE-1048) | 2026-07-26 |
| CPE-1091 | Per-line gutter/fold/minimap/indent-guides visual render | render pinned by `gui-smoke` (CPE-1096): asserts `.cl-row[data-line]` + `.minimap` render for the same fixture, plus the highlighted `<pre class="preview-text code-rows"><code class="cl-code">` (regression guard); job is a non-blocking CI smoke signal (`continue-on-error`, CPE-1048), not a hard gate; fold-*animation*/minimap-*drag* feel still worth an occasional human glance | automated — pinned by `gui-smoke` (CPE-1096; non-blocking per CPE-1048) | 2026-07-26 |
| CPE-1093 | Batch-media dialog: layout/pills/entry | **interaction logic** (op-building/removal/ordering + incomplete-op gating; debounced + generation-tokened `batchMediaPlan` preview incl. stale-response drop; validation-blocks-Apply; streamed-apply `done`/`failed` progress + completion `apply` dispatch; channel teardown on completion/unmount; non-destructive toggle → `BatchJob`) pinned by `BatchMediaDialog.test.ts` (jsdom, backend mocked, CPE-1105); pixel layout/pills/theme "looks good" was human-verified on installed 0.57.35 (2026-07-26). **Render now pinned by `gui-smoke` (CPE-1144, non-blocking per CPE-1048):** `specs/batch-media.smoke.ts` seeds valid PNGs, opens the dialog via the real right-click opener, adds a Resize op, and asserts the op-pill + plan-preview rows (with the backend's computed output names) render on a real `tauri build` binary; exact pixel/theme fidelity + real image-transform *output* still worth an occasional human glance | **render automated** — pixel/feel residual | 2026-07-30 |
| CPE-1094 | Agent-Watch replay scrubber: tab strip / slider drag / play cadence / diff-on-scrub | **render pinned by `gui-smoke` (CPE-1135; non-blocking per CPE-1048):** `wdio.conf.ts#seedReplayFixture` seeds a real audit-journal + baseline for a synthetic `gui-smoke-replay` session into the app-data dir the built binary reads; `specs/replay.smoke.ts` drives the real `tauri build` binary to the Replay tab and asserts `.rp-transport` + an *enabled* `.rp-slider` + `.rp-recon-list` containing the seeded filename render non-degenerate. Slider-*drag* / play-*cadence* / diff-on-scrub *feel* still worth an occasional human glance (same framing as CPE-1090/1091/1114) | render **automated** — feel residual | 2026-07-29 |
| CPE-1098 | Agent-Watch cost ledger tab: card layout / theme colours / live token+USD numbers | headless UAT+review only — needs eyes + a real agent session printing usage on the installed build; stays open (needs a live agent session, not just a static fixture) | open | 2026-07-26 |
| CPE-1100 | Agent-Watch radar tab: overlap rows / actor pills / navigate / live 2-actor race | headless UAT+review only — needs eyes + two concurrent actors racing a file on the installed build; stays open (needs a live 2-actor session, not just a static fixture) | open | 2026-07-26 |

### CPE-1130 (2026-07-29) — cost-History row flipped ✅
CPE-1114's row above flipped from "logic automated — visual residual" to "automated — pinned by
`gui-smoke`", closing the last open item this supplementary table tracked for it. Note on the header
**MVD (still-manual surfaces): 7** count: that number tracks only the 8 numbered rows in the primary
Ledger table above (which CPE-1114 is not one of — it lives in this secondary "new manual debt from
merged PRs" table, same as CPE-1090/1091/1093/1094/1098/1100). Consistent with how the CPE-1090/1091
flips earlier in this same table did **not** move that header number, this flip doesn't either — the
header stays at 7. If the QA Architect later wants this supplementary table folded into the primary
numbered Ledger, that's a separate reorganisation, not part of this ticket's scope.

### QA-Architect pass 2026-07-26 (Foreman-played; crew at agent cap)
- **Batch-media skip-on-error now pinned** (`crates/server` `batch_execute` integration test
  `a_real_looking_but_undecodable_image_is_skipped_while_valid_files_still_succeed`): a mixed batch of a valid
  PNG + valid JPEG + a real-looking-but-undecodable `.jpg` (valid SOI, no image data) → asserts written=2,
  skipped=1 with a reason, both valid outputs decode, the skipped input writes no output. This automates the
  exact scenario a user hit by hand on 0.57.36 ("2 selected → 1 output" that looked like a lost-file bug but
  was a correct skip), so it can never silently regress. Pinned on the `Backend` + `Server crates` 3-OS
  `cargo test` jobs. Complements CPE-1115 (which makes the skip *visible* in the dialog) + CPE-1105 (dialog
  logic) — the CPE-1093 "real image-transform output" residual is now largely covered at the integration level;
  only pure pixel/theme *feel* on the installed build remains human debt.

### Workshift 2026-07-26 (resume) — new manual debt from merged PRs
| CPE-1114 | Cost History tab: SVG over-time bar-chart geometry + hover tooltips; light/dark theme of `.hd-stat`/`.hd-bar`; drawer at 340px/90vw reflow; long agent/model name ellipsis+title; a real multi-week `history.jsonl` round-tripping into believable numbers | **logic fully automated** (`agentMetricsRollup.test.ts` 14 cases assert real values; AgentTimeline History-tab component behaviour, pull-only, empty/error states covered) **+ render pinned by `gui-smoke` (CPE-1130):** `wdio.conf.ts#seedHistoryFixture` seeds a synthetic 3-row `history.jsonl` straight into the real app-data dir the built binary reads from; `specs/cost-history.smoke.ts` seeds a synthetic watched-session announcement (test-mode-only hook) to reach the drawer, opens the History tab, and asserts `.hd-bar` (over-time chart), `.hd-totals`/`.hd-stat` (totals strip), and a `.hd-table` row (by-model/by-agent) all render non-empty on a real `tauri build` binary — non-blocking CI smoke signal (`continue-on-error`, CPE-1048), not a hard gate; exact pixel/theme colour fidelity still worth an occasional human glance (same framing as the CPE-1090/1091 rows above) | **automated — pinned by `gui-smoke` (CPE-1130; non-blocking per CPE-1048)** | 2026-07-29 |

- 2026-07-30 16:31 USMST — **CPE-1126 revert-safety GUI verify** (owner: user). The Agent-Watch restore panel + checkpoint markers are code-complete + reviewer-APPROVED (PR #466), but "confirm-to-revert is safe/clear" + "markers land right" need a user-present build→run. Blocker to full headless coverage: gui-smoke cannot render a checkpoint marker without a `checkpoint_create` test-mode seam (see CPE-1126 P2). Automating that seam would retire this row.
  → ✅ **RETIRED 2026-07-30 19:40 USMST.** CPE-1152 (PR #469, `dc2ea001`) added the seam: `gui-smoke/specs/checkpoint-restore.smoke.ts` drives the real `checkpoint_create` so the restore panel + a scrubber marker render, and `snap()`s `checkpoint-restore-panel` + `checkpoint-revert-confirm`. The **Visual Critic** judged those screenshots `VISUAL PASS` (look + safety-clarity), and the user signed off. The surface is now automatically screenshot-verifiable (non-blocking per CPE-1048) + Critic-judged — no routine human eyes needed. First formal Visual-Critic verdict on a GUI ticket (CPE-1148 loop).
