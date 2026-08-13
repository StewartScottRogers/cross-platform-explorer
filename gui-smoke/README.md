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
6. **Every `samples/` file opens without crashing (CPE-1358, epic CPE-1148)** —
   `wdio.conf.ts#seedSamplesFixture` copies the real repo `samples/` tree into `CPE-1358-samples/`
   inside the shared tmpDir (`fs.cpSync`, so a new sample fixture is picked up automatically, no
   filename list to maintain). `specs/samples.smoke.ts` then walks EVERY file discovered under it: for
   each, navigates to its folder via the address bar (`Ctrl+L` → type the absolute path → Enter),
   selects it, and asserts (a) the app/window is still responding — the crash guard — and (b) the
   preview pane settled into real content (`.preview-img`/`.mp-media`/`.preview-pdf`/
   `[data-testid="font-preview"]`/`.preview-table-wrap`/`.preview-markdown`/`.code-view`/`pre.preview-text`/
   `[data-testid="hexview"]`/`.data-browser`) or an explicit graceful "can't preview this" note/fallback
   (`.preview-note`, or the `aside.details` metadata pane) — never a stuck spinner. `documents/
   malformed.pdf` (the ORIGINAL degenerate PDF that crashed the app, CPE-1357 — see `samples/README.md`'s
   "PDF fixtures" section) runs LAST, in its own `it()`, so a regression in the crash guard doesn't blind
   the rest of the walk; CPE-1357's validate-before-embed fix (`pdf_validity`) already lands this file in
   the metadata-pane fallback rather than WebView2's PDF viewer, so this assertion is expected to PASS
   like every other file — it stays last as a defense-in-depth regression pin, not because it's currently
   expected to fail. Pairs with the headless coverage ratchet `src/lib/sampleCoverage.test.ts`, which
   asserts every supported preview kind has a sample in the first place.
7. **Preview-pane provider surfaces render, in BOTH themes and BOTH pane widths (CPE-1629)** —
   `specs/preview-pane.smoke.ts` opens the preview pane against committed `samples/` files and
   `snap()`s each structured provider: the Binary Inspector's tabs (data-driven off whatever
   `.bp-tabs .tab` buttons actually render, walked against both a native PE — `other/mini.dll` — and a
   managed .NET PE — `other/mini-dotnet.dll`, CPE-1629's own fixture — including CPE-1615's ".NET
   metadata" tab, picked up automatically with zero spec changes when that PR merged), the sqlite
   data-grid, the font glyph-grid specimen, and the certificate/JWT previews' EXPIRED badges. See
   "Preview-pane provider screenshots" below for the full write-up and the one-line recipe for adding a
   new provider.

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

`.github/workflows/gui-smoke.yml` runs this suite in **two** legs:

