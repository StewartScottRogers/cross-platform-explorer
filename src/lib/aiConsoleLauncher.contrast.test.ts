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
 * ── Red-proof, run by hand, RESULTS AT THE SITE (CPE-1933 rule 3) ─────────────────────────────
 * Five sabotages of launcher.html, each run against this file; each failed exactly one test and
 * named the culprit, so none of these checks is decorative:
 *   1. `--msg-ok` -> `#3a9d4a` (the old value): "light: on the measured ground #ffffff" failed with
 *      "#msg.ok, #keys-msg.ok { color: var(--msg-ok) } -> #3a9d4a on #ffffff = 3.44:1, below the
 *      4.5:1 bar (font-size 12px / weight 400)".
 *   2. delete the dark `--msg-err` override: "dark: on the measured ground #121212" failed with
 *      "-> #c42b1c on #121212 = 3.31:1 … Give --msg-err a value for this scheme".
 *   3. re-add `#msg { opacity: .85 }`: the opacity tripwire failed ("expected '.85' to be
 *      undefined").
 *   4. `body { background: Field }`: the ground test failed ("expected 'Field' to be 'Canvas'").
 *   5. `setMsg` back to `el.style.color = "#d08a1a"`: the inline-hex tripwire failed AND both
 *      contrast tests stayed GREEN. That pair is the point (CPE-1929): the tripwire is NOT
 *      shadowed by the measurement, because an inline colour is structurally invisible to a
 *      stylesheet sweep. Deleting it would leave that regression uncovered, not merely unguarded.
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

// ── The launcher's two status-line painters, read out of the file ──────────────────────────────
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

  it("no checked foreground element paints its own background (which would move the ground)", () => {
    for (const s of new Set(SITES.map((x) => x.selector))) {
      const r = allRules.find((x) => x.selector.split("::").pop()!.trim() === s)!;
      const d = decls(r.body);
      expect(
        d.get("background") ?? d.get("background-color"),
        `${s} declares its own background — it no longer sits on Canvas, so its ground must be ` +
          "re-measured and this guard taught about it.",
      ).toBeUndefined();
    }
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
