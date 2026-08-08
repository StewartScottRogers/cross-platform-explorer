// CPE-1155 — faithful, NON-GRABBING mouse input for the gui-smoke harness.
// CPE-1479 — WebDriver-native fallback so the harness works on drivers without CDP (Linux/WebKit).
//
// WHY THIS EXISTS (the exact tension it resolves)
// The harness had two mouse options and both were bad:
//   - `browser.action("pointer")…`  — WebDriver's native Actions API. Faithful (real hit-testing),
//     but on this stack (tauri-driver → msedgedriver → wry/WebView2) it moves/grabs input in a way
//     that hijacks an interactive machine, violating [[automation-must-not-hijack-screen]] — tests
//     can't run while the user is at the keyboard.
//   - `el.dispatchEvent(new MouseEvent(...))` via `browser.execute` — non-grabbing, but UNFAITHFUL:
//     the event is delivered straight to a chosen DOM node's handler, bypassing real hit-testing and
//     native browser behaviour. That is exactly how the CPE-1154 native-menu leak escaped detection:
//     a synthetic `contextmenu` on `.rows` "worked" while a real right-click on blank pane pixels
//     showed the Edge menu.
//
// THE PRIMARY APPROACH — CDP `Input.*` injection (Windows / any Chromium-backed webview).
// The Chromium DevTools Protocol's `Input.dispatchMouseEvent` / `Input.dispatchMouseWheelEvent`
// inject through the browser's REAL input pipeline: true hit-testing, native context menu, real
// event order (mousemove → mousedown → mouseup → click / contextmenu) — as faithful as a physical
// click — but they operate on the page's own coordinate space and, BY DESIGN, never move the OS
// pointer. So the user's physical cursor stays exactly where it is (and the tauri-driver window can
// stay unfocused/off-screen). This is the path taken on Windows (Edge/WebView2), where the machine
// may be interactive, so the non-grabbing guarantee matters most.
//
// msedgedriver (like chromedriver) exposes CDP over a vendor WebDriver endpoint
// `POST /session/:id/chromium/send_command_and_get_result`, surfaced by WebdriverIO as
// `browser.sendCommandAndGetResult(cmd, params)` (and the fire-and-forget `browser.sendCommand`).
// `browser.cdp(...)` — the puppeteer-backed variant the ticket mentions as an alternative — is NOT
// available in this harness (no @wdio/devtools-service / puppeteer attach against wry), so we use the
// vendor endpoint. `cdpAvailable()` below probes it.
//
// THE FALLBACK — the W3C WebDriver Actions API (CPE-1479; Linux / WebKitWebDriver).
// On Linux the native driver is WebKitWebDriver (wry/WebKitGTK), which has NO CDP vendor endpoint —
// `sendCommand*` simply isn't attached, so the CDP path threw
// ("CDP input injection is not reachable through this driver") and EVERY mouse-driven spec failed,
// which also cascaded into the ~20-minute job timeout (specs then sat in `waitUntil`/`waitForExist`
// loops burning their full timeouts waiting for dialogs/navigations that a broken click never
// triggered). When CDP is unavailable we fall back to the standard W3C Actions API
// (`browser.performActions` / `releaseActions`, the `POST /session/:id/actions` endpoint every
// conformant driver — WebKitWebDriver included — implements). It IS the "grabbing" option CPE-1155
// avoided on Windows, but the fallback only runs where CDP is absent: Linux CI under `xvfb`
// (a virtual display with no user to hijack) and attended macOS/Linux local runs (the harness
// already steals window focus at launch there — see wdio.conf.ts's header — and those OSes have no
// non-grabbing tauri-driver path anyway). Windows keeps the non-grabbing CDP fast-path untouched.
//
// USE THIS for mouse behaviour, NOT a bare `dispatchEvent` (unfaithful). See gui-smoke/README.md and
// .claude/qa-architecture/README.md.
import { browser } from "@wdio/globals";

/** CDP mouse-button names (`Input.dispatchMouseEvent#button`). */
export type CdpButton = "none" | "left" | "middle" | "right";

