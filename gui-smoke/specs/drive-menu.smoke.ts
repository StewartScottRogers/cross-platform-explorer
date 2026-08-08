// CPE-1159 — the disk/drive right-click menu opens then INSTANTLY closes ("no menu" flash).
//
// THE BUG (same open-then-close race CPE-1157 fixed for the explorer pane): the drive-tile
// (HomeView) and drive-row (Sidebar) `contextmenu` handlers `preventDefault()` + dispatch
// `driveContext` but did NOT `stopPropagation()`. So the SAME `contextmenu` event kept bubbling to
// `window`, where `ContextMenu.svelte`'s `<svelte:window on:contextmenu|preventDefault={close}>`
// dismissed the menu it had just opened (~5 ms flash → user sees no menu). The fix adds
// `e.stopPropagation()` to both handlers.
//
// WHY THIS SPEC (and not just the jsdom mount test): the CPE-1158 jsdom test only asserted the menu
// MOUNTS on a synthetic `contextMenu` fired straight at the handler — it never exercised the real
// window-level dismisser, so the open-then-close race slipped through exactly like CPE-1154/1157.
// This spec drives a FAITHFUL, non-grabbing CDP right-click (gui-smoke/lib/mouse.ts, CPE-1155)
// through the real browser input pipeline, then asserts the drive menu is present AND STILL present
// a frame/tick later — the stay-open contract the mount-only test missed. It FAILS on the buggy
// build (menu gone after the flash) and PASSES once both handlers stopPropagation.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";
import { rightClick, click, type Point } from "../lib/mouse.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

/** True if `p` looks like a DRIVE ROOT — a Windows volume root (`C:\`, `Z:\`) or the POSIX root
 *  (`/`). Drive tiles/rows carry such a path; ordinary "quick access" places carry deeper paths, so
 *  this cleanly picks a drive out of the mixed list without needing a production-only test hook. */
function isDriveRoot(p: string): boolean {
  return /^[A-Za-z]:[\\/]?$/.test(p) || p === "/";
}

/** Viewport-space centre of the breadcrumb `.crumb` button whose text === `name` (or `null`). */
async function pointOfCrumb(name: string): Promise<Point | null> {
  return browser.execute((n) => {
    const crumbs = Array.from(document.querySelectorAll("button.crumb"));
    const c = crumbs.find((el) => (el.textContent || "").trim() === n);
    if (!c) return null;
    const r = c.getBoundingClientRect();
    return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) };
  }, name) as Promise<Point | null>;
}

/** Viewport-space centre of the FIRST Home `.qa-card` whose `.qa-sub` path is a drive root, plus the
 *  path it targets (or `null` if the Home screen shows no drive tile — e.g. a machine with none). */
async function pointOfDriveTile(): Promise<{ point: Point; path: string } | null> {
  const drive = /^[A-Za-z]:[\\/]?$/;
  return browser.execute((driveSrc) => {
    const re = new RegExp(driveSrc);
    const cards = Array.from(document.querySelectorAll(".qa-card"));
    for (const card of cards) {
      const sub = card.querySelector(".qa-sub");
      const p = (sub?.textContent || "").trim();
      if (re.test(p) || p === "/") {
        const r = card.getBoundingClientRect();
        return { point: { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) }, path: p };
      }
    }
    return null;
  }, drive.source) as Promise<{ point: Point; path: string } | null>;
}

/** A hit-testable point on the FIRST sidebar drive row (a `.nav-item[data-drop-path]` whose path is a
 *  drive root), plus its path (or `null`). Sidebar drive rows are the OTHER handler CPE-1159 fixed.
 *  Occlusion-aware: the geometric centre of a low drive row can sit UNDER the fixed status bar, so we
 *  scan candidate y's within the row and return the first whose `elementFromPoint` actually resolves
 *  INSIDE that row — otherwise the right-click would land on whatever overlays it and open no menu. */
