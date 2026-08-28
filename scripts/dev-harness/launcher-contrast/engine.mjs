// @ts-nocheck — this harness is run by plain `node`, has never been in `tsconfig.json`'s `include`,
// and has never been type-checked. CPE-1966 round 3 gave `sessionChipColours` a real test, which
// imports this module and so drags it into svelte-check's program for the first time: 40 implicit-any
// errors in CDP payloads and probe JSON, none of them a defect, all of them noise that would bury the
// `npm run check` 0/0 gate. Annotating a 1,100-line browser harness is its own ticket, not a rider on
// a contrast fix. The parts worth checking were MOVED OUT instead — `src/lib/jsSource.mjs` is typed
// via JSDoc and covered by `src/lib/jsSource.test.ts`.
//
// CPE-1966 — a real-browser contrast sweep for the AI Console launcher, covering the four things a
// static stylesheet sweep structurally cannot see.
//
// ── Why a browser, and why this file rather than more of the vitest guard ─────────────────────────
// CPE-1921 added `src/lib/aiConsoleLauncher.contrast.test.ts`: a static parse of
// `sidecar/ai-console/src/launcher.html`'s <style> blocks that pairs every `color: var(--token)` rule
// with a MEASURED constant for the engine's `Canvas`. It is cheap, runs on every push, and it caught
// the defect it was written for. It also reported ZERO failures on a page that had four measured ones,
// for four structural reasons (CPE-1966):
//
//   1. it derives its sites from `color:` declarations, so `border-color` / `background` / `box-shadow`
//      — every NON-TEXT role, including the page's ONLY focus indicator — were never sites at all;
//   2. it has no DOM, so it assumed every foreground sits on `Canvas`; elements inside `#tabs`
//      (`rgba(128,128,128,0.10)`), `.toolbar` (`rgba(128,128,128,0.07)`), `#terms` (`#0d0d0d`) or the
//      `#help-overlay` scrim do not;
//   3. it has no states, so `:hover` / `:focus` / `:active` rules were invisible;
//   4. it has no time, so an ANIMATED opacity (`boot-pulse` swings `.boot-label` between .45 and .85)
//      was sampled at whichever single value the source happened to state first — or not at all.
//
// Every one of those four is answerable by the engine and by nothing else, so this harness asks the
// engine: installed `chrome.exe --headless=new` driven over raw CDP, exactly the shape
// `scripts/dev-harness/layout-guard/engine.mjs` already proved out for CPE-1882 (no WebDriver, no npm
// dependency, Node's built-in `fetch` + `WebSocket` only, REQUIRES NODE >= 22 for that `WebSocket`).
//
// ── How each of the four gaps is closed ──────────────────────────────────────────────────────────
//   NON-TEXT ROLES  every `border-*-color`, `outline-color`, `box-shadow` and `background-color` that
//                   is CHROMATIC (max channel - min channel >= CHROMA_MIN) or that CHANGES under a
//                   forced state is measured at SC 1.4.11's 3:1, against the adjacency that role
//                   actually abuts: a border against BOTH the interior it encloses and the exterior it
//                   sits on, an inset shadow against the interior, a fill against the exterior. That
//                   both-adjacencies rule is the whole of site 1: the focus border PASSED against the
//                   page (4.12:1 dark) while failing against the `Field` interior it encloses
//                   (2.46:1), so a guard checking either one alone would have called it clean.
//   REAL GROUNDS    `groundOf` walks the true ancestor chain and composites every background AND every
//                   ancestor `opacity` in paint order, so the ground is what the engine actually paints
//                   rather than an assumed `Canvas`.
//   STATES          CDP `CSS.forcePseudoState` — the engine's own state machinery, not a rewritten
//                   stylesheet — is applied to one element at a time, and the sweep re-measures.
//   TIME            every CSS animation is paused and stepped through `ANIM_SAMPLES` frames across its
//                   own duration; the WORST frame is what gets reported. Sampling one frame is how site
//                   3 stayed hidden.
//
// ── The compositing model, which is the part that is easy to get wrong ───────────────────────────
// CPE-1921's round 2 first reported site 2 at 3.81:1 and had to correct itself, because an element
// carrying `opacity` composites its OWN BACKGROUND toward the ground together with its text — dimming
// only the text overstates the loss. `groundOf` below therefore accumulates the opacity product down
// the ancestor chain and applies it to each element's background as it goes, and the site loop applies
// the SAME accumulated alpha to the foreground. Worked example, and a caution: `.area-help` was a
// `ButtonFace` fill and `ButtonText` text under one `opacity: .75`. Dimming the text only gives 3.81:1
// (the number CPE-1921 round 2 published and retracted); dimming both gives 5.07:1 on this Chrome
// build. Neither is the 4.24/4.28 CPE-1966 was filed with — that figure came from a build resolving
// dark `ButtonFace` near rgb(120,120,120) rather than this one's rgb(107,107,107). The model here is
// right AND the answer still moves with the engine, which is exactly why that rule no longer carries a
// blanket opacity at all: see its comment in launcher.html.
//
// ── Validation, per CPE-1933 rule 3 ──────────────────────────────────────────────────────────────
// `validateAnchors()` runs BEFORE any measurement and refuses to continue unless the WCAG
// implementation reproduces the three standard anchors — #000/#fff = 21.00, #767676/#fff = 4.54,
// #949494/#fff = 3.03 (the two grey anchors are the WCAG-defined boundary greys for 4.5:1 and 3:1 on
// white) — plus two compositing anchors that a text-only model gets wrong. It validates the ONLY
// implementation there is: `COLOR_MATH_SOURCE` below is a single source string that Node evaluates
// and that `probeExpr` pastes into the page, after round 1 shipped a second copy inside the probe
// that no anchor touched and that the Reviewer multiplied by 1.6 to a green, `PASS`, exit-0 run with
// every number 60% wrong. `--verify-pixels` adds a genuinely independent second path — see its
// LIMITS note at `verifyAgainstPixels`, which is narrower than "the two paths agree".
//
// ── What this harness does NOT see (the limits, kept here so they are read with the numbers) ──────
//  1. THE RATIO MATHS IS CROSS-CHECKED BY NOTHING EXTERNAL. `--verify-pixels` checks GROUNDS only,
//     and only for `role === "text"`, `state === "base"`; the arithmetic that turns two colours into
//     a ratio is anchored (five known values) but never independently recomputed. That is precisely
//     the gap round 1's duplicate probe maths slipped through, and it is why there is now one copy.
//  2. NON-ANCESTOR PAINTERS ARE INVISIBLE. `groundOf` composites the ancestor chain. An element that
//     OVERLAPS a site without containing it — the boot overlay is the worked example, which is why
//     `verifyAgainstPixels` has to hide it — contributes nothing to the model's ground. A site that
//     something else paints over is measured against the ground it would have if nothing did.
//  3. ROUGHLY HALF THE SITES ARE NON-TEXT AND NON-CHROMATIC, and none of them is enforced; most of
//     those sit under the 3:1 they would face if they were. They are the neutral hairlines and hover
//     washes SC 1.4.11 excludes as decorative (`var(--line)` = rgba(128,128,128,0.26) and friends),
//     and enforcing them would bury the real findings — but "not enforced" is a judgement, not an
//     absence, so `run.mjs` prints both counts every run (2026-08-28: 390 of 786 dropped, ~350 of
//     those under bar — the second number moves by one or two between runs, which is itself worth
//     knowing) and `--all` lists every one. Read the counts from the report, not from this comment.
//  4. INLINE-ASSIGNED COLOURS ARE MEASURED, NOT ENFORCED — the readings the report names under
//     "MEASURED, NOT ENFORCED", from the session-identity palette shared with the main app
//     (CPE-1977).
//  5. COLOURS ASSIGNED INLINE FROM JS TABLES THE FIXTURES NEVER MOUNT ARE NOT MEASURED AT ALL.
//     `STATE_META` (launcher.html) sets `.state-dot`'s background to #d08a1a / #3a72b5 / #3a9d4a in
//     `renderState()`; the fixtures mount `.state-dot` at its CSS default #7a7a7a, which is
//     non-chromatic and therefore dropped, so those three never appear anywhere in the report — not
//     even under "MEASURED, NOT ENFORCED". Measured by hand: #d08a1a on a light tab is 2.38:1 (the
//     number that retired that hex from `.tab.blocked`) and #3a9d4a is 2.86:1 (retired from
//     `.badge.yes`). They are not a 1.4.11 failure — each dot carries a `title=` and `.pane-state`
//     spells the state out in words, so the colour is not the only carrier — but they ARE unmeasured,
//     and they are in CPE-1977's scope alongside the chip palette.
//
// Run:  node scripts/dev-harness/launcher-contrast/run.mjs
//   or: npm run harness:launcher-contrast

