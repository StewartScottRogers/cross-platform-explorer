// CPE-1190 (UI half) — render pin for `MacroParamPrompt.svelte` (epic CPE-739). Drives the REAL built
// app: creates a macro whose rename step carries an `{ask:suffix}` prompt-parameter token, binds it to
// the context-menu surface, real-right-clicks a file row and runs it from the "Run macro ▸" submenu
// (CPE-1191's `runAction`/`startMacro` in App.svelte), and asserts the param-prompt dialog renders a
// labelled field for "suffix" BEFORE the dry-run confirm ever appears — proving
// `macroParams.extractAskLabels` found the token and App gated the run on it.
//
// Non-destructive: this spec clicks Cancel on the param prompt rather than Continue, so the run flow
// never reaches `MacroRunConfirm`/`macro_plan`/`macro_run` — nothing is planned or applied to disk.
// The macro is deleted again via the library at the end so repeated CI runs don't accumulate entries.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";
import { rightClick, hover, click, type Point } from "../lib/mouse.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");
const MARKER_NAME = "CPE-1045-marker.txt"; // seeded every run — see open-dir.smoke.ts's note
const MACRO_NAME = "CPE-1190 Ask Macro";

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

// CPE-1481: scroll-then-`getBoundingClientRect` via `element.execute`, matching pointOfRowNamed's
// sibling in macro-in-menu.smoke.ts and the rest of the suite (archive-browse.smoke.ts's `pointOfRow`,
// drive-menu.smoke.ts, home-item-menu.smoke.ts) — this previously used `getLocation()`/`getSize()`
// (WebDriver's own "get element rect"), a different code path than the viewport-space
// `getBoundingClientRect()` `rightClick`'s CDP/W3C-Actions coordinates are documented against, and
// never scrolled an out-of-view row into the viewport first.
async function pointOfRowNamed(name: string): Promise<Point | null> {
  const rows = $$(".rows .row");
  for await (const row of rows) {
    if ((await row.getHTML({ includeSelectorTag: false })).includes(name)) {
      await row.scrollIntoView({ block: "center" });
      await browser.pause(150);
      return row.execute((el) => {
        const r = (el as HTMLElement).getBoundingClientRect();
        return { x: Math.round(r.left + Math.min(60, r.width / 2)), y: Math.round(r.top + r.height / 2) };
      }) as Promise<Point>;
    }
  }
  return null;
}

async function pointByText(selector: string, text: string): Promise<Point | null> {
  const els = $$(selector);
  for await (const el of els) {
    if ((await el.getHTML({ includeSelectorTag: false })).includes(text)) {
      await el.scrollIntoView({ block: "center" });
      await browser.pause(150);
      return el.execute((node) => {
        const r = (node as HTMLElement).getBoundingClientRect();
        return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) };
      }) as Promise<Point>;
    }
  }
  return null;
}

async function createAndBindAskMacro(): Promise<void> {
  await openViaPalette("macros", "Manage macros");
  await (await $('.dialog[aria-label="Macros"]')).waitForExist({ timeout: 10_000 });

  await (await $('[data-testid="new-macro-btn"]')).click();
  await (await $('[data-testid="add-step-btn"]')).click(); // default kind: rename
  await (await $("#macro-name")).addValue(MACRO_NAME);
  // {ask:suffix} — the CPE-1190 prompt-parameter token this spec's whole point is exercising.
  await (await $('[data-testid="step-field-0"]')).addValue("{stem}_{ask:suffix}.{ext}");
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

describe("CPE-1190 (UI half) — headless GUI smoke: running an {ask:label} macro prompts for params first", () => {
  before(() => {
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
  });

  afterEach(async function () {
    await snapFailure(this.currentTest, "macro-param-prompt");
  });

  it("running a bound {ask:suffix} macro opens MacroParamPrompt before any dry-run confirm", async () => {
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });
    await crumb.click();

    await createAndBindAskMacro();

    const markerPoint = await pointOfRowNamed(MARKER_NAME);
    expect(markerPoint, `expected a row for the seeded ${MARKER_NAME}`).to.not.equal(null);

    await rightClick(markerPoint!);
    await (await $(".ctx")).waitForExist({ timeout: 10_000, timeoutMsg: "expected the item context menu to open" });

    const runMacroPoint = await pointByText(".ctx .parent", "Run macro");
    expect(runMacroPoint, 'expected a "Run macro ▸" submenu row').to.not.equal(null);
    await hover(runMacroPoint!);
    await (await $(".ctx .flyout")).waitForExist({ timeout: 5_000, timeoutMsg: "expected the Run-macro flyout to open" });

    const macroItemPoint = await pointByText(".ctx .flyout .row", MACRO_NAME);
    expect(macroItemPoint, `expected "${MACRO_NAME}" in the Run-macro submenu`).to.not.equal(null);
    await click(macroItemPoint!); // dispatches action `macro:<name>` -> App.startMacro

    // Gated on the {ask:suffix} token: the param prompt must appear, NOT the dry-run confirm.
    const promptDialog = await $('.dialog[aria-label^="Macro parameters"]');
    await promptDialog.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected MacroParamPrompt to render before any dry-run confirm, since the macro has {ask:suffix}",
    });
    expect(await (await $('[data-testid="run-btn"]')).isExisting(), "the dry-run confirm must NOT be open yet").to.equal(false);

    const suffixField = await $('[data-testid="param-field-suffix"]');
    await suffixField.waitForExist({ timeout: 5_000, timeoutMsg: 'expected a param field for "suffix"' });

    // CPE-1148 Part A: capture the param prompt before dismissing it below.
    await snap("macro-param-prompt");

    // Non-destructive: Cancel — the run flow (macro_plan/macro_run) is never reached from here.
    await (await $('[data-testid="cancel-btn"]')).click();
    await promptDialog.waitForExist({ timeout: 5_000, reverse: true, timeoutMsg: "expected Cancel to close the param prompt" });

    await deleteMacro();
  });
});
