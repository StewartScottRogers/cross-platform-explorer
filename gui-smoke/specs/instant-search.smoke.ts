// CPE-1143 — QA-Architect follow-up pin for the Ctrl+K Instant Search overlay (CPE-1139, epic
// CPE-703): drives the real built app, opens the overlay with the actual global keyboard shortcut
// (App.svelte's `handleKeydown`), and asserts it renders.
//
// The overlay's off-means-off design (InstantSearch.svelte, CPE-1137) means the reliable state to
// pin is the "no resident index" affordance, NOT a populated result list: `index_status` reports
// only volumes resident in the in-memory `IndexService` (crates/server/src/index_service.rs —
// "resident_count 0 for a fresh service", no disk scan). That makes the "Build index" affordance the
// one FALSIFIABLE, non-flaky assertion available headlessly here — see the ticket's explicit guidance
// to keep it as the assertion that "stands alone" rather than depending on an actual index crawl
// (which would also be slow/non-deterministic over a real drive in CI).
//
// CPE-1866 CORRECTION: this comment used to say the harness "launches a brand-new app process per
// spec file, so `hasIndex` is guaranteed `false`" — true before CPE-1866, no longer true in general.
// gui-smoke now shares ONE app process across a whole shard (session-per-shard); `resetAppState`
// (wdio.conf.ts) resets FRONTEND/test-mode state between spec files but never touches the backend
// `IndexService` — there is no reset hook for it, and none is added by this comment. The assertion
// below is DORMANT-SAFE today only because no other spec in this suite ever calls `build_index` (or
// anything that resident-registers a volume) — grep confirms it — so `hasIndex` still happens to be
// false when this spec runs. If a future spec ever builds a real index and shares this spec's shard,
// this assertion would start failing (or, worse, silently pass for the wrong reason if it also
// resident-registers THIS spec's own volume) — an unguarded assumption sitting directly on top of the
// isolation model, not something this ticket fixed. Flagged rather than fixed: adding an
// `IndexService`-clearing test-mode hook for a case nothing exercises today would be speculative
// engineering; re-open this comment the day a spec needs one.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

describe("CPE-1143 — headless GUI smoke: Ctrl+K Instant Search overlay renders", () => {
  before(() => {
    // Written by wdio.conf.ts#onPrepare in the main process before this session started. Not used
    // directly by this spec (no fixture keyed off it), but read for parity with every other spec in
    // this suite and to fail fast with a clear error if the state file is somehow missing.
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
  });

  // CPE-1149: on a failing run, leave a shot of the state it failed in (`instant-search-fail.png`) —
  // the inline `snap("instant-search")` below is only reached on a pass. Non-arrow fn so Mocha binds
  // `this`; `snapFailure` is a no-op on a pass and swallows its own errors.
  afterEach(async function () {
    await snapFailure(this.currentTest, "instant-search");
  });

  it("Ctrl+K opens the overlay and its off-means-off 'Build index' affordance renders", async () => {
    // Wait for the initial `--open=<tmpDir>` navigation (also asserted in open-dir.smoke.ts) so the
    // window has real, settled DOM content and focus isn't sitting in some transient loading state
    // before we send a global keyboard shortcut.
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });

    // Click the breadcrumb first so focus is on a plain, non-input element — `handleKeydown`
    // (App.svelte) ignores Ctrl+K while an INPUT/TEXTAREA is focused, and nothing should be focused
    // in an input this early, but this makes the precondition explicit rather than assumed.
    await crumb.click();

    // The actual global shortcut a real user presses (App.svelte: `ctrl && !shiftKey && key === "k"`
    // → `instantSearchOpen = true`) — not a test-mode seam, since the existing opener already reaches
    // this surface headlessly via WebdriverIO's key-combo support.
    await browser.keys(["Control", "k"]);

    // The overlay mounted.
    const panel = await $(".is-panel");
    await panel.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected .is-panel (Instant Search overlay) to render after Ctrl+K",
    });
    expect(await panel.isDisplayed(), "expected the Instant Search overlay to be visible").to.equal(true);

    // Core assertion (CPE-1143): the guaranteed off-means-off state — `hasIndex` resolves to `false`
    // (see header comment) once `index_status` returns on this fresh app process, so `.is-offer`
    // renders instead of a blank body or a stale loading state.
    const offer = await $(".is-offer");
    await offer.waitForExist({
      timeout: 15_000,
      timeoutMsg: "expected .is-offer (the 'Build index' affordance) to render for a fresh app process",
    });

    // Falsifiable, not just "a container rendered": the actual "Build index" button, enabled (this
    // harness's `--open=<tmpDir>` navigation means `root` is non-empty, so `resolveBuildRoot` yields a
    // real drive root and the button is never disabled here).
    const offerHtml = await offer.getHTML({ includeSelectorTag: false });
    expect(offerHtml).to.include("Build index");

    const buildBtn = await $(".is-offer button.btn.primary");
    expect(await buildBtn.isExisting(), "expected the 'Build index' button to render").to.equal(true);
    expect(await buildBtn.isEnabled(), "expected 'Build index' to be enabled once a folder is open").to.equal(
      true,
    );

    // CPE-1148 Part A: capture the overlay's off-means-off state (after the assertions above). On a
    // FAILING run this line is never reached — the `afterEach` hook above captures
    // `instant-search-fail.png` of the failure state instead (CPE-1149).
    await snap("instant-search");
  });
});
