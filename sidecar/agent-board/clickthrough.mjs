// CPE-1168 — headless click-through for the standalone agent-board sidecar UI.
//
// Retires MANUAL-TEST-BURNDOWN.md row #9 (the agent-board sidecar UI click-through). The sidecar
// serves a loopback HTTP Kanban UI (sidecar/agent-board/src/ui.rs) once it reaches `Ready`, so this
// harness can drive it end-to-end with NO user, NO creds, and NO network beyond loopback:
//
//   1. Launch the built agent-board sidecar (`target/release/agent-board[.exe]`), pointed at this
//      repo's real `Ticketing/` via CPE_BOARD_ROOT so all three views have real content.
//   2. Speak just enough of the ADR-0001 stdio contract to reach `Ready`: the sidecar emits `Hello`,
//      we reply with a `Welcome` envelope (sidecar/contract/src/lib.rs), and it announces its UI URL
//      as a `Status { state: "ui:http://127.0.0.1:<port>" }` event on stdout.
//   3. Drive a HEADLESS Edge (msedgedriver, raw WebDriver HTTP — no WebdriverIO/tauri-driver, no npm
//      install) at that loopback URL, click each top-level view (Board / Epics / Sprints), and assert
//      the rendered list actually swaps per view (the right container shows, the others hide, and the
//      list content is the shape that view serves).
//   4. Snap a screenshot per view into `.screenshots/` (gitignored), then tear the browser + sidecar
//      down in a `finally`.
//
// Deliberately dependency-free: uses only Node's built-in `fetch`/`child_process`. Prereqs:
//   - `msedgedriver` on PATH (or via MSEDGEDRIVER env) + a matching Edge install (this repo keeps
//     msedgedriver in ~/.cargo/bin, same as gui-smoke).
//   - the sidecar built first:  cargo build --release   (in sidecar/agent-board)
//
// Run:  node sidecar/agent-board/clickthrough.mjs
// Exit code 0 = all three views drove and swapped; non-zero = a failure (message on stderr).

import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import readline from "node:readline";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const IS_WIN = process.platform === "win32";

// --- config -----------------------------------------------------------------------------------
const REPO_ROOT = path.resolve(__dirname, "..", ".."); // sidecar/agent-board -> repo root (has Ticketing/)
const SIDECAR_BIN = path.resolve(
  __dirname,
  "target",
  "release",
  IS_WIN ? "agent-board.exe" : "agent-board",
);
const MSEDGEDRIVER =
  process.env.MSEDGEDRIVER ||
  path.resolve(process.env.HOME || process.env.USERPROFILE || ".", ".cargo", "bin", IS_WIN ? "msedgedriver.exe" : "msedgedriver");
const DRIVER_PORT = Number(process.env.DRIVER_PORT || 9736);
const SCREENSHOTS_DIR = path.resolve(__dirname, ".screenshots");
const READY_TIMEOUT_MS = 20_000;
const DRIVER_TIMEOUT_MS = 20_000;

// The Welcome envelope that pushes the sidecar from Hello -> Ready (matches sidecar/contract).
const WELCOME = JSON.stringify({
  schema_version: 1,
  id: 0,
  message: { type: "welcome", negotiated_version: { major: 1, minor: 2 }, capabilities_granted: ["context"] },
});

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function log(...a) {
  console.log("[agent-board-clickthrough]", ...a);
}

// --- 1 + 2: launch the sidecar and read its announced loopback UI URL --------------------------
async function launchSidecarAndGetUrl() {
  if (!fs.existsSync(SIDECAR_BIN)) {
    throw new Error(
      `sidecar binary not found:\n  ${SIDECAR_BIN}\n\nBuild it first:\n  (cd sidecar/agent-board && cargo build --release)`,
    );
  }
  log("launching sidecar:", SIDECAR_BIN);
  const child = spawn(SIDECAR_BIN, [], {
    cwd: REPO_ROOT,
    env: { ...process.env, CPE_BOARD_ROOT: REPO_ROOT },
    stdio: ["pipe", "pipe", "inherit"],
  });

  const url = await new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error(`sidecar never announced a UI URL within ${READY_TIMEOUT_MS}ms`)),
      READY_TIMEOUT_MS,
    );
    let sentWelcome = false;
    const rl = readline.createInterface({ input: child.stdout });
    rl.on("line", (line) => {
      line = line.trim();
      if (!line) return;
      let env;
      try {
        env = JSON.parse(line);
      } catch {
        return;
      }
      const msg = env.message;
      if (!msg) return;
      // Sidecar's opening Hello -> reply with Welcome so it reaches Ready and serves its UI.
      if (msg.type === "hello" && !sentWelcome) {
        sentWelcome = true;
        child.stdin.write(WELCOME + "\n");
        return;
      }
      // The Ready side effect: a Status event announcing "ui:<loopback url>".
      if (msg.type === "event" && msg.event === "status" && typeof msg.state === "string" && msg.state.startsWith("ui:")) {
        const u = msg.state.slice("ui:".length);
        clearTimeout(timer);
        rl.close();
        resolve(u);
      }
    });
    child.on("exit", (code) => {
      clearTimeout(timer);
      reject(new Error(`sidecar exited early (code ${code}) before announcing its UI`));
    });
    child.on("error", reject);
  });

  log("sidecar UI at", url);
  return { child, url };
}