/** A viewport point in CSS pixels — the same space `getBoundingClientRect()` reports and the space
 *  CDP `Input.*` coordinates (and the W3C Actions `origin: "viewport"` space) use (device-independent
 *  pixels), so no DPR scaling is needed. */
export interface Point {
  x: number;
  y: number;
}

/** A `sendCommandAndGetResult`-capable browser (the chromium vendor commands msedgedriver attaches).
 *  Typed structurally so this file needs no `webdriverio` augmentation import. */
interface CdpBrowser {
  sendCommandAndGetResult?: (cmd: string, params: Record<string, unknown>) => Promise<unknown>;
  sendCommand?: (cmd: string, params: Record<string, unknown>) => Promise<unknown>;
}

function cdpBrowser(): CdpBrowser {
  return browser as unknown as CdpBrowser;
}

/**
 * Run one CDP command against the session's active page target via msedgedriver's vendor endpoint.
 * Prefers `sendCommandAndGetResult` (awaits the CDP reply — so a subsequent DOM assertion sees the
 * effect); falls back to `sendCommand` if only that is attached.
 */
export async function cdp(cmd: string, params: Record<string, unknown> = {}): Promise<unknown> {
  const b = cdpBrowser();
  if (typeof b.sendCommandAndGetResult === "function") {
    return b.sendCommandAndGetResult(cmd, params);
  }
  if (typeof b.sendCommand === "function") {
    return b.sendCommand(cmd, params);
  }
  throw new Error(
    "[mouse.ts] Neither browser.sendCommandAndGetResult nor browser.sendCommand is available — " +
      "CDP input injection is not reachable through this driver. See cdpAvailable().",
  );
}

/**
 * True if the CDP input channel is actually usable here: the vendor command is attached AND a
 * trivial, side-effect-free CDP call (`Browser.getVersion`) round-trips. Lets a spec DOCUMENT the
 * finding (CPE-1155 AC) instead of failing opaquely if a future driver drops the endpoint. Also the
 * gate the public mouse fns use (via `useCdp()`) to decide CDP-vs-Actions (CPE-1479).
 */
export async function cdpAvailable(): Promise<boolean> {
  const b = cdpBrowser();
  if (typeof b.sendCommandAndGetResult !== "function" && typeof b.sendCommand !== "function") {
    return false;
  }
  try {
    await cdp("Browser.getVersion", {});
    return true;
  } catch {
    return false;
  }
}

// --- CPE-1479: driver-capability decision, probed once per session ------------------------------
// `cdpAvailable()` short-circuits to `false` with no round trip when the vendor command isn't even
// attached (the WebKitWebDriver case), so this probe is cheap; caching it just avoids repeating the
// `Browser.getVersion` round trip on Chromium for every one of a spec's clicks.
let cdpDecision: Promise<boolean> | undefined;
function useCdp(): Promise<boolean> {
  return (cdpDecision ??= cdpAvailable());
}

/** A `performActions`/`releaseActions`-capable browser (the standard W3C Actions endpoint). Typed
 *  structurally, same reasoning as {@link CdpBrowser}. `performActions` sends `POST
 *  /session/:id/actions`; with `wdio:enforceWebDriverClassic` (wdio.conf.ts) this is classic
 *  WebDriver, which WebKitWebDriver implements. */
interface ActionsBrowser {
  performActions?: (actions: unknown[]) => Promise<void>;
  releaseActions?: () => Promise<void>;
}

function actionsBrowser(): ActionsBrowser {
  return browser as unknown as ActionsBrowser;
}

/** W3C `button` numbers (`pointerDown`/`pointerUp`) — distinct from the CDP name enum above. */
function w3cButton(button: CdpButton): number {
  switch (button) {
    case "right":
      return 2;
    case "middle":
      return 1;
    default:
      return 0; // left (and "none", which is only used for pure moves that carry no button)
  }
}

/** One W3C pointer action tick (a single `pointer` input source's `actions` entry). */
type PointerStep =
  | { type: "pointerMove"; duration: number; origin: "viewport"; x: number; y: number }
  | { type: "pointerDown"; button: number }
  | { type: "pointerUp"; button: number };

