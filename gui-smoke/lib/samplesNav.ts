// Shared address-bar navigation + preview-settle helpers for specs that walk the seeded
// `CPE-1358-samples` copy of the real repo `samples/` tree (see wdio.conf.ts's `seedSamplesFixture`
// header comment for why it's a copy, not a second `--open`).
//
// Extracted from `specs/samples.smoke.ts` (CPE-1358) so `specs/preview-pane.smoke.ts` (CPE-1629) can
// reuse the SAME navigation primitives — in particular `navigateTo`'s two Linux/WebKitGTK workarounds
// (CPE-1507: retrying the first Ctrl+L, and setting the address-bar value via `browser.execute`
// instead of native `setValue()`) are exactly the kind of hard-won, non-obvious fix that must never
// silently drift between two copies of "similar" navigation code. One implementation, two callers.
import path from "node:path";
import { expect } from "chai";
import { $, $$, browser } from "@wdio/globals";

/** A definite "the preview rendered real content" indicator, one selector per PreviewKind's success
 *  markup (PreviewPane.svelte) — see `specs/samples.smoke.ts`'s original header comment for the full
 *  per-kind breakdown of why each selector is here. `aside.details` is the CPE-1357 graceful-fallback
 *  terminal state (DetailsPane.svelte via PreviewPane's default slot).
 *
 *  Deliberately byte-identical to the list `specs/samples.smoke.ts` used before this was extracted
 *  (CPE-1629) — this list is what `gui-smoke/known-failing.json`'s `samples.smoke.ts` entry (CPE-1507)
 *  was measured against, so changing it here would silently change what that spec considers "settled"
 *  without a corresponding, independently-verified ratchet update. `specs/preview-pane.smoke.ts` needs
 *  a couple of ADDITIONAL testids (`cert-preview`/`jwt-preview`/`binary-preview`) this list doesn't
 *  carry — it passes them via `waitForPreviewToSettle`'s `extraSelectors` option instead of widening
 *  this shared constant, so `samples.smoke.ts`'s settle-detection (and therefore its known-failing
 *  baseline) is provably unchanged by this refactor. */
export const PREVIEW_CONTENT_SELECTOR = [
  ".preview-img",
  ".mp-media",
  ".preview-pdf",
  // CPE-1639: was ".preview-font", which matches zero elements — FontPreview.svelte's root is
  // `<div class="font-preview" data-testid="font-preview">` (the words are swapped from the old
  // selector), so the fonts/* case never actually matched on this entry; it only ever "passed" via
  // the loop's other exit conditions. Fixed to the real testid, matching hexview's convention below.
  '[data-testid="font-preview"]',
  ".preview-table-wrap",
  ".preview-markdown",
  ".code-view",
  "pre.preview-text",
  ".preview-editor",
  '[data-testid="hexview"]',
  ".data-browser",
  "aside.details",
].join(", ");

/** The exact English `pv.loading` string (PreviewPane.svelte / src/lib/i18n.ts) — the default locale
 *  this harness runs under (see `specs/file-health.smoke.ts`'s identical note). Distinguishes the
 *  TRANSIENT loading `.preview-note` from a TERMINAL one (e.g. "Can't preview this file"). */
export const PREVIEW_LOADING_TEXT = "Loading preview…";

/** Poll until the preview pane shows SOMETHING for the currently-selected file: either a definite
 *  content element ({@link PREVIEW_CONTENT_SELECTOR}, plus any caller-supplied `extraSelectors`) or a
 *  terminal `.preview-note` (any note whose text isn't the transient loading string). Throws (via
 *  `waitUntil`'s `timeoutMsg`) if neither ever appears within `timeoutMs` — the stuck-spinner failure
 *  mode this guard exists to catch. `extraSelectors` lets a caller recognise a provider's success
 *  markup that isn't in the shared base list (e.g. `specs/preview-pane.smoke.ts`'s Cert/JWT/Binary
 *  Inspector testids) WITHOUT widening that shared list for every other caller — see
 *  {@link PREVIEW_CONTENT_SELECTOR}'s doc comment for why that matters. */
export async function waitForPreviewToSettle(
  label: string,
  opts: { timeoutMs?: number; extraSelectors?: string[] } = {},
): Promise<void> {
  const timeoutMs = opts.timeoutMs ?? 20_000;
  const selector = [PREVIEW_CONTENT_SELECTOR, ...(opts.extraSelectors ?? [])].join(", ");
  await browser.waitUntil(
    async () => {
      if ((await $$(selector).length) > 0) return true;
      const notes = $$(".preview-note");
      for await (const note of notes) {
        const text = await note.getText();
        if (text && text !== PREVIEW_LOADING_TEXT) return true;
      }
      return false;
    },
    {
      timeout: timeoutMs,
      timeoutMsg: `preview pane never settled for ${label} — still "${PREVIEW_LOADING_TEXT}" (or nothing rendered) after ${timeoutMs}ms`,
    },
  );
}