- **`gui-smoke-linux` (ubuntu-latest, CPE-1171)** — runs on every push + PR to `main`. Builds the
  frontend, does a `tauri build -- --no-bundle`, installs the Linux WebView build deps
  (`libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf` — the same set the
  3-OS `Backend` job in `ci.yml` already builds with) plus `webkit2gtk-driver` (`WebKitWebDriver`,
  Linux's native driver, lands on `PATH` at `/usr/bin/WebKitWebDriver`) and `xvfb`, installs
  `tauri-driver`, and runs the suite under `xvfb-run` (so GTK/WebKitGTK has a virtual display to
  initialize against). **This is the BLOCKING gate (CPE-1594)** — see "The ratchet" below for exactly
  what makes it pass or fail.
- **`gui-smoke` (windows-latest)** — builds the frontend, does a `tauri build -- --no-bundle`,
  installs `msedgedriver` + `tauri-driver`, then runs this suite. **Non-blocking
  (`continue-on-error: true`) and off the push/PR path (CPE-1594)** — see CPE-1048 below for why. It
  runs only via `workflow_dispatch` (manual) or the nightly `schedule:` trigger.

No spec or `wdio.conf.ts` capability changes are needed per OS — `APP_BINARY`/`TAURI_DRIVER_BIN`
already resolve per-`process.platform`, and `tauri-driver` itself picks `WebKitWebDriver` vs
`msedgedriver.exe` off `PATH`.

### The ratchet (CPE-1594, case-granular since CPE-1677) — how the Linux leg's verdict is computed

The Linux leg does **not** require every test to pass. It requires **no new failures** beyond a
committed, named list — and the unit of exemption is **one test case**, not a whole spec file:

- **`gui-smoke/known-failing.json`** — a `cases` array. Each entry is one `it()` that is allowed to
  fail today: `{ "spec": "<spec file basename>", "test": "<it() title, verbatim>", "reason": ...,
  "ticket": ... }`. Anything failing that ISN'T listed reds the job — **including a case inside a spec
  file that already has other entries here.**
- **`gui-smoke/lib/ratchet.ts`** (pure, unit-tested in `ratchet.test.ts`) + **`gui-smoke/scripts/run-ratchet.ts`**
  (the `npm run ratchet` I/O wrapper the CI step actually runs) compare the suite's real JSON results
  (`wdio.conf.ts`'s `json` reporter, written to `gui-smoke/.results/`, read down to
  `suites[].tests[].name` + `.state`) against that file and decide:
  1. **`NEW GUI REGRESSION`** — a case failed that isn't listed. Fix it, or (if intentionally deferring
     it) add an entry for **that case** with a reason + ticket. Never exempt the whole file.
  2. **`RATCHET: <case> now passes`** — a case listed as known-failing PASSED this run. **Delete its
     entry from `known-failing.json` in the same PR.** The ratchet is one-way: once a case is fixed, it
     can never quietly re-enter the failing column — leaving a passing case listed would hide a real
     future regression on it, and the QA burndown depends on this list draining.
  3. **`STALE EXEMPTION`** — a listed `test` title matched **no case** in the run. Titles are strings
     and they drift; if a rename silently dropped the exemption it would drop the case's coverage with
     it. Update the entry's `test`, or delete the entry if the case is genuinely gone (and check that
     losing its coverage was intended).
  4. **`SUITE DID NOT COMPLETE`** — fewer spec FILES reported a result than `specs/*.smoke.ts` globs to
     (a timeout, crash, or hang). This is the specific guard against the exact failure mode CPE-1594 was
     filed over: 796 straight `cancelled` runs where a dead leg's timeout-kill made the whole workflow
     unreadable. A truncated run is always RED, never green. (Kept at spec granularity: there is no
     committed expected-case count, and clause 3 already catches a truncation that swallows a listed
     case.)
  5. **`DUPLICATE EXEMPTION`** — the same `spec` + `test` is listed twice. List hygiene: with a
     duplicate present, "delete its entry" in clauses 2/3 would leave the case exempt anyway.
  6. **`UNRECOGNISED TEST STATE`** (CPE-1680) — a case reported a wdio `state` this ratchet has never
     seen. This reduces to its own `"unknown"` outcome, **never** to `"skipped"` — a skipped case is
     exempt from every clause above, so folding an unknown state into it would let a state this ratchet
     doesn't understand (a new wdio version, a new runner mode, a state produced by a crash path)
     silently pass as "safe to ignore". `"unknown"` reds the run unconditionally instead, whether or not
     the case happens to be listed.
  7. **`UNEVIDENCED INTERMITTENT`** (CPE-1680) — an `"intermittent": true` entry whose `reason`/`ticket`
     don't clear a minimum evidence bar (a non-trivial `reason`, a real-looking `ticket`). This can't
     prove a case is actually flaky — only real run history can — so it doesn't try to; it refuses only
     the structurally checkable failure mode an internal audit found: an entry with an **empty** `reason`
     and **empty** `ticket`, which would silence a permanently-broken case forever with nothing
     distinguishing it from the real thing.

  A `skipped`/`pending`/`unknown` case is none of the above by itself: none can red the job (clause 1)
  or retire an exemption (clause 2), but each does prove the title still exists (clause 3) — `unknown` is
  additionally its own always-red outcome (clause 6). A **failing hook** becomes a synthetic case named
  `<hook> "<title>"`, which is unlisted by construction and therefore red — a `before` hook that throws
  usually means its suite's cases never reported at all, and "absent" must never read as green. When a
  case fails and isn't listed, the `NEW GUI REGRESSION` message includes a ready-to-paste
  `known-failing.json` entry built with `JSON.stringify` (not string concatenation), so a title
  containing a double quote — as wdio's own hook titles do — still round-trips as valid JSON.

