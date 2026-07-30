// CPE-1148 (Part A) — screenshot capture for the "Visual Critic" loop.
//
// A small `snap(name)` helper each smoke spec calls at its key rendered state, AFTER its own
// assertions (so a failed assertion still leaves a shot of whatever state it failed in — see each
// call site). Writes a PNG via WebdriverIO's `browser.saveScreenshot(absolutePath)` into a known,
// gitignored artifacts dir (`gui-smoke/.screenshots/`), so one full `npm test` run leaves a gallery
// of the app's main screens on disk for a reviewer — human or a future Visual Critic sub-agent
// (CPE-1148 Part B) — to open. See gui-smoke/README.md's "Screenshots for the Visual Critic" section
// for the full convention.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { browser } from "@wdio/globals";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

/** `gui-smoke/.screenshots/` — artifacts, never committed (see gui-smoke/.gitignore). */
export const SCREENSHOTS_DIR = path.resolve(__dirname, "..", ".screenshots");

/**
 * Save a PNG of the current window state to `gui-smoke/.screenshots/<name>.png`, creating the
 * directory if it doesn't exist yet. `name` should be a stable, per-surface label (e.g.
 * `"open-dir"`, `"organize-dialog"`) — re-running the suite overwrites the same file rather than
 * accumulating timestamped copies, so the dir always reflects the latest run.
 *
 * Deliberately swallows its own errors: a screenshot is a nice-to-have observability artifact, not
 * a test assertion — a failure here (e.g. a headless/CI display quirk) must never fail or mask the
 * spec's real assertions. Call it AFTER the state you want to capture is already asserted/settled.
 */
export async function snap(name: string): Promise<void> {
  try {
    fs.mkdirSync(SCREENSHOTS_DIR, { recursive: true });
    const target = path.join(SCREENSHOTS_DIR, `${name}.png`);
    await browser.saveScreenshot(target);
  } catch (err) {
    // eslint-disable-next-line no-console
    console.error(`[gui-smoke] snap("${name}") failed (non-fatal):`, err);
  }
}
