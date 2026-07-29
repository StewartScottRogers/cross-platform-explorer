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
4. **Code preview renders (CPE-1096)** — a second fixture, `CPE-1096-fixture.rs` (a struct + two
   functions), is seeded into the same temp dir. The suite clicks its row (single-selects it, which
   feeds `PreviewPane`'s `entry` prop) and asserts the code-intelligence UI actually renders:
   `.cl-row[data-line]` per-line rows, the `.outline-bar` with `.outline-pill`s (outline strip,
   CPE-1090), the `.minimap`, and the highlighted `pre.preview-text.code-rows > code.cl-code` (no
   regression on the plain highlighted output). This pins the CPE-1090/1091 render as CI coverage,
   closing their `MANUAL-TEST-BURNDOWN.md` rows (still non-blocking, like the rest of this job — see
   CPE-1048 below).
5. **Cost-History rollup renders (CPE-1130)** — `wdio.conf.ts#seedHistoryFixture` writes a 3-row
   synthetic `SessionMetricsRecord[]` journal (`history.jsonl`, CPE-1113's on-disk schema) straight
   into the REAL app-data directory this build reads from (`<OS app-data root>/<bundle
   identifier>/agent-metrics/history.jsonl`, mirroring `crates/server/src/metrics_journal.rs` +
   `server_ctx.rs`'s `app_data_dir()`), before the app process starts. `specs/cost-history.smoke.ts`
   then: seeds a SYNTHETIC "started" Agent Watch session via a test-mode-only hook
   (`window.__CPE_TEST_INGEST_SESSION__`, App.svelte) so the drawer's `.agent-log-btn` becomes
   reachable without a real running agent; opens the drawer; switches to the History tab; and asserts
   `.hd-bar` (the over-time chart), `.hd-totals`/`.hd-stat` (the totals strip), and a `.hd-table`
   row (by-model/by-agent) all render non-empty. Restores whatever was at the fixture path
   beforehand in `onComplete` (a no-op on CI's ephemeral runner; on a local run it means this suite
   never permanently clobbers a real developer's own Agent Watch history). Closes the
   `MANUAL-TEST-BURNDOWN.md` row for CPE-1114's cost-History visual residual.

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

**This launches a real window**, though as of CPE-1046/1047 the harness now launches it with
`--test-mode --x=-4000` (alongside `--open=<dir>`) — off-screen and non-focused, so it can't grab
your display or steal focus. It is still **shown** (not `visible(false)`), so WebView2 initializes
fully; it just shouldn't disrupt an interactive desktop the way an on-screen focus-stealing window
would. CI (`windows-latest`, no interactive user) remains the intended place to run this.

```
cd gui-smoke
npm install
npm test
```

`npm test` runs `wdio run ./wdio.conf.ts`, which:
- verifies the release binary exists (see above) and errors with setup instructions if not;
- creates a temp dir seeded with `CPE-1045-marker.txt`;
- spawns `tauri-driver`, launches the app with `--test-mode --x=-4000 --open=<tmpdir>` via the
  `'tauri:options': { application, args }` capability (note: **one** `--open=<dir>` token, not the
  two-token `--open <dir>` a human would type at a shell — see the comment in `wdio.conf.ts` for why:
  msedgedriver's own arg handling silently drops a bare positional token that isn't shaped like a
  `--switch`; negative geometry has the same trap — `--x -4000` fails clap parsing, `--x=-4000` is
  required);
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

### CPE-1048 — `DevToolsActivePort file doesn't exist` on `windows-latest`

The harness ran green on a real desktop but red in CI with
`WebDriverError: session not created: DevToolsActivePort file doesn't exist`. Root cause: the
WebView2 **browser process** itself crashes at startup on the stock runner (no GPU, restricted CI
session) before it can write the `DevToolsActivePort` file msedgedriver waits on — a version/timeout
issue it is not.

Fix, in the correct channel:
- `gui-smoke.yml` sets a **job-level env var**,
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--disable-gpu --no-sandbox --disable-dev-shm-usage"`. The
  WebView2 Runtime itself reads this env var and merges the flags into the *browser* process command
  line, and it's inherited straight down the chain: wdio → tauri-driver → msedgedriver → the app →
  WebView2.
- `wdio.conf.ts` also adds `webviewOptions: {}` to the `tauri:options` capability (a reported fix for
  a Windows "session not created" — tauri-apps/tauri discussion #10122) and raises
  `connectionRetryTimeout` / the mocha timeout modestly, for a slow first WebView2 cold start.

**Two distinct arg channels — do not conflate them:** `tauri:options.args` lands on the app exe's
own **clap-parsed argv** (that's how `--test-mode`/`--x=-4000`/`--open=<dir>` work). Chromium browser
flags like `--no-sandbox`/`--disable-gpu` must **never** go there — clap would reject them and the
app would exit, which is worse. They only belong in the `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` env
var. Relatedly, never add `--user-data-dir` to that env var either — msedgedriver owns the
user-data-dir it watches for `DevToolsActivePort`, and overriding it makes WebView2 write the file
somewhere the driver isn't looking, reproducing the same failure.

## Follow-ups (not this ticket — see CPE-1045's "Follow-ups" section)

- **Linux CI leg**: add an `ubuntu-latest` matrix arm using `webkit2gtk-driver` + `xvfb-run` (no
  app code change needed).
- **macOS**: stays attended — `tauri-driver` has no WKWebView WebDriver support.
- **More flows**: Back/Up navigation, dialogs, context menus, tab switching.
- **Visual regression**: reuse WDIO's screenshot capture for light/dark pixel-diffs.
