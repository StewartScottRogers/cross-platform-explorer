// wdio.conf.ts — CPE-1045 headless GUI smoke test harness.
//
// Drives the REAL built cross-platform-explorer.exe through tauri-driver + WebdriverIO and asserts
// `--open <dir>` (CPE-1043, fixed by CPE-1044) actually navigated the explorer into that folder. This
// file is deliberately isolated in gui-smoke/ with its own package.json/lockfile/tsconfig — it never
// touches the app's package.json, src-tauri, or the root vitest suite.
//
// Prereqs (see README.md): `cargo install tauri-driver --locked`, a matching msedgedriver on PATH
// (Windows), and — CRITICAL — a REAL release build via the Tauri CLI's `build` subcommand:
//     npm run build && npm run tauri build -- --no-bundle
//
// --- Foreground disruption on an interactive machine (see README.md "Running this locally") -----
// This launches a REAL, visible, focus-stealing window — there is currently no way to avoid that.
// `--x`/`--y` off-screen placement was tried and rejected: this app's own CPE-600 geometry code
// deliberately clamps the window fully onto the monitor ("off-screen protection... never
// ungrabbable", crates/server/src/geometry.rs `resolve()`), so an off-screen `--x`/`--y` is
// silently pulled back on-screen — it is not a usable escape hatch, not a bug to route around here.
// A real fix (a non-activating/off-screen/`--test-mode` launch) is tracked as a CPE-1046 follow-up
// on the app side; until that lands, do not run this suite on a machine someone is actively using —
// CI (windows-latest, no interactive user) is the intended place to run it.
import { spawn, type ChildProcess } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// --- CPE-1044 guard --------------------------------------------------------------------------
// Tauri decides "embed frontendDist" vs "load devUrl" (localhost:1420) at compile time based on
// whether the invocation went through the Tauri CLI's `build` subcommand — NOT on the cargo
// profile and NOT on `--no-bundle`. A raw `cargo build` (bypassing the Tauri CLI entirely) falls
// back to the dev server, which is exactly the CPE-1044 bug (`--open` landed on Home because the
// binary never had the real frontend embedded). So this harness MUST point at a binary produced
// by `npm run tauri build` (the CLI's build subcommand) — never a bare `cargo build` output.
//
// We build with `--no-bundle` in CI to skip installer packaging + updater-artifact signing (which
// requires TAURI_SIGNING_PRIVATE_KEY secrets this job intentionally does not carry) — that flag
// only skips *packaging*, it does not change the dev-vs-build cfg decision above, so the compiled
// exe still embeds the real frontend. `--debug`, on the other hand, is NOT used here: keep this
// pointed at a genuine release-profile CLI build so the smoke test matches what ships.
const APP_BINARY = path.resolve(
  __dirname,
  "..",
  "src-tauri",
  "target",
  "release",
  process.platform === "win32" ? "cross-platform-explorer.exe" : "cross-platform-explorer",
);

if (!fs.existsSync(APP_BINARY)) {
  throw new Error(
    `[gui-smoke] release binary not found at:\n  ${APP_BINARY}\n\n` +
      "Build it first with a REAL Tauri CLI build (never a bare `cargo build`, and never `--debug` —\n" +
      "see the CPE-1044 comment above):\n" +
      "  npm run build && npm run tauri build -- --no-bundle\n",
  );
}

const TAURI_DRIVER_BIN = path.resolve(
  os.homedir(),
  ".cargo",
  "bin",
  process.platform === "win32" ? "tauri-driver.exe" : "tauri-driver",
);

// The temp dir + marker file (read by specs/open-dir.smoke.ts) are seeded once, in the main
// process, before any worker/session starts — and handed off via a small state file rather than
// relying on env-var inheritance into the (possibly forked) worker process.
export const STATE_FILE = path.resolve(__dirname, ".smoke-state.json");
export const MARKER_NAME = "CPE-1045-marker.txt";

// CPE-1096: a tiny, known source file seeded into the same temp dir so the smoke suite can select
// it in the file list and assert the code-preview's code-intelligence UI (outline strip + per-line
// rows + minimap, CPE-1090/1091) actually renders — burning down the manual visual-verification
// debt those two tickets shipped with (headless-only coverage until now). Deliberately small but
// non-trivial: a struct + two functions (>=1 outline pill each) with multi-line bodies (foldable
// blocks), so `code_intel::analyze` (crates/server/src/code_intel.rs) populates outline, folds, and
// minimap — the minimap in particular is populated for ANY non-empty text (see
// `minimap::minimap_rows`), so asserting on it is safe, not just best-effort.
export const FIXTURE_NAME = "CPE-1096-fixture.rs";
const FIXTURE_SOURCE = `// CPE-1096 gui-smoke fixture — exercises the code-preview outline/rows/minimap.
pub struct Widget {
    pub name: String,
}

pub fn greet(name: &str) -> String {
    let msg = format!("Hello, {name}!");
    msg
}

pub fn describe(widget: &Widget) -> String {
    if widget.name.is_empty() {
        "unnamed".to_string()
    } else {
        widget.name.clone()
    }
}
`;

let tauriDriver: ChildProcess | undefined;
let shuttingDown = false;

function killTauriDriver() {
  shuttingDown = true;
  tauriDriver?.kill();
}

