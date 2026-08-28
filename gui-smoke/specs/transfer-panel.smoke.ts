// CPE-1226 — headless GUI smoke: pins the operations/transfer panel's ARCHIVE-OP row (CPE-1184) and
// spot-checks the CPE-1212 danger-red badge, both against the REAL built app.
//
// Two independent goals in one spec (so a single `--spec transfer-panel` run covers both):
//
//   1. TransferPanel archive row (CPE-1184): right-clicks a seeded file and drives a REAL
//      "Compress to ZIP" through the context menu (`doCompress` in App.svelte -> the queued
//      `start_archive_compress` transfer engine). Rather than trying to catch a mid-progress frame
//      (timing-sensitive, the ticket explicitly allows skipping it), this waits for and snaps the
//      COMPLETED row — `.ops .op.done`, wording "N item(s) compressed" (`TransferPanel.svelte`'s
//      `verb()`/`label()`) — which is deterministic: the row either reaches `.done` or the test
//      times out, no race with a fast/small compress finishing before a mid-flight assertion runs.
//      Uses a DEDICATED seeded subfolder (`seedTransferPanelFixture`, wdio.conf.ts) so the write
//      (the new .zip) never perturbs any other spec's fixture directory.
//
//   2. CPE-1212 danger-badge spot-check (ticket's "Also spot-check while capturing" note): CPE-1212
//      unified drifted danger reds to `--danger` #c42b1c, the largest shade shift landing on the
//      broken-link badge family (`LinkBadge.svelte` `.broken` — see its `color: var(--danger)` /
//      `background: color-mix(in srgb, var(--danger) 12%, transparent)`). Reuses the ALREADY-SEEDED
//      broken-symlink fixture (`CPE-1208-broken-link.txt`, `seedLinkBadgeFixture`, wdio.conf.ts) —
//      no new fixture needed — and captures a dedicated "danger-badge" frame so the Visual Critic can
//      confirm the new red reads correctly, independent of link-badge.smoke.ts's own "link-badge" shot
//      (that one pins the FEATURE; this one pins the COLOUR per the CPE-1226 ticket's explicit ask).
//      Symlink creation is unprivileged on POSIX but needs Developer Mode/elevation on Windows — same
//      best-effort `linkBadgeFixture.supported` gate link-badge.smoke.ts uses; skipped, not failed,
//      when unsupported on this runner.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";
import { rightClick, doubleClick, type Point } from "../lib/mouse.js";
import { scrollIntoViewCentered } from "../lib/scrollIntoView.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

// Seeded literals duplicated here rather than imported across the runner/worker boundary — matches
// this suite's existing convention (see open-dir.smoke.ts's note on FIXTURE_NAME).
const TRANSFER_PANEL_DIR_NAME = "CPE-1226-transfer-panel-folder";
const TRANSFER_PANEL_SRC_NAME = "CPE-1226-compress-me.txt";
// `doCompress` names the created zip after the source's stem (`compressBaseName`/`stripExt`).
const TRANSFER_PANEL_ZIP_NAME = "CPE-1226-compress-me.zip";
const LINK_BROKEN_NAME = "CPE-1208-broken-link.txt";

/** Viewport-space centre of the FIRST `.rows .row` whose HTML includes `name` (or `null`). Same
 *  primitive archive-password.smoke.ts / new-link.smoke.ts already proved reliable against wry's
 *  webview under the classic-WebDriver protocol this harness forces. */
async function pointOfRow(name: string): Promise<Point | null> {
  return browser.execute((n) => {
    const rows = Array.from(document.querySelectorAll(".rows .row"));
    const row = rows.find((r) => (r.innerHTML || "").includes(n));
    if (!row) return null;
    const rect = row.getBoundingClientRect();
    return { x: Math.round(rect.left + rect.width / 2), y: Math.round(rect.top + rect.height / 2) };
  }, name) as Promise<Point | null>;
}

/** The FIRST open `.ctx` menu row (`button[role="menuitem"]`) whose HTML includes `label`, or
 *  `undefined`. Mirrors archive-password.smoke.ts's identical helper. */
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

