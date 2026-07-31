// CPE-1167 — QA-Architect render pin for the ColumnPickerDialog (CPE-1146, epic CPE-707), retiring
// its manual-verify debt. Drives the REAL built app, opens the picker via the Command Palette (the
// same keyboard-first opener a real user has — `tool.columns` in App.svelte's `paletteCommands`, also
// reachable from the details-view header's "manage columns" control), and asserts the dialog's
// available-columns list renders.
//
// It is ALSO the end-to-end confirmation for CPE-1166: those four magic-byte detector columns (True
// Type / Type Mismatch / Text Encoding / Line Endings) are added to the catalog by
// `MetaColumn::all()` (crates/server/src/column_extract.rs) and surfaced to the picker via
// `metadata_columns_available()`; this spec asserts all four appear in the picker's available list on
// the real built app — so if the backend ever stopped enumerating them, or the frontend stopped
// rendering them, this pin fails loudly.
//
// Non-destructive: the spec only opens the picker and reads its available list — it never adds/removes
// a column, so no per-folder column settings are mutated. The seeded tmpDir (opened via `--open`,
// asserted in open-dir.smoke.ts) already carries several files, so `inFolder` is true and the
// palette's folder-scoped `tool.columns` command is enabled — no new fixture is needed.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

// The four CPE-1166 magic-byte detector columns, keyed by the stable id `MetaColumn::id()` emits
// (so the add-button testid `add-<id>` is exact) paired with the display label `MetaColumn::label()`
// registers — both read straight from crates/server/src/column_extract.rs so this stays honest with
// the backend catalog.
const DETECTOR_COLUMNS: Array<{ id: string; label: string }> = [
  { id: "detect.true_type", label: "True Type" },
  { id: "detect.type_mismatch", label: "Type Mismatch" },
  { id: "detect.text_encoding", label: "Text Encoding" },
  { id: "detect.line_endings", label: "Line Endings" },
];

describe("CPE-1167 — headless GUI smoke: the column picker renders its available list (incl. CPE-1166 detectors)", () => {
  before(() => {
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
  });

  // CPE-1149: on a failing run, leave a shot of the state it failed in (`column-picker-fail.png`) —
  // the inline `snap("column-picker")` below is only reached on a pass. Non-arrow fn so Mocha binds
  // `this`; `snapFailure` is a no-op on a pass and swallows its own errors.
  afterEach(async function () {
    await snapFailure(this.currentTest, "column-picker");
  });

  it("opens via the command palette and lists the available columns, including the CPE-1166 detectors", async () => {
    // Wait for the initial `--open=<tmpDir>` navigation (also asserted in open-dir.smoke.ts) so
    // `currentPath` is settled to the seeded tmpDir before reaching for a folder-scoped command —
    // `tool.columns` is `enabled: inFolder`, which is false on the Home screen.
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });
    await crumb.click(); // plain, non-interactive <span> — just moves focus off any input

    // Ctrl+Shift+P opens the Command Palette (App.svelte's `handleKeydown`) — the same reachability
    // path organize.smoke.ts uses.
    await browser.keys(["Control", "Shift", "P"]);

    const paletteInput = await $(".cp-input");
    await paletteInput.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected .cp-input (Command Palette) to render after Ctrl+Shift+P",
    });

    // The input is `autofocus`ed on mount, so this types straight into it. "columns" matches the
    // "Manage columns…" command (label + its `columns metadata … picker` keywords).
    await paletteInput.addValue("columns");

    // Locate the "Manage columns…" row by scanning rendered HTML for its label rather than a
    // `$('=text')` exact-text locator — the latter relies on script-injected text matching that
    // doesn't reliably resolve against wry's webview under the classic WebDriver protocol this harness
    // forces (see organize.smoke.ts's identical note).
    let columnsRow: WebdriverIO.Element | undefined;
    await browser.waitUntil(
      async () => {
        const rows = $$(".cp-row");
        for await (const row of rows) {
          const html = await row.getHTML({ includeSelectorTag: false });
          if (html.includes("Manage columns")) {
            columnsRow = row;
            return true;
          }
        }
        return false;
      },
      { timeout: 10_000, timeoutMsg: 'expected a .cp-row labelled "Manage columns…" to appear' },
    );
    expect(columnsRow, 'expected a .cp-row labelled "Manage columns…"').to.not.equal(undefined);
    await columnsRow!.waitForClickable({ timeout: 10_000 });
    await columnsRow!.click();

    // The dialog mounted (ColumnPickerDialog.svelte's `.dialog[role="dialog"]`).
    const dialog = await $('.dialog[role="dialog"]');
    await dialog.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected the column picker dialog to render after choosing the palette command",
    });

    // The available-columns list renders and is populated. The catalog is fetched once on
    // ExplorerPane mount (`ensureMetaColumnCatalog`, an in-memory `metadata_columns_available` call),
    // so by now `$metaColumnCatalog` has resolved — but wait for a non-empty list rather than assuming
    // timing: an EMPTY list would mean the catalog fetch failed (a broken invoke / bad build), which
    // this must catch loudly rather than passing on an empty picker.
    const availableList = await $('[data-testid="available-list"]');
    await availableList.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected the picker's [data-testid='available-list'] to render",
    });
    await browser.waitUntil(async () => (await $$('[data-testid="available-list"] .row').length) > 0, {
      timeout: 10_000,
      timeoutMsg: "expected the available-columns list to render at least one pickable column row",
    });

    // CPE-1166 end-to-end confirmation: all four magic-byte detector columns surface in the picker's
    // available list on the REAL built app — asserted both by their exact add-button testid
    // (`add-<id>`, proving each is a real pickable row wired to `MetaColumn::id()`) AND by their
    // human-facing label appearing in the list HTML (what the Visual Critic sees). If the backend ever
    // stopped enumerating a detector, or the frontend stopped rendering it, this fails loudly.
    const availableHtml = await availableList.getHTML({ includeSelectorTag: false });
    for (const { id, label } of DETECTOR_COLUMNS) {
      const addBtn = await $(`[data-testid="add-${id}"]`);
      expect(
        await addBtn.isExisting(),
        `expected the picker to offer the CPE-1166 "${label}" column (add-button testid add-${id})`,
      ).to.equal(true);
      expect(
        availableHtml,
        `expected the "${label}" label to render in the available-columns list`,
      ).to.include(label);
    }

    // CPE-1148 Part A: capture the picker (after the assertions above, before it's dismissed below).
    // On a FAILING run this line is never reached and the dialog is still open when the `afterEach`
    // hook fires, so it captures `column-picker-fail.png` of the failure state instead (CPE-1149).
    await snap("column-picker");

    // Non-destructive tidy end state: close the dialog via its Close button (dispatches `close`; no
    // column was added/removed, so nothing was persisted).
    const doneBtn = await $('[data-testid="done-btn"]');
    await doneBtn.click();
  });
});
