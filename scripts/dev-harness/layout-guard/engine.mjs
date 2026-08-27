// CPE-1882 — the generalised real-browser layout-check engine. Generalises
// scripts/dev-harness/statusbar-notice/inner-main.ts's rect/overlap/paint-probe measurements (built for
// CPE-1659/1859, extended for CPE-1836) and scripts/dev-harness/sidebar-drop-stack-overlap/check.mjs's
// CDP-driving shape (built for CPE-1884) into ONE reusable driver any case in cases.mjs can point at a
// component + a width list. Same approach both of those already proved out: plain installed
// `chrome.exe --headless=new` driven over raw CDP (`Emulation.setDeviceMetricsOverride` for the
// viewport, `Runtime.evaluate` to measure) — no WebDriver, no npm dependency, Node's built-in `fetch` +
// `WebSocket` only.
//
// REQUIRES NODE >= 22. `runAllCases` below calls the global `WebSocket` constructor directly —
// it is only a STABLE Node built-in from v22 (unflagged); on Node 20 it is `undefined` and every call
// throws `ReferenceError: WebSocket is not defined`, unconditionally, on every run. This is not
// theoretical: it reached real CI once (job 98371907013, `.github/workflows/gui-smoke.yml`'s
// `layout-guard` job was pinned to `node-version: 20` — every other job in that workflow's `Setup Node`
// step, copy-pasted from) before being caught in UAT and fixed by pinning that ONE job to 22 — see that
// job's own comment for why 22, not the `ws` package, was the fix. A red-proof run against a local Node
// < 22 will fail with exactly that ReferenceError before it measures anything, which reads identically
// to "the harness crashed" rather than "a real layout bug" — if you see it, check `node --version`
// first.
//
// Four independent, composable check kinds (a case's `checks` array picks whichever apply — see
// cases.mjs's own decision table for which kind catches which class of bug, and its header for real
// examples):
//
//  - `siblingOverlap` — no two DIRECT CHILDREN of a given root may occupy overlapping screen space.
//    This is the literal "no element in this row overlaps another" rule from the repo's pill/chip
//    convention (CLAUDE.md "Pills / chips / badges") and from CPE-1882's own ticket text, and the
//    general case of what broke in CPE-1884 (the Drop Stack handle over Sidebar rows) and would catch a
//    misplaced/mis-sized *whole box* colliding with its neighbour. Deliberately DIRECT children only —
//    that sidesteps the "a child is expected to overlap its own parent" problem entirely (no exclusion
//    list needed, unlike the CPE-1836 prototype's `git-*` skip-list), because true siblings never have
//    an ancestor/descendant relationship to begin with.
//
//  - `clipProbe` — does a container's `overflow: hidden` (or lack of it) actually CLIP an overhanging
//    descendant's paint, or does the descendant bleed through onto whatever sits past the container's
//    edge? This is CPE-1836 itself ("the status bar's git block bleeds into the disk label"): the
//    prototype's own comment explains why raw geometry can't see this — an overhanging child's
//    `getBoundingClientRect()` is IDENTICAL whether the parent clips it or not (clipping is a paint-time
//    effect, not a layout-time one), so the only way to observe the fix is `elementFromPoint` at the
//    overhang itself, which follows real paint/clip, not layout geometry.
//
//  - `textOverflow` — does an element's own rendered text exceed its own background box
//    (`scrollWidth > clientWidth`)? The literal second half of the same pill/chip convention rule
//    ("text never wraps inside a pill and overflows its background").
//
//  - `selfPaint` — is a given element actually hit-testable at its own visible centre, i.e. not clipped
//    away by an ancestor's `overflow: hidden` and not covered by something else? This is CPE-1827 ("the
//    Trash titlebar cannot fit seven buttons and a title on one line") and CPE-1884's own "is the row
//    actually clickable" check generalised: a control that still LAYS OUT somewhere doesn't mean it's
//    still reachable once an ancestor clips it or another element paints on top.
//
// `siblingOverlap`/`clipProbe`/`textOverflow` catch a decorative element BLEEDING onto something else;
// `selfPaint` catches an interactive element BECOMING UNREACHABLE. Both are real, distinct failure
// shapes on record in this repo (CPE-1836 is the first kind, CPE-1827/CPE-1884 are the second) — a
// generic harness needs both, not just one.