// --- 3: minimal raw-WebDriver client against msedgedriver -------------------------------------
function makeDriver() {
  const base = `http://127.0.0.1:${DRIVER_PORT}`;
  let sessionId = null;

  async function call(method, endpoint, body) {
    const res = await fetch(base + endpoint, {
      method,
      headers: { "Content-Type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const text = await res.text();
    let json;
    try {
      json = text ? JSON.parse(text) : {};
    } catch {
      json = { raw: text };
    }
    if (!res.ok) {
      const err = json?.value?.message || text || `HTTP ${res.status}`;
      throw new Error(`WebDriver ${method} ${endpoint} failed: ${err}`);
    }
    return json.value;
  }

  return {
    async waitReady() {
      const deadline = Date.now() + DRIVER_TIMEOUT_MS;
      // last error kept for a useful message if we time out
      let last;
      while (Date.now() < deadline) {
        try {
          const v = await call("GET", "/status");
          if (v?.ready) return;
        } catch (e) {
          last = e;
        }
        await sleep(200);
      }
      throw new Error(`msedgedriver not ready within ${DRIVER_TIMEOUT_MS}ms${last ? `: ${last.message}` : ""}`);
    },
    async newSession() {
      const v = await call("POST", "/session", {
        capabilities: {
          alwaysMatch: {
            browserName: "MicrosoftEdge",
            "ms:edgeOptions": {
              args: ["--headless=new", "--disable-gpu", "--no-sandbox", "--disable-dev-shm-usage", "--window-size=1280,900"],
            },
          },
        },
      });
      sessionId = v.sessionId;
    },
    async navigate(url) {
      await call("POST", `/session/${sessionId}/url`, { url });
    },
    // Run JS in the page and return its (JSON) value. Uses execute/sync.
    async execute(script, args = []) {
      return call("POST", `/session/${sessionId}/execute/sync`, { script, args });
    },
    async clickBySelector(css) {
      const el = await call("POST", `/session/${sessionId}/element`, { using: "css selector", value: css });
      const id = el[Object.keys(el)[0]]; // element-id key is the W3C GUID
      await call("POST", `/session/${sessionId}/element/${id}/click`, {});
    },
    async screenshot(name) {
      try {
        const b64 = await call("GET", `/session/${sessionId}/screenshot`);
        fs.mkdirSync(SCREENSHOTS_DIR, { recursive: true });
        fs.writeFileSync(path.join(SCREENSHOTS_DIR, `${name}.png`), Buffer.from(b64, "base64"));
      } catch (e) {
        log(`screenshot("${name}") failed (non-fatal):`, e.message);
      }
    },
    async quit() {
      if (sessionId) {
        try {
          await call("DELETE", `/session/${sessionId}`);
        } catch {
          /* best effort */
        }
        sessionId = null;
      }
    },
  };
}

// A single JS snapshot of the board's three view containers + the active button (see ui.rs DOM).
//
// NOTE on `*Shown` vs `*Hidden`: we assert on REAL computed visibility (getComputedStyle().display
// !== 'none'), NOT merely the `hidden` DOM property. `switchView` toggles the `hidden` attribute, but
// the containers carry author CSS (`.cols`/`.list { display:flex }`) which — being an author rule —
// overrides the UA `[hidden]{display:none}` rule, so a naive `.hidden`-property check would report a
// swap that did not visually happen. This distinction is exactly the CPE-1168 bug this harness
// caught + pinned: the view "swap" left the Kanban columns still on screen. (see ui.rs `[hidden]`.)
const SNAPSHOT_JS = `
  const cols = document.getElementById('cols');
  const epics = document.getElementById('epicsList');
  const sprints = document.getElementById('sprintsList');
  const active = document.querySelector('.viewbtn.active');
  const shown = (el) => getComputedStyle(el).display !== 'none';
  return {
    colsShown: shown(cols),
    epicsShown: shown(epics),
    sprintsShown: shown(sprints),
    colCount: cols.querySelectorAll('.col').length,
    cardCount: cols.querySelectorAll('.card').length,
    epicRows: epics.querySelectorAll('.row').length,
    sprintRows: sprints.querySelectorAll('.row').length,
    epicsEmpty: !!epics.querySelector('.empty'),
    sprintsEmpty: !!sprints.querySelector('.empty'),
    activeView: active ? active.dataset.view : null,
  };
`;

// Poll the snapshot until `pred` holds (the view's fetch has populated the list) or we time out.
async function waitFor(driver, pred, label, timeoutMs = 6000) {
  const deadline = Date.now() + timeoutMs;
  let snap;
  while (Date.now() < deadline) {
    snap = await driver.execute(SNAPSHOT_JS);
    if (pred(snap)) return snap;
    await sleep(150);
  }
  throw new Error(`timed out waiting for ${label}; last snapshot: ${JSON.stringify(snap)}`);
}

function assert(cond, msg) {
  if (!cond) throw new Error("ASSERT FAILED: " + msg);
}

// --- main --------------------------------------------------------------------------------------
async function main() {
  let sidecar;
  const driver = makeDriver();
  let driverProc;
  try {
    ({ child: sidecar, url: driver.url } = await launchSidecarAndGetUrl());
    const url = driver.url;

    log("spawning msedgedriver:", MSEDGEDRIVER, `--port=${DRIVER_PORT}`);
    driverProc = spawn(MSEDGEDRIVER, [`--port=${DRIVER_PORT}`], { stdio: ["ignore", "ignore", "inherit"] });
    driverProc.on("error", (e) => {
      throw new Error(`failed to spawn msedgedriver (${MSEDGEDRIVER}): ${e.message}`);
    });

    await driver.waitReady();
    await driver.newSession();
    await driver.navigate(url);

    // --- Board (default view) — Kanban columns with real cards -------------------------------
    const board = await waitFor(
      driver,
      (s) => s.colsShown && s.colCount >= 1,
      "Board view to render its Kanban columns",
    );
    assert(board.activeView === "board", `Board button active (got ${board.activeView})`);
    assert(board.colsShown === true, "Board columns visible");
    assert(board.epicsShown === false && board.sprintsShown === false, "only Board visible on load");
    assert(board.colCount === 5, `Board renders its 5 status columns (got ${board.colCount})`);
    log(`Board OK — ${board.colCount} columns, ${board.cardCount} cards`);
    await driver.screenshot("agent-board-board");

    // --- Epics view — the list swaps to epic rows --------------------------------------------
    await driver.clickBySelector("#viewepics");
    const epics = await waitFor(
      driver,
      (s) => s.activeView === "epics" && s.epicsShown && (s.epicRows > 0 || s.epicsEmpty),
      "Epics view to load its list",
    );
    assert(epics.colsShown === false, "Board columns hidden in Epics view");
    assert(epics.epicsShown === true, "Epics list visible in Epics view");
    assert(epics.sprintsShown === false, "Sprints list hidden in Epics view");
    assert(epics.epicRows > 0 || epics.epicsEmpty, "Epics list rendered (rows or explicit empty-state)");
    log(`Epics OK — ${epics.epicRows} epic rows${epics.epicsEmpty ? " (empty-state)" : ""}`);
    await driver.screenshot("agent-board-epics");

    // --- Sprints view — the list swaps again -------------------------------------------------
    await driver.clickBySelector("#viewsprints");
    const sprints = await waitFor(
      driver,
      (s) => s.activeView === "sprints" && s.sprintsShown && (s.sprintRows > 0 || s.sprintsEmpty),
      "Sprints view to load its list",
    );
    assert(sprints.colsShown === false, "Board columns hidden in Sprints view");
    assert(sprints.epicsShown === false, "Epics list hidden in Sprints view");
    assert(sprints.sprintsShown === true, "Sprints list visible in Sprints view");
    assert(sprints.sprintRows > 0 || sprints.sprintsEmpty, "Sprints list rendered (rows or explicit empty-state)");
    log(`Sprints OK — ${sprints.sprintRows} sprint rows${sprints.sprintsEmpty ? " (empty-state)" : ""}`);
    await driver.screenshot("agent-board-sprints");

    // --- back to Board — the swap is reversible ----------------------------------------------
    await driver.clickBySelector("#viewboard");
    const backToBoard = await waitFor(
      driver,
      (s) => s.activeView === "board" && s.colsShown,
      "Board view to restore",
    );
    assert(backToBoard.epicsShown === false && backToBoard.sprintsShown === false, "lists hidden back on Board");

    log("PASS — all three views drove and the list swapped per view.");
  } finally {
    await driver.quit();
    if (driverProc) driverProc.kill();
    if (sidecar) {
      // Politely ask the sidecar to shut down, then hard-kill as a backstop.
      try {
        sidecar.stdin.write(
          JSON.stringify({ schema_version: 1, id: 1, message: { type: "request", method: "sidecar.shutdown", params: {} } }) + "\n",
        );
      } catch {
        /* ignore */
      }
      sidecar.kill();
    }
  }
}

main().catch((err) => {
  console.error("[agent-board-clickthrough] FAIL:", err.message);
  process.exit(1);
});
