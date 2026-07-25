---
topic:       headless-gui-smoke-test-tauri-driver
title:       How do we run a headless GUI smoke test that drives the real built Tauri app in CI?
date:        2026-07-25
researcher:  opus
relates:     [CPE-1045, CPE-1043, CPE-1044]
tags:        [gui-testing, e2e, tauri-driver, webdriver, webdriverio, fantoccini, msedgedriver, webview2, ci, headless, smoke-test]
status:      current
sources:     [in-repo, context7, web]
---

## Question
The team just spent ~an hour hand-verifying `--open <dir>` (launch app → eyeball a screenshot →
folder opened?) and a real bug (CPE-1044) slipped past the first attempts because nothing drives the
real WebView2 GUI in CI. Burndown #1 (GUI end-to-end) and #2 (build→deploy→run smoke) are the top MVD
rows. **How, concretely, do we drive the *real built* Tauri app headlessly in CI and assert a
user-visible outcome — specifically that `--open <tmpdir>` navigated into that folder?**

## Findings / Options

**Baseline facts (Tauri v2 official WebDriver docs, verified via context7 2026-07-25):**
- `tauri-driver` is the official WebDriver proxy: `cargo install tauri-driver --locked`. It spawns
  and proxies to the platform's native WebDriver. It is **cross-platform for Windows + Linux only —
  macOS is NOT supported** (no WKWebView WebDriver). So this harness covers Win + Linux; macOS GUI
  stays attended.
- **Windows**: needs Microsoft Edge Driver (`msedgedriver.exe`) whose version matches the installed
  Edge/WebView2. Install with `msedgedriver-tool`
  (`cargo install --git https://github.com/chippers/msedgedriver-tool`, run it, prepend `$PWD` to
  `$GITHUB_PATH`). `windows-latest` runners ship Edge + WebView2 runtime already.
- **Linux**: needs `webkit2gtk-driver` + `xvfb`; the app runs headless under `xvfb-run` (no code
  change). This is how the same harness later extends to the Linux CI leg (burndown #4).
- The client sets capability `tauri:options.application` = path to the built app binary (and supports
  an `args` array to pass launch flags such as `--open`). Browser name is `wry`.
- **Repo-specific caveat (from CPE-1044):** a plain `cargo build`/`--debug` on THIS repo loads the
  **dev server** (`localhost:1420`), not embedded assets, so `--open` navigation only manifests in a
  real `tauri build` (release, embedded `frontendDist`) binary. The harness MUST point at the
  `tauri build` output binary, not a debug/no-bundle build — otherwise the exact bug we're guarding
  against is invisible.
- **Assertion target already exists:** the current folder is the breadcrumb with
  `aria-current="page"` (`document.querySelector('[aria-current="page"]').textContent`), the same
  selector `App.features.test.ts` uses. So "did `--open <tmpdir>` navigate?" is a one-line DOM query
  for the tmpdir's basename.

**Option A — `tauri-driver` + WebdriverIO (Node), in a small `gui-smoke/` dir.** *(recommended)*
- + The **officially documented** path (copy-pasteable `wdio.conf.js` + CI workflow from Tauri docs);
  lowest research risk for a one-pass Worker build. DOM assertions are one-liners
  (`await $('[aria-current="page"]').getText()`). Screenshot capture is built in → seeds
  burndown #3 (visual regression) later.
- − Adds a small Node/WDIO/mocha toolchain separate from the existing vitest (kept isolated in
  `gui-smoke/` with its own `package.json`, so it never touches the app build).

**Option B — `tauri-driver` + `fantoccini` (Rust WebDriver client), in a `gui-smoke` crate.**
- + Single-language with the backend; runs as `cargo test`; no new Node stack; fits the Rust-heavy
  repo. Could live under `crates/` and reuse the existing rust-cache CI plumbing.
- − Not in the official Tauri docs (more DIY: manually spawn `tauri-driver`, build the fantoccini
  `Client` against `http://127.0.0.1:4444`, tear down). Slightly more harness code; higher chance a
  one-pass Worker hits an undocumented rough edge.

**Option C — launch + health-ping smoke, NO WebDriver.** *(fallback / complement, retires #2 only)*
- Spawn the built exe, assert the process stays alive N seconds / answers a health signal, kill it.
- + Trivially portable (all 3 OSes incl. macOS), no driver install, tiny.
- − **Cannot assert the `--open` navigation** (no DOM access), so it does NOT retire burndown #1 —
  only #2. Good as a cheap complementary job, not a substitute.

## Recommendation
**Option A (tauri-driver + WebdriverIO) as the first slice**, driving the **`tauri build` release
binary** on `windows-latest`, with the single assertion: launch with `--open <tmpdir>` (a temp dir
seeded with a known entry) → `[aria-current="page"]` breadcrumb text equals the tmpdir basename **and**
the seeded entry is visible; plus a health check that the window/body rendered. Chosen over B for
lowest build risk (official docs) and because it seeds visual-regression later; over C because only a
DOM-driving test retires burndown #1 (the exact manual check just done). Extends to the Linux leg via
`webkit2gtk-driver` + `xvfb-run` (burndown #4, Linux only — macOS is unsupported by tauri-driver and
stays attended). Pin it as a dedicated CI job so it never regresses.

## Sources
- context7 `/tauri-apps/tauri-docs` (v2): WebDriver — Selenium example, WebdriverIO `wdio.conf.js`,
  CI `webdriver.yml`, manual-setup (Windows msedgedriver / `msedgedriver-tool`).
- Repo: `src/App.svelte` (`window.__CPE_OPEN_DIR__` init-script global; breadcrumb),
  `src/App.features.test.ts:254` (`[aria-current="page"]` selector), `src-tauri/tauri.conf.json`
  (`plugins.cli.args.open`, `frontendDist: ../dist`), `src-tauri/src/lib.rs`
  (`WebviewWindowBuilder` "main" + `initialization_script`), `Tickets/Done/2026/CPE-1044` (debug
  loads dev server → must use `tauri build` bundle), `.github/workflows/ci.yml`.