import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { rm } from "node:fs/promises";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
export const REPO_ROOT = path.resolve(__dirname, "..", "..", "..");

export function defaultChromePath() {
  if (process.env.CHROME_PATH) return process.env.CHROME_PATH;
  switch (process.platform) {
    case "win32":
      return "C:/Program Files/Google/Chrome/Application/chrome.exe";
    case "darwin":
      return "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
    default:
      // GitHub-hosted ubuntu-latest runners ship google-chrome-stable at this path.
      return "/usr/bin/google-chrome";
  }
}

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
    send(method, params = {}) {
      const id = nextId++;
      return new Promise((resolve, reject) => {
        pending.set(id, { resolve, reject });
        ws.send(JSON.stringify({ id, method, params }));
      });
    },
  };
}

/** Builds the in-page JS expression run via `Runtime.evaluate`. Pure function of `checks` (JSON-
 *  serialisable) — no closures over Node state, since it crosses into the browser as a source string. */
function buildProbeExpression(checks) {
  const checksJson = JSON.stringify(checks);
  return `
  (function () {
    var checks = ${checksJson};
    function rectOf(el) {
      var r = el.getBoundingClientRect();
      return { left: r.left, top: r.top, right: r.right, bottom: r.bottom, width: r.width, height: r.height };
    }
    function overlapsRect(a, b) {
      return a.left < b.right - 0.5 && b.left < a.right - 0.5 && a.top < b.bottom - 0.5 && b.top < a.bottom - 0.5;
    }
    function labelOf(el, fallback) {
      if (el.id) return '#' + el.id;
      var cls = (el.className && typeof el.className === 'string') ? el.className.trim().split(/\\s+/)[0] : '';
      return cls ? '.' + cls : fallback;
    }
    var overlaps = [];
    var clipBreaches = [];
    var textOverflows = [];
    var unpainted = [];
    var missing = [];

    for (var i = 0; i < checks.length; i++) {
      var check = checks[i];
      if (check.kind === 'siblingOverlap') {
        var root = document.querySelector(check.root);
        if (!root) { missing.push('siblingOverlap: root not found: ' + check.root); continue; }
        var exclude = check.exclude || [];
        var kids = Array.prototype.filter.call(root.children, function (el) {
          var r = el.getBoundingClientRect();
          if (r.width <= 0 || r.height <= 0) return false;
          for (var e = 0; e < exclude.length; e++) {
            if (el.matches(exclude[e])) return false;
          }
          return true;
        });
        for (var a = 0; a < kids.length; a++) {
          for (var b = a + 1; b < kids.length; b++) {
            var ra = rectOf(kids[a]), rb = rectOf(kids[b]);
            if (overlapsRect(ra, rb)) {
              // CPE-1882 UAT: match clipProbe's "overhangs by N px" -- the reader shouldn't have to
              // subtract two raw rects by hand to tell a 1px rounding wobble from a real collision.
              var overlapW = Math.min(ra.right, rb.right) - Math.max(ra.left, rb.left);
              var overlapH = Math.min(ra.bottom, rb.bottom) - Math.max(ra.top, rb.top);
              overlaps.push(
                check.root + ' children: ' + labelOf(kids[a], 'child' + a) + ' × ' + labelOf(kids[b], 'child' + b) +
                ' overlap by ' + overlapW.toFixed(1) + 'px × ' + overlapH.toFixed(1) + 'px' +
                ' (rectA=' + JSON.stringify(ra) + ' rectB=' + JSON.stringify(rb) + ')'
              );
            }
          }
        }
      } else if (check.kind === 'clipProbe') {
        var container = document.querySelector(check.container);
        if (!container) { missing.push('clipProbe: container not found: ' + check.container); continue; }
        var cRect = rectOf(container);
        var worst = null;
        for (var c = 0; c < check.candidates.length; c++) {
          var sel = check.candidates[c];
          var el = document.querySelector(sel);
          if (!el) continue;
          var r2 = rectOf(el);
          if (r2.right > cRect.right + 0.5 && (!worst || r2.right > worst.rect.right)) {
            worst = { rect: r2, sel: sel };
          }
        }
        if (worst) {
          var probeX = (cRect.right + worst.rect.right) / 2;
          var probeY = (cRect.top + cRect.bottom) / 2;
          var hit = document.elementFromPoint(probeX, probeY);
          var hitIsCandidate = !!(hit && hit.closest(check.container));
          if (hitIsCandidate) {
            clipBreaches.push(
              check.container + ': ' + worst.sel + ' overhangs by ' + (worst.rect.right - cRect.right).toFixed(1) +
              'px AND paints there (probe (' + probeX.toFixed(1) + ',' + probeY.toFixed(1) + ') hit ' +
              labelOf(hit, hit.tagName) + ') — not clipped'
            );
          }
        }
      } else if (check.kind === 'textOverflow') {
        for (var t = 0; t < check.selectors.length; t++) {
          var tsel = check.selectors[t];
          var tel = document.querySelector(tsel);
          if (!tel) { missing.push('textOverflow: not found: ' + tsel); continue; }
          // scrollWidth > clientWidth alone is NOT the bug: CSS overflow: hidden (with or without
          // text-overflow: ellipsis) is the CORRECT, intended way to handle text too long for a pill —
          // it clips the excess, nothing paints outside the box, and scrollWidth legitimately exceeds
          // clientWidth for that whole (correct) state. The actual convention violation ("text never
          // wraps inside a pill and overflows its background") is text escaping VISIBLY, which only
          // overflow-x: visible (no clip at all) allows — so gate on that computed property, not on
          // scrollWidth/clientWidth alone. Caught a false positive this way on .git-branch/.disk/etc.
          // in StatusBar.svelte, which correctly ellipsis-truncate — flagged as "overflow" by
          // scrollWidth alone but painting nothing outside their own box.
          // NOTE: no backtick characters allowed anywhere in this probe string — it is itself the body
          // of an outer template literal in buildProbeExpression() below; a literal backtick here would
          // terminate THAT string early and break every check kind, not just this one.
          var overflowX = getComputedStyle(tel).overflowX;
          if (overflowX === 'visible' && tel.scrollWidth > tel.clientWidth + 2) {
            textOverflows.push(
              tsel + ' scrollWidth=' + tel.scrollWidth + ' clientWidth=' + tel.clientWidth +
              ' overflow-x=visible — text paints past its own background'
            );
          }
        }
      } else if (check.kind === 'selfPaint') {
        for (var s = 0; s < check.selectors.length; s++) {
          var ssel = check.selectors[s];
          var sel_el = document.querySelector(ssel);
          if (!sel_el) { missing.push('selfPaint: not found: ' + ssel); continue; }
          var sr = rectOf(sel_el);
          if (sr.width <= 0 || sr.height <= 0) { unpainted.push(ssel + ' has zero rendered size'); continue; }
          var cx = Math.min(Math.max(sr.left + sr.width / 2, 0), window.innerWidth - 1);
          var cy = Math.min(Math.max(sr.top + sr.height / 2, 0), window.innerHeight - 1);
          var shit = document.elementFromPoint(cx, cy);
          var ok = !!(shit && (shit === sel_el || sel_el.contains(shit)));
          if (!ok) {
            unpainted.push(
              ssel + ' center=(' + cx.toFixed(1) + ',' + cy.toFixed(1) + ') hit=' +
              (shit ? labelOf(shit, shit.tagName) : 'null') + ' — not what actually painted there'
            );
          }
        }
      }
    }
    return { overlaps: overlaps, clipBreaches: clipBreaches, textOverflows: textOverflows, unpainted: unpainted, missing: missing };
  })()
  `;
}