export const config: WebdriverIO.Config = {
  runner: "local",
  hostname: "127.0.0.1",
  port: 4444,
  path: "/",
  specs: ["./specs/**/*.smoke.ts"],
  maxInstances: 1,
  capabilities: [
    {
      // "wry" is Tauri's own webview wrapper (WebView2 on Windows / WebKitGTK on Linux) — not a
      // real browser name.
      browserName: "wry",
      // WebdriverIO v9 auto-upgrades to the WebDriver BiDi protocol whenever the remote end
      // advertises support. Modern msedgedriver does advertise it, but its BiDi
      // `browsingContext` model does not reliably attach to wry's embedded WebView2 control (in
      // testing it stays on the driver's own "about:blank" placeholder context forever, so every
      // DOM query returns empty — the app itself is running and navigating fine, WebdriverIO is
      // just looking at the wrong context). Forcing classic WebDriver (HTTP/JSON) sidesteps that
      // and talks to the actual webview.
      "wdio:enforceWebDriverClassic": true,
      "tauri:options": {
        application: APP_BINARY,
        // Populated in onPrepare once the temp dir exists (see below).
        //
        // tauri-driver itself DOES forward `args` into the native driver's launch options
        // (`ms:edgeOptions.args` on Windows / `webkitgtk:browserOptions.args` on Linux — verified
        // by reading tauri-driver's own source, crates/tauri-driver/src/server.rs
        // `TauriOptions { application, args }`). BUT empirically (verified here with a direct
        // WebDriver `/session` POST + inspecting the spawned process's real command line via
        // `Get-CimInstance Win32_Process`), msedgedriver's OWN arg handling silently DROPS a bare
        // positional token that doesn't look like a `--switch`: `args: ['--open', tmpDir]` arrived
        // at the app as just `--open` with the path gone (msedgedriver logs "Ignoring switch with
        // invalid name: <path>" when this happens — that log line is not just a warning, the arg is
        // actually discarded). The fix: encode it as ONE Chromium-style `--switch=value` token —
        // `--open=<tmpDir>` — which msedgedriver preserves verbatim (including the space in this
        // machine's own username, confirmed via the same process-inspection probe). So do NOT split
        // `--open` and its value into two array entries.
        args: [] as string[],
        // CPE-1048: a Tauri maintainer reported (tauri discussion #10122) that adding an empty
        // `webviewOptions: {}` here — with a recent tauri-driver — resolved a Windows "session not
        // created" failure; it nudges the driver into proper WebView2 mode. Cheap, zero-risk,
        // shipped alongside the WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS env var in gui-smoke.yml
        // (the actual DevToolsActivePort fix) rather than instead of it.
        webviewOptions: {},
      },
    } as WebdriverIO.Capabilities,
  ],
  logLevel: "info",
  framework: "mocha",
  // CPE-1048: modest bump for a slow first WebView2 cold start in CI (no GPU / restricted session)
  // — not a fix for a hard sandbox/GPU crash on its own, just headroom so a slow-but-successful
  // session isn't misread as a failure.
  connectionRetryTimeout: 180_000,
  reporters: ["spec"],
  mochaOpts: {
    ui: "bdd",
    timeout: 90_000,
  },

  // Seed the temp dir + marker file BEFORE any session starts, and wire the launch args (capability
  // `tauri:options.args`) so the app opens off-screen and non-focused (CPE-1046/1047 — can't grab a
  // CI display) directly into the seeded dir via `--open=<dir>` — the exact path CPE-1043 shipped and
  // CPE-1044 fixed (see the single-token comment above for why it's one `--open=<dir>` token here
  // rather than the two-token `--open <dir>` form a human would type). Negative geometry MUST use the
  // `=` form too — `--x -4000` fails clap parsing, `--x=-4000` doesn't. Only real app flags
  // (clap-parsed) go here — Chromium browser flags belong in the WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
  // env var (gui-smoke.yml), never in this array.
  onPrepare: (_config, capabilities) => {
    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "cpe-gui-smoke-"));
    fs.writeFileSync(path.join(tmpDir, MARKER_NAME), "CPE-1045 smoke marker\n", "utf-8");
    // CPE-1096: seed the code-preview fixture alongside the marker, in the same temp dir, so the
    // suite can open it in the same session without a second app launch.
    fs.writeFileSync(path.join(tmpDir, FIXTURE_NAME), FIXTURE_SOURCE, "utf-8");

    const caps = capabilities as unknown as Array<{ "tauri:options": { args: string[] } }>;
    caps[0]["tauri:options"].args = ["--test-mode", "--x=-4000", `--open=${tmpDir}`];

    fs.writeFileSync(STATE_FILE, JSON.stringify({ tmpDir }), "utf-8");
  },

  // tauri-driver proxies WebDriver requests to the platform's native driver (msedgedriver on
  // Windows, WebKitWebDriver on Linux), which is what actually launches the app binary.
  beforeSession: () => {
    tauriDriver = spawn(TAURI_DRIVER_BIN, [], {
      stdio: [null, process.stdout, process.stderr],
    });
    tauriDriver.on("error", (error) => {
      console.error("[gui-smoke] tauri-driver error:", error);
      process.exit(1);
    });
    tauriDriver.on("exit", (code) => {
      if (!shuttingDown) {
        console.error("[gui-smoke] tauri-driver exited early with code:", code);
        process.exit(1);
      }
    });
  },

  // Note: afterSession might not run if the session fails to start, so onComplete (below) also
  // kills tauri-driver — same belt-and-braces pattern as the official Tauri WebDriver example.
  afterSession: () => {
    killTauriDriver();
  },

  onComplete: () => {
    killTauriDriver();
    try {
      const { tmpDir } = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
      fs.rmSync(tmpDir, { recursive: true, force: true });
    } catch {
      // best effort — nothing to clean up if the state file was never written (e.g. onPrepare
      // itself threw before reaching that point).
    }
    fs.rmSync(STATE_FILE, { force: true });
  },
};

for (const sig of ["exit", "SIGINT", "SIGTERM", "SIGHUP"] as const) {
  process.on(sig, killTauriDriver);
}
