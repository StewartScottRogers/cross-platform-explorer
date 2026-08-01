// CPE-1191 — render pin for saved macros surfaced in the right-click context menu (epic CPE-739).
// Drives the REAL built app: creates a macro via the library (`MacrosDialog`), binds it to the
// "context" surface (its Menu checkbox), then real-right-clicks a file row (CDP `rightClick`,
// non-grabbing — see gui-smoke/lib/mouse.ts's CPE-1155 header) and asserts a "Run macro ▸" submenu
// appears carrying the bound macro's name (`ContextMenu.svelte`'s new `macros` prop, CPE-1191).
//
// Non-destructive: this spec never actually clicks the macro row (which would open the
// dry-run-confirm flow and, on Run, touch disk) — it only asserts the submenu renders the binding,
// then dismisses the menu with Escape. The macro itself is deleted again at the end via the library,
// so repeated CI runs never accumulate stray catalog entries or bindings.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";
import { rightClick, hover, type Point } from "../lib/mouse.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");
// Duplicated literal rather than importing across the runner/worker boundary — matches this
// harness's existing convention (see open-dir.smoke.ts's FIXTURE_NAME comment). Seeded by every run.
const MARKER_NAME = "CPE-1045-marker.txt";
const MACRO_NAME = "CPE-1191 Menu Macro";

async function openViaPalette(query: string, labelSubstring: string): Promise<void> {
  await browser.keys(["Control", "Shift", "P"]);
  const paletteInput = await $(".cp-input");
  await paletteInput.waitForExist({ timeout: 10_000, timeoutMsg: "expected .cp-input to render" });
  await paletteInput.addValue(query);

  let row: WebdriverIO.Element | undefined;
  await browser.waitUntil(
    async () => {
      const rows = $$(".cp-row");
      for await (const r of rows) {
        if ((await r.getHTML({ includeSelectorTag: false })).includes(labelSubstring)) {
          row = r;
          return true;
        }
      }
      return false;
    },
    { timeout: 10_000, timeoutMsg: `expected a .cp-row labelled "${labelSubstring}"` },
  );
  await row!.waitForClickable({ timeout: 10_000 });
  await row!.click();
}

/** Viewport-space centre of the first `.row` under `.rows` whose HTML includes `name`, or `null`. */
async function pointOfRowNamed(name: string): Promise<Point | null> {
  const rows = $$(".rows .row");
  for await (const row of rows) {
    if ((await row.getHTML({ includeSelectorTag: false })).includes(name)) {
      const loc = await row.getLocation();
      const size = await row.getSize();
      return { x: Math.round(loc.x + Math.min(60, size.width / 2)), y: Math.round(loc.y + size.height / 2) };
    }
  }
  return null;
}

/** Viewport-space centre of the FIRST element matching `selector` whose HTML includes `text`. */
async function pointByText(selector: string, text: string): Promise<Point | null> {
  const els = $$(selector);
  for await (const el of els) {
    if ((await el.getHTML({ includeSelectorTag: false })).includes(text)) {
      const loc = await el.getLocation();
      const size = await el.getSize();
      return { x: Math.round(loc.x + size.width / 2), y: Math.round(loc.y + size.height / 2) };
    }
  }
  return null;
}

async function createAndBindMacro(): Promise<void> {
  await openViaPalette("macros", "Manage macros");
  await (await $('.dialog[aria-label="Macros"]')).waitForExist({ timeout: 10_000 });

  await (await $('[data-testid="new-macro-btn"]')).click();
  await (await $('[data-testid="add-step-btn"]')).click();
  await (await $("#macro-name")).addValue(MACRO_NAME);
  await (await $('[data-testid="step-field-0"]')).addValue("{stem}_v2.{ext}");
  const saveBtn = await $('[data-testid="save-macro-btn"]');
  await saveBtn.waitForEnabled({ timeout: 5_000 });
  await saveBtn.click();

  await (await $(`[data-testid="macro-${MACRO_NAME}"]`)).waitForExist({ timeout: 10_000 });
  await (await $(`[data-testid="bind-context-${MACRO_NAME}"]`)).click();

  await (await $('.dialog[aria-label="Macros"] .actions .btn.primary')).click(); // Close
}

async function deleteMacro(): Promise<void> {
  await openViaPalette("macros", "Manage macros");
  await (await $('.dialog[aria-label="Macros"]')).waitForExist({ timeout: 10_000 });
  const row = await $(`[data-testid="macro-${MACRO_NAME}"]`);
  if (await row.isExisting()) {
    await (await $(`[data-testid="delete-btn-${MACRO_NAME}"]`)).click();
  }
  await (await $('.dialog[aria-label="Macros"] .actions .btn.primary')).click(); // Close
}

describe("CPE-1191 — headless GUI smoke: a bound macro appears in the right-click context menu", () => {
  before(() => {
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
  });

  afterEach(async function () {
    await snapFailure(this.currentTest, "macro-in-menu");
  });

  it("creates + context-binds a macro, then finds it in the row's Run-macro ▸ submenu", async () => {
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });
    await crumb.click();

    await createAndBindMacro();

    const markerPoint = await pointOfRowNamed(MARKER_NAME);
    expect(markerPoint, `expected a row for the seeded ${MARKER_NAME}`).to.not.equal(null);

    await rightClick(markerPoint!);
    const ctx = await $(".ctx");
    await ctx.waitForExist({ timeout: 10_000, timeoutMsg: "expected the item context menu (.ctx) to open" });

    const runMacroPoint = await pointByText(".ctx .parent", "Run macro");
    expect(runMacroPoint, 'expected a "Run macro ▸" submenu row').to.not.equal(null);
    await hover(runMacroPoint!);

    const flyout = await $(".ctx .flyout");
    await flyout.waitForExist({ timeout: 5_000, timeoutMsg: "expected the Run-macro flyout to open on hover" });
    const flyoutHtml = await flyout.getHTML({ includeSelectorTag: false });
    expect(flyoutHtml, `expected "${MACRO_NAME}" in the Run-macro submenu`).to.include(MACRO_NAME);

    // CPE-1148 Part A: capture the open submenu before dismissing it below.
    await snap("macro-in-menu");

    // Non-destructive: dismiss without clicking the macro (that would start the run flow).
    await browser.keys("Escape");
    await (await $(".ctx")).waitForExist({ timeout: 5_000, reverse: true, timeoutMsg: "expected Escape to close the context menu" });

    await deleteMacro();
  });
});