/** Runs ONE (case, width) check against an ALREADY-CONNECTED CDP `client` (see `runAllCases` below for
 *  why this no longer launches its own Chrome per width — CPE-1882 CI-round-3 finding). Sets the
 *  viewport, navigates, waits for `readySelector` AND confirms `location.href`, runs the probe. */
async function checkOneCaseOnClient(client, { devServerBase, kase, width, height }) {
  // The load-bearing step: `--window-size` alone does not reliably set the CSS viewport under
  // `--headless=new` (see statusbar-notice/index.html's header comment — it clamps internally and
  // rescales), which is why the earlier `--dump-dom`-driven harnesses needed an outer-page/iframe trick
  // to get a trustworthy width. `Emulation.setDeviceMetricsOverride` sets the REAL CSS viewport directly
  // (already proven by sidebar-drop-stack-overlap/check.mjs), so this engine drives harness pages
  // directly — no iframe indirection needed here.
  await client.send("Emulation.setDeviceMetricsOverride", {
    width,
    height,
    deviceScaleFactor: 1,
    mobile: false,
  });
  const expectedUrl = `${devServerBase}${kase.path}`;
  await client.send("Page.navigate", { url: expectedUrl });

  let ready = false;
  let urlMatched = false;
  let lastHref = "";
  // 40s on a cold dev server compiling the module graph on first hit (see
  // sidebar-drop-stack-overlap's identical comment); every subsequent width against the now-warm server
  // resolves in well under a second.
  const deadline = Date.now() + 40000;
  while (Date.now() < deadline) {
    // CPE-1882 UAT/reviewer finding: this repo routinely runs many worktrees on one dev machine
    // concurrently, and TWO SEPARATE `run.mjs` processes could end up pointed at each other's vite dev
    // server if their (now PID-derived, see `runAllCases`) ports ever collided — the actual root cause a
    // reviewer traced from `.tv-sync-badge`, a fixture that exists in NO worktree's committed code,
    // showing up as a real measurement, because the OLD readiness check only confirmed `readySelector`
    // was present, never that this tab is actually looking at THIS run's URL. `location.href` is the one
    // signal that can't lie about that: verify it before trusting anything the DOM says.
    const r = await client.send("Runtime.evaluate", {
      expression: `(function(){var v=!!document.querySelector(${JSON.stringify(kase.readySelector)});return { ready: v, href: location.href };})()`,
      returnByValue: true,
    });
    if (r.result && r.result.value) {
      lastHref = r.result.value.href || "";
      urlMatched = lastHref === expectedUrl;
      if (urlMatched && r.result.value.ready === true) {
        ready = true;
        break;
      }
    }
    await sleep(200);
  }
  if (!urlMatched) {
    throw new Error(
      `navigation mismatch at width=${width} height=${height}: expected location.href="${expectedUrl}" ` +
      `but the tab reports "${lastHref}" — this Chrome instance never reached the URL this run navigated ` +
      `it to (possibly a stale/foreign page from a port collision with another concurrent harness run)`,
    );
  }
  if (!ready) {
    throw new Error(`"${kase.readySelector}" never appeared within 40s at width=${width} height=${height} (url confirmed correct)`);
  }
  await sleep(300); // settle layout/fonts

  const r = await client.send("Runtime.evaluate", {
    expression: buildProbeExpression(kase.checks),
    returnByValue: true,
  });
  return r.result.value;
}

