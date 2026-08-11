// CPE-1629 — real preview-pane width control for gui-smoke specs.
//
// The preview pane's width is the `rightWidth` reactive var in `App.svelte`, bound (via CSS Grid
// `grid-template-columns`) straight onto the app's layout — there is no inline style on the pane
// itself to poke from outside. The one real, in-app way to change it without a drag-resize gesture is
// the pane's own Toolbar settings popover (`Toolbar.svelte`'s gear button, scoped under
// `.preview-pane`): a "Pane width" `<input type="number" min={RIGHT_MIN} bind:value={rightWidth}>`
// (App.svelte, `tb.paneWidth`). This drives that control directly — open the popover, set the value
// the same faithful way `../lib/samplesNav.js#navigateTo` sets the address bar (a real property
// assignment + dispatched `input`/`change` events, not native WebDriver `setValue()`, which the
// CPE-1507 investigation found races Svelte's reactive bindings on WebKitGTK), then close the popover
// again so it doesn't cover the pane in the screenshot.
import { $, browser } from "@wdio/globals";

const GEAR_SELECTOR = ".preview-pane .tb-gear";
const POPOVER_SELECTOR = ".preview-pane .tb-popover";
const WIDTH_INPUT_SELECTOR = `${POPOVER_SELECTOR} input[type="number"]`;

/** `RIGHT_MIN` in `App.svelte` — the narrowest width the preview pane allows. The ticket's "narrow
 *  pane width" case: this is exactly where clipping/pill-reflow defects show up first. */
export const PREVIEW_PANE_NARROW_PX = 220;

/** A roomy-but-still-realistic width — comfortably inside `rightMaxWidth()` for the app's default
 *  1000x700 window (`src-tauri/src/lib.rs`'s `.inner_size(1000.0, 700.0)`) after the sidebar and its
 *  own minimum are accounted for, without relying on the exact live budget. */
export const PREVIEW_PANE_COMFORTABLE_PX = 400;

/** Sets the preview pane's width to `px` via its own Toolbar settings popover (see module header),
 *  then closes the popover again so the screenshot shows the pane, not the popover covering it. */
export async function setPreviewPaneWidth(px: number): Promise<void> {
  const gear = $(GEAR_SELECTOR);
  await gear.waitForExist({ timeout: 10_000 });
  await gear.click(); // Toolbar.svelte: toggles `open`, mounting `.tb-popover`

  const input = $(WIDTH_INPUT_SELECTOR);
  await input.waitForExist({ timeout: 5_000 });
  await browser.execute(
    (sel, value) => {
      const el = document.querySelector(sel) as HTMLInputElement | null;
      if (!el) return;
      el.focus();
      el.value = String(value);
      // Svelte's `bind:value` on a number input reacts to `input`; App.svelte's own `on:change`
      // handler (the clamp + `settings.saveRightWidth` persist) reacts to `change` — dispatch both,
      // exactly mirroring what a real keystroke + blur would fire.
      el.dispatchEvent(new Event("input", { bubbles: true }));
      el.dispatchEvent(new Event("change", { bubbles: true }));
    },
    WIDTH_INPUT_SELECTOR,
    px,
  );

  await gear.click(); // same stopPropagation'd toggle — closes the popover again
  await browser.waitUntil(async () => !(await $(POPOVER_SELECTOR).isExisting()), {
    timeout: 3_000,
    timeoutMsg: "expected the preview-pane settings popover to close after setting its width",
  });
}