import { spawn } from "node:child_process";
import http from "node:http";
import path from "node:path";
import { readFileSync } from "node:fs";
import { rm } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import zlib from "node:zlib";
import { FIXTURES, UNREACHABLE } from "./fixtures.mjs";
// The JS comment stripper lives in `src/lib/` with its own tests, not here. CPE-1966 round 2 shipped
// it private to this harness — this repo's SIXTH hand-rolled stripper, imported nowhere, exercised
// only by "the provenance check passed" in one CI job, and wrong in seven adversarial shapes, four of
// which DELETED real code. `src/lib/jsSource.test.ts` now pins every one of them, and
// `stripScriptBodiesChecked` carries the `vm.Script` desync backstop for the shapes it does not.
import { htmlScriptBodies, stripScriptBodiesChecked } from "../../../src/lib/jsSource.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
export const REPO_ROOT = path.resolve(__dirname, "..", "..", "..");
export const LAUNCHER = path.join(REPO_ROOT, "sidecar", "ai-console", "src", "launcher.html");

/** A colour counts as author-chosen (rather than a neutral hairline/wash) at this channel spread. */
export const CHROMA_MIN = 12;
/** Frames sampled across each CSS animation's own duration. 21 hits both endpoints and the midpoint. */
export const ANIM_SAMPLES = 21;
const CDP_CALL_TIMEOUT_MS = 30000;
const CDP_ENDPOINT_TIMEOUT_MS = 40000;

