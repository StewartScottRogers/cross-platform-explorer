// CPE-1182/1183 — GUI-smoke pin for archive password support (PasswordPromptDialog wiring into
// extract/create) and the enriched compress/extract context-menu rows (Extract to…, Compress to
// .tar.gz, Compress with password…).
//
// The password round-trip is exercised END-TO-END against the REAL app + REAL backend, with NO
// hand-built crypto: rather than hand-rolling AES-encrypted zip bytes (infeasible to get bit-for-bit
// compatible with the `zip` crate's AES implementation from plain Node here), this spec uses the
// app's OWN already-shipped "Compress with password…" action to create a genuinely AES-256-encrypted
// zip from a seeded plain file, then right-clicks THAT zip and extracts it — the real
// `extract_archive`/`extract_zip_encrypted` commands run for real, so the password prompt this spec
// asserts is the actual encryption error path (`zip` crate's "Password required to decrypt file" /
// "The password provided is incorrect" — see crates/server/src/archive.rs), not a simulated one.
//
// No wdio.conf.ts changes: reuses two already-seeded fixtures so this spec is self-contained —
// MARKER_NAME (a plain .txt — the "Compress with password…" source) and ARCHIVE_TARGZ_NAME (an
// already-extractable archive — CPE-1181's fixture — for the enriched-menu snap, which wants both the
// Extract* rows and the Compress* rows visible in the same right-click).
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";
import { rightClick, type Point } from "../lib/mouse.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

// Seeded by wdio.conf.ts#onPrepare / #seedArchiveBrowseFixture (kept as literals here, matching this
// harness's existing convention of duplicating seeded names rather than importing across the
// runner/worker process boundary — see open-dir.smoke.ts's FIXTURE_NAME comment).
const MARKER_NAME = "CPE-1045-marker.txt";
const ARCHIVE_TARGZ_NAME = "CPE-1181-archive.tar.gz";
// `doCompressWithPassword` (App.svelte) names the created zip after the source's stem — no wdio.conf.ts
// change needed, this is just what the app itself will call it.
const CREATED_ZIP_NAME = "CPE-1045-marker.zip";
const CORRECT_PASSWORD = "cpe-1182-smoke-pw";
const WRONG_PASSWORD = "definitely-not-it";

/** Viewport-space centre of the FIRST `.rows .row` whose HTML includes `name` (or `null`).
 *  CPE-1481: scrolls the match into view first — see archive-browse.smoke.ts's identical `pointOfRow`
 *  for the full rationale (CPE-1253's below-the-fold hit-test failure). This spec right-clicks TWO
 *  different rows across two tests (the archive row, then — after that row's own scroll position —
 *  the marker row), so a stale scroll offset left over from the first is exactly the kind of gap this
 *  guards against. */
async function pointOfRow(name: string): Promise<Point | null> {
  await browser.execute((n) => {
    const rows = Array.from(document.querySelectorAll(".rows .row"));
    const row = rows.find((r) => (r.innerHTML || "").includes(n));
    row?.scrollIntoView({ block: "center" });
  }, name);
  await browser.pause(150);
  return browser.execute((n) => {
    const rows = Array.from(document.querySelectorAll(".rows .row"));
    const row = rows.find((r) => (r.innerHTML || "").includes(n));
    if (!row) return null;
    const rect = row.getBoundingClientRect();
    return { x: Math.round(rect.left + rect.width / 2), y: Math.round(rect.top + rect.height / 2) };
  }, name) as Promise<Point | null>;
}

/** The FIRST open `.ctx` menu row (`button[role="menuitem"]`) whose HTML includes `label`, or
 *  `undefined`. Scans rendered HTML rather than a `$('=text')` exact-text locator — the latter relies
 *  on script-injected text matching that doesn't reliably resolve against wry's webview under the
 *  classic WebDriver protocol this harness forces (see column-picker.smoke.ts's identical note). */
async function ctxRowByLabel(label: string): Promise<WebdriverIO.Element | undefined> {
  const rows = $$('.ctx button[role="menuitem"]');
  for await (const row of rows) {
    const html = await row.getHTML({ includeSelectorTag: false });
    if (html.includes(label)) return row;
  }
  return undefined;
}

async function dismissMenu(): Promise<void> {
  if (await $(".ctx").isExisting()) {
    await browser.keys("Escape");
    await browser.pause(150);
  }
}

