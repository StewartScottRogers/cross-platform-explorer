// CPE-1960 — the ONE way this suite scrolls an element into view: the DOM's own
// `Element.scrollIntoView()`, executed in the page, NEVER WebdriverIO's `element.scrollIntoView()`
// command.
//
// WHAT WENT WRONG (the failure this file exists to prevent)
// `macro-param-prompt.smoke.ts` opens the item context menu, hovers "Run macro ▸" to open its flyout,
// and then asked WebdriverIO to scroll the flyout's macro row into view before clicking it. On
// 2026-08-27 that spec turned into a permanent red on `gui-smoke-linux-verdict`:
//
//     element (".ctx .flyout .row") still not existing after 5000ms
//
// The cause was not the app and not a slow render. `webdriverio`'s `scrollIntoView` COMMAND does not
// call the DOM API — it computes a delta and injects a real mouse-wheel through the driver. In
// 9.30.0 that wheel carried `origin: <the element>` and `deltaX/deltaY: 0`, so it was a harmless
// no-op. CPE-1945's `npm audit fix` (PR #1065, merged 22:27Z) moved this project to **9.31.4**, whose
// implementation is:
//
//     await browser.action('wheel').scroll({ duration: 0, x: 0, y: 0, deltaX, deltaY }).perform()
//
// — no `origin`, so the wheel lands at **viewport (0, 0)**, nowhere near the element, with a real
// non-zero delta computed from the element's rect. Measured locally against Chrome 151 with this exact
// webdriverio build: scrolling an already-on-screen `.ctx .flyout .row` emits
// `{"type":"scroll","x":0,"y":0,"deltaX":560,"deltaY":222}`. On WebKitGTK (the Linux CI driver) that
// stray wheel relocates the webview's hover target, `Submenu.svelte`'s `on:mouseleave` fires,
// `closeMenu()` unmounts the flyout, and the very next command on the row refetches
// `.ctx .flyout .row`, finds nothing, and burns the 5 s implicit wait. The failure screenshot from CI
// shows exactly that end state: the context menu still open, still unscrolled, flyout gone.
//
// WHY THE DOM API INSTEAD
//   * A popup-menu row can never be "scrolled into view" in the first place — `ContextMenu.svelte`'s
//     `onMount` clamp and `Submenu.svelte`'s `positionFlyout()` already pin the menu fully on screen,
//     and both are `position: fixed`. `Element.scrollIntoView()` correctly does nothing there; the
//     wheel command actively destroys the menu.
//   * For the rows that genuinely DO need scrolling (a symlink row below the fold in
//     `link-badge`/`transfer-panel`), the DOM API scrolls the row's own nearest scrollable ancestor —
//     `.filelist-pane`. WebdriverIO's wheel-at-(0,0) never scrolled that pane at all: the app's
//     document does not scroll, so the wheel chained to nothing. The DOM API is strictly more correct
//     here, not merely safer.
//   * It is already the majority convention in this suite: `archive-browse.smoke.ts`,
//     `archive-password.smoke.ts`, `drive-menu.smoke.ts`, `home-item-menu.smoke.ts` and
//     `vault.smoke.ts` all call `element.scrollIntoView({ block: "center" })` from inside a
//     `browser.execute` block. This helper makes that the single convention.
//
// `lib/scrollIntoViewUsage.test.ts` fails the build if the WebdriverIO command comes back.
import { browser } from "@wdio/globals";

/**
 * Scroll `el` to the centre of its nearest scrollable ancestor using the page's own
 * `Element.scrollIntoView()`, then settle for a frame or two.
 *
 * Deliberately a no-op for anything already on screen (a `position: fixed` menu/dialog row), which is
 * what CPE-1960 needed and what WebdriverIO's same-named command no longer gives us.
 */
export async function scrollIntoViewCentered(el: WebdriverIO.Element): Promise<void> {
  await el.execute((node) => {
    (node as HTMLElement).scrollIntoView({ block: "center", inline: "nearest" });
  });
  await browser.pause(150);
}