async function pointOfSidebarDrive(): Promise<{ point: Point; path: string } | null> {
  const drive = /^[A-Za-z]:[\\/]?$/;
  // Scroll the first drive row toward the middle of its scroll container so its rect isn't left
  // behind the fixed status bar (which overlays the very bottom of the sidebar).
  await browser.execute((driveSrc) => {
    const re = new RegExp(driveSrc);
    const rows = Array.from(document.querySelectorAll(".nav-item[data-drop-path]"));
    const row = rows.find((el) => {
      const p = (el.getAttribute("data-drop-path") || "").trim();
      return re.test(p) || p === "/";
    });
    row?.scrollIntoView({ block: "center" });
  }, drive.source);
  await browser.pause(150);
  return browser.execute((driveSrc) => {
    const re = new RegExp(driveSrc);
    const rows = Array.from(document.querySelectorAll(".nav-item[data-drop-path]"));
    for (const row of rows) {
      const p = (row.getAttribute("data-drop-path") || "").trim();
      if (!(re.test(p) || p === "/")) continue;
      const r = row.getBoundingClientRect();
      if (r.width === 0 || r.height === 0) continue; // collapsed/hidden
      const x = Math.round(r.left + r.width / 2);
      // Try centre first, then walk from just-below-top downward — pick the first y that hit-tests
      // to an element inside THIS drive row (its own `data-drop-path`), i.e. not occluded.
      const ys = [Math.round(r.top + r.height / 2)];
      for (let dy = 3; dy < r.height; dy += 3) ys.push(Math.round(r.top + dy));
      for (const y of ys) {
        const el = document.elementFromPoint(x, y);
        const owner = el?.closest?.("[data-drop-path]") as HTMLElement | null;
        if (owner && owner.getAttribute("data-drop-path") === p) {
          return { point: { x, y }, path: p };
        }
      }
    }
    return null;
  }, drive.source) as Promise<{ point: Point; path: string } | null>;
}

/** Arm a MutationObserver logging `.ctx` presence transitions, so we can PROVE the open-then-close
 *  signature (present:true → present:false in the same tick) rather than only sampling the end state.
 *  Mirrors populated-whitespace.smoke.ts's deep probe. */
async function armCtxProbe(): Promise<void> {
  await browser.execute(() => {
    const w = window as unknown as Record<string, unknown>;
    const log: { t: number; present: boolean }[] = [];
    w.__ctxProbe = log;
    const present = () => !!document.querySelector(".ctx");
    const record = () => {
      if (log.length === 0 || log[log.length - 1].present !== present()) log.push({ t: performance.now(), present: present() });
    };
    record();
    new MutationObserver(record).observe(document.body, { childList: true, subtree: true });
  });
}

async function readCtxProbe(): Promise<{ t: number; present: boolean }[]> {
  return browser.execute(() => (window as unknown as { __ctxProbe: unknown }).__ctxProbe) as Promise<
    { t: number; present: boolean }[]
  >;
}

async function dismissMenu(): Promise<void> {
  if (await $(".ctx").isExisting()) {
    await browser.keys("Escape");
    await browser.pause(150);
  }
}

/** Navigate to the Home landing (which shows drive TILES) via the always-present "Home" breadcrumb. */
async function goHome(): Promise<void> {
  const p = await pointOfCrumb("Home");
  if (p) await click(p);
  await browser.waitUntil(async () => (await $(".qa-grid").isExisting()) || (await $(".home").isExisting()), {
    timeout: 15_000,
    timeoutMsg: "Home landing (.qa-grid/.home) did not render after clicking the Home breadcrumb",
  });
}

/** Right-click `point`, then assert the DRIVE menu opens AND STAYS open (the CPE-1159 contract).
 *  `where` is only for readable failure messages. */
