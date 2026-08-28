// CPE-1884 real-browser layout guard: asserts DropStackPanel.svelte's always-mounted
// `.drop-stack-handle` (fixed, bottom-left) never paints over an interactive Sidebar.svelte row, at
// any window height. This is exactly the bug this ticket fixed (Sidebar's Trash section and its
// always-last "Reset section order" row were silently unclickable behind the handle at ordinary
// window heights — see the ticket for the measured evidence) — this script is the regression guard
// for it.
//
// Deliberately NOT wired into CI yet: CPE-1882 owns generalising/CI-wiring this class of real-browser
// layout check (it explicitly plans to cover "no element in this row overlaps another" as a standing
// rule, reusing scripts/dev-harness/statusbar-notice/'s prototype). This script is written so CPE-1882
// can lift it directly — same "plain installed chrome.exe --headless=new + raw CDP" approach, no
// WebDriver (msedgedriver here is version-mismatched against the installed Edge and hangs — see
// CPE-1827/CPE-1866's evidence), no npm deps (Node's built-in fetch + WebSocket only, same spirit as
// sidecar/agent-board/clickthrough.mjs).
//
// It drives the REAL app (Sidebar.svelte + DropStackPanel.svelte, not a stand-in) against the
// project's own `vite` dev server, which it starts and tears down itself.
//
// Run:  node scripts/dev-harness/sidebar-drop-stack-overlap/check.mjs
//   or: npm run harness:sidebar-drop-stack-overlap
// Exit code 0 = the handle never paints over a sidebar row at any tested height; non-zero + details on
// stderr otherwise.

import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, "..", "..", "..");
const CHROME =
  process.env.CHROME_PATH || "C:/Program Files/Google/Chrome/Application/chrome.exe";
const DEV_PORT = Number(process.env.HARNESS_DEV_PORT || 4884);
const CDP_PORT_BASE = Number(process.env.HARNESS_CDP_PORT_BASE || 9500);
const WIDTH = 1000;
// A sweep from well below the shortest reasonable window down to a tall one — CPE-1884's own repro
// (CI job 97288403795) hit this at a 700px-tall window; the fix must hold everywhere, not just there.
const HEIGHTS = [420, 480, 540, 570, 600, 660, 700, 750, 800, 900, 1000];

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function waitForHttp(url, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(url);
      if (res.ok) return true;
    } catch {
      /* not up yet */
    }
    await sleep(150);
  }
  return false;
}

/** CPE-1967: every CDP call here used to be UNBOUNDED — `send()` returned a promise that only ever
 *  settled if Chrome answered, so one call that never got a response (a GC pause, a stalled internal
 *  navigation, a briefly unresponsive renderer) blocked the whole run silently and forever, with no
 *  information about where it was stuck. That is the same hole CPE-1882 found and fixed in
 *  `scripts/dev-harness/layout-guard/engine.mjs`, and this file was written as that engine's
 *  prototype, so it kept the pre-fix shape.
 *
 *  This is deliberately the SAME shape and the SAME 15000ms as `CDP_CALL_TIMEOUT_MS` in
 *  `layout-guard/engine.mjs` — one named constant, a per-call `{ timeoutMs }` override, and a
 *  rejection naming the method and the id — rather than a second idiom for the same problem. Keep it
 *  TIGHT: `Runtime.evaluate`/`Emulation.setDeviceMetricsOverride` taking anywhere near 15 seconds
 *  means something is genuinely wrong, and a loud named failure beats a silent wrong measurement.
 *
 *  Note what is NOT copied over. `engine.mjs` also carries `CDP_NAVIGATE_TIMEOUT_MS` (40s) for
 *  `Page.navigate`'s ack against a cold vite dev server; this script's own navigate is passed that
 *  same 40s explicitly at the call site rather than by importing a constant across two harnesses that
 *  are not otherwise coupled. See the call site for the measurement that number came from. */
const CDP_CALL_TIMEOUT_MS = 15000;

let nextId = 1;
function makeCdpClient(ws) {
  const pending = new Map();
  ws.addEventListener("message", (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      if (msg.error) reject(new Error(JSON.stringify(msg.error)));
      else resolve(msg.result);
    }
  });
  return {
    send(method, params = {}, { timeoutMs = CDP_CALL_TIMEOUT_MS } = {}) {
      const id = nextId++;
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
          pending.delete(id);
          reject(new Error(`CDP call "${method}" got no response within ${timeoutMs}ms (id=${id})`));
        }, timeoutMs);
        pending.set(id, {
          resolve: (v) => {
            clearTimeout(timer);
            resolve(v);
          },
          reject: (e) => {
            clearTimeout(timer);
            reject(e);
          },
        });
        ws.send(JSON.stringify({ id, method, params }));
      });
    },
  };
}