export function defaultChromePath() {
  if (process.env.CHROME_PATH) return process.env.CHROME_PATH;
  switch (process.platform) {
    case "win32":
      return "C:/Program Files/Google/Chrome/Application/chrome.exe";
    case "darwin":
      return "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
    default:
      return "/usr/bin/google-chrome";
  }
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ── WCAG 2.1 relative luminance + contrast ratio ─────────────────────────────────────────────────
// https://www.w3.org/TR/WCAG21/#dfn-relative-luminance / #dfn-contrast-ratio
//
// ── ONE IMPLEMENTATION, TWO EXECUTIONS (round-2 review, and the reason this is a source string) ───
// Every number this harness prints is a contrast ratio. Round 1 had TWO independent implementations
// of that arithmetic: this module's, which `validateAnchors()` exercised, and a SECOND COPY inside
// `PROBE_SOURCE` that computed every measured site ratio and that no anchor ever touched. The
// Reviewer multiplied the probe's copy by 1.6: all five anchors green, both pixel cross-checks clean,
// `PASS`, exit 0 — and every printed number 60% wrong. The validator guarded a copy.
//
// So the maths exists exactly ONCE now, as the source string below, and is executed twice:
//   1. HERE, by Node, via `new Function(...)`. `validateAnchors()` therefore runs against the real
//      thing and `sweep` calls it BEFORE spawning Chrome — sabotaging `ratio`/`over`/`luminance`
//      still exits 2 without spending a browser, which is the property round 1 got right and this
//      keeps.
//   2. IN THE PAGE, because `probeExpr` pastes this same string in front of the probe body, so the
//      site loop calls the same `ratio` the anchors just validated. `sweep` additionally re-runs
//      `validateAnchors` inside the page and requires byte-identical JSON back — that leg pins the
//      string against arriving MANGLED (a bad escape, a truncated evaluate), which is the only
//      failure mode left once there is nothing to diverge from.
// A second validator would be a second thing to drift; a second execution of the same one is not.
export const COLOR_MATH_SOURCE = String.raw`
function luminance(c) {
  var l = [c[0], c[1], c[2]].map(function (v) { var s = v / 255; return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4); });
  return 0.2126 * l[0] + 0.7152 * l[1] + 0.0722 * l[2];
}
function ratio(a, b) { var x = luminance(a), y = luminance(b); return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05); }
/* "src" (alpha in src[3]) painted over opaque "dst". */
function over(src, dst) {
  var a = src[3];
  return [src[0] * a + dst[0] * (1 - a), src[1] * a + dst[1] * (1 - a), src[2] * a + dst[2] * (1 - a)];
}
function round2(n) { return Math.round(n * 100) / 100; }
function hex(c) { return "#" + [c[0], c[1], c[2]].map(function (v) { var h = Math.round(v).toString(16); return h.length < 2 ? "0" + h : h; }).join(""); }
/* Author-chosen colour (rather than a neutral hairline/wash) at this max-min channel spread. */
function chromatic(c, min) { return Math.max(c[0], c[1], c[2]) - Math.min(c[0], c[1], c[2]) >= min; }

/*
 * Refuses to let the sweep run unless THIS source reproduces known values. The first three are the
 * standard anchors CPE-1921's Reviewer validated its independent implementation against; the last
 * two pin the COMPOSITING model, which is the half that was wrong once already (dimming an
 * element's text without dimming its own background).
 */
function validateAnchors() {
  var fails = [];
  function check(label, got, want, tol) {
    if (Math.abs(got - want) > (tol === undefined ? 0.005 : tol)) fails.push(label + ": got " + got.toFixed(4) + ", want " + want);
  }
  check("#000 on #fff", ratio([0, 0, 0], [255, 255, 255]), 21);
  check("#767676 on #fff (WCAG's 4.5:1 boundary grey)", round2(ratio([118, 118, 118], [255, 255, 255])), 4.54);
  check("#949494 on #fff (WCAG's 3:1 boundary grey)", round2(ratio([148, 148, 148], [255, 255, 255])), 3.03);
  /* Compositing anchor A: black text at 50% over white is exactly mid-grey, not black.
     rgb(127.5) is NOT "half the contrast of black on white": sRGB is gamma-encoded, so the answer is
     1.05 / (((127.5/255 + 0.055) / 1.055) ^ 2.4 + 0.05) = 3.98, hand-derivable and not 10.5. */
  check("50% black over white composites to rgb(127.5)", round2(ratio(over([0, 0, 0, 0.5], [255, 255, 255]), [255, 255, 255])), 3.98);

  /* Compositing anchor B — a VALUE check on both models, on constants that are a stated worked
     example and NOTHING ELSE. Round 1 called these numbers "the CPE-1921 round-2 mistake, stated as
     a number" while (a) only asserting that the two models DIFFER and (b) using buttonFace = 61,
     which matches no engine — this Chrome resolves dark ButtonFace to 107. Two corrections:
       - both models are now checked against their arithmetic value, so "differ" is not the whole
         claim and a model that drifts by a constant factor no longer sails through;
       - the constants are declared to be a hypothetical white-on-mid-grey-over-near-black stack,
         hand-derivable from over()/ratio() above, NOT any engine's system colours. The real
         engine-resolved version of this same comparison is computed in sweep() from the values the
         browser actually reports and printed in the report under "engine-resolved".
     text-only  : over(#fff@.75, [61,61,61]) = rgb(206.5) against rgb(61)                 -> 6.94
     both dimmed: surface first, over(#fff@.75, over([61,61,61]@.75, [18,18,18]))
                  = rgb(203.81) against rgb(50.25)                                        -> 7.94
     Dimming only the text UNDERSTATES the surface's own loss, so it is the lower of the two; that
     direction is engine-independent and is asserted as well. */
  var base = [18, 18, 18];
  var surface = [61, 61, 61];
  var white = [255, 255, 255];
  var textOnly = ratio(over([white[0], white[1], white[2], 0.75], surface), surface);
  var dimmedSurface = over([surface[0], surface[1], surface[2], 0.75], base);
  var bothDimmed = ratio(over([white[0], white[1], white[2], 0.75], dimmedSurface), dimmedSurface);
  check("opacity .75, text-only model (white on rgb(61) worked example)", round2(textOnly), 6.94);
  check("opacity .75, both-dimmed model (same stack over rgb(18))", round2(bothDimmed), 7.94);
  if (!(bothDimmed > textOnly)) {
    fails.push("compositing anchor B: the both-dimmed model is not above the text-only one — the opacity model is not being applied to backgrounds");
  }
  if (fails.length) throw new Error("WCAG anchor validation FAILED — every number below would be untrustworthy:\n  " + fails.join("\n  "));
  return {
    "#000/#fff": round2(ratio([0, 0, 0], [255, 255, 255])),
    "#767676/#fff": round2(ratio([118, 118, 118], [255, 255, 255])),
    "#949494/#fff": round2(ratio([148, 148, 148], [255, 255, 255])),
    "50% black/white": round2(ratio(over([0, 0, 0, 0.5], [255, 255, 255]), [255, 255, 255])),
    "opacity .75 text-only (WRONG model)": round2(textOnly),
    "opacity .75 both dimmed (engine model)": round2(bothDimmed),
  };
}
`;

const MATH = new Function(
  COLOR_MATH_SOURCE +
    "\nreturn { luminance: luminance, ratio: ratio, over: over, round2: round2, hex: hex, chromatic: chromatic, validateAnchors: validateAnchors };",
)();
export const luminance = MATH.luminance;
export const ratio = MATH.ratio;
export const over = MATH.over;
export const round2 = MATH.round2;
export const hex = MATH.hex;
export const chromatic = MATH.chromatic;
export const validateAnchors = MATH.validateAnchors;

/**
 * The same `validateAnchors`, run by the ENGINE on the source string the probe is about to use, with
 * its result required to match Node's exactly. Not a second validator — a second execution of the
 * one validator, which is what catches the string being mangled in transit rather than being wrong.
 */
export async function validateAnchorsInPage(client, nodeAnchors) {
  const inPage = await client.evaluate(
    `(function () { ${COLOR_MATH_SOURCE}\nreturn JSON.stringify(validateAnchors()); })()`,
  );
  const want = JSON.stringify(nodeAnchors);
  if (inPage !== want) {
    throw new Error(
      "WCAG anchor validation FAILED IN THE PAGE — the maths Node validated is not the maths the probe will run.\n" +
        `  node: ${want}\n  page: ${inPage}`,
    );
  }
  return JSON.parse(inPage);
}

// ── The launcher, prepared for measurement ───────────────────────────────────────────────────────
/**
 * The launcher with its own <script> blocks removed. The scripts talk to a host process that is not
 * running here; leaving them in makes the measured DOM depend on how far a failing websocket handshake
 * happens to get, which is not a property of the stylesheet. What they BUILD is supplied deterministically
 * by fixtures.mjs instead, and `checkFixtureProvenance` pins those fixtures to the real builder source.
 */
export function preparedLauncherHtml() {
  const raw = readFileSync(LAUNCHER, "utf8");
  return raw.replace(/<script\b[\s\S]*?<\/script>/g, "<!-- script removed by the CPE-1966 contrast harness -->");
}

/**
 * Every `<script>` body in launcher.html, in document order, with the HTML around them dropped.
 *
 * The distinction is the whole of CPE-1966 round 3's second blocker. `checkFixtureProvenance` always
 * extracted the bodies first; `sessionChipColours` did not — it ran the JS tokenizer over the ENTIRE
 * HTML DOCUMENT. HTML prose is not JavaScript, and one apostrophe in it (`<p>the agent's log</p>`,
 * outside every script) opened a string literal that ran until the next `'` anywhere in the file:
 * measured, that changed the stripped output by 11,872 characters, net DELETION, and two apostrophes
 * re-synced. The palette parse survived only because the swallowed region happened to be copied
 * through verbatim as a phantom string — luck, not design. A JS scanner is only ever pointed at JS now.
 */
export function launcherScriptBodies(raw = readFileSync(LAUNCHER, "utf8")) {
  return htmlScriptBodies(raw);
}

/**
 * Those bodies with comments stripped, through the shared module's parse backstop.
 *
 * ── Why the comments come out (CPE-1933 rule 2, in the file that cites it) ────────────────────────
 * Round 1's `checkFixtureProvenance` ran `scripts.includes(claim)` over the RAW script bodies.
 * Renaming the launcher's Close-all class to `closeAllBtn` reds correctly (exit 2, before any
 * measurement) — but the same rename PLUS `// historical note: this used to read b.className =
 * "close-all-btn"` passed green, exit 0, counts unchanged, with vitest alongside it: both layers
 * vouching for a button whose class the stylesheet does not style, while the harness measured a
 * `.close-all-btn` fixture the app no longer renders. A comment is prose; the claim is about code.
 *
 * ── Why the stripper is not in this file any more ─────────────────────────────────────────────────
 * It was, for two rounds: this repo's SIXTH private hand-rolled stripper, imported nowhere and
 * exercised only by "the provenance check passed" in one CI job — and wrong in seven adversarial
 * shapes, four of which DELETED real code (`return /[//]/;` lost the rest of its line; `return /[/*]/;`
 * ate everything to the next block-comment terminator). It lives in `src/lib/jsSource.mjs` now, beside
 * the shell and Rust ones, with `src/lib/jsSource.test.ts` pinning every one of those shapes and a
 * `vm.Script` oracle for the ones nobody has thought of.
 */
export function strippedLauncherScripts(raw = readFileSync(LAUNCHER, "utf8")) {
  return stripScriptBodiesChecked(launcherScriptBodies(raw)).join("\n");
}

/** CPE-1933: a fixture claiming to mirror the launcher's own JS must be checked against that JS. */
export function checkFixtureProvenance() {
  const scripts = strippedLauncherScripts();
  if (scripts.length < 2000) throw new Error("launcher.html: no <script> body found — the fixture provenance check is broken, not passing");
  const missing = [];
  for (const f of FIXTURES) {
    for (const claim of f.derivedFrom ?? []) {
      if (!scripts.includes(claim)) missing.push(`${f.name}: launcher.html's script no longer contains ${JSON.stringify(claim)}`);
    }
  }
  if (missing.length) {
    throw new Error(
      "fixture provenance FAILED — a fixture mirrors launcher JS that no longer exists, so it is measuring\n" +
        "a shape the app never renders (CPE-1933: derive provenance, don't claim it):\n  " + missing.join("\n  "),
    );
  }
  return scripts.length;
}

/**
 * The session-identity palette, read out of launcher.html's own script rather than copied here.
 * `sessionColor()` picks an entry by hash, so the chip's white numeral can land on ANY of them —
 * measuring one sampled colour would measure the luck of the sample.
 *
 * Reads the SCRIPT BODIES (see `launcherScriptBodies`), never the whole document: a commented-out
 * older palette must not be the one this parse finds, and the HTML prose around the scripts must not
 * be able to shift the parse at all.
 */
export function sessionChipColours(document = readFileSync(LAUNCHER, "utf8")) {
  const raw = strippedLauncherScripts(document);
  const m = raw.match(/const SESSION_CHIP_COLORS = \[([^\]]*)\]/);
  if (!m) throw new Error("launcher.html: SESSION_CHIP_COLORS not found — the chip fixture cannot be derived");
  const colours = [...m[1].matchAll(/"(#[0-9a-fA-F]{3,8})"/g)].map((x) => x[1]);
  if (colours.length < 2) throw new Error(`launcher.html: SESSION_CHIP_COLORS parsed to ${colours.length} entries — the parse is broken`);
  return colours;
}

