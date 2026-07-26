# Manual Test Burndown — the MVD ledger

The authoritative list of every app aspect that still needs a **human** to verify it, and the automation
that will retire each one. The QA Architect drives the **still-manual count (MVD) toward zero** and never
lets an automated row silently regress. Charter + rules: [README.md](README.md).

**MVD (still-manual surfaces): 7** · _baseline seeded 2026-07-25; row #8 flipped ✅ (CPE-1049); row #6's download/verify sub-surface automated (CPE-1058) — row stays in MVD for the still-attended in-place binary swap._

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

| CPE-1090 | Code-preview outline strip / pills reflow / breadcrumb-on-scroll / jump feel | headless UAT only — interactive visual render on installed build still needs human eyes | open | 2026-07-26 |
| CPE-1091 | Per-line gutter/fold/minimap/indent-guides visual render | headless UAT+review only — fold animation, minimap drag, gutter alignment on WebView2 need human eyes | open | 2026-07-26 |
| CPE-1093 | Batch-media dialog: layout/pills/entry | human-verified on installed 0.57.35 build (2026-07-26, user: "looks good"); real image-transform output still worth a spot-check | partially-closed | 2026-07-26 |
| CPE-1094 | Agent-Watch replay scrubber: tab strip / slider drag / play cadence / diff-on-scrub | headless UAT+review only — needs eyes on a real watched session in the installed build | open | 2026-07-26 |
| CPE-1098 | Agent-Watch cost ledger tab: card layout / theme colours / live token+USD numbers | headless UAT+review only — needs eyes + a real agent session printing usage on the installed build | open | 2026-07-26 |
