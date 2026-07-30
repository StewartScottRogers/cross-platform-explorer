// CPE-1144 — QA-Architect follow-up pin for the Batch-Media dialog (CPE-1093/1105, epic CPE-723):
// drives the real built app, selects two seeded images, opens the dialog via its real opener — the
// right-click context menu's "Batch media…" item (there is NO command-palette entry for it: grep
// App.svelte's `paletteCommands` — no `batch-media` id, unlike `tool.organize`/`tool.findDuplicates`)
// — adds a Resize op, and asserts both the op-pill list and the debounced plan preview actually
// render for the seeded fixture.
//
// Non-destructive: this spec never clicks Apply. `batch_media::plan` (crates/server/src/batch_media.rs),
// the command backing the preview, is pure path-string manipulation — it never opens/decodes the file
// (see that module's `plan` doc comment) — and this spec relies on that rather than re-verifying it.
// The dialog is dismissed via Cancel, so nothing is ever written outside the throwaway tmpDir.
//
// Two DOM gestures this harness has no existing precedent for — ctrl+click (add a second row to an
// existing selection) and right-click (open the context menu) — are driven via `browser.execute`
// dispatching a real `MouseEvent` at the target row's own DOM node, rather than WebDriver's native
// pointer/actions API. Svelte's `on:click`/`on:contextmenu` handlers (FileList.svelte's `rowClick`/
// `rowContext`) just read event properties (`e.ctrlKey`, the event type) off whatever `MouseEvent`
// they receive, so a synthetic-but-real `MouseEvent` exercises the exact same code path a native
// gesture would — and sidesteps the wry/msedgedriver quirks already documented elsewhere in this
// harness (wdio.conf.ts's BiDi-context and `--open` arg-splitting comments) for anything not yet
// proven to work over classic WebDriver against an embedded WebView2 control. Plain, unmodified
// clicks (the `+ Add`/Cancel buttons, the first row select) still use WebdriverIO's normal `.click()`,
// exactly like every other spec in this suite.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

// Duplicated literals rather than importing from wdio.conf.ts — matches this harness's existing
// convention (see organize.smoke.ts's identical comment). Seeded by
// wdio.conf.ts#seedBatchMediaFixture alongside the pre-existing MARKER_NAME/FIXTURE_NAME/organize files.
const BATCH_MEDIA_PNG_A_NAME = "CPE-1144-photo-a.png";
const BATCH_MEDIA_PNG_B_NAME = "CPE-1144-photo-b.png";

/** Dispatch a real (but script-constructed) `MouseEvent` at an element's own DOM node — see the file
 *  header for why this is used instead of WebDriver's native click/actions for these two gestures. */
async function dispatchMouseEvent(
  el: WebdriverIO.Element,
  type: "click" | "contextmenu",
  opts: { ctrlKey?: boolean } = {},
): Promise<void> {
  await browser.execute(
    (node, evtType, ctrlKey) => {
      node.dispatchEvent(
        new MouseEvent(evtType, {
          bubbles: true,
          cancelable: true,
          button: evtType === "contextmenu" ? 2 : 0,
          ctrlKey,
        }),
      );
    },
    el,
    type,
    !!opts.ctrlKey,
  );
}

/** Find the `.row` (FileList.svelte) whose rendered HTML contains this filename — same reasoning as
 *  organize.smoke.ts's `.cp-row` scan: exact-text locators don't reliably resolve against wry's
 *  webview under the classic WebDriver protocol this harness forces. */
async function findRow(name: string): Promise<WebdriverIO.Element> {
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
    { timeout: 15_000, timeoutMsg: `expected a .row for the seeded ${name}` },
  );
  return found!;
}