/** Substitutes the derived palette into the fixtures that need it. */
export function expandFixtures() {
  const palette = sessionChipColours();
  const chips = palette
    .map((c, i) => `<div class="tab" data-fixture="palette-${i}"><span class="tab-chip" style="background:${c}">${i + 1}</span><span class="tab-label">session ${i + 1}</span></div>`)
    .join("");
  return FIXTURES.map((f) => ({
    ...f,
    html: (f.html ?? "")
      .replace("__PALETTE_CHIPS__", chips)
      .replace(/__PALETTE_0__/g, palette[0])
      .replace(/__PALETTE_1__/g, palette[1]),
  }));
}

/** Every class/id the launcher's own stylesheet styles — the enumeration the fixtures must satisfy. */
export function styledSelectorsFromCss() {
  const raw = readFileSync(LAUNCHER, "utf8");
  const css = [...raw.matchAll(/<style>([\s\S]*?)<\/style>/g)]
    .map((m) => m[1])
    .filter((s) => !s.includes("__XTERM_CSS__"))
    .join("\n")
    .replace(/\/\*[\s\S]*?\*\//g, "");
  if (css.trim().length < 500) throw new Error("launcher.html: no stylesheet found — the <style> scrape is broken");
  const names = new Set();
  for (const r of cssRules(css)) {
    for (const m of r.selector.matchAll(/\.([A-Za-z][\w-]*)/g)) names.add(m[1]);
  }
  return { css, classNames: [...names].sort(), rules: cssRules(css) };
}

/** Brace-balanced `selector { ... }` extraction, flattening at-rule bodies in so rules inside
 *  `@media (prefers-color-scheme: dark)` / `(prefers-reduced-motion)` are reachable. */
export function cssRules(source) {
  const out = [];
  let i = 0;
  while (i < source.length) {
    const open = source.indexOf("{", i);
    if (open === -1) break;
    let depth = 0, close = -1;
    for (let j = open; j < source.length; j++) {
      if (source[j] === "{") depth++;
      else if (source[j] === "}" && --depth === 0) { close = j; break; }
    }
    if (close === -1) break;
    const selector = source.slice(i, open).trim();
    const body = source.slice(open + 1, close);
    if (selector.startsWith("@")) {
      if (/^@(media|supports)/.test(selector)) out.push(...cssRules(body));
    } else if (selector) out.push({ selector, body });
    i = close + 1;
  }
  return out;
}

// ── CDP plumbing (same shape as scripts/dev-harness/layout-guard/engine.mjs) ──────────────────────
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
          resolve: (v) => { clearTimeout(timer); resolve(v); },
          reject: (e) => { clearTimeout(timer); reject(e); },
        });
        ws.send(JSON.stringify({ id, method, params }));
      });
    },
    async evaluate(expression) {
      const r = await this.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
      if (r.exceptionDetails) {
        throw new Error("in-page probe threw: " + (r.exceptionDetails.exception?.description ?? JSON.stringify(r.exceptionDetails)));
      }
      return r.result.value;
    },
  };
}

async function waitForHttp(url, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      if ((await fetch(url)).ok) return true;
    } catch { /* not up yet */ }
    await sleep(150);
  }
  return false;
}