describe("CPE-1226 — headless GUI smoke: TransferPanel archive row + CPE-1212 danger-badge spot-check", () => {
  let tmpDir = "";
  let linkBadgeSupported = false;

  before(() => {
    const state = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as {
      tmpDir: string;
      linkBadgeFixture?: { supported: boolean };
    };
    tmpDir = state.tmpDir;
    linkBadgeSupported = state.linkBadgeFixture?.supported ?? false;
    if (!linkBadgeSupported) {
      // eslint-disable-next-line no-console
      console.warn(
        "[transfer-panel.smoke] symlink creation was unprivileged on this runner — skipping the CPE-1212 danger-badge spot-check",
      );
    }
  });

  afterEach(async function () {
    await snapFailure(this.currentTest, "transfer-panel");
    await dismissMenu();
  });

  it("navigates into the dedicated compress-source folder", async () => {
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });

    // CPE-1595: `pointOfRow` used to be called once, immediately after `crumb.waitForExist` — that
    // only proves navigation STARTED, not that the (per docs/design/STREAMING.md) batched listing
    // finished streaming ~30 root-level fixtures into the DOM. A live run (PR #801/CPE-1594, job
    // 93649941153) failed here with "expected a row for the seeded CPE-1226-transfer-panel-folder:
    // expected null to not equal null" — the same batch-streaming race archive-browse.smoke.ts hit on
    // its own initial row lookup — so retry instead of a one-shot call.
    let folderPt: Awaited<ReturnType<typeof pointOfRow>> = null;
    await browser.waitUntil(
      async () => {
        folderPt = await pointOfRow(TRANSFER_PANEL_DIR_NAME);
        return folderPt !== null;
      },
      { timeout: 15_000, timeoutMsg: `expected a row for the seeded "${TRANSFER_PANEL_DIR_NAME}" to render` },
    );
    expect(folderPt, `expected a row for the seeded "${TRANSFER_PANEL_DIR_NAME}"`).to.not.equal(null);
    await doubleClick(folderPt!);

    await browser.waitUntil(async () => (await pointOfRow(TRANSFER_PANEL_SRC_NAME)) !== null, {
      timeout: 15_000,
      timeoutMsg: `expected a row for "${TRANSFER_PANEL_SRC_NAME}" after entering "${TRANSFER_PANEL_DIR_NAME}"`,
    });
  });

  it('right-clicking the file and choosing "Compress to ZIP" queues a real compress that reaches the panel\'s COMPLETED archive row', async () => {
    // The panel must not already show a leftover row from an earlier spec.
    expect(await $(".ops").isExisting()).to.equal(false);

    const srcPt = await pointOfRow(TRANSFER_PANEL_SRC_NAME);
    expect(srcPt, `expected a row for "${TRANSFER_PANEL_SRC_NAME}"`).to.not.equal(null);

    await rightClick(srcPt!);
    const ctx = await $(".ctx");
    await ctx.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected the item context menu (.ctx) to open on the seeded file row",
    });

    const compressRow = await ctxRowByLabel("Compress to ZIP");
    expect(compressRow, 'expected a "Compress to ZIP" row in the context menu').to.not.equal(undefined);
    await compressRow!.waitForClickable({ timeout: 10_000 });
    await compressRow!.click();

    // Deterministic capture (per the ticket's own AC): wait for the COMPLETED row rather than trying
    // to catch a mid-progress frame — `.op.done` is the terminal state `TransferPanel.svelte` renders
    // once `transfer://done` lands (t.finished), so this either reaches it or times out — no race.
    const opDone = await $(".ops .op.done");
    await opDone.waitForExist({
      timeout: 30_000,
      timeoutMsg: 'expected the operations panel to show a COMPLETED archive-op row (".ops .op.done") after "Compress to ZIP"',
    });

    // Wording: TransferPanel's `label()` renders "<N> item(s) compressed" for a clean finish (no
    // cancel/fail/skip) — the CPE-1184 archive-op copy this ticket pins.
    const opHtml = await opDone.getHTML({ includeSelectorTag: false });
    expect(opHtml, 'expected the completed row\'s wording to read "… compressed"').to.match(/item.*compressed/i);

    // The row still carries an icon glyph (the finished state's checkmark, per TransferPanel.svelte's
    // `t.finished ? "check" : …` — the Visual Critic judges the actual glyph from the screenshot).
    expect(await opDone.$("svg.icon").isExisting(), "expected the completed row to render an icon").to.equal(true);

    await snap("transfer-archive-row");

    // Genuinely on disk, not just in the DOM: a real compress wrote a real zip next to the source.
    const zipPath = path.join(tmpDir, TRANSFER_PANEL_DIR_NAME, TRANSFER_PANEL_ZIP_NAME);
    expect(fs.existsSync(zipPath), `expected a real zip at "${zipPath}"`).to.equal(true);
  });

  it("navigating back to the root shows the broken-link badge in the new --danger red (CPE-1212 spot-check)", async function () {
    if (!linkBadgeSupported) return this.skip();

    // The crumb strip is `[Home, ...splitPath(currentPath)]` (App.svelte) — every ancestor segment
    // of the full filesystem path, not just "Home" + the tmpDir — so the tmpDir ROOT crumb is
    // whichever `.crumb` button's text is the tmpDir's own basename, not simply the first one.
    const tmpBasename = path.basename(tmpDir);
    let rootCrumb: WebdriverIO.Element | undefined;
    await browser.waitUntil(
      async () => {
        for (const c of await $$(".address button.crumb")) {
          if ((await c.getText()).trim() === tmpBasename) {
            rootCrumb = c;
            return true;
          }
        }
        return false;
      },
      { timeout: 10_000, timeoutMsg: `expected a .crumb button labelled "${tmpBasename}" (the opened tmpDir root)` },
    );
    await rootCrumb!.click();

    let brokenRow: WebdriverIO.Element | undefined;
    await browser.waitUntil(
      async () => {
        for (const row of await $$(".row")) {
          const html = await row.getHTML({ includeSelectorTag: false });
          if (html.includes(LINK_BROKEN_NAME)) {
            brokenRow = row;
            return true;
          }
        }
        return false;
      },
      { timeout: 15_000, timeoutMsg: `expected a row for "${LINK_BROKEN_NAME}" back at the root` },
    );

    // The root now carries every other spec's fixtures (folders-first, then files, alphabetically) —
    // the broken-link row sits well below the fold, so scroll it into view BEFORE snapping, or the
    // screenshot would show an empty pane with no badge for the Visual Critic to judge.
    await scrollIntoViewCentered(brokenRow!);

    const badge = await brokenRow!.$('[data-testid="link-badge"]');
    // `LinkBadge.svelte` fetches `link_status` LAZILY — via an IntersectionObserver OR a `mouseenter`
    // (`use:lazy` / `on:mouseenter={load}`). Neither the observer NOR a CDP-synthesised `mouseMoved`
    // reliably delivers a `mouseenter` DOM event through wry/WebView2's CDP proxy under this harness's
    // unfocused/off-screen `--test-mode` window — confirmed by direct diagnosis: the pre-existing,
    // UNMODIFIED link-badge.smoke.ts flakes the identical way on this box (its own IntersectionObserver
    // wait times out), and a `mouse.ts` `hover()` (a real CDP `Input.dispatchMouseEvent`) over the badge
    // ALSO left `status` unresolved. What genuinely unblocks it: a plain `dispatchEvent(new
    // MouseEvent("mouseenter"))`, verified by a matching direct `link_status` invoke call in the same
    // diagnosis (both agreed on `{is_symlink:true, broken:true}`). So this is a real, unfaked component
    // + backend round-trip — only the STIMULUS is a direct event dispatch rather than relying on the
    // observer/hit-testing path, because those two don't reach this element in this exact CDP shim.
    await badge.execute((el) => {
      el.dispatchEvent(new MouseEvent("mouseenter", { bubbles: false, cancelable: false }));
    });

    // The broken state only lands once the (now-forced) `link_status` fetch resolves (mirrors
    // link-badge.smoke.ts's identical poll) — poll the class rather than asserting immediately.
    await browser.waitUntil(
      async () => {
        const cls = (await badge.getAttribute("class")) ?? "";
        return cls.includes("broken");
      },
      { timeout: 15_000, timeoutMsg: "expected the broken symlink row's badge to gain the .broken class" },
    );

    await snap("danger-badge");
  });
});