/** Runs every (case × width) combination against ONE launched Chrome instance, reused for the whole
 *  sweep — this used to launch a fresh Chrome + fresh profile dir PER WIDTH (12 launches for the two
 *  shipped cases), which was fine on a dev workstation (~1 minute total) but never proven against a real
 *  CI runner. It wasn't: the first real CI run of this job hit its own `timeout-minutes: 10` cap and was
 *  cancelled without completing a single case — 12 sequential fresh-Chrome-process launches (each paying
 *  full startup + CDP handshake cost) is far more expensive on a shared/throttled GitHub-hosted runner
 *  than on a dev machine. ONE Chrome instance, re-navigated per width via CDP (`Page.navigate` +
 *  `Emulation.setDeviceMetricsOverride`, same as before), removes 11 of 12 process launches for today's
 *  two cases — the per-width overhead becomes a navigation + a `Runtime.evaluate` round trip, not a full
 *  process spawn. This does NOT reintroduce the "fresh profile dir avoids a cached stale app.css" concern
 *  from the original per-width design: that concern was about a profile dir REUSED ACROSS SEPARATE DAYS
 *  of local dev-loop iteration (see sidebar-drop-stack-overlap/check.mjs's identical original comment),
 *  not about reusing one browser process for the few seconds one CI run's own sweep takes — vite's own
 *  dev server always serves the current transformed CSS for every navigation regardless of the browser's
 *  own cache, and each case still gets a genuine fresh `Page.navigate` (not a SPA route change), so
 *  nothing about a PREVIOUS case's DOM/CSS can leak into the next one's measurement.
 *
 *  `cdpPort`'s default is DERIVED FROM `process.pid`, not a fixed literal. This repo routinely runs many
 *  worktrees on one dev machine concurrently (that is the NORMAL condition here, per CLAUDE.md's memory
 *  notes, not an edge case) — a fixed port meant two concurrent `run.mjs` processes (different
 *  worktrees, same codebase) could pick the exact same CDP port, and the second one to connect would end
 *  up talking to the FIRST one's Chrome instance. `checkOneCaseOnClient`'s own `location.href`
 *  verification (above) is the second, independent layer against the same class of bug — it also covers
 *  the DEV-SERVER port (run.mjs), which this PID-derived CDP port does not touch. */