**`"intermittent": true` — the one escape hatch, and how not to abuse it.** Case granularity turns a
genuinely flaky case into a coin-flip gate: clause 1 reds the runs where it fails, clause 2 reds the runs
where it passes, and the job is red either way regardless of the change under test. An entry marked
`"intermittent": true` is exempt in **both** directions. It must still exist (clause 3 still applies),
and `npm run ratchet` prints every intermittent entry **with its observed status on every run**, so it
stays visible and drainable instead of becoming a quiet permanent hole. The bar is evidence, not
annoyance: the entry's `reason` must cite the real runs where the same case both passed and failed on
unchanged code. A case that fails *every* run is a plain entry, not an intermittent one. Since CPE-1680
that bar is also machine-checked (clause 7, `UNEVIDENCED INTERMITTENT`): an entry needs a non-trivial
`reason` and a real-looking `ticket`, or the ratchet refuses it outright. The check can't verify actual
flakiness — only real run history can — it only closes the one hole that WAS silently open: an entry
with an empty `reason` and empty `ticket` used to be accepted exactly like a well-evidenced one.

**No current users (CPE-1679 drained the last four).** The four `samples/audio/track.{flac,mp3,ogg}` +
`samples/video/clip.mp4` cases were `intermittent: true` from CPE-1677 (found by the first live runs
of the case-granular gate) until CPE-1679 found and fixed the real cause: `waitForPreviewToSettle`'s
selector list had no entry for `MediaPlayer.svelte`'s own graceful-fallback markup (`.mp-fallback`,
the "Can't play this media file" UI shown on the `<audio>`/`<video>` element's `error` event). When
GStreamer/WebKitGTK under Xvfb fails to decode one of these four files — confirmed against a real
failure screenshot (CI run `31630437256`'s `samples-walk-fail.png`, taken by `snapFailure` at the
instant `clip.mp4`'s case timed out, showing the pane already settled on `.mp-fallback`'s exact
markup, not "still loading") — the app does exactly what this spec's own name allows ("no crash +
preview renders **or gracefully degrades**"), but the settle check recognised neither `.mp-media`
(unmounted by the `{#if errored}` branch) nor `.preview-note` (not the class MediaPlayer uses), so it
spun for the full 20s and failed. Fixed by adding `.mp-fallback` to `PREVIEW_CONTENT_SELECTOR`
(`gui-smoke/lib/samplesNav.ts`) — the same pattern already used for every other kind's own
graceful-degrade markup (`aside.details` for CPE-1357). Verified with a before/after repeated-open
stress harness against real ubuntu-latest + Xvfb + WebKitGTK (not merged — see the CPE-1679 PR body
for the run ids and counts): genuine 20s-timeout failures were common before the fix and vanished
completely after it, across far more repeated real attempts than the before run survived (an
apparent session death under rapid repeated same-file opens — not a settle-check failure — capped how
many attempts the before run could log; the after run comfortably outlasted it with zero settle
failures). All four entries are removed; if a NEW case ever needs `intermittent`, open a fresh ticket
rather than reusing this one (see the `$comment` above).

**The "session death" above is `mochaOpts.timeout`, not a WebKitGTK/GStreamer leak (CPE-1702).** CPE-1679's
throwaway stress harness (scratch-only, `specs/zzz-cpe1679-stress.smoke.ts` on the never-merged
`cpe-1679-stress-experiment`/`cpe-1679-stress-after` branches) died mid-loop with `A sessionId is
required for this command` on all three of its real CI runs (`31672811976`, `31671831127` before the
fix; `31673687870` after). CPE-1702 pulled the raw job logs (`gh api
repos/:owner/:repo/actions/jobs/<id>/logs` — the "spec"-reporter view `gh run view --log` shows is
pre-truncated and hides this) and found the death lands at **wall-clock ~90.000s in all three runs**
("`1 failing (1m 30s)`" in the mocha reporter, every time) regardless of how many attempts had
completed — 5 before the fix (each failing attempt burned the full 20s settle-timeout), 46 after (each
passing attempt took ~1.85s) — which is the smoking gun: a real WebKitGTK/GStreamer resource leak would
scale with attempt COUNT, not hold wall-clock time fixed across a 9x difference in attempts. No
GStreamer/GLib `CRITICAL`/`WARNING`, no `EMFILE`/"Too many open files", no core dump or segfault
appears anywhere in any of the three raw logs — the app process itself never crashes.