describe("CPE-1182/1183 — headless GUI smoke: archive password prompt + enriched context menu", () => {
  before(() => {
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
  });

  afterEach(async function () {
    await snapFailure(this.currentTest, "archive-password");
    await dismissMenu();
  });

  it("right-clicking an extractable archive shows BOTH the extract rows and the new compress rows (CPE-1182/1183)", async () => {
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });

    const archivePt = await pointOfRow(ARCHIVE_TARGZ_NAME);
    expect(archivePt, `row centre for "${ARCHIVE_TARGZ_NAME}"`).to.not.equal(null);

    await rightClick(archivePt!);
    const ctx = await $(".ctx");
    await ctx.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected the item context menu (.ctx) to open on the archive row",
    });

    const html = await ctx.getHTML({ includeSelectorTag: false });
    // Existing rows, unchanged (CPE-252/251).
    expect(html, 'expected the existing "Extract" row').to.include("Extract");
    expect(html, 'expected the existing "Compress to ZIP" row').to.include("Compress to ZIP");
    // New CPE-1183 rows.
    expect(html, 'expected the new "Extract to…" row').to.include("Extract to…");
    expect(html, 'expected the new "Compress to .tar.gz" row').to.include("Compress to .tar.gz");
    // New CPE-1182 row.
    expect(html, 'expected the new "Compress with password…" row').to.include("Compress with password…");

    await snap("archive-context-menu-enriched");
    await dismissMenu();
  });

  it("Compress with password… creates a real AES-encrypted zip, and extracting it prompts for the password (wrong re-prompts, right one succeeds)", async () => {
    // --- Step 1: create the password-protected zip from the seeded plain-text marker file. -----------
    const markerPt = await pointOfRow(MARKER_NAME);
    expect(markerPt, `row centre for "${MARKER_NAME}"`).to.not.equal(null);

    await rightClick(markerPt!);
    let ctx = await $(".ctx");
    await ctx.waitForExist({ timeout: 10_000, timeoutMsg: "expected the item context menu on the marker row" });

    const compressPasswordRow = await ctxRowByLabel("Compress with password…");
    expect(compressPasswordRow, 'expected a "Compress with password…" row').to.not.equal(undefined);
    await compressPasswordRow!.waitForClickable({ timeout: 10_000 });
    await compressPasswordRow!.click();

    // PasswordPromptDialog (CPE-1179) — the "Set a password" (create) variant.
    let field = await $('[data-testid="password-field"]');
    await field.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected the password dialog to open for Compress with password…",
    });
    await field.setValue(CORRECT_PASSWORD);
    let okBtn = await $('[data-testid="ok-btn"]');
    await okBtn.click();

    // The dialog closes once compress_to_zip_encrypted succeeds, and the new zip appears in the listing.
    await browser.waitUntil(
      async () => !(await $('[data-testid="password-field"]').isExisting()),
      { timeout: 10_000, timeoutMsg: "expected the password dialog to close after a successful create" },
    );
    await browser.waitUntil(
      async () => (await pointOfRow(CREATED_ZIP_NAME)) !== null,
      { timeout: 10_000, timeoutMsg: `expected a new row for "${CREATED_ZIP_NAME}" after Compress with password…` },
    );

    // --- Step 2: extract the encrypted zip — the REAL backend genuinely needs the password. ----------
    const zipPt = await pointOfRow(CREATED_ZIP_NAME);
    expect(zipPt, `row centre for "${CREATED_ZIP_NAME}"`).to.not.equal(null);

    await rightClick(zipPt!);
    ctx = await $(".ctx");
    await ctx.waitForExist({ timeout: 10_000, timeoutMsg: "expected the item context menu on the encrypted zip row" });

    const extractRow = await ctxRowByLabel("Extract");
    // NB: "Extract" is a substring of "Extract to…" too, so make sure we grabbed the exact row, not the
    // destination-picker one, by checking its own text is exactly "Extract".
    expect(extractRow, 'expected an "Extract" row').to.not.equal(undefined);
    expect((await extractRow!.getText()).trim()).to.equal("Extract");
    await extractRow!.waitForClickable({ timeout: 10_000 });
    await extractRow!.click();

    // The real `extract_archive` call fails on the AES-encrypted zip (no password yet) -> the app
    // catches the "password" error and shows PasswordPromptDialog's "Password required" (extract)
    // variant instead of a raw error notice.
    field = await $('[data-testid="password-field"]');
    await field.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected the password dialog to open when extracting the encrypted zip",
    });
    await snap("password-prompt-extract");

    // --- Step 3: a WRONG password re-prompts with the error line instead of silently failing. --------
    await field.setValue(WRONG_PASSWORD);
    okBtn = await $('[data-testid="ok-btn"]');
    await okBtn.click();

    const errorLine = await $('[data-testid="password-error"]');
    await errorLine.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected the wrong password to re-prompt with an error line, not silently fail",
    });
    // Still the same dialog — a wrong password re-prompts, it does not dismiss.
    expect(await $('[data-testid="password-field"]').isExisting(), "the dialog must stay open after a wrong password").to.equal(true);

    // --- Step 4: the RIGHT password succeeds and the dialog closes. -----------------------------------
    field = await $('[data-testid="password-field"]');
    await field.setValue(CORRECT_PASSWORD);
    okBtn = await $('[data-testid="ok-btn"]');
    await okBtn.click();

    await browser.waitUntil(
      async () => !(await $('[data-testid="password-field"]').isExisting()),
      { timeout: 10_000, timeoutMsg: "expected the password dialog to close once the right password decrypts the zip" },
    );
  });
});
