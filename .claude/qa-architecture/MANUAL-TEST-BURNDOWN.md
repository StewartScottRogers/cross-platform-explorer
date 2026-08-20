# Manual Test Burndown — the MVD ledger

The authoritative list of every app aspect that still needs a **human** to verify it, and the automation
that will retire each one. The QA Architect drives the **still-manual count (MVD) toward zero** and never
lets an automated row silently regress. Charter + rules: [README.md](README.md).

**MVD (still-manual surfaces): 6 primary + 10 supplementary = 16 total** · _baseline seeded 2026-07-25; row #8 flipped ✅ (CPE-1049); row #6's download/verify sub-surface automated (CPE-1058) — row stays in MVD for the still-attended in-place binary swap; row #9 added 2026-07-29 (CPE-1129 UAT deferred the standalone-board switcher's live-browser click-through) then flipped ✅ 2026-07-31 (CPE-1168 headless click-through); **row #5 flipped ✅ 2026-08-04 (CPE-1307 — macOS `xattr` OS-interop test, confirmed on the macos-latest CI leg), 7→6**._
_**2026-08-10 QA-Architect pass:** primary ledger **unchanged at 6** (nothing flipped, nothing added). **+5 supplementary rows** logged this shift (see "Sprint 2026-08-10" section at the foot) → supplementary still-manual 5→10, **total 11→16, delta +5**. MVD ROSE this shift and no automation retired anything, because the only CI substrate that could have retired it — `gui-smoke` — has produced **zero terminal verdicts in 800 consecutive runs** (see the diagnosis section below). Retiring ticket for the substrate: **CPE-1594**._
_**2026-08-10 (same shift, CPE-1594 landed):** **rows #1, #2, #4 flipped ✅, primary 6→3, total 16→13.** The `gui-smoke-linux` job is now the BLOCKING gate (ratchet against `gui-smoke/known-failing.json`, `continue-on-error` removed) instead of a non-blocking diagnostic that never concluded — a regression outside the known-failing list now actually reds CI instead of needing a human to notice, which is the bar this ledger uses for ✅. Residuals kept honest, not hidden: **Windows leg** (row 1/2's other half) stays non-blocking/off-hot-path (CPE-1048, unfixed) — a canary only; **macOS** (row 4) stays fully attended, no `tauri-driver` support exists; **7 of 40 Linux specs** are pinned known-failing pending triage (CPE-1595; `network.smoke.ts`'s selector was fixed — the old `=text` link-text locator could never match a `<span>` — but a live PR #801 CI run confirmed it STILL fails on WebKitGTK/Xvfb, the same class of `.fav-title getText()` issue `saved-search.smoke.ts` is already listed for, so it stays in the list; `samples`/`saved-search` remain CPE-1507's). Row #3 stays 🔧: screenshots now reach CI as a build artifact (unblocking the CPE-1148 Visual Critic there for the first time), but real per-surface baselines still aren't blessed — that's still open work, not this ticket's scope. **Correction, same shift:** PR #801's first live run also surfaced two workflow bugs — the screenshot upload silently matched zero files (`actions/upload-artifact@v4` excludes dot-prefixed folders like `.screenshots/` by default; needs `include-hidden-files: true`) and, worse, its `if-no-files-found: error` aborted the job BEFORE the ratchet step ran at all. Both fixed: `include-hidden-files: true` added, the upload downgraded to `warn` and moved to run AFTER the ratchet step so the gate always executes regardless of the artifact's own outcome._
_**2026-08-11 (CPE-1629):** supplementary row **CPE-1586 flipped ✅, supplementary 10→9, total 13→12.** `gui-smoke` had **zero preview-pane coverage at all** (the CPE-1615/PR#820 review had to hand-build a throwaway Vite+Chrome harness to look at the new Binary Inspector tab, then threw it away) and **zero dark-theme coverage anywhere** (every visual surface was verified light-only, per CPE-1586's own note). `gui-smoke/specs/preview-pane.smoke.ts` fixes both at once: it opens the preview pane against committed `samples/` files and `snap()`s the Binary Inspector's tabs (data-driven off `.bp-tabs .tab`, walked against both a native PE — `other/mini.dll` — and a new managed .NET PE fixture — `other/mini-dotnet.dll`), the sqlite data-grid, the font glyph-grid (retiring CPE-1586), and the cert/JWT EXPIRED-badge pills — **every surface in both light AND dark theme, and at both a narrow (220px) AND a comfortable (400px) pane width**. Two new reusable helpers land in the harness for future specs: `lib/theme.ts#setTheme()` and `lib/paneWidth.ts#setPreviewPaneWidth()`. Confirmed on a real `tauri build --no-bundle`: 6/6 tests passing, all screenshots opened and visually verified (narrow width visibly reflows the Binary Inspector's tab strip + wraps its field list; dark theme renders correctly throughout; the managed-PE fixture correctly triggers the app's existing "possible managed .NET" heuristic banner where the native fixture doesn't — proof the fixture is a genuine managed PE, not a placeholder). **CPE-1615 (PR #820, the ".NET metadata" tab) merged into `main` mid-ticket** — merging it into this branch and re-running the suite confirmed the managed-PE walk test picks up the REAL ".NET metadata" tab automatically at full flagship (2x2 combo) depth with ZERO spec changes: `binary-managed-net-metadata-*.png` shows the real Assembly-identity + Referenced-assemblies tables, matching this exact fixture's contents. **The ".NET metadata tab" acceptance criterion is literally satisfied, not just designed for.** `gui-smoke/known-failing.json`'s baseline is UNCHANGED (still 7) — no new spec was added to it. CPE-1577/1570/1576/1578/1573/1560 (the other Sprint 2026-08-10 supplementary rows) are untouched — different surfaces._
_**2026-08-11 (QA-Architect shift, post-50/50-run audit):** **+5 primary rows (#10–#14), primary 3→8, total
12→17, delta +5.** Nothing regressed and nothing flipped this shift — every added row is **discovery of
pre-existing, unlogged debt**, three of it owed to the user for weeks. Rows #10 (tray), #11 (archive
drag-out) and #12 (AI search) come straight off `CHECKPOINT.md`'s "Owed to the USER" list and had **never
been logged here at all** despite charter rule 1 — an audit gap, now closed. Row #13 promotes the
real-remote-server gap from a prose bullet in the 2026-08-09 section to a first-class row (a shipped
product surface with no honest test is not a footnote). Row #14 is genuinely new debt from this run:
CPE-1620/1622 edited `sidecar/ai-console/src/launcher.html` and CPE-1621 edited `console.rs`, and the AI
Console's UI has **no browser-level coverage of any kind** — `gui-smoke` cannot reach it (the sidecar serves
it over HTTP; it is not the Tauri webview). Retiring ticket filed this shift: **CPE-1659** (rows #7 + #13).
**Deliberately NOT logged as rows, with reasons:** (a) the "visual/taste glance on everything shipped" + the
mustard/steel-blue swatch pick-list — that is *taste*, which the CPE-1148 split reserves for the user by
design; the Visual Critic already covers visual *defects* headlessly, so logging taste as burnable MVD would
be logging something we never intend to burn. (b) "`main` has no branch protection" — a real gap where a
human must notice, but a repo setting, not an app surface; it belongs in the run report, not this ledger.
(c) CPE-1641's crashed-session badge in the History tab — `cost-history.smoke.ts` already drives and
`snap()`s that tab; only the *crashed* state is unseeded, which is one extra fixture row in an existing
spec, not a new manual surface. Logging it would be padding._

_**2026-08-12 (CPE-1659 landed):** **rows #7 and #13 flipped ✅, primary 8→6, total 17→15.** The
real-server Docker rig (`Network E2E (ubuntu-latest, real servers)`, CI-blocking on push + pull_request,
~13-14 min) is green: SFTP/WebDAV/FTP/FTPS conformance through the real `cpe_vfs::open` seam against
OpenSSH/Apache/vsftpd, SFTP host-key TOFU both ways against a real sshd, and slice 2's non-loopback
`cpe-net` client↔server run — all against servers this team did not write. Getting there found and fixed
six real bugs, split evenly between the client code under test and the rig itself: **client bugs** — a
WebDAV href `#` truncating the URL as a fragment, a WebDAV directory `DELETE` silently no-op'ing on
Apache's `DirectorySlash` 301 redirect, and `list_dir`'s missing-path error mapping to `Internal` instead
of `NotFound` (row #7's own Slice 2 test caught this on its first-ever execution); **rig bugs** — the
generated vsftpd.conf not owned by root (vsftpd's own safety check refuses to load an unowned config),
vsftpd's default `ssl_ciphers` sharing no cipher overlap with rustls, and the throwaway FTPS cert
inheriting `CA:TRUE` from OpenSSL's default config. The required negative control passed: the WebDAV
directory-delete fix was deliberately reverted and pushed, the rig went RED
(`webdav_conformance_against_real_apache_moddav` failed) while the in-process `cargo test -p cpe-webdav`
suite stayed GREEN, then the fix was restored. Green run:
https://github.com/StewartScottRogers/cross-platform-explorer/actions/runs/31584936382 · Red
(negative-control) run: https://github.com/StewartScottRogers/cross-platform-explorer/actions/runs/31590466435._

_**2026-08-20 (QA-Architect shift, batched run `batched-2026-08-17-1929`):** supplementary row
**CPE-1098 flipped ✅** (stale since 2026-07-26 — `gui-smoke/specs/cost-ledger.smoke.ts` has pinned that
tab's render since **CPE-1173**, which closed in Week-31 and was never reflected here), supplementary
9→8. **+2 new supplementary rows** for debt this run shipped (see the "Sprint 2026-08-17→20" section at
the foot): the Trash view's three new degraded-listing states (CPE-1803/1804/1805) and the StatusBar's two
new advisory lines (CPE-1708's `.filtered-hidden`, CPE-1775's archive skip notice). Supplementary 8→10,
**total 15→16, delta +1**. Primary ledger unchanged at 6.

**Stale-obstacle correction (it changes what row #12 costs).** Rows **#12** and **CPE-1263** have both said
since 2026-07-25 that their blocker is *"the dialog opens via the command palette rather than a free key
combo, so a spec would need to drive the palette"*. **That is no longer true and has not been for weeks.**
Three specs already drive `Ctrl+Shift+P` → `.cp-input` → `addValue` → Enter against the real built app and
are **green on the blocking WebKitGTK/Xvfb leg** (none appears in `known-failing.json`):
`near-duplicates.smoke.ts`, `similar-images.smoke.ts`, `declutter.smoke.ts` — and the palette-open block is
copy-pasted verbatim in all three. So what these two rows are waiting on is not a research problem: it is
extracting `gui-smoke/lib/palette.ts` from code that already works and writing one spec on top of it. Filed
this shift as **CPE-1819**.

**Deliberately NOT logged as rows, with reasons:** (a) the bidi/format-char render guard extended across
CPE-1757/1761/1766/1767/1768/1776/1790 — that is a *new automated guard*, a ratchet addition, not MVD;
(b) the backend-only fixes that dominated this run (S3 key handling, tar/zip extraction refusals, trash
parsing, the ffmpeg pin, temp-dir leaks) — all pinned by `cargo test` on the 3-OS matrix, with no rendering
to look at; (c) `gui-smoke`'s own sharding/verdict/ratchet work (CPE-1753/1728/1772/1781/1799/1677) — that
is the testing substrate improving, which is this role's product, not its debt._

## Legend
`⛰ manual` = still needs human eyes · `🔧 in progress` = automation ticket open · `✅ automated` = retired,
pinned by a CI/guard job (must never regress).

## Ledger

| # | App aspect | Automated coverage today | Status | Automation to build (retires the manual step) | Ticket |
|---|-----------|--------------------------|--------|-----------------------------------------------|--------|
| 1 | **GUI end-to-end** (real Tauri/WebView2 app: navigate, click, dialogs, menus behave) | `tauri-driver` + WebDriver drives the real built app through 40 specs (`gui-smoke/specs/*.smoke.ts` — navigation, dialogs, menus, context-menu, macros, vault, archives, and more). **`gui-smoke-linux` (ubuntu-latest, WebKitWebDriver) is the BLOCKING gate (CPE-1594)**: `gui-smoke/lib/ratchet.ts` reds the job on any spec failing outside the committed `gui-smoke/known-failing.json` (7 of 40, pending CPE-1595 triage) or on the run not completing at all — a real regression on the other 33 specs now reds CI without a human needing to notice | ✅ automated | Pinned by `GUI smoke (ubuntu-latest)` + its ratchet step (CI-blocking). **Residual:** the `gui-smoke` (windows-latest) leg stays non-blocking + off the push/PR path (CPE-1048 — WebView2 `DevToolsActivePort` crash, unfixed; manual/nightly canary only) | CPE-1045 / CPE-1594 |
| 2 | **Build → deploy → run smoke** (installer installs, app launches + responds) | Folded into the same CPE-1045 harness: `open-dir.smoke.ts` asserts the launched window + `<body>` render non-empty content before any other assertion runs. Now covered by the same **blocking** `gui-smoke-linux` ratchet gate as row 1 | ✅ automated | Pinned by `GUI smoke (ubuntu-latest)` + its ratchet step. **Residual:** the Windows leg (real installer/WebView2 launch on the actual target OS) stays non-blocking + manual/nightly-only (CPE-1048) | CPE-1045 / CPE-1594 |
| 3 | **Visual / theme regression** (menus per MENUS.md, tabs per TABS.md, pill reflow, light+dark per CPE-1492/1493) | pixel-diff comparator (`gui-smoke/lib/compare.ts`) exists, unit-tested headlessly (`npm run test:unit`, 15 cases incl. identical/one-pixel-changed/size-mismatch/tolerance/bless-flow), wired as a worked example into `specs/open-dir.smoke.ts` (advisory, `GUI_SMOKE_VISUAL_STRICT=1` to gate). **CPE-1594: both `gui-smoke` legs now upload `gui-smoke/.screenshots/**` as a CI artifact (`if: always()`, so failing-run `-fail.png` shots upload too) — the ~75 `snap()`'d surfaces per run reach CI for the first time**, unblocking the CPE-1148 Visual Critic there instead of requiring a Foreman to run a local `tauri build` by hand. Real per-surface baselines still aren't blessed against those screenshots — that blessing work itself is not yet done, only for 2 synthetic demo baselines | 🔧 in progress | Comparator + CI screenshot substrate done (CPE-1170, CPE-1594); remaining automation: bless real baselines for the screenshot-table surfaces now that the artifact exists, and optionally gate on `GUI_SMOKE_VISUAL_STRICT=1` | CPE-1170 / CPE-1594 |
| 4 | **Cross-OS GUI** (macOS + Linux app behaviour, not just backend) | backend only (3-OS matrix) previously; **Linux GUI now driven headlessly AND blocking (CPE-1594)** by `gui-smoke-linux` (`.github/workflows/gui-smoke.yml`): `ubuntu-latest` + `tauri-driver` + `WebKitWebDriver` under `xvfb-run`, running the full WebdriverIO suite (open-dir navigation, code-preview, organize, batch-media, cost-History, replay, context-menu, archives, vault, macros, and more) against a real `tauri build --no-bundle` binary, gated by the `gui-smoke/lib/ratchet.ts` known-failing ratchet instead of `continue-on-error`. **Confirmed running the whole suite to completion on a LIVE PR #801 CI run** (33/40 passing, 7 known-failing pending CPE-1595 triage) — the earlier "not yet proven green" caveat is resolved; "flaky" was a misdiagnosis, the leg completes reliably in ~28 min. macOS still has no `tauri-driver` support (no WKWebView WebDriver) and stays attended | ✅ automated (Linux) | **Done for Linux** — pinned by `GUI smoke (ubuntu-latest)` + its ratchet step. **macOS residual: attended, no automation path exists yet** (no WKWebView WebDriver support in `tauri-driver`) | CPE-1171 / CPE-1594 |
| 5 | **macOS Finder tag byte-interop** (Finder actually reads CPE's tag bytes) | **self-asserting `cargo test`** (`crates/server/tests/finder_tags_os_interop.rs`): `native_bridge::push` writes tags, then the OS's own `xattr -px com.apple.metadata:_kMDItemUserTags` reads the raw bytes back and the test decodes the binary plist and asserts the tag names (the exact bytes Finder reads); `native_bridge::pull` round-trip also asserted | ✅ automated | Done — pinned by the `Server crates` **macos-latest** `cargo test` leg (3-OS matrix), confirmed green on PR #603 (2026-08-04). Retires the hand-run `native_tags_demo.rs` eyeball. (Was mis-numbered CPE-828 → renumbered CPE-1307.) | CPE-1307 |
| 6 | **Auto-update flow** (updater downloads, verifies signature, swaps in place) | manifest shape + minisign signature + version match automated by `crates/updater-verify` (CPE-1058, **merged** PR #376) | 🟡 partial — download/verify automated & pinned; binary-swap still attended | **Done for the download/verify/version sub-surface** — hermetic `crates/updater-verify` unit tests (manifest shape + minisign signature verify against the configured pubkey + version match) pinned on the 3-OS `Server crates` CI job, plus a `release.yml` guard (`verify-release-artifacts`, skips cleanly without signing secrets) re-checking the real built artifacts. **Residual still-manual: the in-place binary swap on each OS only** (needs a running app/GUI runner — kept in MVD) | CPE-1058 |
| 7 | **Real remote network run** (non-loopback client↔server over the wire) | **self-asserting `cargo test`** (`crates/net/tests/real_server_e2e.rs`, `#[ignore]`d, run with `--ignored` in the new CI job): the real `cpe-server-ref` binary runs in its own container on the job's Docker bridge network, driven from the test process at the container's own IP (172.28.0.14) — a genuinely different network namespace, never `127.0.0.1`. Asserts a real listing crosses the socket AND the missing-path error comes back as a structured `NotFound` (this assertion caught a real dispatcher bug on its first-ever run: `list_dir`'s error mapping flattened every domain error to `Internal`, fixed in `crates/server/src/dispatch.rs`, with an in-process regression test added too) | ✅ automated | Pinned by `Network E2E (ubuntu-latest, real servers)` (CI-blocking, push + pull_request). Confirmed green on a live run: https://github.com/StewartScottRogers/cross-platform-explorer/actions/runs/31584936382 | CPE-819/820 → CPE-1659 |
| 8 | **Native OS metadata interop** (ADS on Win, xattr on Linux) verified with OS tools | **self-asserting `cargo test`** (`native_meta_os_interop.rs`) reads back via the OS's own path (`file:stream` on Win / `getfattr` on Linux / `xattr` on macOS) and compares bytes | ✅ automated | Done — pinned by the `Backend` + `Server crates` 3-OS `cargo test` jobs (ubuntu leg now installs `attr` so `getfattr` always runs) | CPE-1049 |
| 9 | **Standalone agent-board sidecar UI** (Board/Epics/Sprints view-switcher click-to-swap in a live browser) | **live-browser click-through automated** (`sidecar/agent-board/clickthrough.mjs`): launches the built sidecar, does the ADR-0001 stdio handshake to reach `Ready`, drives the announced loopback URL with headless Edge (raw WebDriver via `msedgedriver`, zero-dep), clicks Board/Epics/Sprints and asserts each view's list renders + the others actually **hide** (computed `display`, not just the `hidden` prop) + snaps a screenshot per view; tears the sidecar + browser down. Caught & fixed a real swap bug (CPE-1168: `[hidden]` was overridden by `.cols`/`.list{display:flex}` so panes never hid) — pinned by the new `ui.rs` `board_html_is_valid` assertion. Plus the pre-existing agent-board `ui.rs` HTTP/HTML tests | ✅ automated | **Done** — local harness `node sidecar/agent-board/clickthrough.mjs` (msedgedriver + a `cargo build --release` of the sidecar). Not yet wired as a CI job (needs Edge+msedgedriver on the runner, like `gui-smoke`); a follow-up can add it to `gui-smoke.yml` | CPE-1168 |

| 10 | **System tray icon + tray menu** (icon appears, menu items open/act, close-to-tray and restore behave) | none at any level — no `gui-smoke` spec reaches the tray (it is an OS shell surface outside the webview), no unit test covers the menu wiring end-to-end | ⛰ manual | Needs a substrate that does not exist yet: OS-shell automation for the Windows notification area (UI Automation / `pywinauto`-class driving) or an injectable tray seam so the *menu model* + its actions can at least be asserted headlessly, leaving only "the icon is visibly there" to a human. Log the cheap half first: assert the tray menu model + action dispatch in a unit test | — (owed to user; unfiled) |
| 11 | **Archive drag-out to the OS shell** (dragging an entry out of an opened archive to Explorer/Finder materialises the real file) | none — the in-app drag logic has unit coverage, but the hand-off to the OS shell (OLE drag-drop / `DROPFILES`) is unexercised | ⛰ manual | No substrate reaches this: CDP mouse injection (`gui-smoke/lib/mouse.ts`) drives the *webview*, and the drop target is the OS shell, outside it. Cheapest honest slice: assert the shell payload the app hands the OS (the temp-materialised path + the drag descriptor) in a Rust test, leaving only the physical drag to a human | — (owed to user; unfiled) |
| 12 | **AI search dialog end-to-end** (shipped v0.57.45; query → results → navigate on a real build) | logic covered by jsdom component tests; **no `gui-smoke` spec** — the dialog opens via the command palette rather than a free key combo, the same obstacle recorded for `ContentIndexSearchDialog` (CPE-1263) | ⛰ manual | A `gui-smoke` spec that drives the palette (`Ctrl+Shift+P` → type → Enter) to open it and `snap()`s it in both themes; solving it also retires the CPE-1263 residual. **2026-08-20 correction: the "palette-driven opening is the obstacle" premise above is OBSOLETE** — `near-duplicates`, `similar-images` and `declutter` already do exactly that, green, on the blocking Linux leg. The work left is to extract `gui-smoke/lib/palette.ts` from those three and write the spec. Headless residual that survives: the *ranked-results* leg needs a live embedding endpoint, so the deterministic assertion is the needs-build affordance (same off-means-off shape as `instant-search.smoke.ts`), with the full query→results→navigate loop pinned on the sibling literal content search instead | **CPE-1819** (filed 2026-08-20) |
| 13 | **Remote providers against a REAL server** (SFTP / WebDAV / FTP / FTPS interop: transfer modes, href encoding, host-key TOFU vs a real sshd) | **self-asserting `cargo test`** (`crates/vfs/tests/real_server_conformance.rs`, `#[ignore]`d, run with `--ignored` in the new CI job): one shared conformance fn driven through the real `cpe_vfs::open` routing seam against real OpenSSH `sftp-server` (`atmoz/sftp`), Apache `mod_dav` (`bytemark/webdav`), and vsftpd (`fauria/vsftpd`) in Docker on `ubuntu-latest`. List/stat/byte-exact-5MiB-read/write-roundtrip (incl. `#`/`%`/emoji/CRLF names) all pass; mkdir/rename/delete verified from the **host-mounted directory on disk**; missing-path is a clean `Err`; SFTP host-key TOFU proven `Trusted` **and** `Changed`-is-refused against a real OpenSSH key; FTPS negotiates `AUTH TLS` against vsftpd's real certificate. Three real client bugs found and fixed along the way: a WebDAV href `#` truncating the URL as a fragment, a WebDAV directory `DELETE` silently no-op'ing on Apache's `DirectorySlash` 301 redirect, and a dispatcher gap mapping every missing-path error to `Internal` instead of `NotFound` (row #7). Three real rig/infra bugs found and fixed: vsftpd's generated config not owned by root (its own safety check refuses to load it), vsftpd's default `ssl_ciphers` sharing no overlap with rustls, and the throwaway FTPS cert inheriting `CA:TRUE` from OpenSSL's default config. **Required negative control passed**: the WebDAV directory-delete fix was deliberately reverted and pushed — the rig went RED (`webdav_conformance_against_real_apache_moddav` failed: "the now-empty directory must be gone from disk after delete") while the in-process `cargo test -p cpe-webdav` suite (which never deletes a directory) stayed GREEN — then reverted back. Red run: https://github.com/StewartScottRogers/cross-platform-explorer/actions/runs/31590466435 | ✅ automated | Pinned by `Network E2E (ubuntu-latest, real servers)` (CI-blocking, push + pull_request, ~13-14 min). Shrinks CPE-1518 to its device-specific residue and gives the SMB epic (CPE-1504) a landing pad | CPE-1659 |
| 14 | **AI Console sidecar UI in a real browser** (launcher tabs/`#tabs` strip, model picker, session list, History, "Close all consoles") | jsdom only (`src/lib/ai-console-launcher.test.ts`) + Rust tests on `ui.rs`. **No browser-level coverage at all** — `gui-smoke` drives the Tauri webview and cannot reach this UI, which the sidecar serves over HTTP | ⛰ manual | Copy the proven `sidecar/agent-board/clickthrough.mjs` pattern (CPE-1168): launch the built sidecar, do the ADR-0001 stdio handshake to `Ready`, drive the announced loopback URL with headless Edge via raw WebDriver, click each surface, assert render + that the others actually hide, `snap()` per view. The template already exists and is proven — this is mostly transcription | — (unfiled; runner-up this shift) |

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
  **CI-automatable** slice out of each so they stop being purely hand/attended. (Row 5 done 2026-08-04;
  row 7's slice is now filed as CPE-1659 slice 2.)
- Row 1 (headless GUI) is the **highest-leverage** item — it underlies most skip-to-user escalations and
  unblocks rows 3 and 4. Automate it first. _(Done 2026-08-10 via CPE-1594.)_
- Rows 10–12 are the "Owed to the USER" queue that `CHECKPOINT.md` has carried for weeks. Rows 10 and 11
  are the hard cases: both live **outside the webview**, so no existing substrate (`gui-smoke` + CDP mouse)
  can reach them at all. Their honest first slice is to automate the *model/payload* half and shrink the
  human step to "the icon is there" / "the drag physically works", rather than pretend a jsdom test
  covered them.
- Row 12's real obstacle is **palette-driven opening**, which also blocks CPE-1263. Build the palette
  helper into `gui-smoke/lib/` once and both retire — prefer it over one-off specs.
- When a row flips to ✅, decrement the MVD count in the header and name the pinning CI job in the row.

| CPE-1090 | Code-preview outline strip / pills reflow / breadcrumb-on-scroll / jump feel | render pinned by `gui-smoke` (CPE-1096): opens a seeded code fixture and asserts `.outline-bar` + `.outline-pill` render on a real `tauri build` binary; job is a non-blocking CI smoke signal (`continue-on-error`, CPE-1048, WebView2-flakiness caveat), not a hard gate; fold animation/jump *feel* still worth an occasional human glance | automated — pinned by `gui-smoke` (CPE-1096; non-blocking per CPE-1048) | 2026-07-26 |
| CPE-1091 | Per-line gutter/fold/minimap/indent-guides visual render | render pinned by `gui-smoke` (CPE-1096): asserts `.cl-row[data-line]` + `.minimap` render for the same fixture, plus the highlighted `<pre class="preview-text code-rows"><code class="cl-code">` (regression guard); job is a non-blocking CI smoke signal (`continue-on-error`, CPE-1048), not a hard gate; fold-*animation*/minimap-*drag* feel still worth an occasional human glance | automated — pinned by `gui-smoke` (CPE-1096; non-blocking per CPE-1048) | 2026-07-26 |
| CPE-1093 | Batch-media dialog: layout/pills/entry | **interaction logic** (op-building/removal/ordering + incomplete-op gating; debounced + generation-tokened `batchMediaPlan` preview incl. stale-response drop; validation-blocks-Apply; streamed-apply `done`/`failed` progress + completion `apply` dispatch; channel teardown on completion/unmount; non-destructive toggle → `BatchJob`) pinned by `BatchMediaDialog.test.ts` (jsdom, backend mocked, CPE-1105); pixel layout/pills/theme "looks good" was human-verified on installed 0.57.35 (2026-07-26). **Render now pinned by `gui-smoke` (CPE-1144, non-blocking per CPE-1048):** `specs/batch-media.smoke.ts` seeds valid PNGs, opens the dialog via the real right-click opener, adds a Resize op, and asserts the op-pill + plan-preview rows (with the backend's computed output names) render on a real `tauri build` binary; exact pixel/theme fidelity + real image-transform *output* still worth an occasional human glance | **render automated** — pixel/feel residual | 2026-07-30 |
| CPE-1094 | Agent-Watch replay scrubber: tab strip / slider drag / play cadence / diff-on-scrub | **render pinned by `gui-smoke` (CPE-1135; non-blocking per CPE-1048):** `wdio.conf.ts#seedReplayFixture` seeds a real audit-journal + baseline for a synthetic `gui-smoke-replay` session into the app-data dir the built binary reads; `specs/replay.smoke.ts` drives the real `tauri build` binary to the Replay tab and asserts `.rp-transport` + an *enabled* `.rp-slider` + `.rp-recon-list` containing the seeded filename render non-degenerate. Slider-*drag* / play-*cadence* / diff-on-scrub *feel* still worth an occasional human glance (same framing as CPE-1090/1091/1114) | render **automated** — feel residual | 2026-07-29 |
| CPE-1098 | Agent-Watch cost ledger tab: card layout / theme colours / live token+USD numbers | **render pinned by `gui-smoke` (CPE-1173):** `specs/cost-ledger.smoke.ts` seeds a synthetic session through `window.__CPE_TEST_INGEST_SESSION__` and a per-session usage snapshot through `window.__CPE_TEST_INGEST_COST__` (App.svelte's test-mode-only hooks — the SAME store path `agentCost.ts#ingestCost` a real `ai-console://agent-cost` event takes), then drives the real `tauri build` binary to the Cost tab and asserts `.cl-card`/`.cl-row` render with the watched-session `.cl-chip`/`.cl-current` chip, and `snap()`s it. The row's original "needs a live agent session, not just a static fixture" premise is overturned by the same argument that overturned CPE-1100's: the tab renders purely from the `agentCost` store, so ingesting through the real store seam is equivalent to a real sidecar event. Live token/USD *numbers* off a real paid agent run, and card *feel*, stay a residual (same framing as CPE-1090/1091/1094/1100) | **render automated** — live-numbers/feel residual; pinned by `GUI smoke (ubuntu-latest)` shards + `gui-smoke-linux-verdict`'s ratchet (blocking, CPE-1594/1753); not listed in `known-failing.json` | 2026-07-26 → flipped 2026-08-20 |
| CPE-1100 | Agent-Watch radar tab: overlap rows / actor pills / navigate / live 2-actor race | **render pinned by `gui-smoke` (CPE-1255; non-blocking per CPE-1048):** a read-only spike overturned the earlier "needs a live 2-actor session" assumption — the tab renders purely from `foldOverlaps(entries)` (agentConflicts.ts) over the LIVE `agentTimeline` store, so no real concurrent-actor race is needed. `specs/radar.smoke.ts` seeds two `__CPE_TEST_INGEST_ACTIVITY__` batches (App.svelte, the existing CPE-1135 hook — no new seam) for one path with two distinct synthetic actors 200ms apart (inside the 5s `OVERLAP_WINDOW_MS`), then drives the real `tauri build` binary to the Radar tab and asserts `.rd-list`/`.rd-item` render with exactly two `.rd-pill` actor chips in `.rd-actors`; actor-resolution/navigate-on-click *feel* still worth an occasional human glance (same framing as CPE-1090/1091/1094) | render **automated** — feel residual | 2026-08-02 |
| CPE-1263 | File-content search dialog (`ContentIndexSearchDialog.svelte`, epic CPE-976): needs-build prompt, streamed index-build progress, ranked query→results (name/relative-path/score-bar/snippet), navigate-on-click, debounce + generation-token supersede | **Interaction logic pinned by `ContentIndexSearchDialog.test.ts`** (jsdom, `content_search`/`content_index_build` mocked, 11 cases): needs-build state renders a prompt not a raw error; streamed build progress updates live then unlocks search; build-error surfaced; ranked hits render name+relative-path+score%+snippet; clean no-matches state; navigate-on-click dispatches the file path + closes; Escape closes; debounce waits out the window before searching; a stale search's late result is dropped once superseded; clearing the query cancels the pending search. No `gui-smoke` spec yet. **2026-08-20 correction:** the stated obstacle — "a spec would need to drive the palette rather than a direct key combo" — is obsolete; `near-duplicates.smoke.ts`, `similar-images.smoke.ts` and `declutter.smoke.ts` all drive `Ctrl+Shift+P` → `.cp-input` → Enter today, green on the blocking Linux leg, with the block copy-pasted verbatim in all three. Retiring ticket **CPE-1819** extracts `gui-smoke/lib/palette.ts` and pins this dialog's palette-open + needs-build offer + Escape-closes in both themes. The score-bar-fill *feel* genuinely cannot be pinned headlessly (it needs real hits, which need a live embedding endpoint) and stays the residual | **logic automated (jsdom)** — render/gui-smoke + pixel/feel residual | 2026-08-02 |

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

### Sprint 2026-07-26 (resume) — new manual debt from merged PRs
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

### Sprint 2026-08-05 — GUI batch (CPE-1323/1324 + Metadata Studio 1325-1328 + Declutter 1329)
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

### Sprint 2026-08-05 (cont.) — coverage debt CLOSED (CPE-1331/1332) + backend 3D reader (CPE-1333)
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
  and resolves to the generic hex provider instead — filed as CPE-1359), `fonts/mini.ttf` (font,
  hand-built sfnt), `database/mini.sqlite` (data-grid), `other/tiny.wasm` (info). `documents/doc.pdf`
  replaced with a real, valid 2-page PDF (byte-accurate `xref`), carrying the same full `/Info` metadata
  baseline as every other sample format (`sample_fixtures.rs::pdf_info_baseline` updated to match — an
  intermediate commit had briefly swapped `doc.pdf` for a slimmer Pillow-rendered fixture, both restored
  together). The OLD degenerate bytes are preserved unchanged as `documents/malformed.pdf` — the
  deliberate crash-regression fixture.
- **End-to-end walk:** `gui-smoke/specs/samples.smoke.ts` seeds a copy of the real `samples/` tree into
  the shared tmpDir (`wdio.conf.ts#seedSamplesFixture`, `fs.cpSync` — new samples are picked up
  automatically, no filename list to maintain) and, on a real `tauri build` binary, opens EVERY file:
  navigates to its folder via the address bar (Ctrl+L), selects it, and asserts (a) the app/window is
  still responding (the crash guard) and (b) the preview settled into real content or an explicit
  graceful "can't preview" note/fallback — never a stuck spinner. `documents/malformed.pdf` runs LAST, in
  its own `it()`, as a defense-in-depth regression pin: CPE-1357's validate-before-embed fix
  (`pdf_validity` in media_meta_read.rs, landed on `main` the same day) already lands this file in the
  metadata-pane fallback rather than WebView2's PDF viewer, so this assertion is expected to PASS like
  every other file — it stays last so a FUTURE regression in that validation path can't blind the rest of
  the walk. Non-blocking on both CI legs (`continue-on-error`, CPE-1048), same posture as the rest of
  `gui-smoke`.

**Not yet run live on GitHub Actions** (offsite Actions verification pending, same caveat as CPE-1171's
Linux leg) — locally verified: `npm run check` clean, the full vitest suite (187 files / 2080 tests)
green including the new coverage test, `gui-smoke`'s own `typecheck` + `test:unit` green, and the FULL
`cargo test -p cpe-server` suite green (1606 unit tests + every integration test file, incl.
`sample_fixtures::pdf_info_baseline` and CPE-1357's 6 `pdf_validity` tests) in the merged tree (this
branch merged `origin/main` after CPE-1357's fix landed there first). The `samples.smoke.ts` walk itself
needs a real `tauri build --no-bundle` + `tauri-driver` session (this worktree's sandbox couldn't run
one) — it will execute for the first time in CI.

---
### 2026-08-08 (sprint) — gui-smoke mouse-CDP harness breakage FIXED (partial ratchet)
**Surface:** the whole `gui-smoke` Linux leg (the Visual Critic/UAT automated-GUI substrate) was RED on every
`main` run — `mouse.ts` was CDP-only and threw on WebKitWebDriver, failing all mouse specs → 20-min timeout.
**Fixed (CPE-1479, PR #722 merged aed89022):** W3C-Actions fallback when CDP is absent — 0 CDP errors, mouse
specs execute, 9 specs pass. **NOT yet fully green** → tracked as **CPE-1481** (8 revealed environmental spec
failures on Linux CI + 20-min timeout too short now that specs run). Do NOT mark the gui-smoke GUI-driving row
green until CPE-1481 lands the leg green and names the pinning job. MVD: mouse-driving substrate restored;
full-green pinning still owed.

---
### 2026-08-09 (sprint) — Network sidebar + discovery: attended/hardware debt queued
New manual-verification debt from the network/sidebar batch (all logged so the sprint keeps moving on the
buildable halves; the user clears these on return / when the NAS is set up 2026-08-10):
- **Visual sign-off — permanent Network section (CPE-1516)** + **reorderable sidebar sections (CPE-1520)**:
  code + unit tests land headless, but the *look/feel* (empty Network section reads as a peer of Drives;
  drag-to-reorder drop indicator; nothing feels heavier) needs the user's eyes or a gui-smoke Visual Critic
  screenshot pass. → automate via the gui-smoke Visual Critic once the sidebar settles.
- **Live Windows network discovery (CPE-1519)**: the `WNetEnumResource` walk compiles + the pure mapping is
  unit-tested, but the actual enumeration returns real hosts only against a live LAN — attended verify against
  the QNAP TS-133 (2026-08-10). MVD until a mock-provider or a LAN-in-CI harness exists (hard; likely stays
  attended).
- **Real-NAS E2E for shipped SFTP/WebDAV/FTP (CPE-1518)** + **SMB via Windows-UNC (CPE-1504 leg)**: hardware
  (QNAP) required — inherently attended until a containerized SMB/WebDAV/FTP server is stood up in CI (future
  QA-Architect ticket candidate: a docker-samba + rclone-serve-webdav + vsftpd test rig).

---
### Sprint 2026-08-10 — new manual debt from the batch-1 dispatch wave (PRs #797–#800)
Five new human-eyes-only surfaces, logged the same shift they shipped (charter rule 1). None has a `gui-smoke`
spec, none is reached by any `snap()`, and — critically — **no CI run has produced a screenshot artifact in
weeks** (see the diagnosis below), so the CPE-1148 Visual Critic cannot judge any of them without a Foreman
running a local `tauri build` by hand. Retiring the substrate: **CPE-1594**; the per-surface specs follow it.

| CPE-1586 | **Font preview: specimen rendering fidelity + glyph-grid spacing/contrast, in BOTH light and dark themes** (`FontPreview.svelte`, PR #798). The parse/metadata/copy-action layer is jsdom-covered (`preview/font.test.ts`, `PreviewPane.fontActions.test.ts`), but "does the specimen actually render the face, and is the glyph grid legible" is a pure rendering judgement the UAT tester explicitly refused to claim. **Doubly manual since CPE-1492/1493 shipped a real dark theme: `gui-smoke` has ZERO dark-theme coverage — no spec anywhere flips `data-theme`, so every visual surface in the app is verified light-only.** | ✅ automated — pinned by `gui-smoke` (CPE-1629) | ~~needs `font-preview.smoke.ts` + a `snap()` pair (light+dark) + artifact upload~~ Done | 2026-08-10 |
| CPE-1577 | **User-command Toolbar surface: crowding / overflow when several long-named commands are bound to the Toolbar** (`CommandBar.svelte`, PR #797). Binding logic + surface routing are jsdom-covered (`App.userCommandSurfaces.test.ts`, `CommandBar.test.ts`, `ContextMenu.test.ts`), but layout behaviour under many long labels is a reflow judgement — and per the CLAUDE.md pill/chip rule ("tick-tacks reflow") a wrapping-vs-overflow bug here is exactly the class that only shows up on screen. UAT flagged it human-eyes-only. | ⛰ manual — logic automated, layout-under-load human-only | needs a `gui-smoke` spec that seeds N long-named toolbar-bound commands and `snap()`s the bar at 2 window widths | 2026-08-10 |
| CPE-1570/1576/1578 | **Preview action bars (JSON / image / archive / JWT), incl. the 2 new image-rotate icons** — carried over from the prior session's "Owed to the USER" queue (CHECKPOINT.md), never logged here. Declarative per-provider action wiring is unit-tested; the rendered bar (icon column alignment per the CPE-748 menu-icon rule, button crowding, disabled states) has had no eyes and no screenshot. | ⛰ manual | needs a `preview-actions.smoke.ts` snapping the bar for each provider kind | 2026-08-10 |
| CPE-1573 | **JSON tree viewer render** (expand/collapse chevrons, indent guides, value colouring in both themes) — carried over from the prior session's visual/taste queue. Tree-building logic is unit-tested; the rendered tree is unseen. | ⛰ manual | fold a `snap("json-tree")` into the preview-actions spec above | 2026-08-10 |
| CPE-1560 | **Trash view overlay + sidebar Trash section** (PR #795) — carried over from the prior session's visual/taste queue. Restore/Empty logic + the CPE-1559 bindings are covered; the overlay's look and the sidebar section's weight-as-a-peer-of-Drives are unseen. Known cosmetic defect already noted (folders show a file icon — `TrashEntry` has no `is_dir`) that a screenshot pass would have caught automatically. | ⛰ manual | needs a `trash.smoke.ts` (seed a trashed file, open the view, `snap()`) | 2026-08-10 |

→ ✅ **CPE-1586 RETIRED 2026-08-11 (CPE-1629).** `gui-smoke/specs/preview-pane.smoke.ts` opens `samples/fonts/mini.ttf`
and `snap()`s the glyph-grid specimen at `dark-wide` + `light-narrow` — both the dark-theme gap AND the
narrow-width reflow case this row called out are now covered in the same pass. This also retires the row's
broader claim that "`gui-smoke` has ZERO dark-theme coverage": the new `gui-smoke/lib/theme.ts#setTheme()`
helper flips real `data-theme`, and every surface in `preview-pane.smoke.ts` is captured in both themes —
any future spec can reuse it. Confirmed on a real `tauri build --no-bundle` run (`cd gui-smoke && npm test`
against `--spec ./specs/preview-pane.smoke.ts`): 6/6 passing, screenshots opened and visually verified
(narrow width visibly reflows the tab strip + wraps the Overview `dl`, dark theme renders correctly, the
font specimen shows real glyphs, not a blank pane). Pinned by `GUI smoke (ubuntu-latest)` + its ratchet
step (blocking, CPE-1594) going forward, same as every other `preview-pane.smoke.ts` surface — see
`gui-smoke/README.md`'s "Preview-pane provider screenshots" section for the full write-up. CPE-1577/1570/
1576/1578/1573/1560 above are UNCHANGED — different providers/surfaces, not touched by this ticket.

**Not new debt:** PR #800 (`binary_info`/`binary_disasm` dispatchers) is backend-only — no UI, covered by
`cargo test` + the bindings drift guard. PR #799 (3 new docs pages) is pinned by the CPE-1571 doc-coverage
guard + `sectionDocs.test.ts`; docs prose needs review, not *human eyes on a rendering*.

---
### 2026-08-10 — DIAGNOSIS: `gui-smoke` is not flaky, it is producing NO signal at all (→ CPE-1594)
The crew's standing instruction is "GUI-smoke is flaky, ignore it". The evidence says something worse. Measured
this shift against the GitHub Actions API (`repos/:owner/:repo/actions/workflows/gui-smoke.yml/runs`, 800 runs,
2026-08-03 → 2026-08-10):

- **796 `cancelled`, 4 `failure`, 0 `success`, 5 in-flight.** In the most recent 300 runs (2.5 days) there is
  **not one single terminal verdict** — every run is `cancelled`. A job that never concludes cannot retire one
  minute of manual testing, and cannot fail a regression either.
- **Windows leg — 0 of 40 specs have ever executed an assertion in CI.** Raw job log (run 31409461248, job
  93523746819): every WebDriver session dies with `session not created: DevToolsActivePort file doesn't exist`
  — the CPE-1048 WebView2 startup crash, *unfixed*, despite the `--disable-gpu --no-sandbox
  --disable-dev-shm-usage` mitigation in the workflow env. Each spec burns ~3 min on 1 attempt + 3×60s retries;
  the job is killed by `timeout-minutes: 45` after roughly 7 specs (39 `DevToolsActivePort` errors logged). The
  timeout-kill is what stamps the whole RUN `cancelled` — i.e. **the dead Windows leg is what makes the working
  Linux leg look like a flake.** Cost: a full 45-min `windows-latest` runner (incl. a release `tauri build` +
  two `cargo install`s) burned on every push AND every PR, for zero information — a plausible contributor to the
  "Actions runner backlog, jobs queued 30+ min" the Foreman logged this same session.
- **Linux leg — actually works, and is being thrown away.** Same run, job 93523746768:
  `Spec Files: 33 passed, 7 failed, 40 total (100% completed) in 00:27:48`. It completes in ~39 min wall-clock
  and is **82.5% green**. Its 7 failures are real, readable signal that nobody reads: `archive-browse`,
  `archive-password`, `network`, `samples`, `saved-search`, `shred-dialog`, `transfer-panel`. Note `network`
  fails on *"expected the permanent Network section header to render"* — that is CPE-1516's shipped surface,
  and it may be a genuine regression sitting unnoticed on `main`. Note also the tail has **grown 3 → 7** since
  CPE-1507 catalogued it, which is what happens when nobody is allowed to look.
- **No screenshots ever leave CI.** `gui-smoke.yml` has **no `actions/upload-artifact` step at all** (the only
  workflow in the repo that uploads artifacts is `model-snapshot.yml`). The 75 `snap()` calls across the specs
  write into `gui-smoke/.screenshots/` — a gitignored directory on an ephemeral runner that is then discarded.
  **The entire CPE-1148 "Visual Critic judges screenshots" story therefore has no CI substrate whatsoever**; it
  only ever worked when a Foreman ran a local `tauri build` + local suite by hand. `gui-smoke/baselines/`
  contains exactly two synthetic demo PNGs — no real app surface has ever been blessed (burndown row #3 has
  said so since 2026-07-25 and it is still true).

**Verdict:** rows #1, #2, #3 and #4 have been 🔧 "in progress" for weeks against a substrate that emits nothing.
The fix is **not structural** — the Linux leg already runs the whole suite to completion. It is three small,
independent workflow/harness changes (export the screenshots, ratchet the Linux leg so it can go blocking at
33/40, stop the Windows leg poisoning the run conclusion). Filed as **CPE-1594**.

**RESOLVED — CPE-1594, same shift.** All three changes landed: `actions/upload-artifact` on both legs
(`if: always()`); `gui-smoke/known-failing.json` + `gui-smoke/lib/ratchet.ts` (+`ratchet.test.ts`) turn the
Linux leg's "33 passed, 7 failed" into a real blocking verdict instead of a swallowed `continue-on-error`; the
Windows leg moved off `push`/`pull_request` onto `workflow_dispatch` + a nightly `schedule:`, with its
`timeout-minutes` cut 45→15. **Mid-review triage:** `network.smoke.ts`'s `$("=Network")` selector was fixed
(the old link-text locator could never match a `<span>`), but a live PR #801 CI run proved it STILL fails on
the real WebKitGTK/Xvfb stack — the same class of `.fav-title getText()` issue `saved-search.smoke.ts` is
already listed for — so `known-failing.json` stays at **7** entries; the fix is real but the spec stays listed
until a run shows it green. **Same live run also caught two workflow bugs, fixed same-PR:** the screenshot
upload matched zero files (`actions/upload-artifact@v4` excludes dot-prefixed folders like `.screenshots/` by
default — needs `include-hidden-files: true`), and its `if-no-files-found: error` had aborted the job BEFORE
the ratchet step ever ran, silently skipping the gate entirely. Fixed: `include-hidden-files: true` added,
upload downgraded to `warn` and reordered to run AFTER the ratchet step. Rows #1/#2/#4 flipped ✅ above (macOS
+ the Windows canary noted as residuals); row #3 stays 🔧 (screenshots now reach CI, real baselines still
owed). Follow-up filed: **CPE-1595** (triage the remaining 4 unrelated known-failing specs; `network.smoke.ts`
is tracked there too, pending a live green run).

## Critic technique note (2026-08-11) — headless Chrome viewport trap

When a Visual Critic renders a component to check it, **`--window-size` does not reliably set the CSS
viewport under `chrome.exe --headless=new`.** It clamps to an internal ~500px width regardless of the
requested value, then *rescales the screenshot* to the requested output pixel size. A critic reviewing
CPE-1618 nearly filed a false "filter chips overflow at 260px" defect because of this.

The reliable technique, which that critic worked out and verified:
- Give the harness a wrapper element with an explicit CSS `width`, driven by a query param.
- Always launch Chrome with a **large** window (e.g. 1200×900) so the internal clamp never engages.
- **Confirm the achieved width via `getBoundingClientRect` / `clientWidth` before trusting any screenshot.**

Also: drive Chrome directly from Bash, not PowerShell `Start-Process`, which mangles `&` in URLs and yields
misleading error-page screenshots.

Worth folding into `gui-smoke`'s README when CPE-1629 lands, so this is discoverable rather than rediscovered.

## Critic hazard (2026-08-11) — NEVER kill Chrome by image name

A Visual Critic cleaning up its headless Chrome instance ran `taskkill /IM chrome.exe /T`, which killed
**every** Chrome process on the machine — including any browser window the user had open. The user is often
away during a sprint, so this is exactly the "automation must not hijack the screen" line we don't cross.

**Rule for critics and any agent launching a browser:**
- Capture the PID of the process you launch and kill **only that PID** (`taskkill /PID <pid> /T`), or use a
  dedicated `--user-data-dir` and a distinctive `--remote-debugging-port` so your instance is identifiable.
- **Never** `taskkill /IM chrome.exe`, `/IM msedge.exe`, or any image-name kill of a user-facing application.
- The same applies to `node`, `cargo`, and anything else the user might be running themselves.

If a stray headless instance can't be identified, leave it — an orphaned background process is a far smaller
cost than closing the user's browser session.

## Critic technique, refined (2026-08-11) — use an IFRAME, not a sized wrapper div

The earlier note (wrapper div with an explicit CSS width) is **not sufficient** for anything whose CSS depends
on the *viewport*: `vw` units and `position: fixed` resolve against the true top-level viewport, not a sized
`<div>`. So a dialog using `max-width: 95vw` and a `position: fixed` backdrop cannot be honestly narrow-tested
in a wrapper.

**Correct technique:** mount the component inside an `<iframe>` sized to the width under test, inside a large
(e.g. 1200×900) non-clamped Chrome window. The iframe gets a genuinely separate, correctly-sized CSS viewport.
Confirm it via `innerWidth` / `getBoundingClientRect` from inside the frame before trusting a screenshot.

**This matters because it has already produced a false defect report.** CPE-1635 was filed off a naive
`--headless=new --window-size=420,900` screenshot showing the Checkpoints dialog's buttons cut off. A later
worker could not reproduce it with the iframe method at 420px down to 300px in either theme — and then
reproduced the *identical-looking* cut-off against unmodified code using the naive method, confirming the
original finding was the clamping artifact rather than a CSS bug.

Rule: **before filing a layout defect found in headless Chrome, reproduce it with a verified viewport.**
A false defect costs a worker a full investigation and erodes trust in the visual leg.

---
### Sprint 2026-08-17→20 (batched run) — new manual debt from the trash/listing wave

Two new human-eyes-only surfaces, logged the shift after they shipped (charter rule 1). Both are **new
visible states on components that have NO browser-level coverage of any kind** — neither `TrashView.svelte`
nor `StatusBar.svelte` is referenced by a single `gui-smoke` spec, so nothing has ever seen either render on
a real build. This run was overwhelmingly backend hardening (S3 keys, archive extraction refusals, trash
parsing, temp-dir leaks, the ffmpeg pin), all of it pinned by `cargo test` on the 3-OS matrix; these two are
the whole of its front-end debt.

| Ticket(s) | Surface | Automated coverage today | Status | Automation to build | Logged |
|-----------|---------|--------------------------|--------|---------------------|--------|
| CPE-1803 / CPE-1804 / CPE-1805 | **Trash view's three new degraded-listing states**: degraded-with-no-entries (the "couldn't be read" panel that replaced a false `trash.empty`), degraded-with-entries (a partial list plus an in-place notice), and the skipped-count wording (`trash.skippedOne`/`trash.skippedMany`). The count chip is also deliberately SUPPRESSED on a degraded pass, which is itself a visible layout change | logic pinned by `cargo test` (the walker/command seam) and jsdom component tests; **zero browser-level coverage** — there is still no `trash.smoke.ts`, so no `snap()` has ever captured the Trash overlay at all | ⛰ manual | `trash.smoke.ts`: seed the XDG trash (`~/.local/share/Trash/{files,info}`) on the Linux leg, open the Trash section, and `snap()` all four states — healthy, degraded-empty, degraded-with-entries, skipped-count — in both themes. Compounds the older **CPE-1560** row (the overlay's look + the sidebar section's weight), which asked for the same spec; build one spec covering both. Watch for the `.fav-title` `getText()` WebKitGTK quirk that has `network`/`saved-search` in `known-failing.json` — the Trash sidebar header is the same `<span class="label fav-title">` shape, so open the view by a route that does not read that header's text | 2026-08-20 |
| CPE-1708 / CPE-1775 (+ CPE-1660, CPE-1798) | **StatusBar's advisory lines**: CPE-1708's `.filtered-hidden` ("N entries were hidden because their names could not be shown safely"), and CPE-1775's `notice.archiveSkippedOne`/`archiveSkippedMany` ("N entries were skipped — they couldn't be written safely"). Both are ellipsis-truncated with the full sentence kept only in a `title` tooltip, so **how they behave at a narrow window is a reflow judgement**, exactly the class the CLAUDE.md tick-tacks rule exists for | jsdom component tests on `StatusBar.svelte`; **zero browser-level coverage** — `grep -rl 'status-bar\|\.sb-' gui-smoke/specs/` returns nothing, so no spec asserts or snaps the status bar in any state | ⛰ manual | a `status-bar.smoke.ts` that drives the bar into each advisory state and `snap()`s it at two window widths in both themes. `filteredHidden` and the archive skip notice are both prop/notice-driven, so a test-mode ingest hook in the `__CPE_TEST_INGEST_*` family (App.svelte, CPE-1130/1173) is the cheap seam — no remote S3 listing or real refused-entry archive needed. Note CPE-1780 (Backlog) already records three further status-bar/listing gaps found while building CPE-1708, so this spec has an existing bug list to red-proof against | 2026-08-20 |

