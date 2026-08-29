// Shared session-identity chip (CPE-490). A running Agent Deck session is shown in two places — the
// console's own tab strip and the explorer's left-pane "Agents" list — and with several same-agent
// sessions they're otherwise indistinguishable. The console assigns each session a stable id (`s1`,
// `s2`, …) that flows to both surfaces, so both can derive an IDENTICAL chip (a colour + a number)
// with no cross-window coordination: same id ⇒ same chip ⇒ instant visual correlation.
//
// IMPORTANT: the launcher UI (`sidecar/ai-console/src/launcher.html`) duplicates this exact logic in
// plain JS so its tabs match. The duplication is not removable — the launcher is a single self-contained
// HTML file served by the Rust sidecar, with no bundler and no module graph reaching into `src/` — so
// per CPE-1933 the equality is DERIVED instead of promised: `sessionChip.test.ts` parses the array out
// of launcher.html and asserts it equals this one, and red-proves that by mutating the parsed copy.
// Keep the hash + number rules in sync by hand; the palette is guarded.

/**
 * The chip palette (CPE-490), retuned in CPE-1977.
 *
 * Every entry is a FILL under a white numeral at 10px/700 — "normal" text for WCAG 2.1, so 4.5:1 — and
 * is itself a shape on a tab, so 3:1 against that. Those two bars pull opposite ways and leave one
 * narrow luminance window; all eight sit at its midpoint, clearing ~4.59:1 on white and ~3.08:1 on the
 * worst tab ground in either scheme. The previous palette missed one or both bars on seven of eight.
 * `sessionChip.test.ts` pins the white-text bar (both colours are literals here, so it is exact); the
 * fill-vs-tab bar belongs to the real-browser sweep, which composites the actual grounds
 * (`npm run harness:launcher-contrast`). Hue separation is pinned here too — the palette's job is
 * identity, and eight compliant-but-indistinguishable chips would fail the feature while passing.
 */
export const SESSION_CHIP_COLORS = [
  "#3975ca", // blue
  "#258648", // green
  "#6e7c23", // olive
  "#c136c1", // magenta
  "#d63c3c", // red
  "#248282", // teal
  "#9b6c2b", // amber
  "#8450ff", // violet
];

/** Deterministic colour for a session id (FNV-ish rolling hash → palette). Same id ⇒ same colour. */
export function sessionColor(id: string): string {
  let h = 0;
  for (let i = 0; i < id.length; i++) {
    h = (Math.imul(h, 31) + id.charCodeAt(i)) >>> 0;
  }
  return SESSION_CHIP_COLORS[h % SESSION_CHIP_COLORS.length];
}

/** The short number shown in the chip: the digits of the id (`s2` → `2`); falls back to `•`. */
export function sessionNum(id: string): string {
  const m = /(\d+)/.exec(id || "");
  return m ? m[1] : "•";
}

/** A short, human model label: the last path segment (`anthropic/claude-sonnet-5` → `claude-sonnet-5`),
 *  trimmed of an `-YYYYMMDD`/`:tag` suffix. Empty in, empty out. */
export function shortModel(model: string): string {
  if (!model) return "";
  const last = model.split("/").pop() ?? model;
  return last.replace(/[:@].*$/, "");
}
