// CPE-1968 — before/after screenshots of the Organize dialog, for a Visual Critic pass.
//
// WHY THIS EXISTS AND NOT `gui-smoke`. The real GUI smoke harness drives the BUILT app, which needs a
// `tauri build` first. This does not: it renders the REAL component (../organize-dialog/index.html,
// mounted off the shared layout-guard dev server) in the installed Chrome at gui-smoke's own 1000x700
// viewport. So it is a real layout engine on the real CSS — but it is NOT the built app, and it is not
// wry's webview. Anything that depends on the app's own window chrome, on the folder view behind the
// dialog, or on a WebView2/WebKit layout difference is outside what these shots can show. Say that
// when you hand them over.
//
// It also does not commit the images. The generator is committed (CLAUDE.md: "if you cannot commit the
// generator, you have not measured anything a reviewer can check") and writes PNGs to a gitignored
// scratch directory, so anyone can regenerate them rather than trust a stale binary in the tree.
//
// Approach: plain `chrome.exe --headless=new --screenshot`, no CDP and no npm dependency. The
// layout-guard engine's CDP client would also work, but nothing here needs to drive the page — every
// state is reachable from a URL, and each shot's own measurement is rendered INTO the page by
// main.ts's probe badge, so the pictures carry their numbers with them.
//
// Run:  node scripts/dev-harness/organize-dialog/shots.mjs
//   or: npm run harness:organize-dialog-shots
// Override the browser with CHROME_PATH, the output directory with SHOTS_OUT.

import { spawn } from "node:child_process";
import { mkdir, readdir } from "node:fs/promises";
import path from "node:path";
import { defaultChromePath, REPO_ROOT } from "../layout-guard/engine.mjs";
import { harnessPortFor, startDevServer, stopDevServer } from "../layout-guard/dev-server.mjs";

const CHROME = process.env.CHROME_PATH || defaultChromePath();
// Pid-derived, shared with layout-guard/run.mjs and for the same reason: several worktrees routinely
// run harnesses on this machine at once, and a fixed port means one run silently measures another
// worktree's pages. See dev-server.mjs's header.
const DEV_PORT = Number(process.env.HARNESS_DEV_PORT || harnessPortFor());
const OUT_DIR = process.env.SHOTS_OUT || path.join(REPO_ROOT, ".claude", "dev-harness-shots", "organize-dialog");

// gui-smoke's window: `src-tauri/src/lib.rs`'s `.inner_size(1000.0, 700.0)`, restored by
// gui-smoke/lib/resetAppState.ts after any spec that resizes. Every px figure in CPE-1965 and
// CPE-1968 is quoted at this viewport, so the shots are taken at it too — but NOT by passing it to
// `--window-size`, which is a WINDOW size, not a viewport. Measured on this script's first run:
// `--window-size=1000,700` produced a 984x549 document viewport (the probe badge reported it), which
// silently resolved `45vh` against 549px and understated the pills' shift as 63.5px instead of ~98px.
// shell.html pins the inner viewport with an iframe instead; the window only has to be big enough to
// hold it.
const WIDTH = 1040;
const HEIGHT = 900;

/** A very long delay holds `organize_plan` in flight, which is the only way to photograph the
 *  loading state. 120ms (the component's own debounce) lets the plan land normally. */
const HELD = 600000;

const STATES = [
  { name: "loading", plan: "two", delay: HELD, note: "first organize_plan still in flight" },
  { name: "plan-two-files", plan: "two", delay: 120, note: "a two-file plan — the roomy-box case" },
  { name: "plan-large", plan: "large", delay: 120, note: "26 files over 5 groups — an ordinary Downloads folder" },
];

/** One `chrome --headless=new --screenshot` run. Resolves on exit; rejects on a non-zero code. */
function shoot(url, outFile, userDataDir) {
  return new Promise((resolve, reject) => {
    const chrome = spawn(
      CHROME,
      [
        "--headless=new",
        "--disable-gpu",
        "--no-sandbox",
        "--disable-dev-shm-usage",
        "--hide-scrollbars",
        `--user-data-dir=${userDataDir}`,
        `--window-size=${WIDTH},${HEIGHT}`,
        // The probe badge samples at t=100ms and the debounce fires at 120ms, so the page needs to be
        // alive for well over both before the shot is taken.
        "--virtual-time-budget=3000",
        `--screenshot=${outFile}`,
        url,
      ],
      { stdio: ["ignore", "ignore", "ignore"] },
    );
    chrome.on("error", reject);
    chrome.on("exit", (code) => (code === 0 ? resolve() : reject(new Error(`chrome exited ${code} for ${url}`))));
  });
}

async function main() {
  await mkdir(OUT_DIR, { recursive: true });
  const profileRoot = path.join(REPO_ROOT, ".claude", "dev-harness-chrome-profiles", `organize-shots-${process.pid}`);
  await mkdir(profileRoot, { recursive: true });

  const vite = await startDevServer(DEV_PORT);

  try {
    const base = `http://localhost:${DEV_PORT}`;

    for (const variant of [
      { key: "before", legacy: 1 },
      { key: "after", legacy: 0 },
    ]) {
      for (const state of STATES) {
        const url =
          `${base}/scripts/dev-harness/organize-dialog/shell.html` +
          `?plan=${state.plan}&delay=${state.delay}&legacy=${variant.legacy}`;
        const out = path.join(OUT_DIR, `${variant.key}-${state.name}.png`);
        await shoot(url, out, path.join(profileRoot, `${variant.key}-${state.name}`));
        console.log(`${variant.key.padEnd(6)} ${state.name.padEnd(15)} -> ${out}   (${state.note})`);
      }
    }

    const written = (await readdir(OUT_DIR)).filter((f) => f.endsWith(".png"));
    console.log(`\n${written.length} screenshots in ${OUT_DIR}`);
    console.log(
      "Each shot's top-left badge carries its own measurement: the `.rules` row's screen position at " +
        "t=100ms, its position now, and the difference. A `before` shot's loading/settled pair is the " +
        "swallowed click; an `after` pair reads 0.0px.",
    );
  } finally {
    stopDevServer(vite);
  }
}

main().catch((e) => {
  console.error(e.message ?? e);
  process.exitCode = 1;
});
