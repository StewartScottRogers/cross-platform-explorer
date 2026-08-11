// CPE-1629 — headless GUI smoke: preview-pane provider screenshots, in both themes and both pane
// widths (epic CPE-1148, closing the manual-verification gap CPE-1615/PR#820's review exposed).
//
// THE PROBLEM THIS FIXES: `gui-smoke` drives the real built app and captures screenshots, but before
// this spec it had no coverage of the PREVIEW PANE's structured providers at all. When the Binary
// Inspector (CPE-1597) gained a whole new tab (CPE-1615's ".NET metadata" tab), CI produced no
// screenshot of it — the only way to actually look at the change was a reviewer hand-building a
// throwaway Vite + real-Chrome harness, rebuilt from scratch and thrown away. This spec is that
// coverage, made permanent: it opens the preview pane against COMMITTED sample files (the same
// `CPE-1358-samples` copy `specs/samples.smoke.ts` walks — see `wdio.conf.ts#seedSamplesFixture`) and
// `snap()`s each structured provider surface.
//
// WHY LIGHT+DARK AND NARROW+COMFORTABLE: this crew's hardest-won lesson (3,231 green jsdom tests once
// shipped a build where every submenu was clipped invisible) is that a green suite says nothing about
// LAYOUT. The narrow-pane case in particular is where clipping and pill-reflow defects actually show
// up (CLAUDE.md's "tick-tacks reflow" convention) — several surfaces here (JWT/cert EXPIRED badges,
// the font glyph grid) render pill/badge rows that are exactly the shape that convention exists for.
//
// THE BINARY INSPECTOR TAB WALK IS DATA-DRIVEN, NOT A HARDCODED TAB LIST: it reads whatever buttons
// `.bp-tabs .tab` actually renders at spec-run time, rather than naming "Overview"/"Sections"/etc. by
// hand. That is what makes the ".NET metadata" tab (CPE-1615, PR #820) "covered" in the durable sense
// the ticket asks for — PROVEN, not just designed for it: CPE-1615 merged into `main` WHILE this ticket
// was being worked, and merging it into this branch made the managed-PE walk test below pick up the
// real ".NET metadata" tab automatically, at full flagship (2x2 combo) depth, with ZERO changes to this
// file — confirmed on a real `tauri build` (see `binary-managed-net-metadata-*.png`). See "the
// managed-PE walk" test below for the other half of what that needed: a sample the tab can actually
// render against, since `BinaryPreview.svelte` only shows it when `managed` is true (see
// `samples/README.md`'s "`other/mini-dotnet.dll`" section for that fixture).
//
// See README.md's "Adding a preview-pane spec for a new provider" for the one-line recipe this spec
// itself follows.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$ } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";
import { setTheme, type GuiSmokeTheme } from "../lib/theme.js";
import { PREVIEW_PANE_COMFORTABLE_PX, PREVIEW_PANE_NARROW_PX, setPreviewPaneWidth } from "../lib/paneWidth.js";
import { assertAppStillAlive, openSampleFile } from "../lib/samplesNav.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

// Duplicated literal rather than importing from wdio.conf.ts — matches this harness's existing
// convention (see samples.smoke.ts / file-health.smoke.ts / declutter.smoke.ts's identical notes).
const SAMPLES_DIR_NAME = "CPE-1358-samples";

/** One (theme, width) capture combo and the name suffix it maps to. */
interface Combo {
  theme: GuiSmokeTheme;
  widthPx: number;
  suffix: string;
}

const WIDE = PREVIEW_PANE_COMFORTABLE_PX;
const NARROW = PREVIEW_PANE_NARROW_PX;

/** All four (theme x width) combinations — used in full for each spec's FLAGSHIP surface (the one
 *  named in the ticket as needing deep coverage). */
const ALL_COMBOS: Combo[] = [
  { theme: "light", widthPx: WIDE, suffix: "light-wide" },
  { theme: "light", widthPx: NARROW, suffix: "light-narrow" },
  { theme: "dark", widthPx: WIDE, suffix: "dark-wide" },
  { theme: "dark", widthPx: NARROW, suffix: "dark-narrow" },
];

/** Lowercase, alnum-only slug of a tab button's visible label (e.g. `"Exports (3)"` -> `"exports"`),
 *  used to name screenshots after whatever tabs actually exist rather than a hardcoded list. */