// The actual guarantee CPE-1884's fix provides: `.navigation-pane` gets `overflow-y: auto` from an
// `overflow: hidden` `.pane-col` parent, so NOTHING it contains can ever paint outside the pane's own
// border box, at ANY scroll position — the fix (`margin-bottom: 50px` on `.navigation-pane`) works by
// keeping that box's own bottom edge above the handle's top edge. Checking that one structural
// invariant (paneRect.bottom <= handleRect.top) is a complete proof for every row inside it, at every
// scroll position, without needing to reproduce a specific scroll offset or specific row content —
// which matters, because a specific row's on-load scroll position turned out to be environment-
// dependent (font metrics, real vs. dev-server content) and an easy way to accidentally test the wrong
// state. CPE-1884 found this out the hard way: an earlier version of this probe scrolled the pane to
// its max and checked one row's own click-center, which happened to land just outside the handle's
// range even with the bug reintroduced (real, but not the worst case) — this structural check is the
// version that cannot be fooled that way.
//
// A secondary, defense-in-depth check: at the page's natural (as-loaded, unscrolled-by-us) state, no
// row's own click-center is ACTUALLY painted over by the handle. This catches a different class of
// regression — some future fixed-position element placed OUTSIDE `.navigation-pane` entirely (so the
// structural containment check above wouldn't apply to it) landing in the same corner.
const HANDLE_CONTAINMENT_PROBE_JS = `
  (function () {
    const handle = document.querySelector('.drop-stack-handle');
    const pane = document.querySelector('.navigation-pane');
    const handleRect = handle.getBoundingClientRect();
    const paneRect = pane.getBoundingClientRect();
    const rows = Array.from(document.querySelectorAll('.navigation-pane .nav-item, .navigation-pane .sidebar-reset-btn'));
    const victims = [];
    for (const row of rows) {
      const r = row.getBoundingClientRect();
      if (r.width === 0 || r.height === 0) continue; // not actually painted right now — nothing to click
      const cx = r.left + r.width / 2;
      const cy = r.top + r.height / 2;
      // getBoundingClientRect ignores ancestor clipping — a row scrolled past the pane's own visible
      // edge still reports its laid-out geometry, so without this check a row that is simply clipped
      // out of view (nothing to do with the handle) reads as a false "victim" the moment its geometry
      // happens to fall in the handle's y-range. Only a row whose own click point is actually inside
      // the pane's visible box is a real candidate.
      if (cy < paneRect.top || cy > paneRect.bottom || cx < paneRect.left || cx > paneRect.right) continue;
      const el = document.elementFromPoint(cx, cy);
      const hitHandle = el && (el === handle || handle.contains(el));
      if (hitHandle) {
        victims.push({
          text: (row.textContent || "").trim().slice(0, 40),
          cls: row.className,
          rect: { left: r.left, top: r.top, right: r.right, bottom: r.bottom },
        });
      }
    }
    return {
      paneRect: { left: paneRect.left, top: paneRect.top, right: paneRect.right, bottom: paneRect.bottom },
      handleRect: { left: handleRect.left, top: handleRect.top, right: handleRect.right, bottom: handleRect.bottom },
      // 0.5px slack for subpixel layout rounding, not for the fix's real 50px margin.
      paneContained: paneRect.bottom <= handleRect.top + 0.5,
      rowCount: rows.length,
      naturalPositionVictims: victims,
    };
  })()
`;