describe("CPE-1144 — headless GUI smoke: Batch-Media dialog renders an op + plan preview", () => {
  before(() => {
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
  });

  it("selects two seeded images, opens via the context menu, adds an op, and renders the preview", async () => {
    // Wait for the initial `--open=<tmpDir>` navigation (also asserted in open-dir.smoke.ts) so the
    // file list is settled before we start clicking rows.
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });
    await crumb.click(); // plain, non-interactive <span> — just moves focus off any input

    const rowA = await findRow(BATCH_MEDIA_PNG_A_NAME);
    const rowB = await findRow(BATCH_MEDIA_PNG_B_NAME);

    // Select row A with a plain click (real WebdriverIO click, same as every other spec in this
    // suite), then add row B with a ctrl+click (synthetic MouseEvent — see header comment) — the
    // two-file multi-selection `beginBatchMedia` (App.svelte) requires.
    await rowA.click();
    await dispatchMouseEvent(rowB, "click", { ctrlKey: true });

    await browser.waitUntil(async () => (await $$(".row.selected").length) === 2, {
      timeout: 10_000,
      timeoutMsg: "expected exactly the two seeded rows to be selected",
    });

    // Right-click row B (already inside the selection) to open the context menu WITHOUT collapsing
    // the multi-selection — App.svelte's `onRowContext` only resets to a single selection when the
    // right-clicked row was NOT already selected.
    await dispatchMouseEvent(rowB, "contextmenu");

    const ctxMenu = await $(".ctx");
    await ctxMenu.waitForExist({ timeout: 10_000, timeoutMsg: "expected the context menu (.ctx) to render" });

    let batchMediaItem: WebdriverIO.Element | undefined;
    await browser.waitUntil(
      async () => {
        const items = $$(".ctx .row");
        for await (const item of items) {
          const html = await item.getHTML({ includeSelectorTag: false });
          if (html.includes("Batch media")) {
            batchMediaItem = item;
            return true;
          }
        }
        return false;
      },
      { timeout: 10_000, timeoutMsg: 'expected a context menu row labelled "Batch media…"' },
    );
    expect(batchMediaItem, 'expected a context menu row labelled "Batch media…"').to.not.equal(undefined);
    await batchMediaItem!.waitForClickable({ timeout: 10_000 });
    await batchMediaItem!.click();

    // The dialog mounted, scoped to exactly the two eligible seeded images (both are real PNGs, so
    // neither is filtered out by `partitionEligible`'s `canBatchTransform` check).
    const dialog = await $('[aria-label="Batch media"]');
    await dialog.waitForExist({ timeout: 10_000, timeoutMsg: "expected the Batch-Media dialog to render" });
    const dialogHtml = await dialog.getHTML({ includeSelectorTag: false });
    expect(dialogHtml, "expected the dialog header to report the 2-file selection").to.include("2 files");

    // Add an operation via its real control — the op-kind <select> already defaults to Resize with a
    // valid 1024px value (BatchMediaDialog.svelte's initial state), so "+ Add" is enabled without any
    // further field edits, exactly as a user who accepts the default would experience it.
    const addOpBtn = await $('[data-testid="add-op-btn"]');
    await addOpBtn.waitForClickable({ timeout: 10_000 });
    await addOpBtn.click();

    // Core assertion #1 (CPE-1144): the op-pill list renders the op that was just added.
    await browser.waitUntil(async () => (await $$('[data-testid="op-pill"]').length) > 0, {
      timeout: 10_000,
      timeoutMsg: "expected at least one [data-testid='op-pill'] to render after Add",
    });
    const pill = await $('[data-testid="op-pill"]');
    const pillHtml = await pill.getHTML({ includeSelectorTag: false });
    expect(pillHtml, 'expected the pill to read "Resize 1024px"').to.include("Resize 1024px");

    // Core assertion #2 (CPE-1144): the debounced plan preview (batch_media::plan, a real IPC call —
    // see the file header) renders non-empty rows for BOTH seeded images, named exactly as the backend
    // computes them (a "-1024" suffix from the Resize op, per `plan`'s suffix-building logic) — the
    // FALSIFIABLE check tied to this spec's own fixture, not just "some preview text appeared".
    await browser.waitUntil(async () => (await $$('[data-testid="preview-row"]').length) > 0, {
      timeout: 10_000,
      timeoutMsg: "expected at least one [data-testid='preview-row'] to render",
    });
    const previewRows = await $$('[data-testid="preview-row"]');
    expect(previewRows.length, "expected exactly one preview row per seeded image").to.equal(2);

    const previewEl = await $('[data-testid="plan-preview"]');
    const previewHtml = await previewEl.getHTML({ includeSelectorTag: false });
    expect(previewHtml).to.include("CPE-1144-photo-a-1024.png");
    expect(previewHtml).to.include("CPE-1144-photo-b-1024.png");

    // CPE-1148 Part A: capture the op-pill list + plan preview (after the assertions above, before
    // it's dismissed).
    await snap("batch-media-dialog");

    // Non-destructive: never click Apply — dismiss via Cancel instead (each spec file gets its own
    // fresh app launch/session in this harness, so this isn't required for isolation from other
    // specs, just tidy, and it proves nothing in the dialog was left mid-flight).
    const cancelBtn = await $('[data-testid="cancel-btn"]');
    await cancelBtn.click();
  });
});
