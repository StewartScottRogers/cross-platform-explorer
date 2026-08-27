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
// Five independent, composable check kinds (a case's `checks` array picks whichever apply — see
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
//  - `rectBounds` — does a single element's OWN rendered box stay within simple width/height limits
//    (`maxHeight`, `minWidth`, either or both)? This is CPE-1883 ("the status bar's focus-reveal box
//    ignores its own max-width and stacks one word per line"): a `max-width` CSS declaration existing
//    in the source (which is all jsdom can ever confirm) proves nothing about whether the FLEX
//    ALGORITHM ever lets the box reach it — a flex item can keep shrinking toward its content's
//    min-content width regardless of `max-width` sitting unreached above it, and when that min-content
//    width is a single word (because `white-space: normal` allows wrapping), the box collapses to a
//    tall one-word-per-line column instead of the wide, readable box `max-width` implies. `maxHeight`
//    catches exactly that shape (a column is tall for its width); `minWidth` catches the companion
//    failure of a box that never grows outward at all. Needs the element already in the state under
//    test (e.g. focused) BEFORE this engine measures it — that is the harness PAGE's job (see
//    `scripts/dev-harness/statusbar-notice/inner-main.ts`'s `?focus=` param), not this engine's; unlike
//    the other four kinds, `rectBounds` cannot discover the interaction that produces the state itself.
//    Optional `pseudo` (e.g. `"::after"`) measures a GENERATED-CONTENT box instead of `selector` itself
//    — CPE-1883's actual fix renders its reveal on `::after` (not the real element, to avoid disturbing
//    flex siblings), and a pseudo-element has no `getBoundingClientRect()` (querySelector can't even
//    select one), so this path reads `getComputedStyle(el, pseudo)` instead — width/height only, no
//    position.
//
//  - `pseudoOnScreen` — CPE-1883 round 2 (Visual Critic UAT): `rectBounds` proves the reveal box is the
//    right SHAPE (wide, not a column) but says nothing about WHERE it lands — the shipped-then-reverted
//    `left: 0` anchor grew the box off the RIGHT edge of a 600px window with the compound-busy row, and
//    `body { overflow: hidden }` (app.css) silently clipped the sentence's tail with no ellipsis, no
//    scroll, no visual cue at all. Anchoring the OPPOSITE edge (`right: 0; left: auto`) fixes it by
//    construction (the anchor element itself is always on-screen, so growing toward the anchor's own
//    side can only run off the FAR edge, never the near one) — this check proves that by construction is
//    actually true, not assumed: given `anchorSelector` (the real, still-narrow flex item) + `pseudo`, it
//    reads the pseudo's ACTUAL resolved `left`/`right` offset from `getComputedStyle` (which DOES
//    resolve these to used pixel values for an absolutely positioned pseudo-element, even though it has
//    no `getBoundingClientRect()`) to determine which edge it is really anchored to, combines that with
//    the anchor's real `getBoundingClientRect()` and the pseudo's computed width, and fails if the FAR
//    edge falls outside `[0, window.innerWidth]`. Went through TWO corrections, both found red-proofing
//    it against the reverted `left: 0`, not assumed correct on the first pass: (1) a first version
//    computed the position from the OPTIONAL `edge` config field instead of measuring it, so reverting
//    the CSS anchor didn't change what got measured — fixed to measure the true anchor and only
//    cross-check `edge` as a declared expectation now (a mismatch is itself flagged). (2) the "measure
//    it" fix above then assumed only ONE of computed `left`/`right` would resolve to a definite number —
//    false: BOTH resolve to definite numbers for a fully-determined box (width + one authored offset),
//    the un-authored side is algebraically DERIVED and can drift several px from the authored anchor
//    (sub-pixel rounding) — so this check now trusts whichever side computes to (near) zero, since this
//    CSS pattern always authors its anchor as an exact `0` offset.
//
// `siblingOverlap`/`clipProbe`/`textOverflow` catch a decorative element BLEEDING onto something else;
// `selfPaint` catches an interactive element BECOMING UNREACHABLE; `rectBounds` catches an element's OWN
// box growing the wrong SHAPE; `pseudoOnScreen` catches the right shape landing in the wrong PLACE. All
// are real, distinct failure shapes on record in this repo (CPE-1836 is the first kind, CPE-1827/CPE-1884
// are the second, CPE-1883 is the third and fourth) — a generic harness needs all four, not just one two
// or three.

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
/** CPE-1882 CI-round-3 finding: NO individual CDP call had its own timeout — only the outer
 *  ready-poll loop had an overall 40s deadline, checked BETWEEN calls, not around one. If a single
 *  `Runtime.evaluate`/`Page.navigate` etc. never got a response (a Chrome hiccup mid-call — a GC pause,
 *  a stalled internal navigation, a brief unresponsive period; real on a shared/throttled CI runner,
 *  never reproduced locally), the `await` on that one call blocked forever, hanging the WHOLE run
 *  silently until the job's own external `timeout-minutes` killed it with no information about where it
 *  was stuck. `CDP_CALL_TIMEOUT_MS` below makes every call fail LOUD, by method name, instead. */
