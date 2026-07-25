# Manual Test Burndown — the MVD ledger

The authoritative list of every app aspect that still needs a **human** to verify it, and the automation
that will retire each one. The QA Architect drives the **still-manual count (MVD) toward zero** and never
lets an automated row silently regress. Charter + rules: [README.md](README.md).

**MVD (still-manual surfaces): 8** · _baseline seeded 2026-07-25; update the count on every flip._

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
| 6 | **Auto-update flow** (updater downloads, verifies signature, swaps in place) | none end-to-end | ⛰ manual | Staged updater E2E against a test endpoint + signed test artifact in CI | _unfiled_ |
| 7 | **Real remote network run** (non-loopback client↔server over the wire) | loopback via `cpe-net` example + unit tests | ⛰ manual | Containerised two-host network E2E in CI (server container + client), asserting listing over a real socket | CPE-819/820 |
| 8 | **Native OS metadata interop** (ADS on Win, xattr on Linux) verified with OS tools | `native_tags_demo` prints values; human runs `Get-Item -Stream` / `getfattr` to confirm | 🔧 in progress | Make the example **self-assert** by re-reading via the OS tool and comparing, then run it in the 3-OS matrix | CPE-1049 |

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
