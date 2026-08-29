// CPE-490: the chip must be deterministic + identical for a given session id, so the console tab and
// the left-pane leaf correlate. (The launcher.html JS duplicates this logic; these tests pin the rules
// both copies must follow.)
//
// CPE-1977 added the two things that were only ever promised in a comment: that the launcher's copy of
// the palette IS this one, and that each entry can actually carry the white numeral painted on it.
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
// The launcher's array is read by the SAME function the browser harness uses to build its fixtures —
// one parser, not a second one written here that could agree with itself while both are wrong
// (CPE-1950). It reads launcher.html's <script> bodies, so a decoy in HTML prose cannot win.
import { sessionChipColours } from "../../scripts/dev-harness/launcher-contrast/engine.mjs";
import { SESSION_CHIP_COLORS, sessionColor, sessionNum, shortModel } from "./sessionChip";

const LAUNCHER = join(process.cwd(), "sidecar/ai-console/src/launcher.html");

const hexToRgb = (h: string): [number, number, number] => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)) as [number, number, number];
const chan = (c: number) => { const s = c / 255; return s <= 0.04045 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4); };
const luminance = (hex: string) => { const [r, g, b] = hexToRgb(hex); return 0.2126 * chan(r) + 0.7152 * chan(g) + 0.0722 * chan(b); };
const contrast = (a: string, b: string) => {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
};
const hue = (hex: string) => {
  const [r, g, b] = hexToRgb(hex).map((c) => c / 255);
  const mx = Math.max(r, g, b);
  const d = mx - Math.min(r, g, b);
  if (!d) return 0;
  const h = mx === r ? 60 * (((g - b) / d) % 6) : mx === g ? 60 * ((b - r) / d + 2) : 60 * ((r - g) / d + 4);
  return (h + 360) % 360;
};

describe("sessionChip", () => {
  it("gives the same colour for the same id every time", () => {
    expect(sessionColor("s2")).toBe(sessionColor("s2"));
    expect(SESSION_CHIP_COLORS).toContain(sessionColor("s2"));
  });

  it("spreads different ids across the palette", () => {
    const seen = new Set(["s1", "s2", "s3", "s4", "s5", "s6", "s7", "s8"].map(sessionColor));
    expect(seen.size).toBeGreaterThan(1); // not all the same colour
  });

  it("derives the chip number from the id's digits", () => {
    expect(sessionNum("s1")).toBe("1");
    expect(sessionNum("s12")).toBe("12");
    expect(sessionNum("nodigits")).toBe("•");
  });

  it("shortens a model to its last, tag-trimmed segment", () => {
    expect(shortModel("anthropic/claude-sonnet-5")).toBe("claude-sonnet-5");
    expect(shortModel("claude-sonnet-4-5")).toBe("claude-sonnet-4-5");
    expect(shortModel("openai/gpt-4o:free")).toBe("gpt-4o");
    expect(shortModel("")).toBe("");
  });
});

/**
 * ONE PALETTE, TWO FILES — derived, not promised (CPE-1933/CPE-1977).
 *
 * The duplication is not removable and that is worth stating plainly, because "just import it" is the
 * first thing anyone reaches for: `launcher.html` is a single self-contained document served by the
 * Rust sidecar with no bundler and no module graph that reaches into `src/`, and the browser contrast
 * harness derives the palette by PARSING that file, so a build-time placeholder would take the ground
 * out from under the only thing that measures these colours in a browser. What was removable is the
 * unchecked claim: "MUST match src/lib/sessionChip.ts" was a comment for two years and nothing read it.
 */
describe("sessionChip — the launcher's copy of the palette (CPE-1977)", () => {
  const raw = readFileSync(LAUNCHER, "utf8");

  it("is the same eight values, in the same order, as this module's", () => {
    // Order matters as much as membership: `sessionColor()` indexes by `hash % length`, so two arrays
    // holding the same colours in a different order give the same session two different chips.
    expect(sessionChipColours(raw)).toEqual(SESSION_CHIP_COLORS);
  });

  it("really re-reads launcher.html — a changed value there changes the derivation here", () => {
    // RED-PROOF, run every time rather than done once by hand and written up (CPE-1933 rule 3). A
    // "derivation" that has cached the answer, or that parses a stale copy, passes the assertion above
    // for exactly as long as nobody edits the launcher — which is the whole window in which it matters.
    const first = SESSION_CHIP_COLORS[0];
    const mutated = raw.replace(`"${first}"`, '"#0b0b0b"');
    expect(mutated, `could not find ${first} in launcher.html to mutate — the red-proof is not running`).not.toBe(raw);
    const derived = sessionChipColours(mutated);
    expect(derived).not.toEqual(SESSION_CHIP_COLORS);
    expect(derived[0]).toBe("#0b0b0b");
  });
});

describe("sessionChip — every entry can carry the chip's white numeral (CPE-1977)", () => {
  // `.tab-chip` / `.pane-chip` / `.agent-chip` / `.menu-chip` all paint `color: #fff` at 10px/700.
  // 18.66px BOLD is where WCAG 2.1 starts calling text large, so this is normal text and the bar is
  // 4.5:1 — pinned at the bar for the role, not at the loosest bar any of these colours faces.
  //
  // SCOPE, stated at the scope it was measured at: this leg compares two LITERALS (the palette entry
  // and #fff), so it is exact and needs no browser. The other bar the same fill faces — 3:1 against
  // the tab it sits on — is NOT here, because that ground is composited from `Canvas` plus two rgba
  // washes and only a real browser resolves it: `npm run harness:launcher-contrast` measures it and
  // now enforces it. A copy of those grounds in this file would be a second model that could agree
  // with itself while being wrong.
  it.each(SESSION_CHIP_COLORS)("%s clears 4.5:1 under #ffffff", (colour) => {
    expect(Number(contrast(colour, "#ffffff").toFixed(2))).toBeGreaterThanOrEqual(4.5);
  });

  it("keeps the eight distinguishable — >=25 degrees of hue apart, pairwise", () => {
    // The palette's JOB is identity. Eight colours that all clear both bars and all look alike fail
    // the feature while passing the contrast test, so the bar the launcher's --msg-* triad is held to
    // (aiConsoleLauncher.contrast.test.ts) applies here too, over all 28 pairs.
    expect(SESSION_CHIP_COLORS.length).toBeGreaterThanOrEqual(8);
    for (let i = 0; i < SESSION_CHIP_COLORS.length; i++) {
      for (let j = i + 1; j < SESSION_CHIP_COLORS.length; j++) {
        const [a, b] = [SESSION_CHIP_COLORS[i], SESSION_CHIP_COLORS[j]];
        const d = Math.abs(hue(a) - hue(b));
        expect(
          Number(Math.min(d, 360 - d).toFixed(2)),
          `${a} (${hue(a).toFixed(0)}deg) and ${b} (${hue(b).toFixed(0)}deg) are too close to tell apart`,
        ).toBeGreaterThanOrEqual(25);
      }
    }
  });
});