const CDP_CALL_TIMEOUT_MS = 15000;

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
        const timer = setTimeout(() => {
          pending.delete(id);
          reject(new Error(`CDP call "${method}" got no response within ${CDP_CALL_TIMEOUT_MS}ms (id=${id})`));
        }, CDP_CALL_TIMEOUT_MS);
        pending.set(id, {
          resolve: (v) => { clearTimeout(timer); resolve(v); },
          reject: (e) => { clearTimeout(timer); reject(e); },
        });
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
    var boundsViolations = [];
    // CPE-1883: always recorded (pass OR fail), unlike the violation arrays above — this is the actual
    // measured evidence a ticket's work log wants, not just a yes/no. One entry per rectBounds check.
    // NOTE: no backtick characters allowed in this comment or anywhere else in this probe string — see
    // the textOverflow check's own identical warning below; this file already broke that rule once.
    var rectBoundsInfo = [];
    var offScreen = [];
    // CPE-1883 round 2: same always-recorded-not-just-on-failure convention as rectBoundsInfo above.
    var pseudoOnScreenInfo = [];

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
      } else if (check.kind === 'rectBounds') {
        var rbsel = check.selector;
        var rbel = document.querySelector(rbsel);
        if (!rbel) { missing.push('rectBounds: not found: ' + rbsel); continue; }
        // CPE-1883 addition: check.pseudo (e.g. "::after") measures a GENERATED-CONTENT box instead of
        // the real element -- pseudo-elements are not DOM nodes, so getBoundingClientRect() cannot
        // target one directly (querySelector cannot even select one). getComputedStyle(el, pseudo) is
        // the one API that DOES resolve a pseudo-element's actual rendered width/height in real Chrome;
        // it has no left/top/right/bottom equivalent, which is why this path only ever reports
        // width/height, never position -- exactly what this check's maxHeight/minWidth need and no more.
        // NOTE: no backtick characters allowed anywhere in this probe string -- see the textOverflow
        // check's identical warning above; this file has broken that rule twice already.
        var rbr;
        if (check.pseudo) {
          var pcs = getComputedStyle(rbel, check.pseudo);
          rbr = { width: parseFloat(pcs.width) || 0, height: parseFloat(pcs.height) || 0 };
        } else {
          rbr = rectOf(rbel);
        }
        var rbLabel = rbsel + (check.pseudo ? check.pseudo : '');
        rectBoundsInfo.push({ selector: rbLabel, width: Number(rbr.width.toFixed(1)), height: Number(rbr.height.toFixed(1)) });
        if (typeof check.maxHeight === 'number' && rbr.height > check.maxHeight) {
          boundsViolations.push(
            rbLabel + ' height=' + rbr.height.toFixed(1) + 'px exceeds maxHeight=' + check.maxHeight +
            'px (width=' + rbr.width.toFixed(1) + 'px) — looks like a stacked column, not a wide box'
          );
        }
        if (typeof check.minWidth === 'number' && rbr.width < check.minWidth) {
          boundsViolations.push(
            rbLabel + ' width=' + rbr.width.toFixed(1) + 'px is below minWidth=' + check.minWidth +
            'px (height=' + rbr.height.toFixed(1) + 'px) — never grew outward'
          );
        }
      } else if (check.kind === 'pseudoOnScreen') {
        var posel = document.querySelector(check.anchorSelector);
        if (!posel) { missing.push('pseudoOnScreen: anchor not found: ' + check.anchorSelector); continue; }
        var anchorR = rectOf(posel);
        var poscs = getComputedStyle(posel, check.pseudo);
        var posW = parseFloat(poscs.width) || 0;
        var posLabel = check.anchorSelector + check.pseudo;
        // Reviewer finding (CPE-1883 round 2): the FIRST version of this check trusted check.edge
        // alone and computed a position from it, so reverting the CSS anchor (e.g. right: 0 back to
        // left: 0) did NOT change what got measured -- the check silently re-asserted its own
        // configured expectation instead of the page's actual rendered state, so it could never have
        // caught that exact regression. Fixed: read the REAL resolved left/right offset from computed
        // style (getComputedStyle DOES resolve these to used pixel values for an absolutely positioned
        // pseudo-element, even though it has no getBoundingClientRect()) and derive the anchor edge from
        // THAT, not from check.edge. check.edge is now only a declared expectation, cross-checked
        // against the measured edge below rather than substituted for it.
        // NOTE: no backtick characters allowed anywhere in this probe string -- see textOverflow's own
        // identical warning above; this file has broken that rule three times now.
        var rightPx = parseFloat(poscs.right);
        var leftPx = parseFloat(poscs.left);
        // Reviewer-motivated correction #2, found red-proofing the fix above: getComputedStyle resolves
        // BOTH left AND right to definite numbers for a fully-determined absolutely positioned box
        // (width + exactly one of left/right authored), not just the authored side -- the un-authored
        // side is algebraically DERIVED and measured up to ~8px off the authored side's exact anchor
        // (sub-pixel layout rounding), so trusting whichever side happens to parse as a number is not
        // reliable; both always do. This CSS pattern always anchors with an offset of exactly 0 on the
        // authored side (right: 0, or the pre-fix left: 0) -- so the side whose computed value is at (or
        // essentially at) 0 is the one actually authored, and that is what this check trusts.
        var farLeft, farRight, actualEdge;
        if (Math.abs(rightPx) < 0.5) {
          farRight = anchorR.right - rightPx;
          farLeft = farRight - posW;
          actualEdge = 'right';
        } else if (Math.abs(leftPx) < 0.5) {
          farLeft = anchorR.left + leftPx;
          farRight = farLeft + posW;
          actualEdge = 'left';
        } else {
          missing.push('pseudoOnScreen: ' + posLabel + ' -- neither left (' + poscs.left + ') nor right (' + poscs.right + ') computed near 0; this check only supports a 0-offset anchor');
          continue;
        }
        pseudoOnScreenInfo.push({
          selector: posLabel, edge: actualEdge, left: Number(farLeft.toFixed(1)), right: Number(farRight.toFixed(1)),
          innerWidth: window.innerWidth,
        });
        if (farLeft < -0.5 || farRight > window.innerWidth + 0.5) {
          offScreen.push(
            posLabel + ' (measured anchor: ' + actualEdge + ': 0) spans left=' + farLeft.toFixed(1) +
            ' right=' + farRight.toFixed(1) + 'px, outside the viewport [0, ' + window.innerWidth +
            '] — part of the revealed sentence would be clipped by body { overflow: hidden } with no cue'
          );
        }
        if (check.edge && check.edge !== actualEdge) {
          offScreen.push(
            posLabel + ' expected to anchor via "' + check.edge + '" but computed style measured "' +
            actualEdge + '" instead — the CSS anchor direction itself changed'
          );
        }
      }
    }
    return {
      overlaps: overlaps,
      clipBreaches: clipBreaches,
      textOverflows: textOverflows,
      unpainted: unpainted,
      missing: missing,
      boundsViolations: boundsViolations,
      rectBoundsInfo: rectBoundsInfo,
      offScreen: offScreen,
      pseudoOnScreenInfo: pseudoOnScreenInfo,
    };
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
    // CPE-1883 finding: a headless tab navigated via `Page.navigate` never becomes the OS-level
    // "active" window, so `document.hasFocus()` is false and — because `:focus`/`:focus-visible` both
    // require DOCUMENT focus, not merely `document.activeElement`, per spec — a harness page's own
    // `el.focus()` call sets `activeElement` but neither pseudo-class ever matches, silently. That
    // surfaced as a real case ("statusbar-focus-reveal") hanging on its own `readySelector` for the
    // full 40s below rather than measuring anything. `Emulation.setFocusEmulationEnabled` is CDP's own
    // purpose-built fix: it makes the page report itself as focused/active regardless of real window
    // activation state. Enabled once for the whole session (harmless for cases that never call
    // `.focus()` — it only changes what `document.hasFocus()`/`:focus-visible` report, nothing about
    // layout) rather than per-case, so any future focus-dependent case gets it for free.
    // Reviewer note (CPE-1883 round 2): confirmed global rather than per-case is SAFE today — grepped
    // every scripts/dev-harness/*/{inner-,}main.ts and cases.mjs, nothing else calls `.focus()` or
    // depends on autofocus/`:hover`/`:focus-visible`, and the full 14-case suite re-ran clean with this
    // enabled. It is a standing caveat for whoever adds the next case, though: a FUTURE case whose page
    // autofocuses an element on load (no explicit `?focus=` param needed) will now engage
    // `:focus-visible` styling where it previously would not have, simply because this flag makes
    // `document.hasFocus()` true for every case's page, not just ones that ask for it.
    await client.send("Emulation.setFocusEmulationEnabled", { enabled: true });

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
