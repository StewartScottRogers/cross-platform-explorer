// CPE-1148 (Part A) / CPE-1149 — screenshot capture for the "Visual Critic" loop.
//
// Two companion helpers each smoke spec uses, so one full `npm test` run leaves a gallery of the
// app's main screens on disk for a reviewer — human or a future Visual Critic sub-agent (CPE-1148
// Part B) — to open:
//   - `snap(name)` — called INLINE at a spec's key rendered state, right after its assertions pass,
//     to capture the good frame at exactly the right moment (e.g. a dialog just before the test
//     dismisses it with Cancel). Writes `<name>.png`.
//   - `snapFailure(test, name)` — called from a spec's Mocha `afterEach(function () { … })` hook.
//     When the test that just ran FAILED, it captures `<name>-fail.png`: a shot of whatever state
//     the assertion failed in, which is exactly what a reviewer / the Visual Critic most wants. On a
//     pass it is a no-op (the inline `snap(name)` already captured the good frame at the right
//     instant — an `afterEach` runs only after the whole `it` body, by which point a dialog the spec
//     dismisses would already be gone).
//
// CPE-1149 added `snapFailure` because the inline `snap()` alone is NEVER reached once an earlier
// `expect` throws — so before it, a failing run left no shot at all, contradicting this file's own
// docs. Now a failing run leaves a `<name>-fail.png`; the `-fail` suffix keeps it distinct from (and
// never clobbers) the last good `<name>.png` baseline.
//
// Both write via WebdriverIO's `browser.saveScreenshot(absolutePath)` into a known, gitignored
// artifacts dir (`gui-smoke/.screenshots/`). See gui-smoke/README.md's "Screenshots for the Visual
// Critic" section for the full convention.
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
 * For the failing-run counterpart, see `snapFailure` below.
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

/**
 * Failing-run companion to `snap`, for a spec's Mocha `afterEach` hook. Pass Mocha's
 * `this.currentTest` (so use a non-arrow `afterEach(function () { … })` — an arrow function would
 * not bind Mocha's `this` context) and the spec's canonical surface name.
 *
 * If the test that just ran FAILED, this writes `<name>-fail.png` — a shot of whatever state the
 * assertion failed in (the frame the Visual Critic / a human most wants). If it PASSED (or Mocha
 * gave us no test/state, e.g. a `pending`/skipped test), it is a no-op: the spec's inline
 * `snap(name)` after its assertions already captured the good `<name>.png` frame at exactly the
 * right instant. The `-fail` suffix keeps a failing shot from clobbering that last good baseline.
 *
 * Errors are swallowed transitively via `snap`, so — like `snap` — this can never fail or mask a
 * real assertion.
 */
export async function snapFailure(
  test: { state?: string } | undefined,
  name: string,
): Promise<void> {
  if (test?.state !== "failed") return;
  await snap(`${name}-fail`);
}
