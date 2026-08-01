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
   (Linux instead needs `sudo apt install webkit2gtk-driver xvfb` and running under `xvfb-run` —
   see the "CI" section above for the exact package list; CPE-1171 wires this up in CI. **macOS is
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

`.github/workflows/gui-smoke.yml` runs this suite on push + PR to `main` in **two** legs, both
non-blocking (`continue-on-error: true`, see CPE-1048 below):

- **`gui-smoke` (windows-latest)** — builds the frontend, does a `tauri build -- --no-bundle`,
  installs `msedgedriver` + `tauri-driver`, then runs this suite.
- **`gui-smoke-linux` (ubuntu-latest, CPE-1171)** — same build, then installs the Linux WebView
  build deps (`libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf` — the
  same set the 3-OS `Backend` job in `ci.yml` already builds with) plus `webkit2gtk-driver`
  (`WebKitWebDriver`, Linux's native driver, lands on `PATH` at `/usr/bin/WebKitWebDriver`) and
  `xvfb`, installs `tauri-driver`, and runs the identical suite under `xvfb-run` (so GTK/WebKitGTK
  has a virtual display to initialize against). No spec or `wdio.conf.ts` capability changes were
  needed for the Linux leg — `APP_BINARY`/`TAURI_DRIVER_BIN` already resolve per-OS, and
  `tauri-driver` itself picks `WebKitWebDriver` vs `msedgedriver.exe` off `PATH`.

Either leg reds the job's own step output on a regression in launch-or-navigate, instead of needing
a human to notice — the non-blocking posture just means a flake doesn't red `main`.

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

## Screenshots for the Visual Critic (CPE-1148 Part A / CPE-1149)

Every smoke spec captures its key surface to disk, so one `npm test` run leaves a **gallery of the
app's main screens** at `gui-smoke/.screenshots/<name>.png`. Two shared helpers in
`gui-smoke/lib/snap.ts` split the pass and fail cases:

- **Pass** — each spec calls `snap(name)` **inline**, right after its assertions pass, capturing the
  good frame at exactly the right instant (e.g. a dialog just before the spec dismisses it). Writes
  `<name>.png`.
- **Fail (CPE-1149)** — each spec's `afterEach(function () { … })` hook calls
  `snapFailure(this.currentTest, name)`, which writes `<name>-fail.png` **only when the test that
  just ran failed** — a shot of whatever state the assertion failed in. On a pass it is a no-op, so
  no surface is captured twice.

| Pass file | Fail file | Spec | Surface |
|------|------|------|---------|
| `open-dir.png` | `open-dir-fail.png` | `open-dir.smoke.ts` | The plain directory listing after `--open <dir>` navigation |
| `organize-dialog.png` | `organize-dialog-fail.png` | `organize.smoke.ts` | Auto-organize dialog's grouped proposal preview |
| `instant-search.png` | `instant-search-fail.png` | `instant-search.smoke.ts` | Ctrl+K Instant Search overlay (off-means-off state) |
| `batch-media-dialog.png` | `batch-media-dialog-fail.png` | `batch-media.smoke.ts` | Batch-Media dialog's op-pill list + plan preview |
| `replay-tab.png` | `replay-tab-fail.png` | `replay.smoke.ts` | Agent Watch drawer's Replay tab (transport/slider + reconstruction) |
| `cost-history.png` | `cost-history-fail.png` | `cost-history.smoke.ts` | Agent Watch drawer's cost-History rollup |

`.screenshots/` is gitignored — these are run artifacts, never committed. Both helpers swallow their
own errors (a screenshot is observability, not an assertion — it must never fail or mask a real
assertion), and none of the specs' existing non-blocking (`continue-on-error`) behaviour changes.

**Why the split (and not one always-on `afterEach`)?** The inline `snap()` alone is never reached
once an earlier `expect` throws — so before CPE-1149 a failing run left no shot at all, even though
this section (and `snap.ts`) claimed otherwise. Moving *all* capture into an `afterEach` would fix
that but break the pass shots: several specs dismiss their dialog (Cancel) at the end of the test
body, and an `afterEach` runs only *after* that, capturing the dismissed surface. So the pass shot
stays inline (right frame, right moment) and the hook captures only the failing frame — the `-fail`
suffix keeps a failing shot from clobbering the last good `<name>.png` baseline.

**Capturing the surface you changed:** if your ticket touches a GUI surface that already has a
smoke spec, add (or move) a `snap('your-surface')` call after that spec's assertions and add a
matching `afterEach(function () { await snapFailure(this.currentTest, 'your-surface'); })` — reuse
the existing spec rather than adding a new app launch. If the surface has no spec yet, the cheapest
way to get a screenshot is still to add one (see any file in `specs/` for the pattern: reach the
surface the same way a user does, assert something real about it, `snap()` inline, and wire the
`afterEach` fail-shot). Run the harness locally (`cd gui-smoke && npm test`, prerequisites above) and
the PNG lands in `.screenshots/` for a reviewer — human or, per CPE-1148 Part B, a future
Visual-Critic sub-agent — to open directly.

## Faithful mouse input — `lib/mouse.ts` (CDP, NON-grabbing) — CPE-1155

For any test that needs real **mouse behaviour** — click, right-click (context menu), hover, scroll,
drag — use `gui-smoke/lib/mouse.ts`, **not** `browser.action('pointer')` and **not** a bare
`dispatchEvent`.

```ts
import { rightClick, click, hover, scroll, dragTo, doubleClick, cdpAvailable } from "../lib/mouse.js";

await rightClick(".filelist-pane");        // selector → centre of the first match
await rightClick({ x: 922, y: 523 });      // or an explicit viewport point (blank pane pixels)
await click(".some-button");
await hover(".menu-item");                  // fires mousemove/:hover without moving the OS cursor
await scroll(".filelist-pane", 400);        // wheel down 400px
await dragTo(".row-a", ".row-b");
```

**How it works.** Each helper resolves the target to viewport CSS-pixel coordinates
(`getBoundingClientRect` via `browser.execute`) and dispatches through the **Chrome/Edge DevTools
Protocol** — `Input.dispatchMouseEvent` (`mousePressed`/`mouseReleased`/`mouseMoved`, `button:'right'`
for the context menu) and `Input.dispatchMouseWheelEvent`. These inject through the browser's **real**
input pipeline: true hit-testing, native context menu, real event order — as faithful as a physical
click — but **they never move the OS pointer**. msedgedriver exposes CDP over the vendor endpoint
`POST /session/:id/chromium/send_command_and_get_result`, surfaced by WebdriverIO as
`browser.sendCommandAndGetResult(cmd, params)` (`mouse.ts`'s `cdp()` uses this; `browser.cdp(...)` —
the puppeteer-backed variant — is NOT wired for wry here, so we use the vendor endpoint).

**Why not the alternatives (the exact bug this closes):**
- `browser.action('pointer')…` — WebDriver's native Actions API; it moves/grabs input and hijacks an
  interactive machine ([[automation-must-not-hijack-screen]]).
- `el.dispatchEvent(new MouseEvent('contextmenu'))` — non-grabbing but **unfaithful**: it goes
  straight to a chosen node's handler, bypassing hit-testing and native behaviour. This is exactly how
  the CPE-1154 native-menu leak (and then CPE-1157) escaped a "passing" synthetic-event check.

**Verified here (CPE-1155), Edge/WebView2 150 + msedgedriver 150, classic WebDriver against wry:**
- `cdpAvailable()` → **true**. CDP `Input.*` injection works through msedgedriver and reaches the wry
  WebView2 page (a real right-click on a `.row` opens the app's item menu; on blank pane pixels it
  fires the pane's `contextmenu`).
- **Non-grabbing confirmed empirically** — the OS cursor position (read via PowerShell
  `[System.Windows.Forms.Cursor]::Position`) was byte-identical before and after a `rightClick`. CDP
  input never touches the physical pointer by design, and the tauri-driver window can stay unfocused.
- If a future driver drops the vendor endpoint, `cdpAvailable()` returns false so a spec can report
  the finding instead of failing opaquely (per the CPE-1155 fallback AC).

`specs/populated-whitespace.smoke.ts` is the proof + regression spec: it drives a real CDP right-click
on the blank area of a **populated** folder (the CPE-1157 repro), asserts the empty-area menu opens,
and carries a MutationObserver/`contextmenu` probe that pinned CPE-1157's root cause (menu opened then
closed ~5 ms later because `paneContext` didn't `stopPropagation`).

## Follow-ups (not this ticket — see CPE-1045's "Follow-ups" section)

- ~~**Linux CI leg**: add an `ubuntu-latest` matrix arm using `webkit2gtk-driver` + `xvfb-run` (no
  app code change needed).~~ **Done (CPE-1171)** — see the "CI" section above. Still non-blocking
  and **not yet proven green on `main`**: it has not run live on GitHub Actions as of this writing
  (offsite verification pending, no local Actions runner available). Watch the first few `main`
  runs before considering Manual-Test-Burndown row #4 fully retired.
- **macOS**: stays attended — `tauri-driver` has no WKWebView WebDriver support.
- **More flows**: Back/Up navigation, dialogs, context menus, tab switching.
- **Visual regression**: reuse WDIO's screenshot capture for light/dark pixel-diffs.
