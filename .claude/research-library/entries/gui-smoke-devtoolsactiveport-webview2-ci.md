---
topic:       gui-smoke-devtoolsactiveport-webview2-ci
title:       Why does gui-smoke fail on windows-latest with "session not created / DevToolsActivePort file doesn't exist" and how do we fix it?
date:        2026-07-25
researcher:  opus
relates:     [CPE-1048, CPE-1045, CPE-1046, CPE-1047, epic-616]
tags:        [gui-testing, e2e, tauri-driver, webdriver, webdriverio, msedgedriver, webview2, ci, github-actions, devtoolsactiveport, session-not-created, headless, smoke-test, no-sandbox, disable-gpu]
status:      current
sources:     [in-repo, context7, web]
---

## Question
The CPE-1045 `gui-smoke` job (tauri-driver + WebdriverIO driving the real built exe) is **green on a
real Windows desktop** but **red on GitHub `windows-latest`**: WebDriver session creation fails after a
60s wait with `WebDriverError: session not created: DevToolsActivePort file doesn't exist`. What is the
concrete cause, and what does a Worker change in the workflow / `wdio.conf.ts` to fix it (or, honestly,
is a green stock-runner run not achievable and we make the job non-blocking)?

## Key facts established (grounding)
- **Version alignment is already handled — rule it out as the primary cause.** The job installs the
  driver via `msedgedriver-tool`, whose source reads the **Evergreen WebView2 Runtime** version from
  registry GUID `{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}` (verified in its `src/main.rs`) — i.e. the
  exact runtime the wry app renders with, not the standalone Edge browser. So the driver already
  matches what the app uses. (Residual low-probability risk: the runner's Runtime auto-updates between
  the install step and the test step, or no exact-build driver is published → a patch-level drift.)