/** Navigates the address bar directly to `absDir` (Ctrl+L -> type the absolute path -> Enter — the
 *  same "Type a path (Ctrl+L)" flow a user driving the address bar uses, NavToolbar.svelte), then
 *  waits for the breadcrumb to confirm the folder actually changed. CPE-1507's two WebKitGTK/Linux
 *  workarounds (retried Ctrl+L; value set via `browser.execute` rather than native
 *  `setValue()`/`elementClear`+`elementSendKeys`) are load-bearing — see `specs/samples.smoke.ts`'s
 *  original header comment (now this function's) for the full race-condition writeup; both are
 *  invisible on Windows/CDP and only showed up on a real Linux CI run. */
export async function navigateTo(absDir: string): Promise<void> {
  const PATHEDIT_SELECTOR = ".pathedit";
  const CTRL_L_ATTEMPTS = 3;
  let opened = false;
  for (let attempt = 1; attempt <= CTRL_L_ATTEMPTS && !opened; attempt++) {
    await browser.keys(["Control", "l"]);
    try {
      await $(PATHEDIT_SELECTOR).waitForExist({ timeout: 4_000 });
      opened = true;
    } catch {
      // CPE-1507: Ctrl+L occasionally doesn't register on WebKitWebDriver's Actions-based key
      // delivery — retry rather than fail on the first miss (unless this was the last attempt).
      if (attempt === CTRL_L_ATTEMPTS) {
        throw new Error(
          `expected the address-bar path input (${PATHEDIT_SELECTOR}) to appear after Ctrl+L (${CTRL_L_ATTEMPTS} attempts)`,
        );
      }
    }
  }

  // CPE-1507: set the value via `browser.execute` (bypasses the native elementClear/elementSendKeys
  // pair that races NavToolbar's `on:blur` on WebKitWebDriver). Svelte's `bind:value={draft}` just
  // reads `el.value` off the bubbling `input` event, so a plain property assignment + dispatched
  // event is exactly as faithful here as real keystrokes would be.
  await browser.execute(
    (sel, value) => {
      const el = document.querySelector(sel) as HTMLInputElement | null;
      if (!el) return;
      el.focus();
      el.value = value;
      el.dispatchEvent(new Event("input", { bubbles: true }));
    },
    PATHEDIT_SELECTOR,
    absDir,
  );
  await browser.keys(["Enter"]);

  const expectedName = path.basename(absDir);
  await browser.waitUntil(
    async () => {
      const crumb = await $('[aria-current="page"]');
      if (!(await crumb.isExisting())) return false;
      return (await crumb.getText()) === expectedName;
    },
    {
      timeout: 15_000,
      timeoutMsg: `expected the breadcrumb to show "${expectedName}" after navigating to ${absDir}`,
    },
  );
}

/** The FIRST `.row` whose rendered HTML contains `name` — the same getHTML-scan idiom other specs in
 *  this suite use (declutter.smoke.ts / file-health.smoke.ts) rather than an exact-text locator
 *  (unreliable against wry under classic WebDriver — see open-dir.smoke.ts's note). */
export async function findRowContaining(name: string): Promise<WebdriverIO.Element> {
  let found: WebdriverIO.Element | undefined;
  await browser.waitUntil(
    async () => {
      const rows = $$(".row");
      for await (const row of rows) {
        const html = await row.getHTML({ includeSelectorTag: false });
        if (html.includes(name)) {
          found = row;
          return true;
        }
      }
      return false;
    },
    { timeout: 15_000, timeoutMsg: `expected a .row containing "${name}"` },
  );
  return found!;
}

/** The crash guard: the window/session must still be responding. If the renderer process died, this
 *  WebDriver call itself throws (session/window gone) rather than returning — that failure IS the
 *  regression signal, not a bug in this helper. */
export async function assertAppStillAlive(context: string): Promise<void> {
  const body = await $("body");
  expect(await body.isExisting(), `app/window did not respond after ${context}`).to.equal(true);
  const html = await body.getHTML({ includeSelectorTag: false });
  expect(html.trim().length, `app/window rendered empty content after ${context}`).to.be.greaterThan(0);
}

/** Navigates to `dirAbs`, single-selects `fileName` in it (feeding `PreviewPane`'s `entry` prop, same
 *  as every other spec), and waits for the preview to settle (or gracefully degrade). Returns the
 *  selected row element in case a caller needs to interact with it further. `opts` forwards to
 *  {@link waitForPreviewToSettle} (e.g. `extraSelectors` for a provider's own success markup). */
export async function openSampleFile(
  dirAbs: string,
  fileName: string,
  opts: { timeoutMs?: number; extraSelectors?: string[] } = {},
): Promise<WebdriverIO.Element> {
  await navigateTo(dirAbs);
  const row = await findRowContaining(fileName);
  await row.click();
  await waitForPreviewToSettle(fileName, opts);
  return row;
}