async function checkOneHeight(height, cdpPort) {
  // Lives under the gitignored `.claude/` tree (never `os.tmpdir()` — repo convention keeps transient
  // run artifacts inside the project). A fresh dir every call: a reused profile disk-caches app.css
  // and silently masks a real CSS edit between runs (CPE-1884 hit this once verifying its own fix).
  const userDataDir = path.join(REPO_ROOT, ".claude", "dev-harness-chrome-profiles", `${cdpPort}-${Date.now()}`);
  const chrome = spawn(
    CHROME,
    [
      "--headless=new",
      "--disable-gpu",
      "--no-sandbox",
      "--disable-dev-shm-usage",
      `--remote-debugging-port=${cdpPort}`,
      `--user-data-dir=${userDataDir}`,
      `--window-size=${WIDTH},${height}`,
      "about:blank",
    ],
    { stdio: ["ignore", "ignore", "ignore"] },
  );
  try {
    const cdpUp = await waitForHttp(`http://127.0.0.1:${cdpPort}/json/version`, 15000);
    if (!cdpUp) throw new Error(`chrome CDP endpoint on ${cdpPort} never came up`);
    const targets = await (await fetch(`http://127.0.0.1:${cdpPort}/json/list`)).json();
    const target = targets.find((t) => t.type === "page") || targets[0];
    const ws = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise((resolve, reject) => {
      ws.addEventListener("open", resolve, { once: true });
      ws.addEventListener("error", reject, { once: true });
    });
    const client = makeCdpClient(ws);
    await client.send("Page.enable");
    await client.send("Emulation.setDeviceMetricsOverride", {
      width: WIDTH,
      height,
      deviceScaleFactor: 1,
      mobile: false,
    });
    // 40s rather than the default 15s, and it is the SAME exception `layout-guard/engine.mjs` makes
    // for the same call (`CDP_NAVIGATE_TIMEOUT_MS`, CPE-1914): the first `Page.navigate` of a run
    // lands on a freshly-launched Chrome talking to a cold vite dev server, and that ACK alone was
    // measured at ~18.65s on a loaded Windows dev machine — over the tight per-call cap, with nothing
    // actually wrong (that figure is CPE-1914's, recorded in `engine.mjs` beside its own constant;
    // quoted here as history, not re-measured for this file). It matches the 40s the poll below budgets
    // for the same cold-compile cause, rather than introducing a third number.
    await client.send("Page.navigate", { url: `http://localhost:${DEV_PORT}/` }, { timeoutMs: 40000 });

    let ready = false;
    // 40s, not 15s: on the very FIRST navigation of a freshly-started dev server, vite is compiling
    // the whole module graph (App.svelte alone is large) on demand — seen taking >15s cold in practice
    // even though the server's HTTP root already answered. Every height after the first reuses the now
    // -warm server and resolves in well under a second, so this only costs anything on that first hit.
    const deadline = Date.now() + 40000;
    while (Date.now() < deadline) {
      const r = await client.send("Runtime.evaluate", {
        expression: "!!document.querySelector('.navigation-pane') && !!document.querySelector('.drop-stack-handle')",
        returnByValue: true,
      });
      if (r.result && r.result.value === true) {
        ready = true;
        break;
      }
      await sleep(200);
    }
    if (!ready) throw new Error(`sidebar/handle never appeared within 40s at height=${height}`);
    await sleep(300); // settle layout/fonts

    const r = await client.send("Runtime.evaluate", {
      expression: HANDLE_CONTAINMENT_PROBE_JS,
      returnByValue: true,
    });
    ws.close();
    return r.result.value;
  } finally {
    chrome.kill();
  }
}

async function main() {
  console.log("[sidebar-drop-stack-overlap] starting vite dev server…");
  // `shell: true` on Windows: spawning the "npm"/"npm.cmd" shim directly (no shell) throws EINVAL —
  // npm.cmd is a batch file, not a native PE, and Node's spawn needs the shell to resolve/run it.
  const vite = spawn("npm", ["run", "dev", "--", "--port", String(DEV_PORT), "--strictPort"], {
    cwd: REPO_ROOT,
    stdio: ["ignore", "pipe", "pipe"],
    shell: true,
  });
  let viteFailed = false;
  vite.on("exit", (code) => {
    if (code !== null && code !== 0) viteFailed = true;
  });

  try {
    const devUp = await waitForHttp(`http://localhost:${DEV_PORT}/`, 20000);
    if (!devUp || viteFailed) throw new Error("vite dev server never came up");
    console.log(`[sidebar-drop-stack-overlap] dev server up on :${DEV_PORT}, sweeping ${HEIGHTS.length} heights…`);

    const failures = [];
    for (let i = 0; i < HEIGHTS.length; i++) {
      const height = HEIGHTS[i];
      const result = await checkOneHeight(height, CDP_PORT_BASE + i);
      const problems = [];
      if (!result.paneContained) {
        problems.push(
          `.navigation-pane's own box (bottom=${result.paneRect.bottom.toFixed(1)}) extends into the handle's y-range (top=${result.handleRect.top.toFixed(1)}) — some scroll position can reach it`,
        );
      }
      if (result.naturalPositionVictims.length > 0) {
        problems.push(
          `${result.naturalPositionVictims.length} row(s) painted over by the handle right now: ${JSON.stringify(result.naturalPositionVictims)}`,
        );
      }
      if (problems.length > 0) {
        failures.push({ height, problems });
        console.error(`[sidebar-drop-stack-overlap] h=${height}: FAIL — ${problems.join("; ")}`);
      } else {
        console.log(`[sidebar-drop-stack-overlap] h=${height}: OK (${result.rowCount} rows checked, pane contained above the handle)`);
      }
    }

    if (failures.length > 0) {
      console.error(
        `\n[sidebar-drop-stack-overlap] FAIL — the Drop Stack handle paints over sidebar content at ${failures.length}/${HEIGHTS.length} height(s).`,
      );
      process.exitCode = 1;
    } else {
      console.log(`\n[sidebar-drop-stack-overlap] PASS — clean at all ${HEIGHTS.length} heights.`);
    }
  } finally {
    // `shell: true` means `vite`'s pid is the shell (cmd.exe), not the node process it launches —
    // `vite.kill()` alone leaves the real dev server running and orphaned on Windows. `taskkill /T`
    // kills the whole process tree; POSIX doesn't need this; the fallback keeps a non-Windows dev
    // machine working.
    if (process.platform === "win32") {
      spawn("taskkill", ["/pid", String(vite.pid), "/T", "/F"], { stdio: "ignore" });
    } else {
      vite.kill();
    }
  }
}

main().catch((e) => {
  console.error("[sidebar-drop-stack-overlap] FAIL:", e);
  process.exit(1);
});