// ── The in-page probe ────────────────────────────────────────────────────────────────────────────
// Kept as a source string (it crosses into the browser); a pure function of its JSON config, with no
// closure over Node state — the same discipline layout-guard/engine.mjs's `buildProbeExpression` uses.
const PROBE_SOURCE = String.raw`
(function (cfg) {
  /* The splice point below receives luminance / ratio / over / hex / chromatic / round2 /
     validateAnchors from COLOR_MATH_SOURCE, pasted in by probeExpr(). There is no second copy: it is
     the same text Node evaluated and ran validateAnchors() against before Chrome was even started.
     (Deliberately NOT naming the placeholder in this comment — the splice replaces the first
     occurrence, and the pasted source contains block comments whose own terminator would end this
     one early. Measured: it did, and the page threw "Invalid or unexpected token".) */
  __COLOR_MATH__

  function parseColor(s) {
    if (!s || s === "transparent" || s === "none") return null;
    var m = s.match(/^rgba?\(([^)]+)\)$/);
    if (m) {
      var p = m[1].split(/[,\s\/]+/).filter(function (x) { return x.length; }).map(Number);
      return [p[0], p[1], p[2], p.length > 3 ? p[3] : 1];
    }
    var c = s.match(/^color\(srgb ([^)]+)\)$/);
    if (c) {
      var q = c[1].split(/[\s\/]+/).filter(function (x) { return x.length; }).map(Number);
      return [q[0] * 255, q[1] * 255, q[2] * 255, q.length > 3 ? q[3] : 1];
    }
    return null;
  }
  /* Thin wrappers so the site loop below reads the way it always did, while the arithmetic itself
     comes from the shared source above. hx() and chrom() add no maths — a rename and a bound arg. */
  function hx(c) { return hex(c); }
  function chrom(c) { return chromatic(c, cfg.chromaMin); }

  /* The ground under an element, composited in real paint order: every ancestor background in turn,
     each dimmed by the PRODUCT of the opacities of it and everything above it. Returns the ground the
     element's own text sits on ("inside", which includes the element's own background), the ground
     OUTSIDE its border box ("outside", which does not, and is not affected by the element's own
     opacity), and the accumulated alpha its own painted colours are multiplied by. */
  function groundOf(el) {
    var chain = [];
    for (var n = el; n; n = n.parentElement) chain.push(n);
    chain.reverse();
    var g = [255, 255, 255];      /* the UA canvas base, before any author background */
    var acc = 1, outside = g, outsideAcc = 1, inlineGround = false;
    for (var i = 0; i < chain.length; i++) {
      var cs = getComputedStyle(chain[i]);
      if (chain[i] === el) { outside = g; outsideAcc = acc; }
      var op = parseFloat(cs.opacity);
      if (!isFinite(op)) op = 1;
      acc *= op;
      var bg = parseColor(cs.backgroundColor);
      if (bg && bg[3] > 0) {
        g = over([bg[0], bg[1], bg[2], bg[3] * acc], g);
        /* Remember whether the ground was painted by an INLINE style. The launcher's JS assigns
           "chip.style.background = sessionColor(id)" from a shared identity palette that also drives
           the main app (src/lib/sessionChip.ts); that colour is not authored in this stylesheet, so
           the harness measures and reports it but does not enforce it here. */
        if (chain[i].style && (chain[i].style.backgroundColor || chain[i].style.background)) inlineGround = true;
      }
    }
    return { inside: g, outside: outside, alpha: acc, outsideAlpha: outsideAcc, inlineGround: inlineGround };
  }

  function pathOf(el) {
    var s = el.tagName.toLowerCase();
    if (el.id) s += "#" + el.id;
    if (el.className && typeof el.className === "string") s += "." + el.className.trim().split(/\s+/).join(".");
    var p = el.parentElement;
    if (p && p !== document.body) {
      var ps = p.tagName.toLowerCase() + (p.id ? "#" + p.id : (p.className && typeof p.className === "string" ? "." + p.className.trim().split(/\s+/)[0] : ""));
      s = ps + " > " + s;
    }
    return s;
  }

  function hasOwnText(el) {
    for (var i = 0; i < el.childNodes.length; i++) {
      var n = el.childNodes[i];
      if (n.nodeType === 3 && n.textContent.trim().length) return true;
    }
    return ["INPUT", "SELECT", "TEXTAREA"].indexOf(el.tagName) >= 0;
  }

  function barFor(cs) {
    var size = parseFloat(cs.fontSize) || 16;
    var w = parseInt(cs.fontWeight, 10) || 400;
    var large = size >= 24 || (size >= 18.66 && w >= 700);
    return { bar: large ? 3 : 4.5, size: size, weight: w, large: large };
  }

  function shadowColours(str) {
    /* getComputedStyle serialises box-shadow as "rgb(...) 0px 2px 0px 0px inset, ..." */
    var out = [];
    if (!str || str === "none") return out;
    var re = /(rgba?\([^)]*\)|color\(srgb[^)]*\))([^,]*)/g, m;
    while ((m = re.exec(str))) {
      var c = parseColor(m[1]);
      if (c) out.push({ colour: c, inset: /\binset\b/.test(m[2]) });
    }
    return out;
  }

  var sites = [];
  var els = document.querySelectorAll("*");
  for (var i = 0; i < els.length; i++) {
    var el = els[i];
    var tag = el.tagName;
    if (tag === "SCRIPT" || tag === "STYLE" || tag === "HEAD" || tag === "META" || tag === "TITLE" || tag === "LINK") continue;
    if (el.namespaceURI && el.namespaceURI.indexOf("svg") >= 0) continue;
    if (el.closest("svg")) continue;
    if (cfg.only && cfg.only.length && !el.matches(cfg.only.join(","))) continue;

    var cs = getComputedStyle(el);
    if (cs.visibility === "hidden") continue;
    var cid = el.getAttribute("data-cid");
    var g = groundOf(el);
    var b = barFor(cs);
    var inlineSelf = !!(el.style && (el.style.backgroundColor || el.style.background || el.style.color || el.style.borderColor));
    var base = {
      cid: cid, path: pathOf(el), size: b.size, weight: b.weight, ground: hx(g.inside), outside: hx(g.outside),
      alpha: Math.round(g.alpha * 1000) / 1000, inlineGround: g.inlineGround, inlineSelf: inlineSelf,
    };

    /* TEXT */
    if (hasOwnText(el)) {
      var fg = parseColor(cs.color);
      if (fg && fg[3] * g.alpha > 0.02) {
        var painted = over([fg[0], fg[1], fg[2], fg[3] * g.alpha], g.inside);
        sites.push(Object.assign({}, base, {
          role: "text", prop: "color", declared: cs.color, painted: hx(painted),
          against: hx(g.inside), ratio: ratio(painted, g.inside), bar: b.bar, large: b.large,
        }));
      }
    }

    /* BORDERS — one site per distinct (side colour, width) so a single-side accent bar is not averaged away */
    var seenBorder = {};
    ["Top", "Right", "Bottom", "Left"].forEach(function (side) {
      var w = parseFloat(cs["border" + side + "Width"]) || 0;
      var style = cs["border" + side + "Style"];
      if (w <= 0 || style === "none" || style === "hidden") return;
      var bc = parseColor(cs["border" + side + "Color"]);
      if (!bc || bc[3] * g.alpha <= 0.02) return;
      var key = cs["border" + side + "Color"];
      if (seenBorder[key]) return;
      seenBorder[key] = 1;
      var painted = over([bc[0], bc[1], bc[2], bc[3] * g.alpha], g.inside);
      /* A border declared the SAME colour as the element's own background is not a boundary — it is
         the fill, extended by a pixel (button.primary sets "background: var(--accent);
         border-color: var(--accent)"). Comparing it against the interior it "encloses" asks whether
         a colour contrasts with itself, which is 1:1 by construction and says nothing. The fill's
         own site already measures that surface against what it sits on. Exact string equality, not
         a tolerance: a near-match is a real (if odd) boundary and stays measured. */
      var partOfFill = cs.backgroundColor === key;
      sites.push(Object.assign({}, base, {
        role: "border", prop: "border-" + side.toLowerCase() + "-color", declared: key, painted: hx(painted),
        against: hx(g.inside), ratio: partOfFill ? null : ratio(painted, g.inside), partOfFill: partOfFill,
        againstOutside: hx(g.outside), ratioOutside: ratio(over([bc[0], bc[1], bc[2], bc[3] * g.alpha], g.outside), g.outside),
        bar: 3, chromatic: chrom(bc),
      }));
    });

    /* OUTLINE */
    var ow = parseFloat(cs.outlineWidth) || 0;
    if (ow > 0 && cs.outlineStyle !== "none") {
      var oc = parseColor(cs.outlineColor);
      if (oc && oc[3] * g.alpha > 0.02) {
        var op2 = over([oc[0], oc[1], oc[2], oc[3] * g.alpha], g.outside);
        sites.push(Object.assign({}, base, {
          role: "outline", prop: "outline-color", declared: cs.outlineColor, painted: hx(op2),
          against: hx(g.outside), ratio: ratio(op2, g.outside), bar: 3, chromatic: chrom(oc),
        }));
      }
    }

    /* BOX-SHADOW */
    shadowColours(cs.boxShadow).forEach(function (s, k) {
      if (s.colour[3] * g.alpha <= 0.02) return;
      var dst = s.inset ? g.inside : g.outside;
      var painted = over([s.colour[0], s.colour[1], s.colour[2], s.colour[3] * g.alpha], dst);
      sites.push(Object.assign({}, base, {
        role: s.inset ? "shadow-inset" : "shadow", prop: "box-shadow[" + k + "]", declared: "rgba(" + s.colour.join(",") + ")",
        painted: hx(painted), against: hx(dst), ratio: ratio(painted, dst), bar: 3, chromatic: chrom(s.colour),
      }));
    });

    /* FILL — an element's own background against what it sits on */
    var ownBg = parseColor(cs.backgroundColor);
    if (ownBg && ownBg[3] * g.alpha > 0.02) {
      var pf = over([ownBg[0], ownBg[1], ownBg[2], ownBg[3] * g.alpha], g.outside);
      sites.push(Object.assign({}, base, {
        role: "fill", prop: "background-color", declared: cs.backgroundColor, painted: hx(pf),
        against: hx(g.outside), ratio: ratio(pf, g.outside), bar: 3, chromatic: chrom(ownBg),
      }));
    }
  }

  return JSON.stringify({
    state: cfg.state || "base",
    canvas: (function () { var d = document.createElement("div"); d.style.background = "Canvas"; document.body.appendChild(d); var v = getComputedStyle(d).backgroundColor; d.remove(); return v; })(),
    field: (function () { var d = document.createElement("div"); d.style.background = "Field"; document.body.appendChild(d); var v = getComputedStyle(d).backgroundColor; d.remove(); return v; })(),
    buttonFace: (function () { var d = document.createElement("div"); d.style.background = "ButtonFace"; document.body.appendChild(d); var v = getComputedStyle(d).backgroundColor; d.remove(); return v; })(),
    canvasText: (function () { var d = document.createElement("div"); d.style.color = "CanvasText"; document.body.appendChild(d); var v = getComputedStyle(d).color; d.remove(); return v; })(),
    sites: sites,
  });
})
`;

/** The probe, with the ONE copy of the colour maths spliced in. Refuses to build an expression that
 *  did not receive it — a probe with the placeholder still in it would be a `ReferenceError` at best
 *  and, if the name ever resolved to something else, a silent second implementation at worst. */
function probeExpr(cfg) {
  if (!PROBE_SOURCE.includes("__COLOR_MATH__")) {
    throw new Error("probe source no longer has the __COLOR_MATH__ splice point — it would run its own maths");
  }
  return `(${PROBE_SOURCE.replace("__COLOR_MATH__", COLOR_MATH_SOURCE)})(${JSON.stringify(cfg)})`;
}

