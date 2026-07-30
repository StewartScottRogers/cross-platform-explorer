// CPE-1143 — QA-Architect follow-up pin for the auto-organize dialog (CPE-1142, epic CPE-979):
// drives the real built app, opens `OrganizeDialog.svelte` via the Command Palette (the same
// keyboard-first opener a real user has — Tools → "Organize this folder…" is wired to the identical
// `tool.organize` command, see App.svelte's `paletteCommands`), selects a rule, and asserts the
// grouped proposal preview actually renders rows for the seeded tmpDir's mixed-kind files.
//
// Non-destructive: this spec never clicks Apply, so nothing is ever moved on disk — `organize_plan`
// (the command backing the preview) is read-only by construction (see OrganizeDialog.svelte's header
// comment), and this spec relies on that rather than re-verifying it.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

// Duplicated literals rather than importing from wdio.conf.ts — matches this harness's existing
// convention (see open-dir.smoke.ts's FIXTURE_NAME comment) of not reaching across the
// runner/worker process boundary. Seeded by wdio.conf.ts#seedOrganizeFixture alongside the
// pre-existing MARKER_NAME (.txt)/FIXTURE_NAME (.rs) files.
const ORGANIZE_PNG_NAME = "CPE-1143-photo.png";
const ORGANIZE_ZIP_NAME = "CPE-1143-archive.zip";

describe("CPE-1143 — headless GUI smoke: auto-organize dialog renders a grouped preview", () => {
  before(() => {
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
  });

  it("opens via the command palette, picks a rule, and renders grouped proposal rows", async () => {
    // Wait for the initial `--open=<tmpDir>` navigation (also asserted in open-dir.smoke.ts) so
    // `currentPath` is settled to the seeded tmpDir before we reach for a folder-scoped command —
    // `tool.organize` is `enabled: inFolder`, which is false on the Home screen.
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });
    await crumb.click(); // plain, non-interactive <span> — just moves focus off any input

    // Ctrl+Shift+P opens the Command Palette (App.svelte's `handleKeydown`) — the same reachability
    // path the ticket calls out ("via the command palette").
    await browser.keys(["Control", "Shift", "P"]);

    const paletteInput = await $(".cp-input");
    await paletteInput.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected .cp-input (Command Palette) to render after Ctrl+Shift+P",
    });

    // The input is `autofocus`ed on mount, so this types straight into it.
    await paletteInput.addValue("organize");

    // Locate the "Organize this folder…" row by scanning rendered HTML for its label rather than a
    // `$('=text')` exact-text locator — the latter relies on script-injected text matching that
    // doesn't reliably resolve against wry's webview under the classic WebDriver protocol this
    // harness forces (see cost-history.smoke.ts's identical note on `.tl-tabbar .tab`).
    let organizeRow: WebdriverIO.Element | undefined;
    await browser.waitUntil(
      async () => {
        const rows = $$(".cp-row");
        for await (const row of rows) {
          const html = await row.getHTML({ includeSelectorTag: false });
          if (html.includes("Organize this folder")) {
            organizeRow = row;
            return true;
          }
        }
        return false;
      },
      { timeout: 10_000, timeoutMsg: 'expected a .cp-row labelled "Organize this folder…" to appear' },
    );
    expect(organizeRow, 'expected a .cp-row labelled "Organize this folder…"').to.not.equal(undefined);
    await organizeRow!.waitForClickable({ timeout: 10_000 });
    await organizeRow!.click();

    // The dialog mounted.
    const rulePicker = await $('[data-testid="rule-picker"]');
    await rulePicker.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected the Organize dialog's rule picker to render",
    });

    // Select a rule (By extension) — an explicit user action, not just relying on the dialog's
    // default. Produces the most legible grouping for this fixture (PNG/ZIP/TXT/RS/MP3 subdirs),
    // which the next assertion checks by name.
    const byExtensionRule = await $('[data-testid="rule-by_extension"]');
    await byExtensionRule.waitForClickable({ timeout: 10_000 });
    await byExtensionRule.click();

    // Core assertion (CPE-1143): the grouped preview (fed by the real `organize_plan` command)
    // actually renders proposal rows for the seeded tmpDir's mixed-kind files — the FALSIFIABLE
    // check the ticket asks for. If `organize_plan` ever returned `[]` (a broken invoke, a bad path,
    // or the fixture files were missing), `[data-testid="empty-state"]` would render instead and NO
    // `[data-testid^="group-"]` would exist, so this fails loudly rather than silently passing on an
    // empty view.
    await browser.waitUntil(
      async () => (await $$('[data-testid^="group-"]').length) > 0,
      {
        timeout: 15_000,
        timeoutMsg: "expected at least one [data-testid^='group-'] proposal group to render",
      },
    );
    const groups = await $$('[data-testid^="group-"]');
    expect(groups.length, "expected >=1 proposal group from the seeded mixed-kind fixture").to.be.greaterThan(
      0,
    );

    // Stronger than just "some group rendered": the by-extension rule must have grouped the seeded
    // PNG/ZIP fixture files into their own named subfolders (organize.rs's `ByExtension` uppercases
    // the extension), so these specific groups — tied to files this very spec's fixture seeded — must
    // exist and list those files by name.
    const pngGroup = await $('[data-testid="group-PNG"]');
    await pngGroup.waitForExist({
      timeout: 10_000,
      timeoutMsg: `expected a PNG group for the seeded ${ORGANIZE_PNG_NAME}`,
    });
    const pngGroupHtml = await pngGroup.getHTML({ includeSelectorTag: false });
    expect(pngGroupHtml).to.include(ORGANIZE_PNG_NAME);

    const zipGroup = await $('[data-testid="group-ZIP"]');
    expect(await zipGroup.isExisting(), `expected a ZIP group for the seeded ${ORGANIZE_ZIP_NAME}`).to.equal(
      true,
    );
    const zipGroupHtml = await zipGroup.getHTML({ includeSelectorTag: false });
    expect(zipGroupHtml).to.include(ORGANIZE_ZIP_NAME);

    // The summary line rendered too (a second, independent surface fed by the same plan).
    const summary = await $('[data-testid="summary"]');
    expect(await summary.isExisting(), "expected the plan summary line to render").to.equal(true);

    // Non-destructive: never click Apply — dismiss via Cancel instead, for a clean end state (each
    // spec file gets its own fresh app launch/session in this harness, so this isn't required for
    // isolation from other specs, just tidy).
    const cancelBtn = await $('[data-testid="cancel-btn"]');
    await cancelBtn.click();
  });
});
