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
import { snap, snapFailure } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

// Duplicated literals rather than importing from wdio.conf.ts — matches this harness's existing
// convention (see open-dir.smoke.ts's FIXTURE_NAME comment) of not reaching across the
// runner/worker process boundary. Seeded by wdio.conf.ts#seedOrganizeFixture alongside the
// pre-existing MARKER_NAME (.txt)/FIXTURE_NAME (.rs) files.
const ORGANIZE_PNG_NAME = "CPE-1143-photo.png";
const ORGANIZE_ZIP_NAME = "CPE-1143-archive.zip";

// CPE-1965 — the selector this spec waits on before it clicks a rule pill (see the long note at the
// wait site for why the wait exists at all). Declared here, at column 0 and exactly once, so
// `src/lib/components/OrganizeDialog.test.ts` can read the literal back out of this file and PIN what
// it matches against a real render of `OrganizeDialog.svelte` at t=0, t=119ms and t=180ms. That test is
// the only thing standing between this line and the round-1 version of it, which accepted
// `empty-state` too and was therefore satisfied at t=0 — a wait that waited for nothing.
const CPE1965_SETTLED_PREVIEW = "[data-testid='summary']";

describe("CPE-1143 — headless GUI smoke: auto-organize dialog renders a grouped preview", () => {
  before(() => {
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
  });

  // CPE-1149: on a failing run, leave a shot of the state it failed in (`organize-dialog-fail.png`)
  // — the inline `snap("organize-dialog")` below is only reached on a pass. Non-arrow fn so Mocha
  // binds `this`; `snapFailure` is a no-op on a pass and swallows its own errors.
  afterEach(async function () {
    await snapFailure(this.currentTest, "organize-dialog");
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

    // CPE-1965 — WAIT FOR THE DEFAULT RULE'S PREVIEW TO LAND BEFORE CLICKING A RULE PILL, and do NOT
    // replace this with a longer `waitForClickable`: clickability was never the problem.
    //
    // CPE-1968 UPDATE, READ THIS FIRST: the app defect described below is FIXED. `.preview` now has a
    // single plan-independent height (`clamp(200px, 40vh, 340px)`), so the dialog no longer changes
    // height when the plan lands and there is no longer a reflow to sit out. This wait is therefore
    // BELT-AND-BRACES, not load-bearing — kept because waiting for the preview the very next
    // assertion depends on is honest regardless, and because it costs one poll. The paragraphs below
    // are retained as the diagnosis of a fixed bug, NOT as a live description of the component: they
    // quote declarations (`min-height:120px`, `max-height:45vh`) that no longer exist.
    // `src/lib/components/OrganizeDialog.test.ts` re-derives the real ones on every run and now
    // asserts the OPPOSITE — that the loading and settled heights are equal — so if a content-driven
    // height is ever reintroduced it reds there, at the component, rather than resurfacing here as a
    // 4.3% flake.
    //
    // [FIXED IN CPE-1968 — historical] `OrganizeDialog.svelte`'s backdrop is `display:grid;
    // place-items:center` (the app-wide dialog
    // convention — 28 components share the rule), and its `.preview` box went from `min-height:120px`
    // while the first `organize_plan` was in flight to as much as `max-height:45vh` once the plan
    // rendered. At the 1000x700 window this harness sets, that was a ~195px growth on a VERTICALLY
    // CENTRED dialog, so the `.rules` row above it slid UP by ~98px about 120ms (the dialog's own
    // debounce) after mount. The rule pills are 28px tall. Clicking inside that window was a coin flip:
    // WebDriver computes the element's centre point, the reflow happens, and the synthesized click
    // lands ~98px lower — inside `.preview`, whose ancestor `.dialog` has `on:click|stopPropagation`.
    // So the click SUCCEEDS at the protocol level, nothing is intercepted, the dialog stays open, and
    // `rule` is simply never set.
    //
    // DIRECTLY OBSERVED, not just inferred: in the failing log the driver issues `elementClick` at
    // 28.956 and logs `RESULT null` at 28.985, while the dialog's debounce fires at mount+120ms
    // ~= 28.960. The reflow lands INSIDE the click command's own 29ms window.
    //
    // MEASURED: 3 of 69 shard-4 jobs (4.3%) over 2026-08-27T12:24Z-2026-08-28T01:46Z cutting on
    // JOB-START time (cutting on run-created admits 3 more jobs, all passes: 3 of 72 = 4.2%), across
    // BOTH webdriverio 9.30.0 (`added 479 packages`) and 9.31.4 (`added 489`) — so this is NOT
    // CPE-1960's `scrollIntoView` wheel regression, which only exists on 9.31.4. Every failure sits in
    // a 113-119ms find-to-click band and NONE sits outside it (on the 72-job sweep: 3 of 13 in-band
    // jobs failed, 0 of 59 out-of-band; Fisher p = 0.0048). In-band is a coin flip, not a death
    // sentence — 3 in-band jobs at 115/116/117ms PASSED, which is exactly what a race predicts, and
    // the argument rests on nothing failing OUTSIDE the band rather than on the band being empty of
    // passes. Run 33131342785's `organize-dialog-fail.png` is
    // the proof of mechanism: "By kind" still highlighted, the by_kind plan rendered,
    // `CPE-1143-photo.png` plainly present in the folder behind the dialog.
    //
    // WHAT THIS WAITS ON, and why it is `summary` ALONE. `[data-testid="summary"]` renders only in the
    // `plan.length > 0` branch, i.e. only once the plan has come back and the preview has grown to its
    // final height — so waiting for it genuinely gates the reflow. `empty-state` does NOT: the
    // component initialises `loading = false, plan = []` and `$: rule, path, scheduleLoad()` merely
    // ARMS a 120ms `setTimeout`, so the `{:else if plan.length === 0}` branch renders `empty-state`
    // synchronously AT MOUNT. Round 1 of this fix accepted all three "settled" testids and was
    // therefore satisfied at t=0, buying one `findElements` round-trip (~10-15ms in these logs) and
    // nothing else — it shifted the gap distribution INTO the hazard band rather than gating on the
    // reflow. `src/lib/components/OrganizeDialog.test.ts` now pins that t=0 state against a real render.
    // Tolerating `error` was the same mistake in miniature: the error branch renders in a `.preview`
    // that has NOT grown. If the plan ever legitimately comes back empty this wait times out with the
    // message below — which is fine, because the `group-` assertion further down would have failed
    // anyway; this way it is named at the point that matters instead of 10s later.
    await browser.waitUntil(async () => (await $$(CPE1965_SETTLED_PREVIEW).length) > 0, {
      timeout: 15_000,
      timeoutMsg:
        `expected the Organize dialog's default (by_kind) preview to settle (${CPE1965_SETTLED_PREVIEW}) ` +
        "before picking a rule — the plan never rendered, so the dialog never reached its final height",
    });

    // Select a rule (By extension) — an explicit user action, not just relying on the dialog's
    // default. Produces the most legible grouping for this fixture (PNG/ZIP/TXT/RS/MP3 subdirs),
    // which the next assertion checks by name.
    const byExtensionRule = await $('[data-testid="rule-by_extension"]');
    await byExtensionRule.waitForClickable({ timeout: 10_000 });
    await byExtensionRule.click();

    // CPE-1965: assert the click actually LANDED, at the click site. Without this the swallowed click
    // above surfaced 10s later as "expected a PNG group for the seeded CPE-1143-photo.png" — a message
    // that reads like a broken `organize_plan` or a missing fixture and cost a day of mis-diagnosis
    // (it was filed as an `element not interactable` failure, which was an unrelated line in the same
    // log). `.rule.active` is bound to `rule === r.value` in OrganizeDialog.svelte, so this is a direct
    // read of the state the click was supposed to change, not a proxy for it.
    await browser.waitUntil(
      async () => ((await byExtensionRule.getAttribute("class")) ?? "").split(/\s+/).includes("active"),
      {
        timeout: 10_000,
        timeoutMsg:
          'clicked [data-testid="rule-by_extension"] but it never became .active — the click was ' +
          "swallowed (see the reflow note above), not rejected",
      },
    );

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

    // CPE-1148 Part A: capture the grouped-preview dialog (after the assertions above, before it's
    // dismissed below). On a FAILING run this line is never reached and the dialog is still open when
    // the `afterEach` hook fires, so it captures `organize-dialog-fail.png` of the failure state
    // instead (CPE-1149) — capturing in the hook rather than here is why the pass shot must stay
    // inline: the afterEach runs only after the Cancel click below dismisses the dialog.
    await snap("organize-dialog");

    // Non-destructive: never click Apply — dismiss via Cancel instead, for a clean end state.
    // CPE-1866 CORRECTION: this used to say "each spec file gets its own fresh app launch/session in
    // this harness, so this isn't required for isolation... just tidy" — true before CPE-1866, no
    // longer true. gui-smoke now shares ONE app process across a whole shard (session-per-shard), so
    // this dialog staying open WOULD leak into the next spec file's run if this click were ever
    // removed. It already happens to be here (this spec was written tidy, not lazy), so no behavior
    // change was needed — only the stale reasoning.
    const cancelBtn = await $('[data-testid="cancel-btn"]');
    await cancelBtn.click();
  });
});