// ── Page setup run inside the browser ────────────────────────────────────────────────────────────
const SETUP_SOURCE = String.raw`
(function (fixtures) {
  var mounted = [], failed = [];
  fixtures.forEach(function (f) {
    var parent = document.querySelector(f.parent);
    if (!parent) { failed.push(f.name + ": parent " + f.parent + " not in the launcher's markup"); return; }
    if (f.mode === "classes-on-self") {
      /* One clone of the element per state class, so all three states are measured at once. */
      f.classes.forEach(function (c) {
        var clone = parent.cloneNode(false);
        clone.removeAttribute("id");
        clone.setAttribute("data-fixture-of", f.parent);
        clone.className = ((parent.className || "") + " " + c).trim();
        clone.textContent = f.html || parent.textContent || "Status text";
        parent.parentElement.insertBefore(clone, parent.nextSibling);
      });
      mounted.push(f.name);
      return;
    }
    if (f.applyToParent) parent.classList.add(f.applyToParent);
    parent.insertAdjacentHTML("beforeend", f.html);
    mounted.push(f.name);
  });

  /* HIDDEN PANELS — the second of CPE-1921's four structural blind spots, and the one whose fix is
     "do nothing", which is worth stating explicitly so nobody later "fixes" it by un-hiding things.
     The launcher hides its overlays with the "hidden" attribute and #view-bar with "display: none".
     Neither affects a single COLOUR: "getComputedStyle" resolves "color", "background-color",
     "border-color" and "opacity" for a "display: none" element exactly as it would when shown, and
     "groundOf"'s ancestor walk reads those same computed values — so #keys-panel's contents ARE
     measured, on the true ground (#help-overlay's rgba(0,0,0,.45) scrim included, which is precisely
     the assumption CPE-1966 asked to be checked). Un-hiding them would be actively WRONG: four
     "position: fixed; inset: 0" scrims would stack on top of each other and of the page, and a
     --verify-pixels run would then screenshot four scrims instead of the launcher (measured: it
     turned a predicted #ffffff ground into a painted #242424 one).
     #view-bar is the exception that IS shown: "display: none" is its zero-session state, it becomes
     "display: flex" the moment a second session exists, and showing it costs nothing because it is
     in normal flow rather than an overlay. */
  var vb = document.getElementById("view-bar");
  if (vb) vb.style.display = "flex";

  /* Stable identity across probes: index in document order. */
  var all = document.querySelectorAll("*");
  for (var i = 0; i < all.length; i++) all[i].setAttribute("data-cid", String(i));

  /* Which stylesheet class names actually match something now (the fixture-completeness enumeration). */
  var matched = {};
  (window.__CLASS_NAMES__ || []).forEach(function (c) {
    try { matched[c] = !!document.querySelector("." + CSS.escape(c)); } catch (e) { matched[c] = false; }
  });

  return JSON.stringify({ mounted: mounted, failed: failed, matched: matched, count: all.length });
})
`;

const ANIM_SOURCE = String.raw`
(function (frac) {
  var anims = document.getAnimations();
  var touched = [];
  anims.forEach(function (a) {
    try {
      a.pause();
      var d = a.effect && a.effect.getComputedTiming ? a.effect.getComputedTiming().duration : 0;
      if (typeof d !== "number" || !isFinite(d) || d <= 0) return;
      a.currentTime = frac * d;
      var t = a.effect.target;
      if (t && t.getAttribute) { var s = "[data-cid=\"" + t.getAttribute("data-cid") + "\"]"; if (touched.indexOf(s) < 0) touched.push(s); }
    } catch (e) { /* an animation we cannot step is reported as untouched */ }
  });
  return JSON.stringify({ count: anims.length, targets: touched });
})
`;

// ── Screenshot path (the independent second measurement) ─────────────────────────────────────────
/** Minimal PNG decoder: enough for Chrome's 8-bit RGBA screenshots. No dependency, by design. */
export function decodePng(buf) {
  if (buf.readUInt32BE(0) !== 0x89504e47) throw new Error("not a PNG");
  let pos = 8;
  let width = 0, height = 0, bitDepth = 0, colorType = 0;
  const idat = [];
  while (pos < buf.length) {
    const len = buf.readUInt32BE(pos);
    const type = buf.toString("ascii", pos + 4, pos + 8);
    const data = buf.subarray(pos + 8, pos + 8 + len);
    if (type === "IHDR") {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      bitDepth = data[8];
      colorType = data[9];
      if (bitDepth !== 8 || (colorType !== 6 && colorType !== 2)) throw new Error(`unsupported PNG: depth ${bitDepth} colour type ${colorType}`);
    } else if (type === "IDAT") idat.push(data);
    else if (type === "IEND") break;
    pos += 12 + len;
  }
  const channels = colorType === 6 ? 4 : 3;
  const raw = zlib.inflateSync(Buffer.concat(idat));
  const stride = width * channels;
  const out = Buffer.alloc(height * stride);
  let rp = 0;
  for (let y = 0; y < height; y++) {
    const filter = raw[rp++];
    const line = raw.subarray(rp, rp + stride);
    rp += stride;
    const cur = out.subarray(y * stride, (y + 1) * stride);
    const prev = y > 0 ? out.subarray((y - 1) * stride, y * stride) : null;
    for (let x = 0; x < stride; x++) {
      const a = x >= channels ? cur[x - channels] : 0;
      const b = prev ? prev[x] : 0;
      const c = x >= channels && prev ? prev[x - channels] : 0;
      let v = line[x];
      if (filter === 1) v += a;
      else if (filter === 2) v += b;
      else if (filter === 3) v += (a + b) >> 1;
      else if (filter === 4) {
        const p = a + b - c;
        const pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - c);
        v += pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      } else if (filter !== 0) throw new Error(`unsupported PNG filter ${filter}`);
      cur[x] = v & 0xff;
    }
  }
  return { width, height, channels, data: out };
}
export function pixelAt(png, x, y) {
  const i = y * png.width * png.channels + x * png.channels;
  return [png.data[i], png.data[i + 1], png.data[i + 2]];
}

/**
 * Serves `html` on 127.0.0.1, walking a short list of candidate ports rather than betting the run on
 * one derived from the pid. `EADDRINUSE`/`EACCES` on a given port is an environment fact, not a
 * finding, and retrying is the difference between a flaky red and a green run.
 */
async function listenWithRetry(html, requested) {
  const candidates = requested !== undefined
    ? [requested]
    : Array.from({ length: 12 }, (_, k) => 30000 + ((process.pid * 7 + k * 1013) % 20000));
  const tried = [];
  for (const httpPort of candidates) {
    const server = http.createServer((req, res) => {
      res.writeHead(200, { "content-type": "text/html; charset=utf-8", "cache-control": "no-store" });
      res.end(html);
    });
    try {
      await new Promise((resolve, reject) => {
        server.once("error", reject);
        server.listen(httpPort, "127.0.0.1", () => { server.removeAllListeners("error"); resolve(); });
      });
      return { server, httpPort };
    } catch (err) {
      tried.push(`${httpPort} (${err.code || err.message})`);
      await new Promise((r) => server.close(r));
      if (requested !== undefined) throw err;
    }
  }
  throw new Error(`could not bind a local port for the launcher fixture; tried ${tried.join(", ")}`);
}

// ── The sweep ────────────────────────────────────────────────────────────────────────────────────
/**
 * Serves the prepared launcher, drives one headless Chrome through both colour schemes, and returns
 * every measured site in every reachable state.
 */
