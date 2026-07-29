# Manual Test Burndown — the MVD ledger

The authoritative list of every app aspect that still needs a **human** to verify it, and the automation
that will retire each one. The QA Architect drives the **still-manual count (MVD) toward zero** and never
lets an automated row silently regress. Charter + rules: [README.md](README.md).

**MVD (still-manual surfaces): 8** · _baseline seeded 2026-07-25; row #8 flipped ✅ (CPE-1049); row #6's download/verify sub-surface automated (CPE-1058) — row stays in MVD for the still-attended in-place binary swap; row #9 added 2026-07-29 (CPE-1129 UAT deferred the standalone-board switcher's live-browser click-through)._

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
| 9 | **Standalone agent-board sidecar UI** (Board/Epics/Sprints view-switcher click-to-swap in a live browser) | HTTP/HTML surface asserted: agent-board `ui.rs` tests + CPE-1129 UAT curled `/api/cards`+`/api/epics`+`/api/sprints` and asserted the switcher DOM/endpoints; live-browser click behavior unverified | ⛰ manual | Fold the served sidecar UI into a `gui-smoke`-style headless drive (launch the sidecar, drive the loopback URL with WebDriver, click each view button, assert the list swaps) | _unfiled_ |

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
| CPE-1093 | Batch-media dialog: layout/pills/entry | **interaction logic** (op-building/removal/ordering + incomplete-op gating; debounced + generation-tokened `batchMediaPlan` preview incl. stale-response drop; validation-blocks-Apply; streamed-apply `done`/`failed` progress + completion `apply` dispatch; channel teardown on completion/unmount; non-destructive toggle → `BatchJob`) pinned by `BatchMediaDialog.test.ts` (jsdom, backend mocked, CPE-1105); pixel layout/pills/theme "looks good" was human-verified on installed 0.57.35 (2026-07-26); real image-transform *output* still worth an occasional spot-check | logic **automated** — pixel/theme feel residual | 2026-07-26 |
| CPE-1094 | Agent-Watch replay scrubber: tab strip / slider drag / play cadence / diff-on-scrub | headless UAT+review only — needs eyes on a real watched session in the installed build | open | 2026-07-26 |
| CPE-1098 | Agent-Watch cost ledger tab: card layout / theme colours / live token+USD numbers | headless UAT+review only — needs eyes + a real agent session printing usage on the installed build; stays open (needs a live agent session, not just a static fixture) | open | 2026-07-26 |
| CPE-1100 | Agent-Watch radar tab: overlap rows / actor pills / navigate / live 2-actor race | headless UAT+review only — needs eyes + two concurrent actors racing a file on the installed build; stays open (needs a live 2-actor session, not just a static fixture) | open | 2026-07-26 |

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
| CPE-1114 | Cost History tab: SVG over-time bar-chart geometry + hover tooltips; light/dark theme of `.hd-stat`/`.hd-bar`; drawer at 340px/90vw reflow; long agent/model name ellipsis+title; a real multi-week `history.jsonl` round-tripping into believable numbers | **logic fully automated** (`agentMetricsRollup.test.ts` 14 cases assert real values; AgentTimeline History-tab component behaviour, pull-only, empty/error states covered) — residual is **pixel/theme/geometry-only** on the installed build with a real persisted journal; candidate to fold into the `gui-smoke` fixture (seed a synthetic history.jsonl + assert `.hd-*`/`.hd-bar` render) | logic automated — visual residual | 2026-07-26 |