What actually happens: this file's `it()` calls `this.timeout(ITERATIONS * FILES.length * 25_000 +
60_000)` (≈34 minutes for 20×4 attempts) to give its long loop room, but `wdio.conf.ts`'s
`mochaOpts.timeout: 90_000` — deliberately generous **for the real suite**, which opens each file
once (CPE-1481) — fires anyway: an in-test `this.timeout()` override is not reliably honoured by
`@wdio/mocha-framework` (a long-documented framework limitation, not unique to this repo — see
webdriverio/webdriverio#1794 and the wdio-mocha-framework issue tracker). When the 90s Runnable
timeout fires, WDIO's worker tears the WebDriver session down immediately (`COMMAND deleteSession()`
appears in the log the instant the `Timeout` error is thrown) — but the stress loop's own `for`
await-chain has no cancellation awareness and is still mid-flight, so its very next command
legitimately finds the session gone and throws exactly the "sessionId" error being chased. It is a
self-inflicted teardown race, not exhaustion of any OS/GStreamer/WebKitGTK resource.

**The ceiling for a future stress harness:** do not rely on `this.timeout()` inside the test body to
extend past `wdio.conf.ts`'s 90s default — it will not reliably take effect. Either keep a single
stress `it()` under ~85 wall-clock seconds (budget per-attempt time accordingly — at CPE-1679's
post-fix ~1.85s/attempt that's ~45 safe attempts per session, far fewer if any attempt can still hit
the 20s settle-timeout path), or split the loop across multiple `it()`s/files so each one starts a
fresh session inside its own 90s budget, or add a real per-spec `mochaOpts` override for that one
scratch file. Whichever you pick, a "SESSION-DEAD" tally entry means you hit this ceiling, not that
GStreamer leaked — don't go looking for a leak that was ruled out here.

**Why case granularity (CPE-1677).** The original ratchet exempted whole spec files. `samples.smoke.ts`
is listed for 22 of its 46 cases (the CPE-1507 preview-settle tail), which meant the other 24 guarded
nothing: the CPE-1639 worker deliberately broke the font-preview case inside that file, ran the real
job, and the baseline run and the broken run printed the **byte-identical** verdict — `38 passed, 3
failed, 3 known-failing listed — OK` — and both passed. Only the raw per-test log showed the flip. Hence
also: `npm run ratchet` now always prints the per-case failing set, green or red, so a verdict can never
again be identical across a clean and a broken run.

**Retiring a `known-failing.json` entry** (once its case actually passes): reproduce locally
(`cd gui-smoke && xvfb-run --auto-servernum npm test` on Linux, or just `npm test` if your desktop
already runs the Linux driver stack), confirm the case passes, delete **that case's** entry from
`known-failing.json`, and open the PR. `npm run ratchet` will fail loudly (clause 2 above) if you forget
and leave a passing case listed. If you RENAME a listed case, update its `test` in the same commit —
clause 3 will red the job otherwise.

**Testing the ratchet locally without a real `tauri-driver` run** — `npm run ratchet` reads its inputs
from disk and every path is overridable via env var, so it can be pointed at a saved/synthetic report:

```
GUI_SMOKE_RESULTS_DIR=/path/to/synthetic/.results \
GUI_SMOKE_KNOWN_FAILING=/path/to/synthetic/known-failing.json \
GUI_SMOKE_SPECS_DIR=./specs \
npm run ratchet
```

(`gui-smoke/lib/ratchet.test.ts` exercises the pure `evaluate()` function directly, headlessly, with no
env vars or disk I/O at all — that's the primary test surface; the env-var override above is for
smoke-testing the `run-ratchet.ts` I/O wrapper itself.)

### Screenshot artifacts (CPE-1594) — unlocks the Visual Critic in CI

Both legs upload `gui-smoke/.screenshots/**` as a build artifact, `if: always()` (so a failing/timed-out
run's `-fail.png` shots — CPE-1149 — still upload). Download with:

```
gh run download <run-id> -n gui-smoke-screenshots-ubuntu -D <dir>
```

(or `-n gui-smoke-screenshots-windows` for the Windows leg, when it runs). `<run-id>` is in the run's
URL or `gh run list --workflow=gui-smoke.yml`. This is what lets a reviewer — human or, per CPE-1148
Part B, a Visual-Critic sub-agent — judge a PR's rendered UI directly from the CI artifact, without a
Foreman running a local `tauri build` by hand first.

**Two ordering/config details that PR #801's first live run got wrong and had to fix — worth knowing if
this step ever looks broken again:**

- **`include-hidden-files: true` is required.** `gui-smoke/.screenshots/` starts with a dot, and
  `actions/upload-artifact@v4` excludes anything under a dot-prefixed folder by default ("any file
  beginning with `.` or files within folders beginning with `.`" — the action's own docs). Without this
  flag the step silently uploads nothing and logs `No files were found with the provided path` — the
  suite can genuinely be writing every PNG correctly and this step still reports empty.
- **On the Linux leg, the upload step runs AFTER the `Ratchet` step, not before.** The suite step always
  "succeeds" (`|| true` swallows its exit code) so the Ratchet step — the real gate — always runs next
  unconditionally. The screenshot upload comes last with `if: always()` and `if-no-files-found: warn`
  (not `error`): if it ever fails or finds nothing, that must never take down the job before the ratchet
  verdict has been computed. Putting a can-fail, `error`-severity artifact step BETWEEN the suite and the
  ratchet is exactly what silently skipped the ratchet on PR #801's first run — GitHub Actions steps
  default to running only if every prior step succeeded, so an aborted upload step meant "no ratchet
  output anywhere in the log" even though the suite itself had completed cleanly.

Either leg reds the job's own step output on a regression in launch-or-navigate, instead of needing
a human to notice. The Windows leg's non-blocking posture (CPE-1048) means a WebView2 crash there
never reds anything; the Linux leg's blocking posture (CPE-1594, above) means an actual new regression
does.

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
| `thumbnail-gallery.png` | `thumbnail-gallery-fail.png` | `thumbnail-gallery.smoke.ts` | Gallery view streaming real PNG/SVG thumbnails through the CPE-1237 priority queue + cache |
| `samples-pdf-valid.png` | `samples-walk-fail.png` | `samples.smoke.ts` | The replaced, now-valid `documents/doc.pdf` rendering in the PDF preview |
| `samples-font.png` | `samples-walk-fail.png` | `samples.smoke.ts` | The new `fonts/mini.ttf` sample rendering in the font-specimen preview |
| `samples-data-grid.png` | `samples-walk-fail.png` | `samples.smoke.ts` | The new `data/mini.sqlite` sample rendering in the data-grid preview |
| `samples-pdf-malformed.png` | `samples-walk-fail.png` | `samples.smoke.ts` | The CPE-1357 crash-regression fixture (`documents/malformed.pdf`) — the app surviving via the `pdf_validity` reject → metadata-pane fallback |
| `binary-native-<tab>-<combo>.png` | `preview-pane-…-fail.png` | `preview-pane.smoke.ts` | Binary Inspector tabs over a native PE (`other/mini.dll`) — `<tab>` is whatever `.bp-tabs .tab` renders (Overview in all 4 `<combo>`s: `light-wide`/`light-narrow`/`dark-wide`/`dark-narrow`; up to 2 more tabs once, at a shared combo — `restTabLimit`, see `walkBinaryInspectorTabs`'s doc comment) |
| `binary-managed-<tab>-<combo>.png` | `preview-pane-…-fail.png` | `preview-pane.smoke.ts` | Binary Inspector tabs over a MANAGED .NET PE (`other/mini-dotnet.dll`, CPE-1629) — same data-driven walk; includes CPE-1615's real ".NET metadata" tab (`binary-managed-net-metadata-*.png`, full flagship depth, picked up automatically once that PR merged — real assembly identity + referenced-assemblies table) |
| `preview-pane-data-grid-<combo>.png` | `preview-pane-…-fail.png` | `preview-pane.smoke.ts` | The sqlite data-grid preview (`database/mini.sqlite`), `light-wide` + `dark-narrow` |
| `preview-pane-font-<combo>.png` | `preview-pane-…-fail.png` | `preview-pane.smoke.ts` | The font glyph-grid specimen (`fonts/mini.ttf`), `dark-wide` + `light-narrow` — a real pill-reflow surface |
| `preview-pane-cert-*-<combo>.png` | `preview-pane-…-fail.png` | `preview-pane.smoke.ts` | Certificate preview: `expired.pem`'s EXPIRED badge at `dark-narrow` (the pill-clipping case) + `chain.pem` at `light-wide` |
| `preview-pane-jwt-*-<combo>.png` | `preview-pane-…-fail.png` | `preview-pane.smoke.ts` | JWT preview: `expired.jwt`'s EXPIRED badge at `dark-narrow` + `rich-claims.jwt` at `light-wide` |

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
Visual-Critic sub-agent — to open directly. **If the surface renders in the PREVIEW PANE specifically**
(a new file-preview provider, or a new tab/section inside an existing one like the Binary Inspector),
see the next section — `specs/preview-pane.smoke.ts` is the one file that already owns that pattern,
and adding to it is usually cheaper than a whole new spec.

## Preview-pane provider screenshots — `specs/preview-pane.smoke.ts` (CPE-1629)

Before this spec, `gui-smoke` had **zero coverage of the preview pane's structured providers**. When
the Binary Inspector (CPE-1597) gained a whole new tab (CPE-1615's ".NET metadata" tab), CI produced
no screenshot of it — the only way to actually look at the change was a reviewer hand-building a
throwaway Vite + real-Chrome harness, mounting the component against canned data, then throwing the
harness away. This spec is that coverage, made permanent and reusable: it opens the preview pane
against the same committed `CPE-1358-samples` copy `samples.smoke.ts` walks (`wdio.conf.ts
#seedSamplesFixture` — new samples are picked up automatically, no fixture-list to maintain) and
`snap()`s each structured provider surface, **in both light and dark theme, and at both a narrow and a
comfortable pane width** — the narrow case is deliberate: that's where clipping and pill-reflow
defects actually show up (CLAUDE.md's "tick-tacks reflow" convention; the cert/JWT EXPIRED badges and
the font glyph-grid are exactly that shape of surface).

**Two small, reusable helpers make the theme/width axes cheap to drive from any spec:**
- `lib/theme.ts#setTheme("light" | "dark")` — stamps `document.documentElement.dataset.theme` directly
  (the same DOM effect `src/lib/theme.ts#applyTheme` produces at runtime), skipping a
  `localStorage`+reload round-trip.
- `lib/paneWidth.ts#setPreviewPaneWidth(px)` — drives the preview pane's own Toolbar "Pane width"
  number input through its settings popover (open the gear, set the value the same faithful way
  `lib/samplesNav.ts#navigateTo` sets the address bar, close the popover). `PREVIEW_PANE_NARROW_PX`
  (220, `App.svelte`'s `RIGHT_MIN`) and `PREVIEW_PANE_COMFORTABLE_PX` (400) are exported constants.

**Cost is real — respect it when adding a surface.** Each (theme, width) combo change is a genuine
popover round trip against a live WebView2/WebKitGTK session (multiple seconds observed on a real
`tauri build`, not a jsdom mock) — multiplying that by "every combo × every tab/section" blew the
suite's 90s-per-test `mochaOpts.timeout` the first time this spec was written (see the Binary Inspector
tab walk's own header comment for the exact fix: only ONE flagship tab/section gets the full 2×2
matrix; every other tab is capped at `restTabLimit` and captured at a SINGLE shared combo applied once,
not re-toggled per tab). `preview-pane.smoke.ts`'s own `describe` block also widens every test's mocha
timeout to 240s (`this.timeout(240_000)` at the SUITE level — note: a `beforeEach` hook's own
`this.timeout()` only widens that hook's timeout, not the following test's, a real bug this spec's first
real-app run caught) as headroom for this cost on a loaded CI runner. Even with those two fixes, this
spec's own full run took ~9-10 minutes on a real `tauri build` — noticeably heavier than most of this
suite's other specs; budget for that if the Linux leg's overall 45-minute job cap (CPE-1481/1594) is
ever felt to be tight.

### Adding a spec for a new preview provider — the one-line recipe

Most new preview providers need only ONE more `it()` in `specs/preview-pane.smoke.ts`, following the
data-grid/font/cert/jwt tests as the template:

```ts
it("the <your provider> preview renders <what> (samples/<path>)", async function () {
  await openSampleFile(path.join(samplesRootAbs, "<subdir>"), "<sample-file>", {
    extraSelectors: ['[data-testid="your-provider-preview"]'], // only if not already in
  });                                                            // PREVIEW_CONTENT_SELECTOR
  await $('[data-testid="your-provider-preview"]').waitForExist({ timeout: 10_000 });

  await applyCombo({ theme: "dark", widthPx: NARROW, suffix: "dark-narrow" }); // the pill/reflow case
  await snap("preview-pane-<your-provider>-dark-narrow");

  await applyCombo({ theme: "light", widthPx: WIDE, suffix: "light-wide" });   // a comfortable/roomy pass
  await snap("preview-pane-<your-provider>-light-wide");

  await assertAppStillAlive("previewing samples/<path>");
});
```

Reuse an EXISTING `samples/` fixture if one already exercises your provider (check
`samples/README.md`'s coverage table first); add one, following that file's conventions, only if none
does. For a MULTI-TAB/MULTI-SECTION provider (like the Binary Inspector), reuse
`walkBinaryInspectorTabs` as the pattern — query the tab strip's own buttons rather than hardcoding tab
names, so a future tab is captured automatically with zero changes here. Proven, not hypothetical:
CPE-1615's ".NET metadata" tab merged into `main` WHILE this spec was being written, and this exact
mechanism picked it up automatically at full flagship depth with zero code changes — see the managed-PE
walk test's own comment and `binary-managed-net-metadata-*.png`.

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

## Visual-regression comparator — `lib/compare.ts` (CPE-1170)

Burns down `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` row 3 ("Visual / theme regression").
`lib/snap.ts` only *captures* a PNG — nothing compared it to anything, so a real regression (a menu
rendering with invisible text, a broken pill row, a theme colour drifting) was only caught if a human
or the CPE-1148 Visual Critic happened to open the shot. `lib/compare.ts` adds the missing half: a
pixel-diff comparator that decodes two PNGs and reports the delta.

**Zero new dependencies.** No `pngjs`/`pixelmatch` in `gui-smoke/package-lock.json`, and this doesn't
add them — the captures here are small, deterministic 8-bit PNGs, so `decodePng`/`encodeRgbaPng`
hand-roll PNG chunk parsing + the PNG scanline-filter set (Sub/Up/Average/Paeth) and lean on Node's
**built-in** `zlib` for the actual DEFLATE inflate/deflate. Scope: 8-bit depth, non-interlaced,
color types 0/2/4/6 (grayscale/RGB/grayscale+alpha/RGBA) — covers every screenshot this harness
produces; interlaced or palette (color type 3) PNGs throw a named error rather than silently
misdecoding.

### API

```ts
import { comparePngBuffers, compareSnapshotToBaseline } from "../lib/compare.js";

// Pure, in-memory: given two PNG Buffers, returns
// { diffPixels, totalPixels, diffRatio, mismatchAboveThreshold, sizeMismatch?, error? }.
const result = comparePngBuffers(baselineBuf, capturedBuf, { pixelTolerance: 0, ratioThreshold: 0.01 });

// Filesystem-aware wrapper a spec actually calls: diffs `.screenshots/<name>.png` (what `snap(name)`
// just wrote) against the committed `baselines/<name>.png`.
await compareSnapshotToBaseline("open-dir");
```

### Wiring it into a spec (the worked example)

`specs/open-dir.smoke.ts` calls it right after its `snap("open-dir")`, the same "call it after your
real assertions pass" convention `snap()` itself follows:

```ts
await snap("open-dir");
await compareSnapshotToBaseline("open-dir");
```

### Advisory by default — `GUI_SMOKE_VISUAL_STRICT`

`compareSnapshotToBaseline` mirrors `snap()`'s "never mask a real assertion" swallow: any I/O/decode
problem (missing capture, missing baseline, corrupt PNG) is logged and returned as an `error` field,
never thrown. An over-threshold diff is likewise only a `console.warn` **by default** — it feeds the
CPE-1148 Visual Critic as a reported delta, it doesn't fail the spec.

Set the env var **`GUI_SMOKE_VISUAL_STRICT=1`** to promote an over-threshold diff into a thrown
`Error` (failing the calling spec) — the one intentional exception to "advisory only", for whoever
wants this to gate a build:

```
GUI_SMOKE_VISUAL_STRICT=1 npm test
```

### Baselines — where they live, and how to bless one

Golden PNGs live in `gui-smoke/baselines/<name>.png` — **committed** (unlike `.screenshots/`, which
is gitignored run output). See `gui-smoke/baselines/README.md` for the full convention. Short version:

- **No baseline yet for a surface?** `compareSnapshotToBaseline` returns an advisory `error` ("no
  baseline at ...") rather than failing — nothing breaks until one exists.
- **Bless one** (first time, or after an intentional visual change) by running the harness once with
  `GUI_SMOKE_UPDATE_BASELINE=1`:
  ```
  GUI_SMOKE_UPDATE_BASELINE=1 npm test
  ```
  This copies each `.screenshots/<name>.png` a spec compared over `baselines/<name>.png`. Review the
  diff, then commit the PNG like any other source file.
- Keep this directory small — bless a baseline only for a surface actually worth pinning pixel-exact,
  not one per `snap()` call by default.
- Two tiny **synthetic** demo baselines (`demo-swatch.png`, `demo-swatch-gradient.png`, generated by
  `scripts/bless-demo-baselines.ts`) are committed to prove the bless → commit → compare loop end to
  end without needing a real `tauri build`; see their README for why they're synthetic rather than a
  real app screenshot.

### Unit-testing the comparator headlessly (no WebView2 needed)

```
cd gui-smoke
npm run test:unit
```

Runs `lib/compare.test.ts` under Node's built-in test runner via `tsx --test` — no `tauri build`, no
`tauri-driver`, no real window. It feeds `decodePng`/`comparePngBuffers` synthetic PNGs (built with
the companion `encodeRgbaPng` test/fixture helper): byte-identical images (→ zero diff), a
one-pixel-changed image (→ nonzero diff, ratio-threshold behaviour), a `pixelTolerance` case, a
different-size pair (→ a clear `sizeMismatch` flag, not a bogus pixel count), the two committed demo
baselines self-comparing to zero diff, and the full `compareSnapshotToBaseline` file-based flow
(missing baseline → advisory error; `GUI_SMOKE_UPDATE_BASELINE=1` bless; advisory drift; strict-mode
throw).

## Follow-ups (not this ticket — see CPE-1045's "Follow-ups" section)

- ~~**Linux CI leg**: add an `ubuntu-latest` matrix arm using `webkit2gtk-driver` + `xvfb-run` (no
  app code change needed).~~ **Done (CPE-1171)**, and **now the blocking gate (CPE-1594)** — see the
  "CI" section above.
- **Triage the ratchet's known-failing tail** (CPE-1595): `network.smoke.ts`'s selector was already
  fixed (stale `=text` link-text locator against a `<span>`, not a CPE-1516 regression — see
  `specs/network.smoke.ts`'s CPE-1594 comment), but it still fails live on WebKitGTK/Xvfb — same class
  of `.fav-title getText()` issue `saved-search.smoke.ts` is known-failing for; CPE-1595 has the working
  theory of a shared root cause. Also: `archive-browse`/`archive-password`/`shred-dialog`/
  `transfer-panel` (`samples`/`saved-search` are CPE-1507's).
- **macOS**: stays attended — `tauri-driver` has no WKWebView WebDriver support.
- **More flows**: Back/Up navigation, dialogs, context menus, tab switching.
- **Visual regression baselines for more surfaces**: CPE-1170 built the comparator + wired one worked
  example (`open-dir`); blessing real baselines (not the synthetic demo pair) for the other
  `snap()`'d surfaces in the table above needs an actual `tauri build` run (`GUI_SMOKE_UPDATE_BASELINE=1
  npm test`), which this ticket's headless sandbox couldn't perform.