/**
 * Play a W3C pointer action sequence through the standard Actions endpoint, then `releaseActions` so
 * the driver's virtual pointer state (held buttons) is reset before the next call — matching how a
 * real gesture ends. The `performActions`-missing branch should be unreachable (every conformant
 * driver has it) but is guarded so a truly incapable driver fails with a clear message rather than a
 * `TypeError`.
 */
async function performPointer(steps: PointerStep[]): Promise<void> {
  const b = actionsBrowser();
  if (typeof b.performActions !== "function") {
    throw new Error(
      "[mouse.ts] Neither CDP input injection nor the WebDriver Actions API " +
        "(browser.performActions) is reachable through this driver — cannot inject mouse input.",
    );
  }
  await b.performActions([{ type: "pointer", id: "mouse", parameters: { pointerType: "mouse" }, actions: steps }]);
  if (typeof b.releaseActions === "function") await b.releaseActions();
}

/** A `pointerMove` to a viewport point (shared prefix of every gesture below). */
function moveStep(p: Point): PointerStep {
  return { type: "pointerMove", duration: 0, origin: "viewport", x: p.x, y: p.y };
}

/** The CSS-pixel bitmask CDP wants in `buttons` while a button is held (mousePressed/last mouseMoved). */
function buttonsMask(button: CdpButton): number {
  switch (button) {
    case "left":
      return 1;
    case "right":
      return 2;
    case "middle":
      return 4;
    default:
      return 0;
  }
}

/**
 * Resolve a CSS selector to the viewport-space centre of the FIRST matching element. Returns `null`
 * if nothing matches (so a caller can assert/skip rather than click a phantom point). Runs through
 * `browser.execute`, the one primitive this harness has already proven reliable against wry's webview
 * under the classic-WebDriver protocol it forces.
 */
export async function pointOf(selector: string): Promise<Point | null> {
  return browser.execute((sel) => {
    const el = document.querySelector(sel);
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) };
  }, selector) as Promise<Point | null>;
}

function isPoint(v: string | Point): v is Point {
  return typeof v === "object" && v !== null && typeof (v as Point).x === "number";
}

async function resolve(target: string | Point): Promise<Point> {
  if (isPoint(target)) return target;
  const p = await pointOf(target);
  if (!p) throw new Error(`[mouse.ts] no element matched selector: ${JSON.stringify(target)}`);
  return p;
}

/** A single CDP mouse event at a point. */
async function mouseEvent(
  type: "mousePressed" | "mouseReleased" | "mouseMoved",
  p: Point,
  button: CdpButton,
  clickCount: number,
  heldButtons = 0,
): Promise<void> {
  await cdp("Input.dispatchMouseEvent", {
    type,
    x: p.x,
    y: p.y,
    button,
    buttons: heldButtons,
    clickCount,
  });
}

/** Move the (synthetic) pointer over a point — fires `mousemove`/`pointermove`, updates `:hover`.
 *  Does NOT move the OS cursor on the CDP path. */
export async function hover(target: string | Point): Promise<void> {
  const p = await resolve(target);
  if (await useCdp()) {
    await mouseEvent("mouseMoved", p, "none", 0);
    return;
  }
  await performPointer([moveStep(p)]);
}

/** A faithful left click: move → press → release, firing the real mousedown/mouseup/click order. */
export async function click(target: string | Point): Promise<void> {
  const p = await resolve(target);
  if (await useCdp()) {
    await mouseEvent("mouseMoved", p, "none", 0);
    await mouseEvent("mousePressed", p, "left", 1, buttonsMask("left"));
    await mouseEvent("mouseReleased", p, "left", 1, 0);
    return;
  }
  await performPointer([
    moveStep(p),
    { type: "pointerDown", button: w3cButton("left") },
    { type: "pointerUp", button: w3cButton("left") },
  ]);
}

/**
 * A faithful right click at an element OR an explicit viewport point: move → press(right) →
 * release(right). Chromium (CDP) / WebKit (Actions) synthesises the native `contextmenu` event from
 * this real button sequence — the whole reason this exists (a bare `dispatchEvent("contextmenu")`
 * never exercises hit-testing or the native menu). Accepts a `Point` so a spec can aim at blank pane
 * pixels that match no single selector (e.g. below the last row).
 */