- **`DevToolsActivePort file doesn't exist` is a *browser-process-start* failure, not a version or
  window-position failure.** msedgedriver (in "launch"/`UseWebView` mode, per Microsoft's WebView2
  WebDriver doc) launches the app exe, hands the hosted WebView2 a temp `--user-data-dir`, and **waits
  for the WebView2 browser process to write the `DevToolsActivePort` file into that dir**. If that
  process crashes at startup (sandbox can't init / GPU process dies) the file never appears → this exact
  error after the timeout. This is *the* canonical Chromium-in-CI cause across chromedriver/msedgedriver.
- **What differs on `windows-latest` vs. a real desktop:** no hardware GPU (software rendering) and a
  restricted CI session — precisely the conditions under which the Chromium sandbox / GPU process
  crashes. Hence it reproduces only in CI.
- **`--test-mode --x=-4000` (CPE-1046/1047) is safe and does NOT cause this.** Verified in
  `src-tauri/src/lib.rs`: test-mode sets `.focused(false)` + skips the on-screen clamp (off-screen), but
  the window is still **shown** — it is *not* `visible(false)`. So WebView2 still initializes fully; an
  off-screen/unfocused window renders and opens its debug port normally. It's good-to-have in CI
  (can't grab a display, matches the anti-disruption convention) but it neither creates nor fixes the
  DevToolsActivePort failure.
- **Two DISTINCT arg channels — do not conflate them (this is the #1 implementation trap):**
  - `tauri:options.args` → lands on the **app exe's argv** (clap-parsed). This is how `--open=<dir>`
    works, and where `--test-mode --x=-4000` must go. **Chromium browser flags must NOT go here** —
    clap would reject `--no-sandbox`/`--disable-gpu` and the app would exit → DevToolsActivePort missing.
  - `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` **environment variable** → read by the WebView2 Runtime
    itself and merged into the *browser* process command line. wry/Tauri honor it (it's the mechanism
    behind Tauri's `additionalBrowserArgs`). Set at job level, it is inherited down wdio → tauri-driver
    → msedgedriver → the launched app → WebView2. This is the channel for `--no-sandbox`/`--disable-gpu`.
  - **Trap within the trap:** do NOT put `--user-data-dir=...` in that env var. In launch mode
    msedgedriver owns the user-data-dir it watches for `DevToolsActivePort`; overriding it makes
    WebView2 write the file somewhere the driver isn't looking → same failure. Only pass
    *process-stability* flags that don't touch the profile dir.

## Findings / Options

**Option A — Inject CI browser-stability flags via `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`.**
*(recommended, primary lever)*
- Add a job-level env in `gui-smoke.yml`:
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--disable-gpu --no-sandbox --disable-dev-shm-usage"`
  (`--no-sandbox` is the canonical DevToolsActivePort fix; `--disable-gpu` covers the no-GPU runner;
  `--disable-dev-shm-usage` is harmless on Windows). Set it on the job (or at least on the "Run GUI
  smoke suite" step) so it's inherited by the app the driver launches.
- \+ Directly targets the verified cause (browser process crash at startup); no app-code change; cheap
  and reversible. − Not a guaranteed one-shot on finicky CI; env-var inheritance through the
  wdio→tauri-driver→msedgedriver→app chain should be confirmed (it holds by normal inheritance, but is
  worth a one-line log check on first run).

**Option B — Add `webviewOptions: {}` to the `tauri:options` capability.** *(cheap complementary)*
- A Tauri maintainer reported (tauri discussion #10122) that adding an empty `webviewOptions: {}` to
  `tauri:options` (with a recent tauri-driver) resolved a Windows "session not created" — it nudges the
  driver into proper WebView2 mode. \+ One line, zero risk. − May be a no-op on this tauri-driver
  version; include it alongside A, not instead of it.

**Option C — Adopt `--test-mode --x=-4000` in `tauri:options.args`.** *(required by the ticket; not a fix
by itself)*
- Per CPE-1046/1047 AC: `args: ['--test-mode', '--x=-4000', \`--open=${tmpDir}\`]` (negative geometry
  MUST use the `=` form — `--x -4000` fails clap). \+ Off-screen + unfocused, can't grab a CI display,
  matches convention; safe (window still shown → WebView2 still inits). − Does **not** address
  DevToolsActivePort; ship it *with* A.

**Option D — Version/timeout hardening.** *(belt-and-braces)*
- Keep `msedgedriver-tool` immediately before the test step (minimize Runtime-drift window); optionally
  `choco upgrade microsoft-edge` is **not** needed (driver tracks the Runtime, not Edge). Bump the wdio
  connection/session timeouts (`connectionRetryTimeout`, mocha timeout) so a slow first WebView2 cold
  start isn't misread as failure. \+ Removes secondary flakiness. − Won't fix a hard sandbox/GPU crash
  on its own.

**Option E — Make the job non-blocking (honest fallback).** *(only if A–D don't go green in ~1–2 tries)*
- `continue-on-error: true` on the job (keeps the diagnostic signal, stops reddening `main`) and/or
  move triggers to `workflow_dispatch` + a note; document that the real-desktop run + attended checks
  remain the guarantee, with a self-hosted/interactive-session runner as the path to a truly blocking
  gate. \+ Honest, unblocks the pipeline. − Loses the automatic regression gate (burndown #1/#2 stay 🔧).

## Recommendation
**Ship A + B + C + D together**, then judge from the CI run:
1. `gui-smoke.yml`: add job env
   `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--disable-gpu --no-sandbox --disable-dev-shm-usage"`.
2. `wdio.conf.ts`: add `webviewOptions: {}` to the capability's `tauri:options`; set the launch args to
   `['--test-mode', '--x=-4000', \`--open=${tmpDir}\`]` (in `onPrepare`); keep browser flags OUT of
   `args` and OUT of any `--user-data-dir`. Optionally raise the session/connect timeout.
This attacks the *verified* cause (browser-process startup crash) through the *correct* channel (the
env var, not the clap-parsed app argv), while satisfying the CPE-1046/1047 off-screen AC. Chosen over
starting with E because WebView2 automation **does** run on stock `windows-latest` in the wild (Tauri's
own WebDriver CI, the WebdriverIO tauri service), so green is realistically achievable — the flags are
the known lever. **But** CI WebView2 automation is genuinely finicky, so wire E (`continue-on-error`)
as the pre-agreed fallback: if A–D aren't green within ~1–2 iterations, flip the job non-blocking with a
documented reason rather than leaving `main` red, and keep burndown #1/#2 at 🔧 with a note.

**Confidence:** HIGH that the flags-via-env-var is the right lever and the correct channel (mechanism +
canonical cause both verified). MODERATE that it goes green first try (residual GPU/sandbox/version-drift
flakiness is real) — hence the ready fallback.

## Sources
- Repo: `.github/workflows/gui-smoke.yml`, `gui-smoke/wdio.conf.ts`, `gui-smoke/README.md`,
  `gui-smoke/specs/open-dir.smoke.ts`; `src-tauri/src/lib.rs` (`resolve_startup_test_mode`,
  `.focused(false)`, no `visible(false)`; `apply_cli_geometry` `allow_offscreen`);
  `Tickets/Done/2026/CPE-1046`, `CPE-1047`; `Tickets/Doing/CPE-1048`.
- `chippers/msedgedriver-tool` `src/main.rs` — detects **WebView2 Runtime** version via registry GUID
  `{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}` (not the Edge browser).
- Microsoft Learn — "Automate and test WebView2 apps with Microsoft Edge WebDriver" (launch vs attach;
  `--remote-debugging-port`; `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` override).
- Microsoft Learn / SeleniumHQ / MicrosoftEdge/EdgeWebDriver #140 — `DevToolsActivePort` = browser
  failed to start / remote debugging blocked; standard fixes `--no-sandbox`, `--disable-dev-shm-usage`,
  kill stale processes, `RemoteDebuggingAllowed` policy; msedgedriver ≥95 fails under LocalSystem (a
  self-hosted-as-service concern, not GitHub-hosted).
- tauri-apps/tauri discussion #10122 — adding `webviewOptions: {}` to `tauri:options` fixed a Windows
  "session not created". tauri-apps/tauri #11144 & wry docs — `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`
  / `additionalBrowserArgs` is the Windows-only browser-flag channel.
- WebdriverIO Tauri docs (edge-webdriver-windows, CI) & Tauri v2 WebDriver CI doc — `msedgedriver-tool`
  install pattern; `windows-latest` runs GUI WebView2 tests.
