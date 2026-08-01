// CPE-1181 — extends the already-working in-app ZIP archive browsing (CPE-242/1179) to non-zip
// formats (tar/tar.gz/tgz/7z/iso). The backend's `read_archive_entries` (crates/server/src/archive.rs)
// was already format-agnostic before this ticket; the frontend gap was `ARCHIVE_EXTS` (App.svelte)
// only recognising the zip family, so double-clicking a `.tar.gz` fell through to `openExternal`
// instead of entering it. This spec drives the REAL built app: double-clicks a genuinely valid,
// hand-built `.tar.gz` fixture (seeded by wdio.conf.ts#seedArchiveBrowseFixture) and asserts the
// in-app archive view actually opened — its one entry renders as a row, and the breadcrumb trail
// ends on the archive's own name (`archiveCrumbs` in App.svelte), exactly as it already does for zip.
//
// Leaf-open (opening the entry inside the tar.gz) is NOT exercised here: `openInArchive` routes
// non-zip formats through the new `extractArchiveEntryAny` command (CPE-1180), which is guarded at
// runtime — if that command isn't present yet on this build it falls back to a "can't open this entry
// yet" notice rather than a broken call (see App.svelte's `openInArchive`). Once CPE-1180 lands and a
// build carries both changes, a live run of this suite (CI's job, not a local dev sandbox) is what
// actually exercises the real extract-and-open path end-to-end.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";
import { doubleClick, type Point } from "../lib/mouse.js"; // CPE-1155: non-grabbing CDP double-click

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

// Seeded by wdio.conf.ts#seedArchiveBrowseFixture into the opened tmpDir (kept as literals here,
// matching this suite's existing convention of duplicating seeded names rather than importing across
// the runner/worker process boundary — see open-dir.smoke.ts's FIXTURE_NAME comment).
const ARCHIVE_TARGZ_NAME = "CPE-1181-archive.tar.gz";
const ARCHIVE_TARGZ_INNER_NAME = "CPE-1181-note.txt";

/** Viewport-space centre of the FIRST `.rows .row` whose text content includes `name` (or `null`). */
async function pointOfRow(name: string): Promise<Point | null> {
  return browser.execute((n) => {
    const rows = Array.from(document.querySelectorAll(".rows .row"));
    const row = rows.find((r) => (r.textContent || "").includes(n));
    if (!row) return null;
    const rect = row.getBoundingClientRect();
    return { x: Math.round(rect.left + rect.width / 2), y: Math.round(rect.top + rect.height / 2) };
  }, name) as Promise<Point | null>;
}

describe("CPE-1181 — headless GUI smoke: browsing into a non-zip (.tar.gz) archive", () => {
  before(() => {
    // Written by wdio.conf.ts#onPrepare in the main process before this session started — confirms
    // the seeded tmpDir (carrying the fixture) is what --open navigated into.
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
  });

  // CPE-1149: on a failing run, leave a shot of the state it failed in
  // (`archive-browse-targz-fail.png`) — the inline `snap(...)` below is only reached on a pass.
  // Non-arrow fn so Mocha binds `this`; `snapFailure` is a no-op on a pass and swallows its own errors.
  afterEach(async function () {
    await snapFailure(this.currentTest, "archive-browse-targz");
  });

  it("double-clicking a .tar.gz enters it: rows render and the breadcrumb shows the archive name", async () => {
    // Wait for the initial `--open=<tmpDir>` navigation (also asserted in open-dir.smoke.ts) so the
    // seeded fixture is actually in the current listing before we look for its row.
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });

    // Locate the fixture's row by scanning rendered row HTML for its filename (the reliable primitive
    // under wry's classic-WebDriver webview — see open-dir.smoke.ts's note on why a text-based
    // `$('=name')` locator isn't used here), then resolve its centre point for a faithful CDP click.
    const rows = $$(".row");
    let archiveRowFound = false;
    for await (const row of rows) {
      const html = await row.getHTML({ includeSelectorTag: false });
      if (html.includes(ARCHIVE_TARGZ_NAME)) {
        archiveRowFound = true;
        break;
      }
    }
    expect(archiveRowFound, `expected a file row containing "${ARCHIVE_TARGZ_NAME}"`).to.equal(true);

    const archivePt = await pointOfRow(ARCHIVE_TARGZ_NAME);
    expect(archivePt, `row centre for "${ARCHIVE_TARGZ_NAME}"`).to.not.equal(null);

    // A faithful double-click (CPE-1155: real hit-testing via CDP, never moves the OS cursor) —
    // App.svelte's `open()` routes a double-clicked archive-extension file through `enterArchive()`,
    // which reads it with the already format-agnostic `readArchiveEntries` and swaps the pane into the
    // in-app archive view (`archive` becomes non-null, `archiveChildren`/`archiveCrumbs` take over).
    await doubleClick(archivePt!);

    // Core assertion #1 (rows render): the tar.gz's one entry appears as a row in the (now virtual)
    // listing — proves `enterArchive` actually populated `archive.entries` and `archiveChildren`
    // rendered them, not just that SOME navigation happened.
    const body = await $("body");
    await browser.waitUntil(
      async () => {
        const html = await body.getHTML({ includeSelectorTag: false });
        return html.includes(ARCHIVE_TARGZ_INNER_NAME);
      },
      {
        timeout: 15_000,
        timeoutMsg: `expected the in-archive listing to render a row for "${ARCHIVE_TARGZ_INNER_NAME}"`,
      },
    );

    // Core assertion #2 (breadcrumb shows the archive name): `archiveCrumbs` appends the archive's own
    // name as the LAST crumb once inside it (App.svelte's `crumbs` derivation). NavToolbar.svelte
    // renders every crumb except the last as `<button class="crumb">`, but the last (active) crumb —
    // exactly the in-archive one we need here — is a non-button `<span class="crumb current"
    // aria-current="page">` (NavToolbar.svelte's `{#if i === crumbs.length - 1}` branch). Scanning only
    // `button.crumb` therefore always excludes the in-archive crumb and can never pass; query `.crumb`
    // (which matches both the buttons and the trailing span — `.crumb-sep` is a distinct class so it's
    // not swept in) to see the real, full trail.
    const crumbs = await $$(".crumb");
    const crumbTexts: string[] = [];
    for await (const c of crumbs) crumbTexts.push((await c.getText()).trim());
    expect(
      crumbTexts,
      `expected the breadcrumb trail to end on the archive name "${ARCHIVE_TARGZ_NAME}"`,
    ).to.include(ARCHIVE_TARGZ_NAME);
    expect(crumbTexts[crumbTexts.length - 1]).to.equal(ARCHIVE_TARGZ_NAME);

    await snap("archive-browse-targz");
  });
});