export async function sweep({ chromePath = defaultChromePath(), port, cdpPort, verifyPixels = false } = {}) {
  const anchors = validateAnchors();
  const scriptBytes = checkFixtureProvenance();
  const { classNames } = styledSelectorsFromCss();
  const html = preparedLauncherHtml();

  // `30000 + pid % 20000` with no retry is a flaky red waiting to happen: the Reviewer hit
  // `EACCES 127.0.0.1:49668` once locally, on a port this process does not get to choose. So the
  // listen is retried across a short walk of candidates and only gives up — loudly, naming every
  // port it tried — when all of them are taken, which is a real environment problem rather than
  // noise. A port passed in explicitly is honoured exactly once: the caller asked for that one.
  const { server, httpPort } = await listenWithRetry(html, port);
  const debugPort = cdpPort ?? 20000 + ((process.pid + 7919) % 20000);

  const userDataDir = path.join(REPO_ROOT, ".claude", "dev-harness-chrome-profiles", `launcher-contrast-${debugPort}-${Date.now()}`);
  const chrome = spawn(
    chromePath,
    [
      "--headless=new", "--disable-gpu", "--no-sandbox", "--disable-dev-shm-usage",
      `--remote-debugging-port=${debugPort}`, `--user-data-dir=${userDataDir}`,
      "--window-size=1200,900", "--hide-scrollbars", "about:blank",
    ],
    { stdio: ["ignore", "ignore", "ignore"] },
  );
  // Without this, `CHROME_PATH=/nonexistent/chrome.exe` is an UNHANDLED 'error' event: Node exits 1
  // with an ENOENT stack, the `finally` below never runs (Chrome unkilled, server open, profile dir
  // left on disk), and exit 1 is indistinguishable from "a colour regressed". Capturing it turns the
  // same input into the intended exit 2 with the path named, and lets the cleanup run.
  // Raced against the endpoint wait below rather than merely recorded, so a bad path fails in
  // milliseconds with the right message instead of after the full 40s endpoint timeout.
  const spawnFailed = new Promise((_, reject) => {
    chrome.once("error", (err) =>
      reject(new Error(`could not start Chrome at ${chromePath}: ${err.message}\n` +
        "Set CHROME_PATH to an installed Chrome/Chromium binary.")));
  });
  spawnFailed.catch(() => {});   // never an unhandled rejection if the race is already settled

  const schemes = {};
  try {
    const up = await Promise.race([
      waitForHttp(`http://127.0.0.1:${debugPort}/json/version`, CDP_ENDPOINT_TIMEOUT_MS),
      spawnFailed,
    ]);
    if (!up) {
      throw new Error(`chrome CDP endpoint on ${debugPort} never came up within ${CDP_ENDPOINT_TIMEOUT_MS}ms`);
    }
    const targets = await (await fetch(`http://127.0.0.1:${debugPort}/json/list`)).json();
    const target = targets.find((t) => t.type === "page") || targets[0];
    const ws = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise((resolve, reject) => {
      ws.addEventListener("open", resolve, { once: true });
      ws.addEventListener("error", reject, { once: true });
    });
    const client = makeCdpClient(ws);
    await client.send("Page.enable");
    await client.send("DOM.enable");
    await client.send("CSS.enable");
    await client.send("Emulation.setFocusEmulationEnabled", { enabled: true });

    for (const scheme of ["light", "dark"]) {
      await client.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: scheme }] });
      const loaded = new Promise((resolve) => {
        const onMsg = (ev) => {
          if (JSON.parse(ev.data).method === "Page.loadEventFired") { ws.removeEventListener("message", onMsg); resolve(); }
        };
        ws.addEventListener("message", onMsg);
      });
      await client.send("Page.navigate", { url: `http://127.0.0.1:${httpPort}/launcher.html?scheme=${scheme}` });
      await loaded;

      // The anchors again, this time executed BY THE ENGINE on the same source string the probe is
      // about to run, with the result required to match Node's exactly (see COLOR_MATH_SOURCE).
      await validateAnchorsInPage(client, anchors);

      await client.evaluate(`window.__CLASS_NAMES__ = ${JSON.stringify(classNames)};`);
      const setup = JSON.parse(await client.evaluate(`(${SETUP_SOURCE})(${JSON.stringify(expandFixtures())})`));
      if (setup.failed.length) throw new Error("fixture mount FAILED:\n  " + setup.failed.join("\n  "));

      // cid -> nodeId, aligned by document order (both come from the same `*` traversal).
      const { root } = await client.send("DOM.getDocument", { depth: -1 });
      const { nodeIds } = await client.send("DOM.querySelectorAll", { nodeId: root.nodeId, selector: "*[data-cid]" });
      if (nodeIds.length !== setup.count) {
        throw new Error(`cid/nodeId alignment broken: ${setup.count} tagged elements but ${nodeIds.length} nodeIds`);
      }

      const base = JSON.parse(await client.evaluate(probeExpr({ chromaMin: CHROMA_MIN, state: "base" })));
      const all = base.sites.map((s) => ({ ...s, state: "base", scheme }));

      // ── STATES ────────────────────────────────────────────────────────────────────────────────
      // One element at a time through CDP's own `CSS.forcePseudoState`: this is the engine applying
      // the state, not a rewritten stylesheet, so nothing about the cascade has to be simulated.
      const stateRules = pseudoRulesFromCss();
      const forced = [];
      // NOT a bare `catch { continue; }` any more (round-3 blocker 1). A CDP change that made
      // `DOM.querySelectorAll` reject the launcher's selectors would have skipped every rule
      // SILENTLY: the log would print "0 forced pseudo-state readings" and the run would still say
      // PASS. What a guard swallowed is now counted, named, and failed on in run.mjs — "did not run"
      // is not "found nothing", and that is the rule this whole leg exists under.
      const stateSkips = [];
      for (const { pseudo, base: baseSel } of stateRules) {
        let matchIds = [];
        try {
          matchIds = (await client.send("DOM.querySelectorAll", { nodeId: root.nodeId, selector: baseSel })).nodeIds;
        } catch (err) {
          stateSkips.push(`${baseSel}:${pseudo} — DOM.querySelectorAll refused the selector: ${err?.message ?? String(err)}`);
          continue;
        }
        for (const nodeId of matchIds) {
          await client.send("CSS.forcePseudoState", { nodeId, forcedPseudoClasses: [pseudo] });
          const cid = nodeIds.indexOf(nodeId);
          const only = cid >= 0 ? [`[data-cid="${cid}"]`, `[data-cid="${cid}"] *`] : [baseSel, `${baseSel} *`];
          const r = JSON.parse(await client.evaluate(probeExpr({ chromaMin: CHROMA_MIN, state: `:${pseudo}`, only })));
          for (const s of r.sites) all.push({ ...s, state: `${baseSel}:${pseudo}`, pseudo, scheme });
          await client.send("CSS.forcePseudoState", { nodeId, forcedPseudoClasses: [] });
          forced.push(`${baseSel}:${pseudo}`);
        }
      }

      // ── ANIMATION FRAMES ──────────────────────────────────────────────────────────────────────
      // Every CSS animation paused and stepped across its own duration. A single static frame is how
      // `.boot-label`'s trough (site 3) stayed invisible.
      //
      // `animMeta.count` is METADATA — how many animation objects the page reports — and round 2's
      // report printed "3 CSS animations x 21 frames" straight out of it. Wrapping this whole block
      // in `if (false && …)` therefore took ZERO frames and the report still claimed the leg ran, in
      // a run that exited 0. Intent is not work. `animFrames` and `animReadings` below are counted
      // from probes that actually happened, and they are what run.mjs floors and prints.
      const animMeta = JSON.parse(await client.evaluate(`(${ANIM_SOURCE})(0)`));
      let animFrames = 0;
      if (animMeta.targets.length) {
        for (let k = 0; k < ANIM_SAMPLES; k++) {
          const frac = k / (ANIM_SAMPLES - 1);
          await client.evaluate(`(${ANIM_SOURCE})(${frac})`);
          const r = JSON.parse(await client.evaluate(probeExpr({ chromaMin: CHROMA_MIN, state: `anim@${frac.toFixed(2)}`, only: animMeta.targets })));
          for (const s of r.sites) all.push({ ...s, state: `animation frame ${(frac * 100).toFixed(0)}%`, animated: true, scheme });
          animFrames++;
        }
        await client.evaluate(`document.getAnimations().forEach(function(a){try{a.play()}catch(e){}});`);
      }

      let pixels = null;
      if (verifyPixels) pixels = await verifyAgainstPixels(client, all.filter((s) => s.scheme === scheme && s.state === "base" && s.role === "text"));

      schemes[scheme] = {
        canvas: base.canvas, field: base.field, buttonFace: base.buttonFace, canvasText: base.canvasText,
        // The compositing comparison again, on the colours THIS engine actually resolved rather than
        // the worked example's constants — `.area-help`'s old `opacity: .75` ButtonText-on-ButtonFace
        // stack. Recorded per scheme because it is the number that MOVES between Chrome builds: the
        // dark figures are 3.81 (text-only) / 5.07 (both dimmed) at ButtonFace rgb(107,107,107), and
        // were 4.24/4.28 on the build CPE-1966 was filed against, which resolved it near rgb(120).
        // Reported rather than asserted for exactly that reason — a hard expected value here would
        // pin the harness to one Chrome build, and the point is to make the drift legible.
        composite: engineResolvedComposite(base),
        systemColours: { Canvas: base.canvas, CanvasText: base.canvasText, Field: base.field, ButtonFace: base.buttonFace },
        sites: all, matched: setup.matched, mounted: setup.mounted, forced, animations: animMeta.count, pixels,
        // ── WORK DONE, per leg, per scheme (round-3 blocker 1) ─────────────────────────────────
        // Counted OUT OF `all` — the one array the report is actually built from — rather than from
        // each leg's own bookkeeping. That choice is the round-3 lesson repeated: a count taken
        // where the work is *intended* still reads as work. The Reviewer's base-leg sabotage was
        // `const all = []`, which leaves the base probe's own `sites.length` at 422 and would sail
        // straight through a floor on it, while the report saw nothing. Deriving all three from
        // `all` means every sabotage that empties it reds all three at once.
        // `animations` (the page's metadata) and `animFrames` (frames actually stepped) are BOTH
        // reported, side by side, so they can be compared rather than confused; only the second is
        // ever floored.
        baseReadings: all.filter((s) => s.state === "base").length,
        stateRuleCount: stateRules.length,
        stateReadings: all.filter((s) => s.pseudo).length,
        stateSkips,
        animTargets: animMeta.targets.length,
        animFrames,
        animReadings: all.filter((s) => s.animated).length,
      };
    }
    ws.close();
  } finally {
    const exited = new Promise((resolve) => chrome.once("exit", resolve));
    chrome.kill();
    await Promise.race([exited, sleep(3000)]);
    await new Promise((r) => server.close(r));
    await rm(userDataDir, { recursive: true, force: true }).catch(() => {});
  }

  return { anchors, scriptBytes, classNames, schemes, unreachable: UNREACHABLE };
}

