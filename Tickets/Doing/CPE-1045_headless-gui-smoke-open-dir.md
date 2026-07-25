---
id: CPE-1045
title: "Headless GUI smoke test — drive the built app in CI and assert --open <dir> navigated"
type: feature
component: Multiple
priority: high
status: Backlog
tags: ready
created: 2026-07-25
estimate: 6h
burndown: "MANUAL-TEST-BURNDOWN rows #1 (GUI end-to-end) + #2 (build→deploy→run smoke)"
---

## Summary
Stand up the **first slice of headless GUI end-to-end automation**: a CI-capable smoke test that builds
the real Tauri app, launches it **with no human**, and asserts a **user-visible outcome** — the exact
check we just did by hand for `--open <dir>`. This retires the highest-leverage manual-verification
surface (burndown **#1 GUI end-to-end** and **#2 build→deploy→run smoke**).

**Why now.** The team spent ~an hour hand-verifying `--open <dir>` (CPE-1043) by launching the built
app and eyeballing screenshots, and a real bug (CPE-1044 — it landed on Home instead of navigating)
slipped past the first attempts **because nothing drives the real WebView2 GUI in CI**. Every "have the
user look at the GUI" escalation flows from this gap. Automate the smallest thing that genuinely drives
the real built app and asserts the folder actually opened.

Keep scope **tight**: ONE build, ONE launch, ONE core assertion (+ a health check). This is a first
slice, not the whole E2E suite — see Follow-ups.

## Design (buildable)

**Tooling** (Tauri v2 official WebDriver path — see the Library entry
`headless-gui-smoke-test-tauri-driver`):
- `tauri-driver` — `cargo install tauri-driver --locked` (the official WebDriver proxy; version is
  independent of the Tauri crate version).
- **WebdriverIO** (Node, mocha) as the client — copy the official `wdio.conf.js` shape. Assertions are
  one-liners against the DOM.
- **Windows driver**: Microsoft Edge Driver matching the runner's Edge/WebView2, installed via
  `msedgedriver-tool` (`cargo install --git https://github.com/chippers/msedgedriver-tool`, run it,
  prepend its dir to `PATH`).

**Where files live** — a self-contained `gui-smoke/` directory at the repo root, with its **own**
`package.json` (WDIO + mocha + chai) so it never perturbs the app build or the vitest suite:
```
gui-smoke/
  package.json          # webdriverio, @wdio/*, mocha, chai — isolated from the app
  wdio.conf.ts          # spawns/kills tauri-driver; capability tauri:options.{application,args}
  specs/open-dir.smoke.ts
  README.md             # how to run locally (prereqs: tauri build + msedgedriver on PATH)
```

**How the app is launched headlessly**
1. Build the **real bundle**: `npm run build` then `npm run tauri build` → the release binary at
   `src-tauri/target/release/cross-platform-explorer.exe`.
   **Critical (CPE-1044):** a plain `cargo build`/`--debug` on this repo loads the **dev server**
   (`localhost:1420`), NOT embedded assets, so `--open` only navigates in a real `tauri build` bundle.
   The harness MUST point `tauri:options.application` at the `tauri build` release binary — not a
   debug/`--no-bundle` build — or the very bug we're guarding against stays invisible.
2. `wdio.conf` spawns `tauri-driver` (from `~/.cargo/bin`) before the session and kills it after;
   client connects to `http://127.0.0.1:4444`, browserName `wry`.
3. Pass the launch flag via the capability:
   `'tauri:options': { application, args: ['--open', <tmpdir>] }`. The Worker must **confirm
   `tauri-driver` forwards `args`** to the app; if not, fall back to setting the
   `window.__CPE_OPEN_DIR__` delivery another way (e.g. an env var the app reads in setup) — but
   `args` is the intended, clean path.

**The first assertion** (the manual check, automated). The spec:
1. Creates a temp dir (mocha `before`) and seeds it with a known entry, e.g. `CPE-1045-marker.txt`.
2. Launches the app with `--open <tmpdir>`.
3. Asserts **navigation happened** — the current-folder breadcrumb, selector `[aria-current="page"]`
   (the same one `src/App.features.test.ts:254` uses), has `textContent` === the tmpdir's basename.
4. Asserts the **folder contents rendered** — `CPE-1045-marker.txt` is visible in the listing.
5. **Health check**: the window exists and `<body>` rendered non-empty (app launched and is
   responding) — this alone also covers burndown #2 (build→deploy→run smoke).
6. Teardown: quit the session, kill `tauri-driver`, remove the temp dir.

**CI job that pins it** (so it never regresses). A new job in `.github/workflows/ci.yml` (or a sibling
`gui-smoke.yml`), on **`windows-latest`** for this slice:
- install Rust; `npm ci` + `npm run build`; `npm run tauri build`;
- `cargo install --git https://github.com/chippers/msedgedriver-tool` → run it → prepend to `PATH`;
- `cargo install tauri-driver --locked`;
- in `gui-smoke/`: `npm ci` then `npm test`.
Runs on push + PR to `main` like the rest of CI, so a regression in launch-or-navigate reds the
pipeline. (Windows runners already have Edge + the WebView2 runtime.)

## Acceptance Criteria
- [ ] `gui-smoke/` exists, self-contained (own `package.json`); running its suite locally against a
      `tauri build` release binary is documented in `gui-smoke/README.md`.
- [ ] The smoke spec launches the **built bundle** headlessly via `tauri-driver` + WebdriverIO with
      `--open <tmpdir>` and **passes** these automatable assertions:
      (a) `[aria-current="page"]` breadcrumb text === the temp dir's basename (navigation happened);
      (b) the seeded marker entry is visible in the listing;
      (c) health check — window present and `<body>` rendered non-empty.
- [ ] The harness points at the **`tauri build` release binary** (not debug/`--no-bundle`), per the
      CPE-1044 caveat; this is asserted/commented in `wdio.conf`.
- [ ] A CI job on `windows-latest` builds the app, installs `msedgedriver` (matching Edge) +
      `tauri-driver`, and runs the smoke suite on push + PR to `main`. The job is **green** and would
      go **red** if `--open` stopped navigating (verify by a local negative check, e.g. temporarily
      break navigation and confirm the assertion fails).
- [ ] No impact on existing gates: `npm run check`, `npm test` (vitest), and `cargo test` are
      unaffected (the `gui-smoke/` toolchain is isolated).
- [ ] Burndown rows #1 and #2 reference CPE-1045; on merge, flip them per the burndown rules (this
      ticket lands the pinning job, so they become ✅ once green — decrement MVD accordingly).

## Follow-ups (noted, NOT this ticket — keeps this a first slice)
- **Linux CI leg** (burndown #4, Linux only): add an `ubuntu-latest` matrix arm using
  `webkit2gtk-driver` + `xvfb-run` (the app runs headless with no code change). **macOS is
  unsupported by `tauri-driver`** (no WKWebView WebDriver) and stays attended — call this out so no
  one burns time trying.
- **More flows** (burndown #1 breadth): navigate/Back, open a dialog, a context menu, tab switch.
- **Visual / theme regression** (burndown #3): reuse WDIO's screenshot capture for light+dark
  pixel-diff over key screens.
- Consider promoting shared launch helpers if a second GUI spec appears.

## Work Log
2026-07-25 — Filed by the QA Architect (workshift). Retires burndown #1/#2; directly automates the
`--open <dir>` check hand-done for CPE-1043/1044. Approach + tradeoffs (WebdriverIO vs. fantoccini vs.
a no-WebDriver health-ping) captured in Library entry `headless-gui-smoke-test-tauri-driver`.
Recommended: `tauri-driver` + WebdriverIO driving the `tauri build` release binary on Windows, asserting
the `[aria-current="page"]` breadcrumb navigated into the temp dir.
