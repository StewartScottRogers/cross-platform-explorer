// CPE-1045 — the manual `--open <dir>` verification (CPE-1043/1044) automated: launches the real
// built app headlessly via tauri-driver, and asserts the folder actually opened.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js"; // ESM relative import — resolves to lib/snap.ts under tsx
import { compareSnapshotToBaseline } from "../lib/compare.js"; // CPE-1170 — advisory visual-diff vs. gui-smoke/baselines/

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");
const MARKER_NAME = "CPE-1045-marker.txt";
// CPE-1096: seeded by wdio.conf.ts#onPrepare alongside MARKER_NAME, into the same temp dir. Kept
// as a literal here (not imported from wdio.conf.ts) to match this file's existing convention of
// duplicating the seeded filename rather than reaching across the runner/worker boundary.
const FIXTURE_NAME = "CPE-1096-fixture.rs";

describe("CPE-1045 — headless GUI smoke: --open <dir> navigates", () => {
  let tmpBasename = "";

  before(() => {
    // Written by wdio.conf.ts#onPrepare in the main process before this session started.
    const { tmpDir } = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
    tmpBasename = path.basename(tmpDir);
  });

  // CPE-1149: on ANY failing test in this spec, leave a shot of the state it failed in
  // (`open-dir-fail.png`) — the inline `snap("open-dir")` below is only reached on a pass. Non-arrow
  // fn so Mocha binds `this`; `snapFailure` is a no-op on a pass and swallows its own errors.
  afterEach(async function () {
    await snapFailure(this.currentTest, "open-dir");
  });

  // Health check (burndown #2 — build -> deploy -> run smoke): the window launched and is
  // actually responding, independent of whether navigation itself worked.
  it("the app window launched and <body> rendered non-empty content", async () => {
    const body = await $("body");
    expect(await body.isExisting()).to.equal(true);

    const html = await body.getHTML({ includeSelectorTag: false });
    expect(html.trim().length).to.be.greaterThan(0);
  });

  // Core assertion (burndown #1 — GUI end-to-end): the same manual check hand-done for
  // CPE-1043/1044 — does the current-folder breadcrumb (`[aria-current="page"]`, the selector
  // App.features.test.ts already uses) show the folder we launched with `--open`?
  it("--open <tmpdir> navigated: the breadcrumb shows the folder name", async () => {
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });

    const text = await crumb.getText();
    expect(text).to.equal(tmpBasename);
  });

  // The folder's contents actually rendered — not just that the breadcrumb string updated.
  //
  // Note: this deliberately checks the rendered <body> HTML for the marker filename rather than
  // locating it with WebdriverIO's `$('=text')` exact-text selector. That locator strategy relies
  // on script injection that doesn't reliably resolve against wry's webview under the classic
  // WebDriver protocol this harness forces (see the `wdio:enforceWebDriverClassic` comment above) —
  // it timed out here even once navigation had genuinely succeeded. `body.getHTML()` is the same
  // primitive the health check above already uses successfully, so polling it for the marker
  // filename is a more reliable proxy for "the listing rendered this entry".
  it("the seeded marker file is visible in the listing", async () => {
    const body = await $("body");
    await browser.waitUntil(
      async () => {
        const html = await body.getHTML({ includeSelectorTag: false });
        return html.includes(MARKER_NAME);
      },
      {
        timeout: 15_000,
        timeoutMsg: `expected <body> to contain the marker filename "${MARKER_NAME}"`,
      },
    );

    // CPE-1148 Part A: capture the plain directory-listing surface (after the assertions above).
    // On a FAILING run this line is never reached — the `afterEach` hook above captures
    // `open-dir-fail.png` of the failure state instead (CPE-1149).
    await snap("open-dir");

    // CPE-1170 — visual-regression comparator, wired here as the worked example: diff the capture
    // just written above against the committed golden `gui-smoke/baselines/open-dir.png`. Advisory
    // by default (a mismatch only logs — see compareSnapshotToBaseline's doc comment); set
    // GUI_SMOKE_VISUAL_STRICT=1 to make an over-threshold diff fail this spec instead. No baseline
    // has been blessed for this surface yet in this repo (blessing needs a real `tauri build` run,
    // not available in a headless dev sandbox) — until one exists this call is a documented no-op
    // that logs "no baseline" and returns advisory-only, same failure-safe shape as `snap()` itself.
    await compareSnapshotToBaseline("open-dir");
  });

  // CPE-1096 — QA-debt burndown: CPE-1090 (outline strip/breadcrumb) and CPE-1091 (per-line
  // gutter/fold/indent/minimap) both shipped with headless-only verification. Opening the seeded
  // code fixture and asserting these selectors render pins that visual debt under CI, closing the
  // MANUAL-TEST-BURNDOWN.md rows for both tickets.
  it("selecting a code file renders the code-intelligence preview (CPE-1096)", async () => {
    // Locate the fixture's row by scanning each rendered row's HTML for the fixture filename,
    // rather than a text-based `$('=text')` locator — the latter relies on script-injected text
    // matching that (per the note on the marker-visibility test above) doesn't reliably resolve
    // against wry's webview under the classic WebDriver protocol this harness forces. `getHTML()`
    // on a CSS-located element is the same primitive already proven reliable here (`body.getHTML()`
    // above), just scoped to one row instead of the whole body.
    const rows = $$(".row");
    let fixtureRow: WebdriverIO.Element | undefined;
    for await (const row of rows) {
      const html = await row.getHTML({ includeSelectorTag: false });
      if (html.includes(FIXTURE_NAME)) {
        fixtureRow = row;
        break;
      }
    }
    expect(fixtureRow, `expected a file row containing "${FIXTURE_NAME}"`).to.not.equal(undefined);

    // CPE-1866 diagnostic (temporary, until the shard-3 "element click intercepted" regression is
    // root-caused — see the ticket's Work Log): dump exactly what `document.elementFromPoint` sees at
    // the row's own center BEFORE clicking, and walk up its ancestor chain, so a real CI run's log
    // names the actual interceptor instead of leaving it to guesswork. Removed once the cause is fixed.
    // eslint-disable-next-line no-console
    console.log(
      "[CPE-1866 diag]",
      JSON.stringify(
        await browser.execute((sel) => {
          const rows = Array.from(document.querySelectorAll(".row"));
          const row = rows.find((r) => (r.textContent || "").includes(sel));
          if (!row) return { error: "row not found in live re-query" };
          const rect = row.getBoundingClientRect();
          const cx = rect.left + rect.width / 2;
          const cy = rect.top + rect.height / 2;
          const top = document.elementFromPoint(cx, cy);
          const chain: unknown[] = [];
          let el: Element | null = top;
          while (el) {
            const cs = window.getComputedStyle(el);
            chain.push({
              tag: el.tagName,
              class: (el.className || "").toString(),
              id: (el as HTMLElement).id || undefined,
              zIndex: cs.zIndex,
              position: cs.position,
              pointerEvents: cs.pointerEvents,
              opacity: cs.opacity,
              display: cs.display,
              visibility: cs.visibility,
            });
            el = el.parentElement;
          }
          const pane = document.querySelector(".filelist-pane") as HTMLElement | null;
          return { rowRect: { left: rect.left, top: rect.top, width: rect.width, height: rect.height }, point: { cx, cy }, topSameAsRow: top === row, chain, paneScrollTop: pane ? pane.scrollTop : null };
        }, FIXTURE_NAME),
        null,
        2,
      ),
    );

    // A plain click (no ctrl/shift) single-selects the row, which feeds PreviewPane's `entry` prop
    // and triggers the codeIntel fetch (PreviewPane.svelte's `loadCodeIntelFor`).
    await fixtureRow!.click();

    // Per-line code rows (CPE-1091): one `.cl-row[data-line]` span per source line inside the
    // highlighted `<code>` — assert the first line rendered.
    const firstLine = await $('.cl-row[data-line="1"]');
    await firstLine.waitForExist({
      timeout: 15_000,
      timeoutMsg: "expected .cl-row[data-line=\"1\"] to render for the code fixture",
    });

    // Outline strip populated (CPE-1090): the fixture declares a struct + two fns, so
    // `code_outline::outline` must yield >=1 symbol and the pill row must render.
    const outlineBar = await $(".outline-bar");
    await outlineBar.waitForExist({
      timeout: 15_000,
      timeoutMsg: "expected .outline-bar to render for a file with symbols",
    });
    const pills = await $$(".outline-bar .outline-pill");
    expect(pills.length, "expected at least one .outline-pill in the outline strip").to.be.greaterThan(0);

    // Minimap (CPE-1091): `code_intel::analyze` populates >=1 minimap row for any non-empty text
    // (crates/server/src/minimap.rs — not gated on file size/language), so this is reliably
    // present, not merely best-effort.
    const minimap = await $(".minimap");
    expect(await minimap.isExisting(), "expected .minimap to render alongside the code rows").to.equal(true);

    // Regression: the highlighted code still renders as real text (not just empty markup) — the
    // same `.preview-text` class non-code previews use, now on the code-rows `<pre>`/`<code>`.
    const codeEl = await $(".cl-code");
    const codeHtml = await codeEl.getHTML({ includeSelectorTag: false });
    expect(codeHtml).to.include("greet");
    const pre = await $("pre.preview-text.code-rows");
    expect(await pre.isExisting(), "expected the highlighted pre.preview-text.code-rows to render").to.equal(true);
  });
});
