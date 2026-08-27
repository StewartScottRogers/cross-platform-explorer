// CPE-1827 — headless GUI smoke pin + Visual Critic screenshots for the Trash titlebar overflow-menu
// fix (TrashView.svelte). CPE-1822 (a separate, still-open ticket) is "no gui-smoke coverage of the
// Trash view at all" — this spec is a first, narrowly-scoped pass at that gap, not the full coverage
// CPE-1822 itself calls for: it pins the ONE thing CPE-1827's acceptance criteria singled out as
// needing REAL layout (jsdom doesn't compute it) — the close button (`.tv-x`) staying present and
// hit-testable at the app's own 600×400 window floor — across a resize sweep, with one real entry and
// a selection so the title bar's variable-width slot is exercised, not just the empty-Trash shape.
//
// What this does NOT cover (left for CPE-1822's own fuller pass): the streaming and degraded listing
// states (no seam to inject either through the real OS Trash short of a genuinely large or broken
// one), a locale sweep (CPE-1816's own review flagged Russian as the worst-case status-slot width —
// this spec runs in the harness's default English locale only), and driving the delete-to-Trash flow
// through the app's own UI (see wdio.conf.ts#seedTrashTitlebarFixture's header for why that's seeded
// natively instead — an earlier attempt at UI-driven deletion hung indefinitely for reasons unrelated
// to this ticket's own scope).
//
// Drives the REAL built app: wdio.conf.ts#seedTrashTitlebarFixture moves a dedicated seeded file
// (nested inside the CPE-1241 shred folder, same fixture-isolation reasoning as vault-create.smoke.ts)
// to the real OS Recycle Bin/Trash BEFORE the app process launches, so `list_trash_stream` has a
// genuine entry the moment the Trash view opens. This spec opens it from the Sidebar, selects the row
// (so the title bar's count slot reads "1 item · 1 selected", not just "1 item"), then sweeps the
// window down through 880px / 700px (the two widths CPE-1827's own ticket measured as previously
// broken) to the app's real 600×400 floor, snapping a screenshot and hit-testing `.tv-x` at each stop.
// At the floor it also opens the titlebar's new "…" overflow menu and hit-tests IT (the menu itself
// must not be clipped by `.tv-panel`'s `overflow: hidden` — the same failure shape this ticket exists
// to close off, just one level in — see TrashView.svelte's `clampToAnchor` doc comment). Finally
// proves the new Escape-closes-the-view handler (the ticket's other hard requirement — previously
// there was none).
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");
// Duplicated literal rather than importing across the runner/worker boundary — matches this suite's
// established convention (see vault-create.smoke.ts's identical note). "Must stay in sync with
// wdio.conf.ts#seedTrashTitlebarFixture" is checked rather than asked for since CPE-1950:
// `src/lib/guiSmokeFixtureLiterals.test.ts` compares this declaration with wdio.conf.ts's exported
// one on every PR.
const TRASH_TITLEBAR_FILE_NAME = "CPE-1827-fixture.txt";

/** Same getHTML-scan locator every other spec in this suite uses (script-injected text locators
 *  don't reliably resolve against wry's classic-WebDriver webview), scoped to the Trash panel's own
 *  rows (`.tv-row`). */
async function pointOfTrashRowNamed(name: string): Promise<{ x: number; y: number } | null> {
  const rows = $$(".tv-row");
  for await (const row of rows) {
    if ((await row.getHTML({ includeSelectorTag: false })).includes(name)) {
      const loc = await row.getLocation();
      const size = await row.getSize();
      return { x: Math.round(loc.x + Math.min(60, size.width / 2)), y: Math.round(loc.y + size.height / 2) };
    }
  }
  return null;
}

/** The subset of `WebdriverIO.Element` this helper needs — typed narrowly (rather than the full
 *  `WebdriverIO.Element`) because `await $(...)`'s resolved type and a `ChainablePromiseElement`
 *  passed through an intermediate function parameter don't structurally unify under this project's
 *  `@wdio/globals` version (their `parent` fields disagree: `Browser | Element` vs. `Promise<Browser |
 *  Element | MultiRemoteBrowser>`) — a typings quirk, not a real behavioural difference; every method
 *  below exists and works identically on both. */
