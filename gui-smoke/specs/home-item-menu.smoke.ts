// CPE-1162 — right-clicking a Home Recent/Favorites/Folders ROW opens a file/folder-like context
// menu that STAYS open (does not self-close).
//
// WHY THIS SPEC (and not just the jsdom mount tests): the jsdom tests (HomeView.test.ts /
// HomeItemContextMenu.test.ts) assert the row DISPATCHES `homeItemContext` and the ContextMenu
// `home-item` branch RENDERS the right actions — but, exactly like CPE-1154/1157/1158/1159, a
// synthetic `contextMenu` fired straight at the handler never exercises the real window-level
// dismisser, so an open-then-close race (a missing `stopPropagation`) would slip through. This spec
// drives a FAITHFUL, non-grabbing CDP right-click (gui-smoke/lib/mouse.ts, CPE-1155) through the real
// browser input pipeline over a real Home row, then asserts the menu is present AND STILL present a
// tick later — the stay-open contract. It also asserts the opened menu is the HOME-ITEM variant
// (its list-management "Remove from …" row + cross-view "Add to Favorites"), not the empty-area menu
// (which Home must never show) nor the on-item quick-action menu.
//
// The row it targets: the harness launches with `--open=<tmpDir>`, and navigating there records that
// folder into the Recent-Folders MRU — so the Home "Folders" tab reliably has at least one row to
// right-click without seeding a separate profile.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";
import { rightClick, click, type Point } from "../lib/mouse.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

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

/** Navigate to the Home landing via the always-present "Home" breadcrumb. */
async function goHome(): Promise<void> {
  const p = await pointOfCrumb("Home");
  if (p) await click(p);
  await browser.waitUntil(async () => (await $(".home").isExisting()), {
    timeout: 15_000,
    timeoutMsg: "Home landing (.home) did not render after clicking the Home breadcrumb",
  });
}

/** Click the lower-section pill whose text matches `label` (Recent / Favorites / Folders). */
async function clickPill(label: RegExp): Promise<void> {
  const p = (await browser.execute((src) => {
    const re = new RegExp(src);
    const pills = Array.from(document.querySelectorAll("button.pill"));
    const pill = pills.find((el) => re.test((el.textContent || "").trim()));
    if (!pill) return null;
    const r = pill.getBoundingClientRect();
    return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) };
  }, label.source)) as Point | null;
  if (p) await click(p);
  await browser.pause(150);
}

/** Viewport-space centre of the FIRST Home list `.recent-row` (a Recent/Favorites/Folders entry), or
 *  `null` if the current tab shows none. Aims at the row's name cell so the hit lands on the row
 *  itself, not its trailing ✕/★ affordance. */
async function pointOfFirstRow(): Promise<Point | null> {
  return browser.execute(() => {
    const row = document.querySelector(".recent-row");
    if (!row) return null;
    const target = row.querySelector(".rname") ?? row;
    const r = target.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) return null;
    return { x: Math.round(r.left + Math.min(40, r.width / 2)), y: Math.round(r.top + r.height / 2) };
  }) as Promise<Point | null>;
}

/** Arm a MutationObserver logging `.ctx` presence transitions, to PROVE the open-then-close signature
 *  (present:true → present:false in the same tick) rather than only sampling the end state. Mirrors
 *  drive-menu.smoke.ts's probe. */
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

describe("CPE-1162 — Home Recent/Favorites/Folders row right-click menu opens and STAYS open", () => {
  before(() => {
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")); // confirm the harness seeded state
  });

  afterEach(async function () {
    await snapFailure(this.currentTest, "home-item-menu");
    await dismissMenu();
  });

  it("the Home Folders tab shows at least one recorded folder row (from --open=<tmpDir>)", async () => {
    await goHome();
    await clickPill(/Folders/);
    const row = await pointOfFirstRow();
    // eslint-disable-next-line no-console
    console.log(`[CPE-1162] first Folders row: ${JSON.stringify(row)}`);
    expect(row, "Home Folders tab should list the folder opened via --open=<tmpDir>").to.not.equal(null);
  });

  it("right-clicking a Home Folders row opens the home-item menu and it stays open", async () => {
    await goHome();
    await clickPill(/Folders/);
    const row = await pointOfFirstRow();
    expect(row, "a Folders row to right-click").to.not.equal(null);

    await dismissMenu();
    await armCtxProbe();
    await rightClick(row!);
    // Give the buggy build's window-level dismisser every chance to fire, then read the END state.
    await browser.pause(500);

    const probe = await readCtxProbe();
    const ctxExists = await $(".ctx").isExisting();
    const html = ctxExists ? await $(".ctx").getHTML({ includeSelectorTag: false }) : "";
    const hasQuickrow = ctxExists ? await $(".ctx .quickrow").isExisting() : false;
    // eslint-disable-next-line no-console
    console.log(`[CPE-1162] home-item probe:`, JSON.stringify({ ctxExists, hasQuickrow, ctxLog: probe }));

    // Stay-open contract: still present a tick after the right-click (fails on an open-then-close race).
    expect(ctxExists, "home-item menu (.ctx) must STILL be present a tick after right-click").to.equal(true);
    // It's the HOME-ITEM variant: carries the list-management "Remove from Recent folders" +
    // cross-view "Add to Favorites"…
    expect(html, "opened menu is the home-item variant (Remove from Recent folders)").to.include("Remove from Recent folders");
    expect(html, "opened menu is the home-item variant (Add to Favorites)").to.include("Add to Favorites");
    // …and NOT the on-item quick-action row, nor the empty-area (Paste/Ctrl+V) menu Home must never show.
    expect(hasQuickrow, "home-item menu must not show the on-item quick-action row").to.equal(false);
    expect(html.includes("Ctrl+V"), "home-item menu is not the empty-area (Paste) variant").to.equal(false);

    await snap("home-item-menu");
  });
});
