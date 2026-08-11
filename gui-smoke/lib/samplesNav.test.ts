// CPE-1639 — headless unit tests for PREVIEW_CONTENT_SELECTOR, runnable WITHOUT a `tauri build` or a
// tauri-driver session (same convention as ratchet.test.ts / compare.test.ts):
//   npm run test:unit          (from gui-smoke/)
//
// The bug this guards: ".preview-font" matched zero elements anywhere in the shipped frontend —
// FontPreview.svelte's real root is `<div class="font-preview" data-testid="font-preview">` (the two
// words were swapped in the selector), so the fonts/* case in waitForPreviewToSettle never actually
// matched on it; it only ever "passed" via the loop's other exit conditions (coincidence, not
// detection). This file (a) locks in the fix, and (b) audits every OTHER entry in the list against the
// real frontend source so the same class of bug can't silently reappear for a different kind.
import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { PREVIEW_CONTENT_SELECTOR } from "./samplesNav.js";

const __dirname = fileURLToPath(new URL(".", import.meta.url));
const COMPONENTS_DIR = join(__dirname, "..", "..", "src", "lib", "components");

/** All `.svelte` component source, concatenated — enough to check a class/testid is actually used
 *  somewhere, without needing a real build. */
function componentsSource(): string {
  return readdirSync(COMPONENTS_DIR)
    .filter((f) => f.endsWith(".svelte"))
    .map((f) => readFileSync(join(COMPONENTS_DIR, f), "utf8"))
    .join("\n");
}

/** True if `className` appears as a whole token inside some `class="..."` attribute in `source`. */
function hasClass(source: string, className: string): boolean {
  const re = /class="([^"]*)"/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(source))) {
    if (m[1].split(/\s+/).includes(className)) return true;
  }
  return false;
}

/** Split a compound selector like "pre.preview-text" or "aside.details" down to its class name, or a
 *  `[data-testid="…"]` selector down to its testid. */
function parseSelector(sel: string): { kind: "class"; name: string } | { kind: "testid"; name: string } {
  const testid = sel.match(/^\[data-testid="([^"]+)"\]$/);
  if (testid) return { kind: "testid", name: testid[1] };
  const cls = sel.match(/\.([a-zA-Z0-9_-]+)$/);
  if (cls) return { kind: "class", name: cls[1] };
  throw new Error(`unrecognized selector shape in PREVIEW_CONTENT_SELECTOR: "${sel}"`);
}

describe("PREVIEW_CONTENT_SELECTOR (CPE-1639)", () => {
  const selectors = PREVIEW_CONTENT_SELECTOR.split(", ");

  it("no longer contains the dead '.preview-font' selector", () => {
    assert.equal(selectors.includes(".preview-font"), false);
  });

  it("uses the font preview's real testid instead", () => {
    assert.ok(selectors.includes('[data-testid="font-preview"]'));
  });

  it("every entry matches at least one real element in the shipped frontend (systematic audit)", () => {
    const source = componentsSource();
    for (const sel of selectors) {
      const parsed = parseSelector(sel);
      const found = parsed.kind === "testid"
        ? source.includes(`data-testid="${parsed.name}"`)
        : hasClass(source, parsed.name);
      assert.ok(found, `selector "${sel}" (parsed as ${parsed.kind} "${parsed.name}") matches nothing in src/lib/components`);
    }
  });
});
