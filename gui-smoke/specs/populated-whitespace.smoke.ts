// CPE-1155 (harness proof) + CPE-1157 (bug repro/diagnosis/regression) — a FAITHFUL, NON-GRABBING
// right-click driven by gui-smoke/lib/mouse.ts (CDP `Input.dispatchMouseEvent`).
//
// Unlike a synthetic `dispatchEvent` (which let CPE-1154/1157 slip by delivering the event straight
// to a chosen handler), CDP input injects through the real browser pipeline — true hit-testing, real
// event order, the native context menu — but never moves the OS cursor. This spec:
//
//   A) CPE-1155: proves the CDP mouse channel is reachable here, a real right-click opens the app's
//      OWN menu (`.ctx`) not the native WebView2 one, and does NOT move the physical OS cursor
//      (captured via PowerShell before/after — belt-and-braces on top of CDP's design guarantee).
//
//   B) CPE-1157: in a POPULATED folder, right-clicks the BLANK area below the last row and asserts the
//      empty-area menu opens. This is the regression test — it FAILS on the buggy build and PASSES
//      once fixed. It also carries a deep DOM probe (which element receives the event, whether the
//      menu opens-then-closes vs never-opens, defaultPrevented after the pane handler) that pinned the
//      root cause: `paneContext` (ExplorerPane) opens the menu but does NOT `stopPropagation`, so the
//      SAME contextmenu keeps bubbling to `window`, where ContextMenu.svelte's own
//      `<svelte:window on:contextmenu={close}>` immediately dismisses it. The on-ITEM menu survives
//      only because `rowContext` DOES `stopPropagation`.
//
// Navigation is done with mouse.ts itself (CDP clicks on rows/breadcrumbs), so the spec dogfoods the
// helper and is deterministic regardless of the shared session's prior folder state.
import { expect } from "chai";
import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";
import { rightClick, click, doubleClick, cdpAvailable, type Point } from "../lib/mouse.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");
const MARKER_NAME = "CPE-1045-marker.txt"; // seeded into the tmpDir root — proves it's populated
const EMPTY_DIR_NAME = "CPE-1154-empty-folder"; // seeded EMPTY subdir (CPE-1154)

/** Current OS cursor position via .NET, so we can assert CDP input never moved the physical pointer. */
function osCursor(): { x: number; y: number } {
  const out = execSync(
    'powershell -NoProfile -Command "Add-Type -AssemblyName System.Windows.Forms; ' +
      '$p=[System.Windows.Forms.Cursor]::Position; Write-Output \\"$($p.X),$($p.Y)\\""',
    { encoding: "utf-8" },
  ).trim();
  const [x, y] = out.split(",").map((n) => parseInt(n, 10));
  return { x, y };
}

/** Viewport-space centre of the FIRST `.row` whose text contains `name` (or `null`). */
async function pointOfRow(name: string): Promise<Point | null> {
  return browser.execute((n) => {
    const rows = Array.from(document.querySelectorAll(".rows .row"));
    const row = rows.find((r) => (r.textContent || "").includes(n));
    if (!row) return null;
    const rect = row.getBoundingClientRect();
    return { x: Math.round(rect.left + rect.width / 2), y: Math.round(rect.top + rect.height / 2) };
  }, name) as Promise<Point | null>;
}

/** Viewport-space centre of the breadcrumb `.crumb` button whose text === `name` (or `null`). */
async function pointOfCrumb(name: string): Promise<Point | null> {
  return browser.execute((n) => {
    const crumbs = Array.from(document.querySelectorAll("button.crumb"));
    const c = crumbs.find((el) => (el.textContent || "").trim() === n);
    if (!c) return null;
    const rect = c.getBoundingClientRect();
    return { x: Math.round(rect.left + rect.width / 2), y: Math.round(rect.top + rect.height / 2) };
  }, name) as Promise<Point | null>;
}

async function currentCrumb(): Promise<string> {
  const el = await $('[aria-current="page"]');
  return (await el.isExisting()) ? el.getText() : "";
}

/** Ensure the explorer is at the populated tmpDir root (double-clicked into a subfolder by a prior
 *  test? click the tmpDir breadcrumb to come back). Uses CDP clicks (non-grabbing). */
async function ensurePopulatedRoot(tmpDir: string): Promise<void> {
  const base = path.basename(tmpDir);
  if ((await currentCrumb()) === base) return;
  const p = await pointOfCrumb(base);
  if (p) {
    await click(p);
    const crumb = await $('[aria-current="page"]');
    await browser.waitUntil(async () => (await crumb.getText()) === base, {
      timeout: 15_000,
      timeoutMsg: `could not return to populated root "${base}" via breadcrumb`,
    });
  }
}

