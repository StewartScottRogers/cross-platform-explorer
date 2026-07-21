// Shared session-identity chip (CPE-490). A running Agent Deck session is shown in two places — the
// console's own tab strip and the explorer's left-pane "Agents" list — and with several same-agent
// sessions they're otherwise indistinguishable. The console assigns each session a stable id (`s1`,
// `s2`, …) that flows to both surfaces, so both can derive an IDENTICAL chip (a colour + a number)
// with no cross-window coordination: same id ⇒ same chip ⇒ instant visual correlation.
//
// IMPORTANT: the launcher UI (`sidecar/ai-console/src/launcher.html`) duplicates this exact logic in
// plain JS so its tabs match. Keep the palette + hash + number rules in sync across both.

/** The chip palette — small + high-contrast in light and dark. Mirror in launcher.html. */
export const SESSION_CHIP_COLORS = [
  "#3a72b5", // blue
  "#3a9d4a", // green
  "#b5872b", // amber
  "#a24bd0", // purple
  "#c94f4f", // red
  "#2aa1a1", // teal
  "#c96f18", // orange
  "#6b7bd6", // indigo
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
