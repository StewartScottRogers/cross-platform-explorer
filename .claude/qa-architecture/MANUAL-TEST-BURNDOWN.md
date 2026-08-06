# Manual Test Burndown — the MVD ledger

The authoritative list of every app aspect that still needs a **human** to verify it, and the automation
that will retire each one. The QA Architect drives the **still-manual count (MVD) toward zero** and never
lets an automated row silently regress. Charter + rules: [README.md](README.md).

**MVD (still-manual surfaces): 6** · _baseline seeded 2026-07-25; row #8 flipped ✅ (CPE-1049); row #6's download/verify sub-surface automated (CPE-1058) — row stays in MVD for the still-attended in-place binary swap; row #9 added 2026-07-29 (CPE-1129 UAT deferred the standalone-board switcher's live-browser click-through) then flipped ✅ 2026-07-31 (CPE-1168 headless click-through); **row #5 flipped ✅ 2026-08-04 (CPE-1307 — macOS `xattr` OS-interop test, confirmed on the macos-latest CI leg), 7→6**._

## Legend
`⛰ manual` = still needs human eyes · `🔧 in progress` = automation ticket open · `✅ automated` = retired,
pinned by a CI/guard job (must never regress).

## Ledger

| # | App aspect | Automated coverage today | Status | Automation to build (retires the manual step) | Ticket |
|---|-----------|--------------------------|--------|-----------------------------------------------|--------|
| 1 | **GUI end-to-end** (real Tauri/WebView2 app: navigate, click, dialogs, menus behave) | none headless — clicked by hand | 🔧 in progress | `tauri-driver` + WebDriver (Edge WebDriver on Win, WebKitWebDriver on Linux) driving the built app in CI; assert core flows. First slice: `--open <tmpdir>` → breadcrumb navigated | CPE-1045 |
| 2 | **Build → deploy → run smoke** (installer installs, app launches + responds) | done by hand each GUI verify | 🔧 in progress | CI smoke job: launch the built app, assert it answers a health/ping (folded into the CPE-1045 harness: window + `<body>` render check) | CPE-1045 |
| 3 | **Visual / theme regression** (app is light-theme-only — no dark variant to baseline; menus per MENUS.md, tabs per TABS.md, pill reflow) | pixel-diff comparator (`gui-smoke/lib/compare.ts`) exists, unit-tested headlessly (`npm run test:unit`, 15 cases incl. identical/one-pixel-changed/size-mismatch/tolerance/bless-flow), and wired as a worked example into `specs/open-dir.smoke.ts` (advisory, `GUI_SMOKE_VISUAL_STRICT=1` to gate). Real baselines for the rest of the `snap()`'d surfaces still need blessing against an actual `tauri build` (`GUI_SMOKE_UPDATE_BASELINE=1 npm test`) — not yet done for any real app surface, only for 2 synthetic demo baselines proving the flow | 🔧 in progress | Comparator done (CPE-1170); remaining automation: bless real baselines for the surfaces in the screenshot table above once a `tauri build` run is available, and optionally add a CI job that runs `GUI_SMOKE_VISUAL_STRICT=1` | CPE-1170 |
| 4 | **Cross-OS GUI** (macOS + Linux app behaviour, not just backend) | backend only (3-OS matrix) previously; **Linux GUI now driven headlessly** by a new `gui-smoke-linux` CI job (`.github/workflows/gui-smoke.yml`): `ubuntu-latest` + `tauri-driver` + `WebKitWebDriver` under `xvfb-run`, running the SAME WebdriverIO suite as the Windows leg (open-dir navigation, code-preview, organize, batch-media, cost-History, replay, context-menu specs) against a real `tauri build --no-bundle` binary. Non-blocking (`continue-on-error`, same posture as the Windows leg per CPE-1048) and **not yet run live on GitHub Actions** — offsite Actions verification pending as of this writing. macOS still has no `tauri-driver` support (no WKWebView WebDriver) and stays attended | 🔧 in progress | Linux leg built this ticket; flip to ✅ once it's proven green on a few `main` runs. macOS residual: attended, no automation path exists yet | CPE-1171 |
| 5 | **macOS Finder tag byte-interop** (Finder actually reads CPE's tag bytes) | **self-asserting `cargo test`** (`crates/server/tests/finder_tags_os_interop.rs`): `native_bridge::push` writes tags, then the OS's own `xattr -px com.apple.metadata:_kMDItemUserTags` reads the raw bytes back and the test decodes the binary plist and asserts the tag names (the exact bytes Finder reads); `native_bridge::pull` round-trip also asserted | ✅ automated | Done — pinned by the `Server crates` **macos-latest** `cargo test` leg (3-OS matrix), confirmed green on PR #603 (2026-08-04). Retires the hand-run `native_tags_demo.rs` eyeball. (Was mis-numbered CPE-828 → renumbered CPE-1307.) | CPE-1307 |
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
- Rows 5 and 7 already have attended tickets (CPE-1307, CPE-819/820); the QA Architect's job is to fold a
  **CI-automatable** slice out of each so they stop being purely hand/attended.
- Row 1 (headless GUI) is the **highest-leverage** item — it underlies most skip-to-user escalations and
  unblocks rows 3 and 4. Automate it first.
- When a row flips to ✅, decrement the MVD count in the header and name the pinning CI job in the row.

| CPE-1090 | Code-preview outline strip / pills reflow / breadcrumb-on-scroll / jump feel | render pinned by `gui-smoke` (CPE-1096): opens a seeded code fixture and asserts `.outline-bar` + `.outline-pill` render on a real `tauri build` binary; job is a non-blocking CI smoke signal (`continue-on-error`, CPE-1048, WebView2-flakiness caveat), not a hard gate; fold animation/jump *feel* still worth an occasional human glance | automated — pinned by `gui-smoke` (CPE-1096; non-blocking per CPE-1048) | 2026-07-26 |
| CPE-1091 | Per-line gutter/fold/minimap/indent-guides visual render | render pinned by `gui-smoke` (CPE-1096): asserts `.cl-row[data-line]` + `.minimap` render for the same fixture, plus the highlighted `<pre class="preview-text code-rows"><code class="cl-code">` (regression guard); job is a non-blocking CI smoke signal (`continue-on-error`, CPE-1048), not a hard gate; fold-*animation*/minimap-*drag* feel still worth an occasional human glance | automated — pinned by `gui-smoke` (CPE-1096; non-blocking per CPE-1048) | 2026-07-26 |
| CPE-1093 | Batch-media dialog: layout/pills/entry | **interaction logic** (op-building/removal/ordering + incomplete-op gating; debounced + generation-tokened `batchMediaPlan` preview incl. stale-response drop; validation-blocks-Apply; streamed-apply `done`/`failed` progress + completion `apply` dispatch; channel teardown on completion/unmount; non-destructive toggle → `BatchJob`) pinned by `BatchMediaDialog.test.ts` (jsdom, backend mocked, CPE-1105); pixel layout/pills/theme "looks good" was human-verified on installed 0.57.35 (2026-07-26). **Render now pinned by `gui-smoke` (CPE-1144, non-blocking per CPE-1048):** `specs/batch-media.smoke.ts` seeds valid PNGs, opens the dialog via the real right-click opener, adds a Resize op, and asserts the op-pill + plan-preview rows (with the backend's computed output names) render on a real `tauri build` binary; exact pixel/theme fidelity + real image-transform *output* still worth an occasional human glance | **render automated** — pixel/feel residual | 2026-07-30 |
| CPE-1094 | Agent-Watch replay scrubber: tab strip / slider drag / play cadence / diff-on-scrub | **render pinned by `gui-smoke` (CPE-1135; non-blocking per CPE-1048):** `wdio.conf.ts#seedReplayFixture` seeds a real audit-journal + baseline for a synthetic `gui-smoke-replay` session into the app-data dir the built binary reads; `specs/replay.smoke.ts` drives the real `tauri build` binary to the Replay tab and asserts `.rp-transport` + an *enabled* `.rp-slider` + `.rp-recon-list` containing the seeded filename render non-degenerate. Slider-*drag* / play-*cadence* / diff-on-scrub *feel* still worth an occasional human glance (same framing as CPE-1090/1091/1114) | render **automated** — feel residual | 2026-07-29 |
| CPE-1098 | Agent-Watch cost ledger tab: card layout / theme colours / live token+USD numbers | headless UAT+review only — needs eyes + a real agent session printing usage on the installed build; stays open (needs a live agent session, not just a static fixture) | open | 2026-07-26 |
| CPE-1100 | Agent-Watch radar tab: overlap rows / actor pills / navigate / live 2-actor race | **render pinned by `gui-smoke` (CPE-1255; non-blocking per CPE-1048):** a read-only spike overturned the earlier "needs a live 2-actor session" assumption — the tab renders purely from `foldOverlaps(entries)` (agentConflicts.ts) over the LIVE `agentTimeline` store, so no real concurrent-actor race is needed. `specs/radar.smoke.ts` seeds two `__CPE_TEST_INGEST_ACTIVITY__` batches (App.svelte, the existing CPE-1135 hook — no new seam) for one path with two distinct synthetic actors 200ms apart (inside the 5s `OVERLAP_WINDOW_MS`), then drives the real `tauri build` binary to the Radar tab and asserts `.rd-list`/`.rd-item` render with exactly two `.rd-pill` actor chips in `.rd-actors`; actor-resolution/navigate-on-click *feel* still worth an occasional human glance (same framing as CPE-1090/1091/1094) | render **automated** — feel residual | 2026-08-02 |
| CPE-1263 | File-content search dialog (`ContentIndexSearchDialog.svelte`, epic CPE-976): needs-build prompt, streamed index-build progress, ranked query→results (name/relative-path/score-bar/snippet), navigate-on-click, debounce + generation-token supersede | **Interaction logic pinned by `ContentIndexSearchDialog.test.ts`** (jsdom, `content_search`/`content_index_build` mocked, 11 cases): needs-build state renders a prompt not a raw error; streamed build progress updates live then unlocks search; build-error surfaced; ranked hits render name+relative-path+score%+snippet; clean no-matches state; navigate-on-click dispatches the file path + closes; Escape closes; debounce waits out the window before searching; a stale search's late result is dropped once superseded; clearing the query cancels the pending search. No `gui-smoke` spec yet — the dialog only opens via the command palette ("Search file contents…", no free keyboard shortcut was available), so a spec would need to drive the palette (`Ctrl+Shift+P` → type → Enter) rather than a direct key combo like `instant-search.smoke.ts`; that + real pixel/theme/score-bar-fill *feel* on the installed build still needs a human glance before this flips to "render automated" | **logic automated (jsdom)** — render/gui-smoke + pixel/feel residual | 2026-08-02 |

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

### File-Health panel — render-automated 2026-08-05 (CPE-1315..1321)
The new File-Health panel (4 folder scans: dangling-links, type-mismatch, orphan-sidecars, empty-dirs) + the
archive-safety right-click dialog are now **gui-smoke render-covered**: `gui-smoke/specs/file-health.smoke.ts`
+ `wdio.conf.ts::seedFileHealthFixture` drive a real `tauri build` binary, open Tools → File Health, and assert
each tab renders a real row over live IPC (first live exercise of the 3 `_stream` scan commands) — proven green
locally 2026-08-05. Advances MVD rows 1/3 (headless GUI + visual). The Visual Critic judged the panel's
screenshots and caught 2 real layout defects (mismatch badge overflow, orphan missing badge), both fixed
(CPE-1319/1321) — the CPE-1148 Visual-Critic loop working end-to-end without a user round-trip.

### Workshift 2026-08-05 — GUI batch (CPE-1323/1324 + Metadata Studio 1325-1328 + Declutter 1329)
Seven GUI tickets shipped across 4 epics. QA/coverage changes:
- **File-Health exclude UI (CPE-1323) + near-dup cleanup (CPE-1324):** render-covered by the existing
  `file-health.smoke.ts` / `near-duplicates.smoke.ts` specs and **Visual-Critic judged VISUAL PASS** on a real
  `tauri build` this shift (no user round-trip). Advances the "GUI render + visual" automation.
- **Declutter dialog (CPE-1329):** NEW `gui-smoke/specs/declutter.smoke.ts` + `seedDeclutterFixture` (one file
  per `ClutterReason`) drives the real build and asserts the 4 findings render under labelled groups — render
  automated on arrival. (Visual-Critic pass captured this shift.)
- **Metadata Studio (CPE-1325/1326/1327/1328):** logic fully jsdom-covered (checkpoint order, batch strip/copy
  payloads, per-field revert isolation, truthful-checkpoint on `Err(String)`). **No gui-smoke spec reaches
  MetadataStudioDialog** (it opens on a media selection, not a palette command) → its render/visual is still
  manual. Owed automation: a `metadata-studio.smoke.ts` that seeds a writable media file + opens the dialog.
- **OWED DEBT (interactive-state screenshots):** the File-Health/near-dup specs capture only the RESTING state —
  they never type an exclude pattern (so no filled exclude-pill row) or check a cleanup box (so no enabled
  Move-to-Bin). A future QA slice should add post-interaction `snap()`s so the Visual Critic can judge the
  filled/enabled states, not just the empty ones.
- **Docs gap (minor):** Declutter (like NearDuplicatesDialog) did not register a `sectionDocs.ts` Section/doc
  page. Not a CI failure (no new Section enum added), but a `src/docs` page for the new Tools features is owed
  per the self-maintaining-docs rule — worth a small follow-up ticket.

### Workshift 2026-08-05 (cont.) — coverage debt CLOSED (CPE-1331/1332) + backend 3D reader (CPE-1333)
- ✅ **Metadata Studio render/visual gap RETIRED:** `gui-smoke/specs/metadata-studio.smoke.ts` (CPE-1331) now drives
  a real `tauri build`, seeds a byte-accurate ID3v2.3 mp3, opens the Studio via the palette, and asserts the
  editable Title/Artist inputs render with correct values over live IPC. **Visual Critic judged it VISUAL PASS.**
  The 4 metadata tickets (CPE-1325-1328) shipped this shift are no longer render/visual-manual.
- ✅ **Interactive-state screenshots RETIRED:** file-health spec now snaps a FILLED exclude-pill (after clicking a
  quick-add chip); near-dup spec now snaps the ENABLED "Move 1 to Recycle Bin" (after checking a box). Both
  Visual-Critic PASS. The "resting-state-only" owed debt from earlier this shift is closed.
- ✅ **Docs gap RETIRED (CPE-1332):** src/docs pages added for Declutter (23), Near-Duplicates (24), Metadata
  Studio (25) — the CPE-579 self-maintaining-docs guardrail is satisfied for these Tools features.
- **New coverage (CPE-1333, 3D reader):** `model_3d.rs` + `file_type.rs` 3D signatures are pure-logic cargo-tested
  (20 inline tests incl. malformed/hostile-input → None, no panic; Reviewer's panic/DoS pass clean). Backend, not
  a manual surface. Follow-up owed: a frontend 3D-model metadata column/pane (wires `read_model_info`).

### CPE-1358 (2026-08-06) — "open each supported file type by hand" RETIRED
A PDF preview crash (CPE-1357) was found by hand — `samples/documents/doc.pdf` was itself a degenerate,
unloadable fixture (`/Kids [] /Count 0`, no `xref` table) that took the whole app down. This ticket turns
"open every kind of file and see if it breaks" into two permanent CI ratchets so that class of bug can't
hide again:
- **Headless coverage ratchet:** `src/lib/sampleCoverage.test.ts` computes the REAL preview-provider
  `kind` (via `pickProvider`, `src/lib/preview/provider.ts` — the production code path, not a
  hand-maintained duplicate) for every file under `samples/` and fails if any supported kind has zero
  samples. Filled the gaps: `images/photo.tiff` (decoded-image), `text/table.tsv` (tsv),
  `archives/sample.zip` (archive — `sample.rar` does NOT count, it's not in the frontend's `ARCHIVE_EXT`
  and resolves to the generic hex provider instead), `fonts/mini.ttf` (font, hand-built sfnt),
  `database/mini.sqlite` (data-grid), `other/tiny.wasm` (info). `documents/doc.pdf` replaced with a real, valid
  2-page PDF (byte-accurate `xref`), preserving the exact `/Info` metadata baseline
  `sample_fixtures.rs::pdf_info_baseline` already asserted. The OLD degenerate bytes are preserved
  unchanged as `documents/malformed.pdf` — the deliberate crash-regression fixture.
- **End-to-end walk:** `gui-smoke/specs/samples.smoke.ts` seeds a copy of the real `samples/` tree into
  the shared tmpDir (`wdio.conf.ts#seedSamplesFixture`, `fs.cpSync` — new samples are picked up
  automatically, no filename list to maintain) and, on a real `tauri build` binary, opens EVERY file:
  navigates to its folder via the address bar (Ctrl+L), selects it, and asserts (a) the app/window is
  still responding (the crash guard) and (b) the preview settled into real content or an explicit
  graceful "can't preview" note — never a stuck spinner. `documents/malformed.pdf` runs LAST, in its own
  `it()`, specifically so a still-open crash there doesn't blind the rest of the walk; it's expected to
  FAIL today and pass once CPE-1357's fix lands. Non-blocking on both CI legs (`continue-on-error`,
  CPE-1048), same posture as the rest of `gui-smoke`.

**Not yet run live on GitHub Actions** (offsite Actions verification pending, same caveat as CPE-1171's
Linux leg) — locally verified: `npm run check` clean, the full vitest suite (187 files / 2080 tests)
green including the new coverage test, `gui-smoke`'s own `typecheck` + `test:unit` green, and the Rust
`sample_fixtures` test green against the new `doc.pdf`. The `samples.smoke.ts` walk itself needs a real
`tauri build --no-bundle` + `tauri-driver` session (this worktree's sandbox couldn't run one) — watch the
first CI run for the malformed-pdf-guard result and the cascading-failure note in that spec's header
comment (a still-open CPE-1357 crash there is expected to red the later-alphabetical specs too, sharing
one app session — not a separate new regression).
