# gui-smoke — headless GUI smoke test (CPE-1045)

Drives the **real built** Cross-Platform Explorer binary through
[`tauri-driver`](https://github.com/tauri-apps/tauri/tree/dev/crates/tauri-driver) +
[WebdriverIO](https://webdriver.io/) and asserts that `--open <dir>` (CPE-1043, fixed by CPE-1044)
actually navigates the explorer into that folder — delivered to the launched process as a single
`--open=<dir>` token (see "Running this locally" below for why). This retires the manual
launch-the-app-and-eyeball-a-screenshot verification for that flow.

Self-contained: its own `package.json`/lockfile/`tsconfig.json`. Nothing here touches the app's
`package.json`, `src-tauri/`, or the root `vitest` suite — it is a separate Node project.

## What it asserts

1. **Health check** — the window launched and `<body>` rendered non-empty content (covers
   burndown #2, build → deploy → run smoke).
2. **Navigation** — the current-folder breadcrumb (`[aria-current="page"]`, the same selector
   `src/App.features.test.ts` uses) shows the temp folder's basename.
3. **Contents rendered** — the seeded `CPE-1045-marker.txt` entry is visible in the listing.

## Prerequisites

1. **`tauri-driver`** — the official WebDriver proxy (version is independent of the Tauri crate
   version):
   ```
   cargo install tauri-driver --locked
   ```
2. **Windows: a matching Microsoft Edge Driver** on `PATH` (the runner's Edge/WebView2 version
   must match `msedgedriver.exe`):
   ```powershell
   cargo install --git https://github.com/chippers/msedgedriver-tool
   & "$HOME/.cargo/bin/msedgedriver-tool.exe"
   $env:PATH = "$PWD;$env:PATH"
   ```
   (Linux would instead need `webkit2gtk-driver` + `xvfb-run` — see Follow-ups below; **macOS is
   not supported by `tauri-driver`** at all, no WKWebView WebDriver exists.)
3. **A real Tauri CLI release build** — not a debug build, not a bare `cargo build`:
   ```
   npm run build
   npm run tauri build -- --no-bundle
   ```
   `--no-bundle` only skips installer packaging + updater-artifact signing (which needs secrets
   this test doesn't carry) — it does **not** change whether the frontend gets embedded. What
   *does* matter, per the CPE-1044 root cause: Tauri decides "embed `frontendDist`" vs. "load
   `devUrl` (`localhost:1420`)" based on going through the Tauri **CLI's `build` subcommand** —
   never on cargo profile, never on `--no-bundle`. A bare `cargo build` (bypassing the CLI
   entirely) falls back to the dev server and `--open` silently no-ops (that's the exact bug
   CPE-1044 fixed). `wdio.conf.ts` fails fast with a clear message if the expected binary is
   missing at `src-tauri/target/release/`.

## Running this locally — read before you `npm test`

**This launches a real, visible, focus-stealing window.** There is currently no way to run it
headless-and-invisible on a real desktop session: an off-screen `--x`/`--y` launch position was
tried and doesn't work — this app's own CPE-600 window-geometry code deliberately clamps the window
fully onto the monitor ("off-screen protection... never ungrabbable", see
`crates/server/src/geometry.rs::resolve()`), so an off-screen position is silently pulled back
on-screen. A real fix (a non-activating / off-screen / `--test-mode` launch) is tracked as a
follow-up on the app side (CPE-1046). **Until that lands, only run this suite on a machine nobody
is actively using** — CI (`windows-latest`, no interactive user) is the intended place, not your
own foreground desktop.

```
cd gui-smoke
npm install
npm test
```

`npm test` runs `wdio run ./wdio.conf.ts`, which:
- verifies the release binary exists (see above) and errors with setup instructions if not;
- creates a temp dir seeded with `CPE-1045-marker.txt`;
- spawns `tauri-driver`, launches the app with `--open=<tmpdir>` via the
  `'tauri:options': { application, args }` capability (note: **one** `--open=<dir>` token, not the
  two-token `--open <dir>` a human would type at a shell — see the comment in `wdio.conf.ts` for why:
  msedgedriver's own arg handling silently drops a bare positional token that isn't shaped like a
  `--switch`);
- forces classic WebDriver (`wdio:enforceWebDriverClassic`) rather than the BiDi protocol
  WebdriverIO v9 auto-negotiates — BiDi's `browsingContext` model didn't reliably attach to wry's
  embedded WebView2 control in testing (queries kept returning the driver's own empty
  `about:blank` placeholder context forever, even though the app itself was running and had
  navigated correctly);
- runs `specs/open-dir.smoke.ts`;
- tears down the session, kills `tauri-driver`, and removes the temp dir.

## CI

`.github/workflows/gui-smoke.yml` runs this on `windows-latest` for push + PR to `main`: builds
the frontend, does a `tauri build -- --no-bundle`, installs `msedgedriver` + `tauri-driver`, then
runs this suite — so a regression in launch-or-navigate reds the pipeline instead of needing a
human to notice.

## Follow-ups (not this ticket — see CPE-1045's "Follow-ups" section)

- **CPE-1046 — non-disruptive local launch**: a `--test-mode` (or similar) app flag that launches
  the main window non-activating/off-screen/invisible, so this suite can run on someone's
  interactive desktop without stealing focus. Until it ships, this harness should only be run on a
  machine nobody is actively using (see "Running this locally" above); once it lands, add
  `--test-mode` to this harness's `tauri:options.args` alongside `--open=<dir>`.
- **Linux CI leg**: add an `ubuntu-latest` matrix arm using `webkit2gtk-driver` + `xvfb-run` (no
  app code change needed).
- **macOS**: stays attended — `tauri-driver` has no WKWebView WebDriver support.
- **More flows**: Back/Up navigation, dialogs, context menus, tab switching.
- **Visual regression**: reuse WDIO's screenshot capture for light/dark pixel-diffs.