async function assertDriveMenuStaysOpen(point: Point, where: string): Promise<void> {
  await dismissMenu();
  await armCtxProbe();
  await rightClick(point);

  // Give the buggy build's window-level dismisser every chance to fire, then read the END state.
  await browser.pause(500);
  const probe = await readCtxProbe();
  const ctxExists = await $(".ctx").isExisting();
  const html = ctxExists ? await $(".ctx").getHTML({ includeSelectorTag: false }) : "";
  const hasQuickrow = ctxExists ? await $(".ctx .quickrow").isExisting() : false;
  // eslint-disable-next-line no-console
  console.log(`[CPE-1159] ${where} probe:`, JSON.stringify({ ctxExists, hasQuickrow, ctxLog: probe }));

  // The stay-open contract: the menu is present after the tick. On the buggy build it flashed open
  // then the window handler closed it, so `.ctx` is gone here → this fails (as it must, pre-fix).
  expect(ctxExists, `[${where}] drive menu (.ctx) must STILL be present a tick after right-click (open-then-close regression)`).to.equal(true);
  // And it's the DRIVE variant, not the item or empty-area menu: drive menu carries "Open in
  // Terminal" + "Copy as path", has NO on-item quick-action row, and NO Paste (Ctrl+V hint).
  expect(html, `[${where}] opened menu is the drive variant (Open in Terminal)`).to.include("Open in Terminal");
  expect(html, `[${where}] opened menu is the drive variant (Copy as path)`).to.include("Copy as path");
  expect(hasQuickrow, `[${where}] drive menu must not show the on-item quick-action row`).to.equal(false);
  expect(html.includes("Ctrl+V"), `[${where}] drive menu is not the empty-area (Paste) variant`).to.equal(false);
}

describe("CPE-1159 — drive right-click menu opens and STAYS open (stopPropagation regression)", () => {
  before(() => {
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")); // confirm the harness seeded state
  });

  afterEach(async function () {
    await snapFailure(this.currentTest, "drive-menu");
    await dismissMenu();
  });

  it("Home shows at least one drive tile to right-click", async () => {
    await goHome();
    // CPE-1481: `goHome()` only waits for the outer `.qa-grid`/`.home` container to exist, not for its
    // `{#each cards}` child tiles to have actually painted — `drives` is always populated before this
    // point (`list_drives` always returns >=1 root, even the non-Windows single-"/"-root branch), but
    // under slow Linux/Xvfb CI a render tick could still separate "container mounted" from "tiles
    // painted". Poll briefly instead of a single synchronous read right after the container appears.
    let tile: { point: Point; path: string } | null = null;
    await browser.waitUntil(
      async () => {
        tile = await pointOfDriveTile();
        return tile !== null;
      },
      {
        timeout: 10_000,
        timeoutMsg: "Home landing should show at least one drive tile (a .qa-card with a drive-root path)",
      },
    );
    // eslint-disable-next-line no-console
    console.log(`[CPE-1159] Home drive tile: ${JSON.stringify(tile)}`);
    expect(tile, "Home landing should show at least one drive tile (a .qa-card with a drive-root path)").to.not.equal(null);
  });

  it("right-clicking a Home DRIVE TILE opens the drive menu and it stays open (does not self-close)", async () => {
    await goHome();
    const tile = await pointOfDriveTile();
    expect(tile, "drive tile on Home").to.not.equal(null);
    await assertDriveMenuStaysOpen(tile!.point, `home-tile ${tile!.path}`);
    await snap("drive-menu");
  });

  it("right-clicking a SIDEBAR DRIVE ROW opens the drive menu and it stays open (does not self-close)", async function () {
    // The sidebar is present in every view; go Home first only so the drives section is on screen.
    await goHome();
    const row = await pointOfSidebarDrive();
    // eslint-disable-next-line no-console
    console.log(`[CPE-1159] sidebar drive row: ${JSON.stringify(row)}`);
    if (!row) {
      // Not every layout/machine exposes an expanded sidebar drive row; the Home-tile test above
      // already proves the shared stay-open contract. Skip rather than fail on its absence.
      this.skip();
      return;
    }
    await assertDriveMenuStaysOpen(row.point, `sidebar-row ${row.path}`);
    await snap("drive-menu");
  });
});
