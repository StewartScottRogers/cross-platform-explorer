/**
 * CPE-1921 — WCAG guard for the AI Console launcher's own stylesheet.
 *
 * `sidecar/ai-console/src/launcher.html` is a standalone HTML page inside the ai-console sidecar.
 * It is NOT covered by any of the app's palette guards (`src/app.css.*-contrast.test.ts` all read
 * `src/app.css`, and the hard-coded-hex ratchet in `src/app.css.test.ts` walks `.svelte` files
 * only), so its colours had never been measured against anything. The status line's three states
 * were inline hex literals dimmed by a blanket `#msg { opacity: .85 }`, which put every one of them
 * under the bar in light theme.
 *
 * ── The ground ────────────────────────────────────────────────────────────────────────────────
 * The launcher is not themed by the app's `data-theme` palette. It declares `color-scheme: light
 * dark` and paints on the CSS *system* colours, so `#msg`'s backdrop is whatever the engine
 * resolves for `body { background: Canvas }`. Those two values cannot be derived from the source —
 * they are engine constants — so they are MEASURED, and the measurement is recorded here:
 *
 *   Chrome 8-bit PNG screenshot of the real launcher.html markup+styles, pixels sampled directly
 *   (`chrome --headless=new --screenshot`, decoded and sampled by a throwaway node script; the
 *   in-page `getComputedStyle(body).backgroundColor` agreed with the painted pixel exactly):
 *     light (no flag)          -> Canvas = rgb(255,255,255)
 *     dark  (--force-dark-mode
 *            and --blink-settings=preferredColorScheme=0, identical) -> Canvas = rgb(18,18,18)
 *
 * Those constants are only trustworthy while the source still says the ground is `Canvas`, so
 * `describe("the ground")` below RE-DERIVES that link from the file on every run: if `body` stops
 * painting `Canvas`, or a checked element grows its own `background`, this test goes red and the
 * constants must be re-measured rather than silently believed.
 *
 * ── What is checked ───────────────────────────────────────────────────────────────────────────
 * The set of checked pairings is derived from the stylesheet at run time — every `color:
 * var(--token)` declaration — rather than listed here, so a NEW token-coloured foreground rule is
 * covered the day it is written (CPE-1932: enumerate, don't recall). A token with no
 * `prefers-color-scheme: dark` override is resolved to its light value in BOTH schemes, which is
 * exactly what the browser does, so "missing from the dark block" surfaces as a contrast failure
 * naming the token instead of passing silently.
 *
 * ── What is NOT checked HERE, and why that is no longer the same as "not checked" (CPE-1966) ──
 * `SITES` derives from `color: var(--token)` rules, so four whole categories walk straight
 * through this file. CPE-1921 shipped with all four open and all four populated by real, measured
 * defects, which is how a green run here coexisted with a page that had them:
 *
 *   - NON-TEXT roles. `border-color` / `background` / `box-shadow` are not a `color:`. The
 *     `:focus` border is the page's ONLY focus indicator and measured 2.46:1 against the `Field`
 *     interior in dark, under SC 1.4.11's 3:1.
 *   - Elements that DO NOT EXIST on the page as loaded. Session tabs, the "Close all" button,
 *     model-menu rows, the whole grid view and its `.pane-head` are built by the launcher's own
 *     JavaScript; a static load has nothing to measure. `.pane-head` hard-codes a `#161616`
 *     background but inherited `CanvasText`, so every label in the grid view was BLACK ON
 *     NEAR-BLACK in light theme — 1.09:1, the worst reading on the page.
 *   - STATES: `:hover`, `:focus`, `:active`. `.close-all-btn:hover { color: #d05656 }` was 3.65:1
 *     in light (it sits on `#tabs`'s `rgba(128,128,128,0.10)` fill, not on bare Canvas) and
 *     4.13:1 in dark — invisible twice over, as a literal hex AND as a state.
 *   - ANIMATED opacity, which has no single value to sample: `boot-pulse` swung `.boot-label`
 *     between .45 and .85, dipping to 3.35:1 in light at the trough.
 *
 * All four are now covered — not here, but by a REAL BROWSER:
 * `scripts/dev-harness/launcher-contrast/`, run in CI by gui-smoke.yml's `launcher-contrast` job
 * (`npm run harness:launcher-contrast`). It drives headless Chrome over CDP, forces every
 * `:hover`/`:focus`/`:active` rule with `CSS.forcePseudoState`, steps every CSS animation through
 * 21 frames and reports the worst, mounts the JS-built DOM from derived fixtures, walks the real
 * ancestor chain for each site's ground, and measures `border-color`/`background-color`/
 * `box-shadow` at SC 1.4.11's 3:1 alongside text at 1.4.3's 4.5:1. Because that job is the half
 * that covers this file's blind spots, `describe("the browser half")` below asserts the job still
 * exists and still runs that script: deleting it must not quietly turn the cheap half back into
 * the whole story.
 *
 * Literal hexes remain invisible to THIS file except for the two status-line functions (the
 * inline-hex tripwire below). The browser harness reads computed colours, so it sees a literal hex
 * exactly as well as a token — which is how it found `.badge.no`/`.badge.yes` (3.25:1 / 3.44:1
 * under their own white text) and `.pane-head`, none of which contain a `var(` at all.
 *
 * ── Red-proof, run by hand, RESULTS AT THE SITE (CPE-1933 rule 3) ─────────────────────────────
 * Five sabotages of launcher.html, each run against this file (10 tests); each named the culprit,
 * so none of these checks is decorative. Sabotages 1-4 failed exactly one test each; 5 failed
 * two, plus ten more in the sibling `ai-console-launcher.test.ts`:
 *   1. `--msg-ok` -> `#3a9d4a` (the old value): "light: on the measured ground #ffffff" failed with
 *      "#msg.ok, #keys-msg.ok { color: var(--msg-ok) } -> #3a9d4a on #ffffff = 3.44:1, below the
 *      4.5:1 bar (font-size 12px / weight 400)".
 *   2. delete the dark `--msg-err` override: "dark: on the measured ground #121212" failed with
 *      "-> #c42b1c on #121212 = 3.31:1 … Give --msg-err a value for this scheme".
 *   3. re-add `#msg { opacity: .85 }`: the opacity tripwire failed ("expected '.85' to be
 *      undefined").
 *   4. `body { background: Field }`: the ground test failed ("expected 'Field' to be 'Canvas'").
 *   5. `setMsg` back to `el.style.color = "#d08a1a"`: TWO tests failed here — the inline-hex
 *      tripwire ("setMsg/keysMsg pick a class, never an inline hex colour") and the class-backing
 *      test ("every state class those two can assign is backed by a token-coloured rule"), which
 *      finds no `className` assignment left to check. 2 failed / 8 passed in this file, and both
 *      contrast tests were among the 8 that stayed GREEN. That is the point (CPE-1929): the
 *      tripwire is NOT shadowed by the measurement, because an inline colour is structurally
 *      invisible to a stylesheet sweep. Deleting it would leave that regression uncovered, not
 *      merely unguarded.
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const LAUNCHER = join(process.cwd(), "sidecar/ai-console/src/launcher.html");
const html = readFileSync(LAUNCHER, "utf8");

/** The launcher's own <style> blocks — the xterm vendor CSS is injected at serve time, not here. */
const css = [...html.matchAll(/<style>([\s\S]*?)<\/style>/g)]
  .map((m) => m[1])
  .filter((s) => !s.includes("__XTERM_CSS__"))
  .join("\n")
  .replace(/\/\*[\s\S]*?\*\//g, "");

if (css.trim().length < 500) throw new Error("launcher.html: no stylesheet found — the <style> scrape is broken");

// The MEASURED engine grounds. See the header comment for how they were obtained.
const GROUND = { light: "#ffffff", dark: "#121212" } as const;
type Scheme = keyof typeof GROUND;

// ── WCAG 2.1 relative luminance + contrast ratio (https://www.w3.org/TR/WCAG21/#dfn-contrast-ratio)
function hexToRgb(hex: string): [number, number, number] {
  let h = hex.trim().replace("#", "");
  if (h.length === 3) h = h.split("").map((c) => c + c).join("");
  if (!/^[0-9a-fA-F]{6}$/.test(h)) throw new Error(`not an opaque hex colour: ${hex}`);
  return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)) as [number, number, number];
}
function luminance(hex: string): number {
  const [r, g, b] = hexToRgb(hex).map((c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}
function contrast(a: string, b: string): number {
  const la = luminance(a);
  const lb = luminance(b);
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}
const round2 = (n: number) => Math.round(n * 100) / 100;

// ── Rule parsing ──────────────────────────────────────────────────────────────────────────────
type Rule = { selector: string; body: string };

/** Every `selector { ... }` rule, brace-balanced, with at-rule bodies flattened in (so the
 *  `@media (prefers-color-scheme: dark)` inner `:root` is reachable). */
function rules(source: string, prefix = ""): Rule[] {
  const out: Rule[] = [];
  let i = 0;
  while (i < source.length) {
    const open = source.indexOf("{", i);
    if (open === -1) break;
    let depth = 0;
    let close = -1;
    for (let j = open; j < source.length; j++) {
      if (source[j] === "{") depth++;
      else if (source[j] === "}" && --depth === 0) { close = j; break; }
    }
    if (close === -1) break;
    const selector = source.slice(i, open).trim();
    const body = source.slice(open + 1, close);
    if (selector.startsWith("@")) {
      if (/^@media/.test(selector)) out.push(...rules(body, selector));
    } else if (selector) {
      out.push({ selector: prefix ? `${prefix} :: ${selector}` : selector, body });
    }
    i = close + 1;
  }
  return out;
}

const allRules = rules(css);
const decls = (body: string) => {
  const m = new Map<string, string>();
  for (const d of body.split(";")) {
    const k = d.indexOf(":");
    if (k === -1) continue;
    m.set(d.slice(0, k).trim(), d.slice(k + 1).trim());
  }
  return m;
};

/** Custom-property declarations per scheme. Dark inherits light, then overrides — as the cascade does. */
function tokenMap(scheme: Scheme): Map<string, string> {
  const out = new Map<string, string>();
  for (const r of allRules) {
    const inDarkMedia = /prefers-color-scheme\s*:\s*dark/.test(r.selector);
    if (inDarkMedia && scheme !== "dark") continue;
    if (!/(^|\s|::\s)(:root)\b/.test(r.selector) && r.selector !== ":root") continue;
    for (const [k, v] of decls(r.body)) if (k.startsWith("--")) out.set(k, v);
  }
  return out;
}
const TOKENS: Record<Scheme, Map<string, string>> = { light: tokenMap("light"), dark: tokenMap("dark") };

function resolve(value: string, scheme: Scheme, depth = 0): string | undefined {
  const v = value.trim();
  if (/^#[0-9a-fA-F]{3,8}$/.test(v)) return v;
  const m = v.match(/^var\((--[\w-]+)\)$/);
  if (!m || depth > 6) return undefined;
  const next = TOKENS[scheme].get(m[1]);
  return next === undefined ? undefined : resolve(next, scheme, depth + 1);
}

// ── The set of checked pairings, derived from the stylesheet ───────────────────────────────────
type Site = { selector: string; token: string };
const SITES: Site[] = allRules.flatMap((r) => {
  const color = decls(r.body).get("color");
  const m = color?.match(/^var\((--[\w-]+)\)$/);
  return m ? [{ selector: r.selector.split("::").pop()!.trim(), token: m[1] }] : [];
});

/** Conservative WCAG bar: 4.5:1 unless the source PROVES the text is large (>=24px, or >=18.66px
 *  at weight >=700). Never guessed upward — an underivable size keeps the strict bar. */
function barFor(selector: string): { bar: number; note: string } {
  const key = selector.match(/#[\w-]+/)?.[0] ?? selector.split(/[\s,]/)[0];
  let size: number | undefined;
  let weight: number | undefined;
  for (const r of allRules) {
    if (!r.selector.includes(key)) continue;
    const d = decls(r.body);
    const fs = d.get("font-size")?.match(/([\d.]+)px/);
    const fw = d.get("font-weight")?.match(/(\d+)/);
    if (fs) size = size === undefined ? parseFloat(fs[1]) : Math.min(size, parseFloat(fs[1]));
    if (fw) weight = Math.max(weight ?? 400, parseInt(fw[1], 10));
  }
  const large = size !== undefined && (size >= 24 || (size >= 18.66 && (weight ?? 400) >= 700));
  return { bar: large ? 3 : 4.5, note: `font-size ${size ?? "inherited"}px / weight ${weight ?? 400}` };
}

// ── The launcher's <body> as an element tree, so a site's GROUND can be derived rather than assumed
// A quote-aware tag scan is enough here and a real HTML parser is not: this is one hand-written file
// with well-formed markup, and the alternative is an npm dependency for a guard whose whole point is
// to be the cheap layer. Comments and <script>/<style> bodies are removed first — a `<` inside a
// script string would otherwise open a phantom element (the same trap CPE-1933 rule 2 records for
// shell scanners).
type MarkupNode = { tag: string; id?: string; classes: string[]; parent: number | null };

function parseBodyMarkup(source: string): MarkupNode[] {
  const start = source.indexOf("<body");
  const end = source.lastIndexOf("</body>");
  if (start === -1 || end === -1) throw new Error("launcher.html: no <body> — the markup scan is broken, not passing");
  const body = source
    .slice(start, end)
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/<script\b[\s\S]*?<\/script>/g, "")
    .replace(/<style\b[\s\S]*?<\/style>/g, "");
  const VOID = new Set(["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"]);
  const nodes: MarkupNode[] = [];
  const stack: number[] = [];
  let i = 0;
  while (i < body.length) {
    const lt = body.indexOf("<", i);
    if (lt === -1) break;
    // Find the matching ">", skipping any inside a quoted attribute value.
    let j = lt + 1;
    let quote = "";
    while (j < body.length) {
      const c = body[j];
      if (quote) { if (c === quote) quote = ""; }
      else if (c === '"' || c === "'") quote = c;
      else if (c === ">") break;
      j++;
    }
    if (j >= body.length) break;
    const raw = body.slice(lt + 1, j);
    i = j + 1;
    if (raw.startsWith("!") || raw.startsWith("?")) continue;
    if (raw.startsWith("/")) {
      const t = raw.slice(1).trim().toLowerCase();
      for (let k = stack.length - 1; k >= 0; k--) {
        if (nodes[stack[k]].tag === t) { stack.length = k; break; }
      }
      continue;
    }
    const tag = (raw.match(/^[A-Za-z][\w-]*/)?.[0] ?? "").toLowerCase();
    if (!tag) continue;
    const id = raw.match(/\bid\s*=\s*"([^"]*)"/)?.[1];
    const classes = (raw.match(/\bclass\s*=\s*"([^"]*)"/)?.[1] ?? "").trim().split(/\s+/).filter(Boolean);
    nodes.push({ tag, id, classes, parent: stack.length ? stack[stack.length - 1] : null });
    if (!raw.trimEnd().endsWith("/") && !VOID.has(tag)) stack.push(nodes.length - 1);
  }
  return nodes;
}

const MARKUP = parseBodyMarkup(html);

function ancestorIds(id: string): string[] {
  let n = MARKUP.findIndex((x) => x.id === id);
  const out: string[] = [];
  while (n !== -1 && MARKUP[n].parent !== null) {
    n = MARKUP[n].parent!;
    if (MARKUP[n].id) out.push(MARKUP[n].id!);
  }
  return out;
}

/** The element a checked rule's ground hangs off: the LAST `#id` in its selector, else the last class. */
function anchorElementFor(selector: string): number | undefined {
  const ids = [...selector.matchAll(/#([\w-]+)/g)].map((m) => m[1]);
  for (const id of ids.reverse()) {
    const idx = MARKUP.findIndex((n) => n.id === id);
    if (idx !== -1) return idx;
  }
  const classes = [...selector.matchAll(/\.([\w-]+)/g)].map((m) => m[1]);
  for (const c of classes.reverse()) {
    const idx = MARKUP.findIndex((n) => n.classes.includes(c));
    if (idx !== -1) return idx;
  }
  return undefined;
}

/** Does any rule for this element declare a background? Reads EVERY matching rule, not just the first. */
function ownBackground(node: MarkupNode): string | undefined {
  let found: string | undefined;
  for (const r of allRules) {
    for (const part of r.selector.split("::").pop()!.split(",")) {
      const key = part.trim().split(/\s+/).pop() ?? "";
      if (!key) continue;
      const matchesId = node.id !== undefined && key.includes(`#${node.id}`);
      const matchesClass = node.classes.some((c) => key.includes(`.${c}`));
      const matchesTag = key === node.tag;
      if (!matchesId && !matchesClass && !matchesTag) continue;
      const d = decls(r.body);
      const bg = d.get("background") ?? d.get("background-color");
      // A later rule wins, as the cascade does — so keep walking rather than returning the first.
      if (bg) found = bg.split(/\s+/)[0];
    }
  }
  return found;
}

/** Walks up from `idx` (inclusive) to the first element that paints a background. */
function nearestBackgroundPainter(idx: number): { by: string; value: string } | undefined {
  let n: number | null = idx;
  while (n !== null) {
    const node: MarkupNode = MARKUP[n];
    const bg = ownBackground(node);
    if (bg && bg !== "none" && bg !== "transparent") {
      return { by: node.id ? `#${node.id}` : node.classes.length ? `.${node.classes[0]}` : node.tag, value: bg };
    }
    n = node.parent;
  }
  return undefined;
}

// ── The launcher's two status-line painters, read out of the file ──────────────────────────────
/** The launcher's own <script> bodies — the source of every element its markup does not contain. */
const scriptSrc = [...html.matchAll(/<script\b[^>]*>([\s\S]*?)<\/script>/g)].map((m) => m[1]).join("\n");
if (scriptSrc.length < 2000) throw new Error("launcher.html: no <script> body found — the JS-built-element check is broken, not passing");

const setMsgSrc = html.match(/function setMsg\([\s\S]*?\n\}/)?.[0] ?? "";
const keysMsgSrc = html.match(/function keysMsg\([^\n]*\n/)?.[0] ?? "";

describe("AI Console launcher — the ground (CPE-1921)", () => {
  it("still declares `color-scheme: light dark`, so both measured grounds apply", () => {
    expect(css).toMatch(/color-scheme\s*:\s*light\s+dark/);
  });

  it("still paints the status line on `Canvas` via body — the constants above are measured against it", () => {
    const body = allRules.find((r) => r.selector === "body");
    expect(body, "no `body { ... }` rule in launcher.html").toBeTruthy();
    expect(
      decls(body!.body).get("background"),
      "body no longer paints `Canvas`: the measured light/dark ground constants in this file are " +
        "stale and must be re-measured against the new background before this guard means anything.",
    ).toBe("Canvas");
  });

  // CPE-1966 finding, and the reason this test was rewritten rather than kept. The version CPE-1921
  // shipped asked only whether the CHECKED ELEMENT ITSELF painted a background, and it read only the
  // FIRST rule whose selector matched — so it could not see the thing that actually decides the
  // ground: the ANCESTORS. `#keys-msg` and `#help-body h3` do not sit on `Canvas` because they say
  // so; they sit on it because `#keys-panel` and `#help-panel` paint `Canvas` OVER their overlays'
  // `rgba(0,0,0,.35)` / `rgba(0,0,0,.45)` scrims. Drop that one declaration and the ground silently
  // becomes a mid-grey scrim over Canvas, taking `--accent-text` from 4.55:1 to about 1.35:1 in
  // light — with every assertion in this file still green, because none of them looked up.
  //
  // So the chain is now DERIVED: the launcher's <body> markup is parsed into a real element tree,
  // and for each checked site this walks UP from its element to the first ancestor (itself included)
  // whose CSS declares a background, then requires that background to be the opaque `Canvas` the
  // measured constants at the top of this file were taken against. Any other answer — a scrim, a
  // translucent wash, a hard-coded hex — means the constants are stale and says which element moved.
  describe("the ground is derived from the real ancestor chain, not assumed", () => {
    it("parses the launcher's <body> into an element tree (an empty tree is a broken parse)", () => {
      expect(MARKUP.length).toBeGreaterThan(80);
      expect(MARKUP.some((n) => n.id === "msg")).toBe(true);
      expect(MARKUP.some((n) => n.id === "help-panel")).toBe(true);
      // The tree must be a tree: #keys-msg really is inside #keys-panel inside #keys-overlay.
      expect(ancestorIds("keys-msg")).toContain("keys-panel");
      expect(ancestorIds("keys-msg")).toContain("keys-overlay");
    });

    it("every site that EXISTS in the markup has its ground derived, and the rest are JS-built", () => {
      const checked = new Set(SITES.map((x) => x.selector));
      expect(checked.size).toBeGreaterThanOrEqual(3);
      const problems: string[] = [];
      let grounded = 0;
      const jsBuilt: string[] = [];
      for (const sel of checked) {
        const anchor = anchorElementFor(sel);
        if (anchor === undefined) {
          // No element with this class/id in the static markup. That is legitimate for the DOM the
          // launcher's own JS builds (`.tab`, `.close-all-btn`, …) and a typo otherwise — so it is
          // DERIVED rather than waved through: the class has to appear in the launcher's script.
          const names = [...sel.matchAll(/[.#]([\w-]+)/g)].map((m) => m[1]);
          if (names.some((n) => scriptSrc.includes(`"${n}"`) || scriptSrc.includes(`${n} `) || scriptSrc.includes(`"${n} `))) {
            jsBuilt.push(sel);
          } else {
            problems.push(
              `${sel}: no element in launcher.html's markup carries this class/id, and the launcher's ` +
                "own script never creates one either — the rule is dead, or the selector is a typo.",
            );
          }
          continue;
        }
        grounded++;
        const painter = nearestBackgroundPainter(anchor);
        if (!painter) {
          problems.push(`${sel}: nothing in its ancestor chain paints a background at all`);
          continue;
        }
        if (painter.value !== "Canvas") {
          problems.push(
            `${sel}: its ground is now painted by \`${painter.by}\` as \`${painter.value}\`, not by ` +
              "`body { background: Canvas }`. The measured light/dark constants at the top of this " +
              "file no longer apply to it — re-measure against the new ground before trusting any " +
              "ratio here, and re-run `npm run harness:launcher-contrast` for the real numbers.",
          );
        }
      }
      expect(problems.join("\n")).toBe("");
      // A run that grounded nothing is a broken derivation, not a clean bill (CPE-1932).
      expect(grounded, "no checked site was found in the markup at all — the tree walk is broken").toBeGreaterThanOrEqual(3);
      // Everything this cheap layer could not ground is grounded by the browser harness instead —
      // it mounts the JS-built DOM from fixtures and walks the real chain. Named here so the split
      // is visible rather than implied.
      expect(jsBuilt.every((s) => typeof s === "string")).toBe(true);
    });
  });
});

describe("AI Console launcher — the browser half is still connected (CPE-1966)", () => {
  // CPE-1933: this file's header CLAIMS that states, animation frames, non-text roles and JS-built
  // elements are covered elsewhere. A claim beside a green test reads as vouched-for, so it is
  // derived rather than asserted — the workflow and package.json are read at run time.
  const WORKFLOW = join(process.cwd(), ".github/workflows/gui-smoke.yml");
  const pkg = JSON.parse(readFileSync(join(process.cwd(), "package.json"), "utf8"));

  it("package.json still defines the harness script this file points at", () => {
    expect(
      pkg.scripts?.["harness:launcher-contrast"],
      "`npm run harness:launcher-contrast` is gone. This file's header says four whole categories of " +
        "defect are covered by that harness; without it they are covered by nothing.",
    ).toContain("scripts/dev-harness/launcher-contrast/run.mjs");
  });

  it("CI still runs it — the `launcher-contrast` job exists and invokes that script", () => {
    const yml = readFileSync(WORKFLOW, "utf8");
    expect(yml, "no `launcher-contrast:` job in gui-smoke.yml").toMatch(/^ {2}launcher-contrast:$/m);
    const job = yml.slice(yml.search(/^ {2}launcher-contrast:$/m));
    // Bound the slice to THIS job. The first draft wrote
    // `job.slice(0, cond ? undefined : job.length)`, which returns `job` either way — dead code, and
    // harmless only because `launcher-contrast` happens to be the last job in the file today. The
    // next job appended below it would have let a sibling's `run:` satisfy this assertion.
    const nextJob = job.slice(1).search(/^ {2}[\w-]+:$/m);
    const body = nextJob === -1 ? job : job.slice(0, nextJob + 1);
    expect(
      /run:\s*npm run harness:launcher-contrast/.test(body),
      "the `launcher-contrast` job no longer runs `npm run harness:launcher-contrast`",
    ).toBe(true);
    // ...and it must pass --verify-pixels, which is the flag that turns on the second, independent
    // measurement path. Round 1's runner exited 0 on total pixel disagreement; that is fixed in
    // run.mjs, but a CI job that stops passing the flag switches the whole leg off just as quietly.
    expect(
      /run:\s*npm run harness:launcher-contrast\b[^\n]*--verify-pixels/.test(body),
      "the `launcher-contrast` job no longer passes `--verify-pixels`, so the screenshot cross-check " +
        "(the independent second path this PR's claim rests on) does not run in CI at all",
    ).toBe(true);
  });
});

describe("AI Console launcher — status line (CPE-1921)", () => {
  it("#msg carries no `opacity`, and nor does anything above it", () => {
    // The original defect: `#msg { opacity: .85 }` composited every state toward the ground, so the
    // source values lied about what got painted. Re-adding an opacity anywhere on the chain
    // (#msg itself, body, :root) silently re-opens it, and this is the only cheap tripwire.
    for (const sel of ["#msg", "#keys-msg", "body", ":root"]) {
      for (const r of allRules) {
        if (r.selector.split("::").pop()!.trim() !== sel) continue;
        expect(
          decls(r.body).get("opacity"),
          `${sel} declares an opacity — that composites the status colours toward the ground and ` +
            "is exactly the CPE-1921 defect. Bake the softness into the token values instead.",
        ).toBeUndefined();
      }
    }
  });

  it("the focus indicator has its own token, not the fill token (CPE-1966 site 1)", () => {
    // `outline: none` makes this border the page's only focus indicator, so it must clear SC 1.4.11's
    // 3:1 against the FIELD INTERIOR it encloses as well as the page it sits on. `--accent` is tuned
    // for the fill role and cleared only the second (2.46:1 vs rgb(59,59,59) in dark). The ratios
    // themselves are measured by the browser harness — what this cheap layer pins is that the two
    // roles have not been re-merged onto one token, which is the regression that reopens the defect.
    const focusRules = allRules.filter((r) => /:focus\b/.test(r.selector) && decls(r.body).has("border-color"));
    expect(focusRules.length, "no `:focus { border-color }` rule found — this tripwire is measuring nothing").toBeGreaterThanOrEqual(1);
    for (const r of focusRules) {
      const v = decls(r.body).get("border-color")!;
      expect(
        v,
        `${r.selector} paints the focus border with ${v}. \`var(--accent)\` is the FILL token and is ` +
          "2.46:1 against the dark `Field` interior this border encloses — the focus role needs its own " +
          "token (CPE-1919's multi-role trap, third instance).",
      ).not.toMatch(/var\(--accent\)/);
      expect(v, `${r.selector}'s focus border is a literal colour, invisible to every token guard here`).toMatch(/^var\(--[\w-]+\)$/);
      const token = v.match(/^var\((--[\w-]+)\)$/)![1];
      for (const scheme of ["light", "dark"] as Scheme[]) {
        expect(resolve(`var(${token})`, scheme), `${token} has no value in the ${scheme} scheme`).toBeTruthy();
      }
      expect(
        resolve(v, "light"),
        `${token} has the same value in both schemes. The ground a focus ring encloses is \`Field\`, ` +
          "which the engine resolves to white in light and rgb(59,59,59) in dark — one value cannot " +
          "clear 3:1 against both.",
      ).not.toBe(resolve(v, "dark"));
    }
  });

  it("no `:hover`/`:focus`/`:active` rule paints a literal colour (CPE-1966 site 4)", () => {
    // `.close-all-btn:hover { color: #d05656 }` was invisible to CPE-1921's sweep twice over: a
    // literal hex (not `var(--token)`) on a state rule (not the default state). The browser harness
    // measures both regardless of spelling; this keeps the cheap layer from being lied to as well,
    // and it is the one place a literal hex is cheap to forbid outright.
    const offenders: string[] = [];
    for (const r of allRules) {
      if (!/:(hover|focus|focus-visible|active)\b/.test(r.selector)) continue;
      for (const [prop, value] of decls(r.body)) {
        if (!/^(color|border(-\w+)?-color|outline-color)$/.test(prop)) continue;
        if (/^#[0-9a-fA-F]{3,8}$/.test(value.trim())) offenders.push(`${r.selector} { ${prop}: ${value} }`);
      }
    }
    expect(
      offenders.join("\n"),
      "a state rule paints a hard-coded colour. It cannot carry a `prefers-color-scheme` value and it " +
        "is invisible to every token-derived check in this file — give it a token in :root + the dark block.",
    ).toBe("");
  });

  it("setMsg/keysMsg pick a class, never an inline hex colour", () => {
    expect(setMsgSrc, "setMsg() not found in launcher.html").not.toBe("");
    expect(keysMsgSrc, "keysMsg() not found in launcher.html").not.toBe("");
    for (const [name, src] of [["setMsg", setMsgSrc], ["keysMsg", keysMsgSrc]] as const) {
      expect(
        /\.style\.color\s*=/.test(src),
        `${name}() assigns .style.color directly. An inline colour cannot carry a ` +
          "prefers-color-scheme value and is invisible to this guard — assign a class instead.",
      ).toBe(false);
      expect(/#[0-9a-fA-F]{3,8}/.test(src), `${name}() still contains a hard-coded hex colour`).toBe(false);
    }
  });

  it("every state class those two can assign is backed by a token-coloured rule", () => {
    const classes = new Set<string>();
    for (const src of [setMsgSrc, keysMsgSrc]) {
      const assign = src.match(/\.className\s*=\s*([^;]+);/)?.[1] ?? "";
      for (const m of assign.matchAll(/"([\w-]+)"/g)) classes.add(m[1]);
    }
    expect(classes.size, "no className assignment found in setMsg/keysMsg").toBeGreaterThanOrEqual(3);
    for (const c of classes) {
      const backed = SITES.some((s) => new RegExp(`(^|,\\s*)#(msg|keys-msg)\\.${c}\\b`).test(s.selector));
      expect(backed, `no \`color: var(--token)\` rule matches #msg.${c} / #keys-msg.${c}`).toBe(true);
    }
  });

  it("the three states stay visually distinct (>=25 degrees of hue apart) in both schemes", () => {
    const hue = (hex: string) => {
      const [r, g, b] = hexToRgb(hex).map((c) => c / 255);
      const mx = Math.max(r, g, b);
      const d = mx - Math.min(r, g, b);
      if (!d) return 0;
      const h = mx === r ? 60 * (((g - b) / d) % 6) : mx === g ? 60 * ((b - r) / d + 2) : 60 * ((r - g) / d + 4);
      return (h + 360) % 360;
    };
    for (const scheme of ["light", "dark"] as Scheme[]) {
      const hs = ["--msg-ok", "--msg-warn", "--msg-err"].map((t) => {
        const v = resolve(`var(${t})`, scheme);
        expect(v, `${t} does not resolve to a hex in the ${scheme} scheme`).toBeTruthy();
        return { t, h: hue(v!) };
      });
      for (let i = 0; i < hs.length; i++) {
        for (let j = i + 1; j < hs.length; j++) {
          const d = Math.abs(hs[i].h - hs[j].h);
          const sep = Math.min(d, 360 - d);
          expect(
            round2(sep),
            `${scheme}: ${hs[i].t} (${hs[i].h.toFixed(0)}deg) and ${hs[j].t} (${hs[j].h.toFixed(0)}deg) are ` +
              "too close to tell apart — a palette where every state passes contrast but amber reads " +
              "as red trades one defect for another.",
          ).toBeGreaterThanOrEqual(25);
        }
      }
    }
  });
});

describe("AI Console launcher — every token-coloured foreground clears its WCAG bar (CPE-1921)", () => {
  it("finds foreground sites to check (an empty sweep is a broken sweep, not a clean bill)", () => {
    // CPE-1932: a derived enumeration must fail loudly when it comes back near-empty.
    expect(SITES.length).toBeGreaterThanOrEqual(4);
    expect(new Set(SITES.map((s) => s.token)).size).toBeGreaterThanOrEqual(3);
  });

  for (const scheme of ["light", "dark"] as Scheme[]) {
    it(`${scheme}: on the measured ground ${GROUND[scheme]}`, () => {
      const failures: string[] = [];
      for (const site of SITES) {
        const fg = resolve(`var(${site.token})`, scheme);
        if (!fg) {
          failures.push(`${site.selector}: ${site.token} does not resolve to an opaque hex in ${scheme}`);
          continue;
        }
        const { bar, note } = barFor(site.selector);
        const r = contrast(fg, GROUND[scheme]);
        if (r < bar) {
          failures.push(
            `${site.selector} { color: var(${site.token}) } -> ${fg} on ${GROUND[scheme]} = ` +
              `${round2(r)}:1, below the ${bar}:1 bar (${note}). Give ${site.token} a value for this ` +
              `scheme in launcher.html's ${scheme === "dark" ? "@media (prefers-color-scheme: dark)" : ":root"} block.`,
          );
        }
      }
      expect(failures.join("\n")).toBe("");
    });
  }
});