function slugify(label: string): string {
  return label
    .replace(/\(.*?\)/g, "") // drop a trailing row-count parenthetical, e.g. "Sections (0)"
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

async function applyCombo(combo: Combo): Promise<void> {
  await setTheme(combo.theme);
  await setPreviewPaneWidth(combo.widthPx);
}

/** Walks the Binary Inspector's tabs (data-driven off whatever `.bp-tabs .tab` buttons actually render
 *  — see the file header comment for why), capturing one screenshot per tab visited. `snapNamePrefix`
 *  names the files (`<prefix>-<tab-slug>-<combo-suffix>.png`).
 *
 *  Cost matters here: each (theme, width) combo change is a real popover open/set/close round trip
 *  (`../lib/paneWidth.js`) against a live WebView2/WebKitGTK session, and this suite's per-test budget
 *  is finite (`mochaOpts.timeout`, widened for this file — see the `describe` block below — plus the
 *  Linux leg's overall 45-minute job cap, CPE-1481/1594). So:
 *   - only the tab at index 0 (the flagship — Overview for a native PE today, and whichever tab's slug
 *     matches `fullMatrixSlugContains` for the managed-PE walk below, so a FUTURE ".NET metadata" tab
 *     automatically gets the same depth the moment CPE-1615 ships it) gets the full 2x2 combo matrix;
 *   - at most `restTabLimit` further tabs are visited, at a SINGLE shared `restCombo` applied once (not
 *     re-toggled per tab) — proving "the rest of the tabs render at all" without paying a combo-toggle
 *     for each one. This is deliberately NOT "every tab" (unlike `samples.smoke.ts`'s file-kind walk,
 *     where walking every file IS the point) — a couple of representative tabs is enough to catch a
 *     real rendering regression; screenshotting all six on every run is the kind of "25 near-identical
 *     shots" noise `README.md`'s "Screenshots for the Visual Critic" section already warns against. */
async function walkBinaryInspectorTabs(
  snapNamePrefix: string,
  restCombo: Combo,
  restTabLimit: number,
  fullMatrixSlugContains?: string,
): Promise<void> {
  // `[data-testid="binary-preview"]` sits on BinaryPreview.svelte's OUTERMOST wrapper (`.bp-preview`),
  // which is present even while `loading` is still true (it only wraps a "Loading…" `<p>` then) — so
  // this only proves the component mounted, not that `info` loaded and the tab strip exists yet. Wait
  // for an actual `.bp-tabs .tab` to exist before touching the strip at all, rather than inferring
  // readiness from the wrapper.
  await $(".bp-tabs .tab").waitForExist({ timeout: 10_000 });
  const tabButtons = await $$(".bp-tabs .tab");
  const tabCount = await tabButtons.length;
  expect(tabCount, "expected the Binary Inspector to render at least one tab").to.be.greaterThan(0);

  let restComboApplied = false;
  let restTabsVisited = 0;
  for (let i = 0; i < tabCount; i++) {
    // Re-query each iteration: switching tabs re-renders `.bp-panel`'s content, and on some drivers a
    // stale element handle from before a click no longer resolves reliably (same defensive pattern
    // other specs in this suite use around re-renders, e.g. cost-history.smoke.ts).
    const button = (await $$(".bp-tabs .tab"))[i];
    const label = (await button.getText()).trim();
    const slug = slugify(label);

    const useFullMatrix = i === 0 || (!!fullMatrixSlugContains && slug.includes(fullMatrixSlugContains));
    if (!useFullMatrix && restTabsVisited >= restTabLimit) continue; // capped — see doc comment above

    // CPE-1629 Linux CI failure, attempt 2: adding `waitForClickable` (the house convention —
    // cost-history.smoke.ts and every other tab/row click in this suite) did NOT fix this. The real job
    // log (run 31492855405) shows `isElementClickable`'s hit-test (`elementFromPoint` at the button's
    // own center) returning `false` for the full 10s, even though `getComputedStyle` reports
    // `display: flex` and `checkVisibility()` reports `true` throughout — the button is real, painted,
    // and exactly where it should be, but WebKitGTK never resolves it as the hit-test winner at its own
    // center point. The CI job sets `WEBKIT_DISABLE_COMPOSITING_MODE: 1`, which is documented to change
    // WebKitGTK's paint/hit-test path; `.bp-tabs` is also a `overflow-x: auto` scroll container, which
    // is exactly the kind of element whose non-composited hit-testing this env var is known to disrupt.
    // Confirmed it's not a "wrong tab" bug: both flagship tests fail on this exact line, on iteration
    // i=0 (the already-active "Overview" tab) every time — the loop never reaches a second tab.
    //
    // Fix: don't ask WebDriver to hit-test-and-synthesize a mouse click at all. `button.execute()`
    // (same idiom link-badge.smoke.ts uses to dispatch a `mouseenter` directly) hands the real DOM node
    // into the browser and calls its native `.click()` method there — no `elementFromPoint` involved.
    // Svelte's `on:click` is a plain `addEventListener('click', …)`, which fires on a native `.click()`
    // exactly as it would on a hit-tested mouse click, so this is a faithful trigger of the same
    // handler, not a state-setting shortcut. This still requires the button to exist (thrown by a stale
    // element reference if not) and still exercises the real click path end-to-end.
    await button.execute((el) => (el as HTMLElement).click());
    await $(`.bp-panel[data-tab]`).waitForExist({ timeout: 5_000 });

    if (useFullMatrix) {
      for (const combo of ALL_COMBOS) {
        await applyCombo(combo);
        await snap(`${snapNamePrefix}-${slug}-${combo.suffix}`);
      }
      restComboApplied = false; // the flagship's last combo may not be `restCombo` — re-apply once more below
    } else {
      if (!restComboApplied) {
        await applyCombo(restCombo);
        restComboApplied = true;
      }
      await snap(`${snapNamePrefix}-${slug}-${restCombo.suffix}`);
      restTabsVisited += 1;
    }
  }

  await assertAppStillAlive(`walking Binary Inspector tabs (${snapNamePrefix})`);
}

describe("CPE-1629 — headless GUI smoke: preview-pane provider screenshots", function () {
  // Each (theme, width) combo change is a real popover round trip against a live WebView2/WebKitGTK
  // session — measured at several seconds each, and variable under load — so the flagship 2x2-matrix
  // tests below can run past the suite's default 90s `mochaOpts.timeout` (wdio.conf.ts). Set at the
  // SUITE level (not in a `beforeEach` hook — `this.timeout()` called inside a hook only widens that
  // hook's own timeout, not the following test's; this is the one place in this describe block that
  // actually reaches every `it()` here) so every test in this file gets the same generous budget
  // without guessing per-test which one needs it.
  this.timeout(240_000);

  let samplesRootAbs = "";

  before(() => {
    const state = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir?: string };
    if (!state.tmpDir) throw new Error("expected STATE_FILE to carry the seeded tmpDir");
    samplesRootAbs = path.join(state.tmpDir, SAMPLES_DIR_NAME);
  });

  afterEach(async function () {
    // this.currentTest.title is unique per `it()` in this file, so a failing run's shot never
    // clobbers another surface's — same discipline as samples.smoke.ts's "samples-walk" name, just
    // slugged per-test since this file covers many distinct surfaces.
    const name = this.currentTest ? `preview-pane-${slugify(this.currentTest.title)}` : "preview-pane";
    await snapFailure(this.currentTest, name);
  });

  afterEach(async () => {
    // Always leave the pane back at a comfortable/light baseline between tests, independent of
    // whether the test above passed or failed — later specs in the suite (if this file isn't last)
    // must not inherit a narrow/dark pane from here.
    await setTheme("light");
    await setPreviewPaneWidth(PREVIEW_PANE_COMFORTABLE_PX);
  });

  it("the Binary Inspector's tabs render across a native PE (other/mini.dll) — flagship depth on Overview", async function () {
    await openSampleFile(path.join(samplesRootAbs, "other"), "mini.dll", {
      extraSelectors: ['[data-testid="binary-preview"]'],
    });
    // restTabLimit=2: Overview gets the full matrix; two more tabs (Sections, then the next one the
    // walk reaches) are captured once at a shared combo — enough to prove "the rest of the tab strip
    // isn't broken" without paying a combo-toggle per tab (see the function's own doc comment).
    await walkBinaryInspectorTabs("binary-native", { theme: "dark", widthPx: NARROW, suffix: "dark-narrow" }, 2);
  });

  it("the Binary Inspector's tabs render across a MANAGED .NET PE (other/mini-dotnet.dll) — picks up .NET metadata automatically once CPE-1615 ships", async function () {
    // `mini-dotnet.dll` (CPE-1629, samples/README.md) is byte-for-byte the same fixture shape
    // `dotnet_metadata.rs`'s own `build_minimal_managed_pe()` test helper asserts `is_managed()` true
    // for — see `crates/server/tests/sample_fixtures.rs::mini_dotnet_dll_parses_as_a_real_managed_pe`.
    // CPE-1615 merged into `main` while this ticket was in flight; `fullMatrixSlugContains: "net"`
    // means this loop found the real ".NET metadata" tab (its slug is "net-metadata") the moment this
    // branch picked up that merge, and gave it the same flagship 2x2 depth as Overview — CONFIRMED on a
    // real `tauri build`, zero changes needed to this file (`binary-managed-net-metadata-*.png`: real
    // assembly identity "MyAssembly" v1.0.0.0, referenced assemblies mscorlib + System.Core, matching
    // this exact fixture's contents).
    await openSampleFile(path.join(samplesRootAbs, "other"), "mini-dotnet.dll", {
      extraSelectors: ['[data-testid="binary-preview"]'],
    });
    // restTabLimit=1: the native-PE test above already proves "the rest of the tab strip renders";
    // this test's real job is the DIFFERENT byte-parse path (a managed PE, not a native one) plus the
    // ".NET metadata" tab itself (now real, not just anticipated) — one extra plain tab is enough on
    // top of that.
    await walkBinaryInspectorTabs("binary-managed", { theme: "light", widthPx: WIDE, suffix: "light-wide" }, 1, "net");
  });

  it("the data-grid preview renders a real sqlite table (database/mini.sqlite)", async function () {
    await openSampleFile(path.join(samplesRootAbs, "database"), "mini.sqlite");
    await $(".data-browser").waitForExist({ timeout: 10_000 });

    for (const combo of [
      { theme: "light" as const, widthPx: WIDE, suffix: "light-wide" },
      { theme: "dark" as const, widthPx: NARROW, suffix: "dark-narrow" },
    ]) {
      await applyCombo(combo);
      await snap(`preview-pane-data-grid-${combo.suffix}`);
    }
    await assertAppStillAlive("previewing database/mini.sqlite");
  });

  it("the font specimen preview renders real glyphs (fonts/mini.ttf) — glyph grid is a reflow surface", async function () {
    await openSampleFile(path.join(samplesRootAbs, "fonts"), "mini.ttf", {
      extraSelectors: ['[data-testid="font-preview"]'],
    });
    await $('[data-testid="font-preview"]').waitForExist({ timeout: 10_000 });

    for (const combo of [
      { theme: "dark" as const, widthPx: WIDE, suffix: "dark-wide" },
      { theme: "light" as const, widthPx: NARROW, suffix: "light-narrow" },
    ]) {
      await applyCombo(combo);
      await snap(`preview-pane-font-${combo.suffix}`);
    }
    await assertAppStillAlive("previewing fonts/mini.ttf");
  });

  it("the certificate preview renders an EXPIRED badge without clipping at a narrow width (crypto/expired.pem)", async function () {
    // `expired.pem` is chosen specifically (over a valid cert) because its `cert-expired` badge is a
    // pill — exactly the CLAUDE.md "tick-tacks reflow" surface this ticket's narrow-width requirement
    // exists to catch (a pill whose text overflows its background at a narrow pane is the regression
    // class, not a hypothetical).
    await openSampleFile(path.join(samplesRootAbs, "crypto"), "expired.pem", {
      extraSelectors: ['[data-testid="cert-preview"]'],
    });
    await $('[data-testid="cert-preview"]').waitForExist({ timeout: 10_000 });
    await $('[data-testid="cert-expired"]').waitForExist({ timeout: 10_000 });

    await applyCombo({ theme: "dark", widthPx: NARROW, suffix: "dark-narrow" });
    await snap("preview-pane-cert-expired-dark-narrow");

    // A second, richer sample at the comfortable width/light theme — `chain.pem` carries more real
    // certificate-chain data than the single self-signed `expired.pem` does.
    await openSampleFile(path.join(samplesRootAbs, "crypto"), "chain.pem", {
      extraSelectors: ['[data-testid="cert-preview"]'],
    });
    await $('[data-testid="cert-preview"]').waitForExist({ timeout: 10_000 });
    await applyCombo({ theme: "light", widthPx: WIDE, suffix: "light-wide" });
    await snap("preview-pane-cert-chain-light-wide");

    await assertAppStillAlive("previewing crypto/expired.pem + crypto/chain.pem");
  });

  it("the JWT preview renders an EXPIRED badge without clipping at a narrow width (crypto/expired.jwt)", async function () {
    // Same pill-reflow reasoning as the cert test above — `expired.jwt`'s `jwt-expired` badge.
    await openSampleFile(path.join(samplesRootAbs, "crypto"), "expired.jwt", {
      extraSelectors: ['[data-testid="jwt-preview"]'],
    });
    await $('[data-testid="jwt-preview"]').waitForExist({ timeout: 10_000 });
    await $('[data-testid="jwt-expired"]').waitForExist({ timeout: 10_000 });

    await applyCombo({ theme: "dark", widthPx: NARROW, suffix: "dark-narrow" });
    await snap("preview-pane-jwt-expired-dark-narrow");

    // `rich-claims.jwt` at the comfortable width/light theme — more claims than expired.jwt carries.
    await openSampleFile(path.join(samplesRootAbs, "crypto"), "rich-claims.jwt", {
      extraSelectors: ['[data-testid="jwt-preview"]'],
    });
    await $('[data-testid="jwt-preview"]').waitForExist({ timeout: 10_000 });
    await applyCombo({ theme: "light", widthPx: WIDE, suffix: "light-wide" });
    await snap("preview-pane-jwt-rich-claims-light-wide");

    await assertAppStillAlive("previewing crypto/expired.jwt + crypto/rich-claims.jwt");
  });
});