/** `rgb(r, g, b)` / `rgba(...)` as a triple; the two forms Chrome serialises system colours in. */
function rgbTriple(s) {
  const p = String(s).match(/-?[\d.]+/g);
  return p && p.length >= 3 ? [parseFloat(p[0]), parseFloat(p[1]), parseFloat(p[2])] : null;
}

/**
 * The two opacity models applied to the system colours the browser REPORTED, so the report carries a
 * real value against a real resolved colour rather than only against a worked example's constants.
 * `.area-help` was `ButtonText` on `ButtonFace` under one `opacity: .75`; dimming only the text
 * understates the surface's own loss, so the both-dimmed figure is the higher of the two.
 */
export function engineResolvedComposite(base) {
  const face = rgbTriple(base.buttonFace);
  const text = rgbTriple(base.canvasText);   // ButtonText tracks CanvasText in both of Chrome's schemes
  const page = rgbTriple(base.canvas);
  if (!face || !text || !page) return null;
  const a = 0.75;
  const textOnly = ratio(over([...text, a], face), face);
  const dimmedFace = over([...face, a], page);
  const bothDimmed = ratio(over([...text, a], dimmedFace), dimmedFace);
  return {
    stack: `opacity .75, ${hex(text)} on ButtonFace ${hex(face)} over Canvas ${hex(page)}`,
    textOnly: round2(textOnly),
    bothDimmed: round2(bothDimmed),
    modelsDiffer: round2(textOnly) !== round2(bothDimmed),
  };
}

/** The distinct (base selector, pseudo-class) pairs the launcher's own stylesheet declares. */
export function pseudoRulesFromCss() {
  const { rules } = styledSelectorsFromCss();
  const out = new Map();
  for (const r of rules) {
    for (const part of r.selector.split(",")) {
      const p = part.trim().replace(/\s+/g, " ");
      const m = p.match(/:(hover|focus|focus-visible|active)\b/);
      if (!m) continue;
      const baseSel = p.replace(/:(hover|focus|focus-visible|active)\b/g, "").trim();
      if (!baseSel || baseSel.includes("::")) continue;
      const pseudo = m[1] === "focus-visible" ? "focus-visible" : m[1];
      out.set(`${baseSel}|${pseudo}`, { base: baseSel, pseudo });
    }
  }
  return [...out.values()];
}

/**
 * The independent second path: a real screenshot, decoded here, sampled at each site's own centre.
 * Agreement between "what the cascade says" and "what the compositor painted" is what makes the
 * computed-style numbers trustworthy — CPE-1921's Reviewer ran the same two paths and required them
 * to agree within 1/255.
 *
 * LIMITS, stated precisely because round 1's PR body claimed more than this does. The two paths are
 * genuinely independent — a from-scratch PNG decode versus the cascade — but they are compared for
 * GROUNDS ONLY, only where `role === "text"`, and only in `state === "base"`. Nothing here checks a
 * border's, fill's or shadow's painted colour, no forced state is screenshot, no animation frame is,
 * and the RATIO ARITHMETIC is cross-checked by neither path (see limit 1 in this file's header —
 * that is the gap the duplicate probe maths slid through). Disagreements FAIL the run in `run.mjs`,
 * as does a pass that verified nothing: a screenshot leg that measures zero grounds prints "0
 * verified, 0 disagreeing", which reads as success and is the repo's "did not run ≠ found nothing"
 * rule, so it is a floor rather than a shrug.
 */
async function verifyAgainstPixels(client, textSites) {
  // The boot overlay is `position: fixed; inset: 0; z-index: 9999; background: Canvas` and covers the
  // whole page until `endBoot()` fades it. It has to come out of the way for a screenshot to show the
  // launcher at all — but ONLY for the screenshot: it stays in place for the computed-style pass so
  // `.boot-label`'s animation keeps running and its trough (CPE-1966 site 3) is still sampled.
  await client.evaluate(`(function(){var b=document.getElementById("boot-overlay"); if(b) b.style.display="none";})()`);
  const shot = await client.send("Page.captureScreenshot", { format: "png" });
  const png = decodePng(Buffer.from(shot.data, "base64"));
  const rects = JSON.parse(await client.evaluate(`
    JSON.stringify(Array.prototype.map.call(document.querySelectorAll("[data-cid]"), function (e) {
      var r = e.getBoundingClientRect();
      return { cid: e.getAttribute("data-cid"), x: r.x, y: r.y, w: r.width, h: r.height };
    }))`));
  const byCid = new Map(rects.map((r) => [r.cid, r]));
  const rows = [];
  for (const s of textSites) {
    const r = byCid.get(s.cid);
    if (!r || r.w < 10 || r.h < 10) continue;
    // The MODE of a grid of interior samples, not one corner pixel. A corner lands on the border, on
    // a border-radius arc, or outside a pill entirely - measured: sampling (x+2, y+2) reported a
    // badge's #3a9d4a fill as #d2e5d5, which is the antialiased edge, not the ground. Glyphs are a
    // minority of an element's interior pixels, so the most common interior colour IS the painted
    // background - and the ground is the half of the ratio that moves when an ancestor changes.
    const counts = new Map();
    const inset = 4;
    for (let gx = 0; gx < 9; gx++) {
      for (let gy = 0; gy < 5; gy++) {
        const x = Math.round(r.x + inset + ((r.w - 2 * inset) * gx) / 8);
        const y = Math.round(r.y + inset + ((r.h - 2 * inset) * gy) / 4);
        if (x < 0 || y < 0 || x >= png.width || y >= png.height) continue;
        const k = hex(pixelAt(png, x, y));
        counts.set(k, (counts.get(k) ?? 0) + 1);
      }
    }
    if (!counts.size) continue;
    const got = [...counts.entries()].sort((a, b) => b[1] - a[1])[0][0];
    const want = s.ground;
    const delta = Math.max(...[0, 1, 2].map((i) => Math.abs(parseInt(got.slice(1 + i * 2, 3 + i * 2), 16) - parseInt(want.slice(1 + i * 2, 3 + i * 2), 16))));
    rows.push({ path: s.path, predicted: want, painted: got, delta });
  }
  await client.evaluate(`(function(){var b=document.getElementById("boot-overlay"); if(b) b.style.display="";})()`);
  return rows;
}
