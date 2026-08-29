// CPE-1983 — before/after screenshots of the Checkpoint dialog, for a Visual Critic pass.
//
// Directly modelled on scripts/dev-harness/organize-dialog/shots.mjs (CPE-1968); everything that
// file's header says about scope applies here unchanged and is not restated: it renders the REAL
// component in the installed Chrome at gui-smoke's own 1000x700 viewport, so it is a real layout
// engine on the real CSS — but it is NOT the built app and not wry's webview. Anything that depends
// on the app's window chrome, on the folder view behind the dialog, or on a WebView2/WebKit layout
// difference is outside what these shots can show. Say that when you hand them over.
//
// It also does not commit the images. The generator is committed (CLAUDE.md: "if you cannot commit
// the generator, you have not measured anything a reviewer can check") and writes PNGs to a
// gitignored scratch directory.
//
// Run:  node scripts/dev-harness/checkpoint-dialog/shots.mjs
//   or: npm run harness:checkpoint-dialog-shots
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
const OUT_DIR = process.env.SHOTS_OUT || path.join(REPO_ROOT, ".claude", "dev-harness-shots", "checkpoint-dialog");

// The window only has to be big enough to hold shell.html's 1000x700 iframe; the iframe is what pins
// the viewport (see shell.html).
const WIDTH = 1040;
const HEIGHT = 900;

/** A very long delay holds `checkpointList` in flight, which is the only way to photograph the
 *  loading state. */
const HELD = 600000;

/**
 * The settled states' delay, and it is NOT 0 — measured, because 0 silently made the probe useless.
 * The badge samples Refresh's position at t=100ms and compares it with where Refresh is now; with an
 * immediate resolve, BOTH samples are post-load and every `before` shot reported `Refresh moved
 * 0.0px`, which reads exactly like the fixed build. 500ms puts the t=100ms sample genuinely inside
 * the loading window while still landing long before the 3000ms virtual-time budget ends.
 */
const SETTLE = 500;

// Every state in which `.list` renders something DIFFERENT, so none of them is reviewed only in the
// abstract. CPE-1968's harness shipped a fixture its STATES list never used, so the empty state went
// unphotographed and the Visual Critic then found the unlooked-at frame was the weakest one. Every
// key of LISTS in main.ts appears here.
const STATES = [
  { name: "loading", list: "few", delay: HELD, note: "checkpointList still in flight" },
  { name: "empty", list: "none", delay: SETTLE, note: "no checkpoints yet — the roomiest empty box" },
  { name: "few", list: "few", delay: SETTLE, note: "two checkpoints — the 'is this box absurdly empty?' case" },
  { name: "many", list: "many", delay: SETTLE, note: "12 checkpoints — overflows the box, which is what it is for" },
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
        // The probe badge samples at t=100ms, so the page needs to be alive well past that.
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
  const profileRoot = path.join(REPO_ROOT, ".claude", "dev-harness-chrome-profiles", `checkpoint-shots-${process.pid}`);
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
          `${base}/scripts/dev-harness/checkpoint-dialog/shell.html` +
          `?list=${state.list}&delay=${state.delay}&legacy=${variant.legacy}`;
        const out = path.join(OUT_DIR, `${variant.key}-${state.name}.png`);
        await shoot(url, out, path.join(profileRoot, `${variant.key}-${state.name}`));
        console.log(`${variant.key.padEnd(6)} ${state.name.padEnd(9)} -> ${out}   (${state.note})`);
      }
    }

    const written = (await readdir(OUT_DIR)).filter((f) => f.endsWith(".png"));
    console.log(`\n${written.length} screenshots in ${OUT_DIR}`);
    console.log(
      "Each shot's top-left badge carries its own measurement: Refresh's screen position at t=100ms, " +
        "its position now, the difference, and — the hit-test — whether the point a pointer held over " +
        "Refresh is now inside `.list`, the box that carries `Revert…`. A `before` loading/settled " +
        "pair is the destructive mis-click; an `after` pair reads 0.0px.",
    );
  } finally {
    stopDevServer(vite);
  }
}

main().catch((e) => {
  console.error(e.message ?? e);
  process.exitCode = 1;
});