async function dismissMenu(): Promise<void> {
  if (await $(".ctx").isExisting()) {
    await browser.keys("Escape");
    await browser.pause(150);
  }
}

/** Deep probe: a MutationObserver logging `.ctx` presence transitions (open-then-close vs never-open)
 *  + a bubble-phase window `contextmenu` listener recording the real target and `defaultPrevented`
 *  AFTER the pane handler ran (paneContext calls preventDefault, so `true` ⇒ it fired). */
async function armDeepProbe(): Promise<void> {
  await browser.execute(() => {
    const w = window as unknown as Record<string, unknown>;
    const probe = { ctxLog: [] as { t: number; present: boolean }[], ctxEvents: [] as unknown[] };
    w.__deepProbe = probe;
    const present = () => !!document.querySelector(".ctx");
    const record = () => {
      const p = present();
      const log = probe.ctxLog;
      if (log.length === 0 || log[log.length - 1].present !== p) log.push({ t: performance.now(), present: p });
    };
    record();
    const obs = new MutationObserver(record);
    obs.observe(document.body, { childList: true, subtree: true });
    window.addEventListener(
      "contextmenu",
      (e: MouseEvent) => {
        const t = e.target as Element | null;
        probe.ctxEvents.push({
          t: performance.now(),
          target: t ? `${t.tagName}.${(t.className || "").toString()}` : null,
          defaultPreventedAtWindowBubble: e.defaultPrevented,
        });
      },
      false, // bubble — runs AFTER the target/element handlers
    );
  });
}

async function readDeepProbe(): Promise<{
  ctxLog: { t: number; present: boolean }[];
  ctxEvents: { target: string | null; defaultPreventedAtWindowBubble: boolean }[];
}> {
  return browser.execute(() => (window as unknown as { __deepProbe: unknown }).__deepProbe) as Promise<{
    ctxLog: { t: number; present: boolean }[];
    ctxEvents: { target: string | null; defaultPreventedAtWindowBubble: boolean }[];
  }>;
}

async function paneGeometry() {
  return browser.execute(() => {
    const rect = (sel: string) => {
      const el = document.querySelector(sel);
      if (!el) return null;
      const r = el.getBoundingClientRect();
      return { left: r.left, top: r.top, right: r.right, bottom: r.bottom, width: r.width, height: r.height };
    };
    const rows = document.querySelector(".rows");
    const allRows = rows ? rows.querySelectorAll(".row") : ([] as unknown as NodeListOf<Element>);
    const last = allRows.length ? allRows[allRows.length - 1] : null;
    const lastR = last ? last.getBoundingClientRect() : null;
    return {
      dpr: window.devicePixelRatio,
      pane: rect(".filelist-pane"),
      rows: rect(".rows"),
      rowCount: allRows.length,
      lastRow: lastR ? { left: lastR.left, top: lastR.top, right: lastR.right, bottom: lastR.bottom, height: lastR.height } : null,
    };
  }) as Promise<{
    dpr: number;
    pane: { left: number; top: number; right: number; bottom: number; width: number; height: number } | null;
    rows: { left: number; top: number; right: number; bottom: number; width: number; height: number } | null;
    rowCount: number;
    lastRow: { left: number; top: number; right: number; height: number } | null;
  }>;
}