export async function runAllCases({ cases, devServerBase, chromePath, cdpPort = 20000 + (process.pid % 20000) }) {
  const userDataDir = path.join(
    REPO_ROOT,
    ".claude",
    "dev-harness-chrome-profiles",
    `layout-guard-${cdpPort}-${Date.now()}`,
  );
  const maxHeight = Math.max(...cases.map((k) => k.height));
  const chrome = spawn(
    chromePath,
    [
      "--headless=new",
      "--disable-gpu",
      "--no-sandbox",
      "--disable-dev-shm-usage",
      `--remote-debugging-port=${cdpPort}`,
      `--user-data-dir=${userDataDir}`,
      `--window-size=1200,${maxHeight}`,
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

    const results = [];
    for (const kase of cases) {
      for (const width of kase.widths) {
        const measured = await checkOneCaseOnClient(client, { devServerBase, kase, width, height: kase.height });
        results.push({ case: kase.name, width, ...measured });
      }
    }
    ws.close();
    return results;
  } finally {
    // CPE-1882 UAT/reviewer finding: this used to leak `userDataDir` forever (only `chrome.kill()` ran,
    // no cleanup at all) — 1.8 GB left behind across 13 local runs. Wait for the process to actually
    // exit (not just for `kill()` to have been CALLED) before deleting its own profile dir, so an
    // in-progress Windows file lock doesn't turn every cleanup into a silent no-op; bounded at 3s so a
    // hung/already-dead process can't stall the whole run over a directory that isn't the point of it.
    const exited = new Promise((resolve) => chrome.once("exit", resolve));
    chrome.kill();
    await Promise.race([exited, sleep(3000)]);
    // Best-effort past this point: a still-held Windows lock right after exit is a real possibility, not
    // a bug in this code, so a cleanup failure here must never fail (or even log noisily during) the
    // actual layout checks it rode in on.
    await rm(userDataDir, { recursive: true, force: true }).catch(() => {});
  }
}