export async function rightClick(target: string | Point): Promise<void> {
  const p = await resolve(target);
  if (await useCdp()) {
    await mouseEvent("mouseMoved", p, "none", 0);
    await mouseEvent("mousePressed", p, "right", 1, buttonsMask("right"));
    await mouseEvent("mouseReleased", p, "right", 1, 0);
    return;
  }
  await performPointer([
    moveStep(p),
    { type: "pointerDown", button: w3cButton("right") },
    { type: "pointerUp", button: w3cButton("right") },
  ]);
}

/** A faithful double-click: two press/release pairs, the second carrying clickCount 2 (CDP). On the
 *  Actions path the two back-to-back down/up pairs at a stationary point are dispatched with no
 *  intervening pause, so they fall inside the platform double-click interval and synthesise `dblclick`. */
export async function doubleClick(target: string | Point): Promise<void> {
  const p = await resolve(target);
  if (await useCdp()) {
    await mouseEvent("mouseMoved", p, "none", 0);
    await mouseEvent("mousePressed", p, "left", 1, buttonsMask("left"));
    await mouseEvent("mouseReleased", p, "left", 1, 0);
    await mouseEvent("mousePressed", p, "left", 2, buttonsMask("left"));
    await mouseEvent("mouseReleased", p, "left", 2, 0);
    return;
  }
  const down: PointerStep = { type: "pointerDown", button: w3cButton("left") };
  const up: PointerStep = { type: "pointerUp", button: w3cButton("left") };
  await performPointer([moveStep(p), down, up, down, up]);
}

/**
 * Faithful wheel scroll over an element/point: dispatches `Input.dispatchMouseWheelEvent` (CDP) or a
 * W3C `wheel`-source `scroll` action (Actions fallback) so the real scroll pipeline runs (the element
 * under the point scrolls, `wheel`/`scroll` events fire). `dy` > 0 scrolls down (content moves up),
 * matching a physical wheel; pass `dx` for horizontal.
 */
export async function scroll(target: string | Point, dy: number, dx = 0): Promise<void> {
  const p = await resolve(target);
  if (await useCdp()) {
    await cdp("Input.dispatchMouseWheelEvent", {
      type: "mouseWheel",
      x: p.x,
      y: p.y,
      deltaX: dx,
      deltaY: dy,
    });
    return;
  }
  const b = actionsBrowser();
  if (typeof b.performActions !== "function") {
    throw new Error(
      "[mouse.ts] Neither CDP input injection nor the WebDriver Actions API " +
        "(browser.performActions) is reachable through this driver — cannot inject wheel input.",
    );
  }
  await b.performActions([
    {
      type: "wheel",
      id: "wheel",
      actions: [{ type: "scroll", duration: 0, origin: "viewport", x: p.x, y: p.y, deltaX: dx, deltaY: dy }],
    },
  ]);
  if (typeof b.releaseActions === "function") await b.releaseActions();
}

/**
 * Faithful drag: press(left) at `from`, move to `to` while the button is held (so drag/dragover fire
 * with a real held-button state), then release at `to`. An intermediate midpoint move is included so
 * drag-threshold logic that needs >1 move sees genuine motion.
 */
export async function dragTo(from: string | Point, to: string | Point): Promise<void> {
  const a = await resolve(from);
  const b = await resolve(to);
  const mid: Point = { x: Math.round((a.x + b.x) / 2), y: Math.round((a.y + b.y) / 2) };
  if (await useCdp()) {
    await mouseEvent("mouseMoved", a, "none", 0);
    await mouseEvent("mousePressed", a, "left", 1, buttonsMask("left"));
    await mouseEvent("mouseMoved", mid, "left", 0, buttonsMask("left"));
    await mouseEvent("mouseMoved", b, "left", 0, buttonsMask("left"));
    await mouseEvent("mouseReleased", b, "left", 1, 0);
    return;
  }
  await performPointer([
    moveStep(a),
    { type: "pointerDown", button: w3cButton("left") },
    moveStep(mid),
    moveStep(b),
    { type: "pointerUp", button: w3cButton("left") },
  ]);
}