describe("CPE-1155 + CPE-1157 — faithful non-grabbing right-click on populated whitespace", () => {
  let tmpDir = "";

  before(() => {
    ({ tmpDir } = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string });
  });

  afterEach(async function () {
    await snapFailure(this.currentTest, "populated-whitespace-rightclick");
    await dismissMenu();
  });

  it("CPE-1155: the CDP mouse-input channel is available in this driver", async () => {
    const ok = await cdpAvailable();
    // eslint-disable-next-line no-console
    console.log(`[CPE-1155] cdpAvailable() = ${ok}`);
    expect(ok, "CDP Input injection via msedgedriver's chromium/send_command endpoint").to.equal(true);
  });

  it("is at the POPULATED tmpDir root with files rendered", async () => {
    await ensurePopulatedRoot(tmpDir);
    const body = await $("body");
    await browser.waitUntil(
      async () => (await body.getHTML({ includeSelectorTag: false })).includes(MARKER_NAME),
      { timeout: 15_000, timeoutMsg: "populated tmpDir listing did not render the marker file" },
    );
    expect(await $(".rows").isExisting(), "populated folder renders .rows").to.equal(true);
    expect(await $(".empty-state").isExisting(), "populated folder must NOT show .empty-state").to.equal(false);
    expect((await $$(".row")).length, "populated folder has >=1 row").to.be.greaterThan(0);
  });

  it("CPE-1157 (regression + diagnosis): right-click the BLANK area below the last row opens the empty-area menu", async () => {
    await ensurePopulatedRoot(tmpDir);
    await dismissMenu();

    const geo = await paneGeometry();
    // eslint-disable-next-line no-console
    console.log("[CPE-1157] geometry:", JSON.stringify(geo));
    expect(geo.pane, "pane rect").to.not.equal(null);
    expect(geo.rows, "rows rect").to.not.equal(null);
    const paneR = geo.pane!;
    const rowsR = geo.rows!;
    let y = Math.round(rowsR.bottom + 20);
    if (y >= paneR.bottom - 2) y = Math.round(paneR.bottom - 4);
    const point: Point = { x: Math.round(paneR.left + paneR.width / 2), y };

    // Confirm the point is genuinely blank pane (not a row) before clicking.
    const hit = (await browser.execute(
      (x, yy) => {
        const el = document.elementFromPoint(x, yy);
        return el ? `${el.tagName}.${(el.className || "").toString()}` : null;
      },
      point.x,
      point.y,
    )) as string | null;
    // eslint-disable-next-line no-console
    console.log(`[CPE-1157] blank point (${point.x},${point.y}) elementFromPoint = ${hit}`);

    await armDeepProbe();
    const before = osCursor();
    await rightClick(point);
    const after = osCursor();
    // eslint-disable-next-line no-console
    console.log(`[CPE-1157] OS cursor before=${before.x},${before.y} after=${after.x},${after.y}`);
    expect(after, "CDP right-click must not move the OS cursor").to.deep.equal(before);

    await browser.pause(500);
    const probe = await readDeepProbe();
    const ctxExists = await $(".ctx").isExisting();
    const hasPaste = ctxExists ? (await $(".ctx").getHTML({ includeSelectorTag: false })).includes("Ctrl+V") : false;
    const hasQuickrow = ctxExists ? await $(".ctx .quickrow").isExisting() : false;
    // eslint-disable-next-line no-console
    console.log("[CPE-1157] DEEP PROBE:", JSON.stringify({ ctxExists, hasPaste, hasQuickrow, ctxLog: probe.ctxLog, ctxEvents: probe.ctxEvents }, null, 2));

    await snap("populated-whitespace-rightclick");

    // The regression assertion (CPE-1157 AC): empty-area menu opens, is the empty variant, no quickrow.
    expect(ctxExists, "empty-area .ctx must open on right-click below the last row").to.equal(true);
    expect(hasPaste, "the opened menu is the empty-area variant (Paste/Ctrl+V present)").to.equal(true);
    expect(hasQuickrow, "empty-area menu must NOT show the on-item quick-action row").to.equal(false);
  });

  it("CPE-1157: on-item right-click still opens the item menu (regression guard for the fix)", async () => {
    await ensurePopulatedRoot(tmpDir);
    await dismissMenu();
    const p = await pointOfRow(MARKER_NAME);
    expect(p, `row for ${MARKER_NAME}`).to.not.equal(null);
    await rightClick(p!);
    const ctx = await $(".ctx");
    await ctx.waitForExist({ timeout: 10_000, timeoutMsg: "item menu did not open on a real right-click on a row" });
    expect(await $(".ctx .quickrow").isExisting(), "on-item menu shows the quick-action row").to.equal(true);
  });

  it("CPE-1155: real right-click in an EMPTY folder opens the app menu (not native) without moving the OS cursor", async () => {
    await ensurePopulatedRoot(tmpDir);
    await dismissMenu();
    const folderPt = await pointOfRow(EMPTY_DIR_NAME);
    expect(folderPt, `row for ${EMPTY_DIR_NAME}`).to.not.equal(null);
    await doubleClick(folderPt!);
    const emptyState = await $(".empty-state");
    await emptyState.waitForExist({ timeout: 15_000, timeoutMsg: "empty-state did not render after entering the empty folder" });
    await dismissMenu();

    const pane = await $(".filelist-pane");
    const loc = await pane.getLocation();
    const size = await pane.getSize();
    const point: Point = { x: Math.round(loc.x + size.width / 2), y: Math.round(loc.y + 12) };

    const before = osCursor();
    await rightClick(point);
    const after = osCursor();
    // eslint-disable-next-line no-console
    console.log(`[CPE-1155] empty-folder OS cursor before=${before.x},${before.y} after=${after.x},${after.y}`);
    expect(after, "CDP input must NOT move the physical OS cursor").to.deep.equal(before);

    const ctx = await $(".ctx");
    await ctx.waitForExist({ timeout: 10_000, timeoutMsg: "app .ctx menu did not open on a real CDP right-click (empty folder)" });
    expect((await ctx.getHTML({ includeSelectorTag: false })).includes("Ctrl+V"), "empty-area menu carries Paste/Ctrl+V").to.equal(true);
    await snap("populated-whitespace-rightclick");
  });
});