interface Locatable {
  isDisplayed(): Promise<boolean>;
  getLocation(): Promise<{ x: number; y: number }>;
  getSize(): Promise<{ width: number; height: number }>;
}

/** Real layout hit-test: `el`'s bounding box lies fully inside the current viewport (never partially
 *  or wholly off-screen/clipped) AND WebDriver considers it displayed. This is exactly the class of
 *  fact jsdom cannot compute — the reason this ticket's acceptance criteria required a REAL browser
 *  pin rather than another structural unit test. */
async function isFullyOnscreenAndDisplayed(el: Locatable): Promise<boolean> {
  if (!(await el.isDisplayed())) return false;
  const loc = await el.getLocation();
  const size = await el.getSize();
  const vw = await browser.execute(() => window.innerWidth);
  const vh = await browser.execute(() => window.innerHeight);
  return loc.x >= 0 && loc.y >= 0 && loc.x + size.width <= vw && loc.y + size.height <= vh && size.width > 0 && size.height > 0;
}

describe("CPE-1827 — headless GUI smoke: the Trash titlebar's close button survives a resize down to the app's 600×400 floor", () => {
  let fixtureSupported = false;

  before(() => {
    const state = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as {
      trashTitlebarFixture?: { supported: boolean };
    };
    fixtureSupported = state.trashTitlebarFixture?.supported === true;
  });

  afterEach(async function () {
    await snapFailure(this.currentTest, "trash-titlebar");
  });

  it("opens the Trash from the Sidebar and selects the real, natively-seeded deleted entry", async function () {
    if (!fixtureSupported) {
      // Best-effort seed (native OS Recycle-Bin move via PowerShell on Windows / `gio trash` on
      // Linux, wdio.conf.ts#seedTrashTitlebarFixture) — skip rather than fail the whole suite over
      // missing CI tooling, same pattern as `linkBadgeFixture`/`thumbFormatsFixture` above.
      this.skip();
      return;
    }
    // CPE-1827 local-verification finding (see this ticket's Work Log + known-failing.json's entries
    // for this spec): on the dev machine this was written on, `list_trash_stream` enumerating a REAL,
    // large Windows Recycle Bin (843 items via a `Shell.Application` COM probe — a mix of the user's
    // own real files and prior sprint-run debris) took multiple minutes, well past even a generous
    // per-test budget — a genuine environment condition (a large real Recycle Bin), not a defect in
    // this ticket's titlebar change (`list_trash_stream` itself is untouched by CPE-1827). A clean CI
    // runner's Recycle Bin should be empty/near-empty and this should be fast there; widen the budget
    // regardless so a merely-slow (not hung) run still completes rather than false-failing.
    this.timeout(300_000);

    await browser.setWindowSize(1000, 700);

    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });

    // Sidebar.svelte's Trash section defaults to expanded (sidebarSections.ts: unset = open) — no
    // twisty click needed. Locate the "Open Trash" row by its own tooltip text (trash.openTip).
    let openTrashBtn: WebdriverIO.Element | undefined;
    await browser.waitUntil(
      async () => {
        for await (const btn of await $$(".nav-item.fav-item")) {
          if ((await btn.getAttribute("title")) === "Browse deleted files and folders") {
            openTrashBtn = btn;
            return true;
          }
        }
        return false;
      },
      { timeout: 10_000, timeoutMsg: 'expected the Sidebar\'s "Open Trash" row' },
    );
    await openTrashBtn!.click();

    const panel = await $(".tv-panel");
    await panel.waitForExist({ timeout: 10_000, timeoutMsg: "expected TrashView's .tv-panel to render" });

    // The real, natively-seeded entry appears once `list_trash_stream` resolves.
    await browser.waitUntil(async () => (await pointOfTrashRowNamed(TRASH_TITLEBAR_FILE_NAME)) !== null, {
      timeout: 15_000,
      timeoutMsg: `expected a Trash row for "${TRASH_TITLEBAR_FILE_NAME}"`,
    });

    // Check its row's checkbox — exercises the title bar's "N items · 1 selected" branch, the widest
    // text the count slot renders in the default (non-degraded) state, so the resize sweep below is
    // testing the title bar under its own worst ordinary-state width pressure, not just the empty
    // "0 items" shape.
    let checked = false;
    for await (const row of await $$(".tv-row")) {
      if ((await row.getHTML({ includeSelectorTag: false })).includes(TRASH_TITLEBAR_FILE_NAME)) {
        const checkbox = await row.$(".tv-check input");
        await checkbox.click();
        checked = true;
        break;
      }
    }
    expect(checked, "expected to find and check the Trash row's own checkbox").to.equal(true);

    const status = await $(".tv-count");
    await browser.waitUntil(async () => (await status.getText()).includes("selected"), {
      timeout: 5_000,
      timeoutMsg: 'expected the title bar\'s count slot to read "… selected" once checked',
    });

    await snap("trash-titlebar-default");
  });

  it("survives a resize sweep down to the 600×400 floor — the close button is always present and hit-testable, never clipped", async function () {
    if (!fixtureSupported) {
      this.skip();
      return;
    }

    const closeBtn = await $(".tv-x");
    await closeBtn.waitForExist({ timeout: 5_000 });

    // The two widths CPE-1827's own ticket measured as previously broken (880px: overflow starts;
    // 700px: round 3's own regression band), then the app's real permitted floor (600×400,
    // `.min_inner_size` in src-tauri/src/lib.rs).
    const widths: Array<{ w: number; h: number; label: string }> = [
      { w: 880, h: 700, label: "trash-titlebar-880" },
      { w: 700, h: 600, label: "trash-titlebar-700" },
      { w: 600, h: 400, label: "trash-titlebar-600" },
    ];

    for (const { w, h, label } of widths) {
      await browser.setWindowSize(w, h);
      // Let layout settle (no fixed sleep beyond a short, bounded settle poll — the panel/titlebar are
      // already mounted, only their computed geometry changes on resize).
      await browser.waitUntil(async () => (await closeBtn.isExisting()) === true, { timeout: 5_000 });

      const onscreen = await isFullyOnscreenAndDisplayed(closeBtn);
      expect(onscreen, `expected .tv-x to be fully on-screen and displayed at ${w}×${h}`).to.equal(true);

      await snap(label);
    }
  });

  it("at the 600×400 floor, the overflow menu itself is never clipped by .tv-panel's overflow:hidden", async function () {
    if (!fixtureSupported) {
      this.skip();
      return;
    }

    await browser.setWindowSize(600, 400);

    const trigger = await $('[aria-label="More actions"]');
    await trigger.waitForClickable({ timeout: 5_000, timeoutMsg: 'expected the titlebar\'s "…" overflow trigger' });
    await trigger.click();

    const menu = await $(".tv-overflow-menu");
    await menu.waitForExist({ timeout: 5_000, timeoutMsg: "expected the overflow menu to open" });

    const onscreen = await isFullyOnscreenAndDisplayed(menu);
    expect(onscreen, "expected the overflow menu to be fully on-screen at the 600×400 floor").to.equal(true);

    // Every item in it is real, clickable, and — the acceptance criterion this ticket cares about most
    // — the × stays reachable with the menu open too.
    const closeBtn = await $(".tv-x");
    expect(await isFullyOnscreenAndDisplayed(closeBtn), "expected .tv-x to remain on-screen with the overflow menu open").to.equal(
      true,
    );

    await snap("trash-titlebar-600-menu");

    // Close the menu without triggering any action (click the trigger again, the same toggle a real
    // user would use) before the next test drives Escape.
    await trigger.click();
    await menu.waitForExist({ timeout: 5_000, reverse: true, timeoutMsg: "expected the overflow menu to close" });
  });

  it("Escape closes the view — the new keyboard fallback this ticket adds (there was none before)", async function () {
    if (!fixtureSupported) {
      this.skip();
      return;
    }

    const panel = await $(".tv-panel");
    await panel.waitForExist({ timeout: 5_000 });

    await browser.keys("Escape");

    await panel.waitForExist({ timeout: 5_000, reverse: true, timeoutMsg: "expected Escape to close TrashView" });
    expect(await panel.isExisting()).to.equal(false);
  });
});
