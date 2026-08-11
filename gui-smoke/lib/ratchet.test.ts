// CPE-1594 — headless unit tests for the ratchet verdict function. Runs under Node's built-in test
// runner via `tsx` (same convention as `compare.test.ts`), so it's verifiable WITHOUT a `tauri build`
// or `tauri-driver` session. Run with:
//   npm run test:unit          (from gui-smoke/)
import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { evaluate, type KnownFailingFile, type SpecResult } from "./ratchet.js";

// CPE-1594/CPE-1595: `network.smoke.ts` was triaged and found to be a stale test selector, not a
// product regression — `$("=Network")` maps to WebDriver's "link text" strategy, which only matches
// `<a>` elements, but the header is a plain `<span class="label fav-title">` (Sidebar.svelte). Fixed in
// the same PR (see `specs/network.smoke.ts`), so the real `known-failing.json` — and this mirror of it
// — carries 6 entries, not 7.
const KNOWN_FAILING: KnownFailingFile = {
  specs: {
    "samples.smoke.ts": { reason: "preview pane never settles on WebKitGTK", ticket: "CPE-1507" },
    "saved-search.smoke.ts": { reason: "sidebar header never renders on Linux", ticket: "CPE-1507" },
    "archive-browse.smoke.ts": { reason: "element click intercepted", ticket: "CPE-1595" },
    "archive-password.smoke.ts": { reason: "element not interactable", ticket: "CPE-1595" },
    "shred-dialog.smoke.ts": { reason: "'.ctx button.row' still not clickable after 10s", ticket: "CPE-1595" },
    "transfer-panel.smoke.ts": { reason: "seeded CPE-1226 transfer row never appears", ticket: "CPE-1595" },
  },
};

const ALL_40_SPECS = [
  "archive-browse.smoke.ts",
  "archive-password.smoke.ts",
  "batch-media.smoke.ts",
  "checkpoint-restore.smoke.ts",
  "column-picker.smoke.ts",
  "context-menu.smoke.ts",
  "cost-history.smoke.ts",
  "cost-ledger.smoke.ts",
  "declutter.smoke.ts",
  "drive-menu.smoke.ts",
  "file-health.smoke.ts",
  "home-item-menu.smoke.ts",
  "instant-search.smoke.ts",
  "link-badge.smoke.ts",
  "macro-in-menu.smoke.ts",
  "macro-param-prompt.smoke.ts",
  "macros-library.smoke.ts",
  "metadata-studio.smoke.ts",
  "native-tags.smoke.ts",
  "near-duplicates.smoke.ts",
  "network.smoke.ts",
  "new-link.smoke.ts",
  "open-dir.smoke.ts",
  "organize.smoke.ts",
  "pdf-video-thumbnails.smoke.ts",
  "populated-whitespace.smoke.ts",
  "radar.smoke.ts",
  "replay.smoke.ts",
  "samples.smoke.ts",
  "saved-search.smoke.ts",
  "shred-dialog.smoke.ts",
  "similar-images.smoke.ts",
  "snapshot-diff.smoke.ts",
  "snapshot-schedule-settings.smoke.ts",
  "spotlight.smoke.ts",
  "terminal-panel.smoke.ts",
  "thumbnail-gallery.smoke.ts",
  "transfer-panel.smoke.ts",
  "vault.smoke.ts",
  "vault-create.smoke.ts",
];

/** The real, current `main` state: 34 pass, the 6 KNOWN_FAILING specs fail, all 40 report. */
function mainStateResults(): SpecResult[] {
  const knownFailingNames = new Set(Object.keys(KNOWN_FAILING.specs));
  return ALL_40_SPECS.map((spec) => ({
    spec,
    status: knownFailingNames.has(spec) ? "failed" : "passed",
  }));
}

describe("evaluate — the known-6 baseline (current main state)", () => {
  it("is green: 34 pass, 6 known-failing, all 40 report", () => {
    const result = evaluate({
      results: mainStateResults(),
      knownFailing: KNOWN_FAILING,
      expectedSpecCount: 40,
    });

    assert.equal(result.ok, true);
    assert.deepEqual(result.newFailures, []);
    assert.deepEqual(result.fixedButStillListed, []);
    assert.equal(result.incomplete, false);
    assert.deepEqual(result.messages, []);
  });
});

describe("evaluate — clause 1: NEW GUI REGRESSION (a 7th failing spec)", () => {
  it("goes red when a spec outside known-failing.json fails", () => {
    const results = mainStateResults().map((r) =>
      r.spec === "open-dir.smoke.ts" ? { ...r, status: "failed" as const } : r,
    );

    const result = evaluate({ results, knownFailing: KNOWN_FAILING, expectedSpecCount: 40 });

    assert.equal(result.ok, false);
    assert.deepEqual(result.newFailures, ["open-dir.smoke.ts"]);
    assert.deepEqual(result.fixedButStillListed, []);
    assert.equal(result.incomplete, false);
    assert.ok(result.messages.some((m) => m.includes("NEW GUI REGRESSION") && m.includes("open-dir.smoke.ts")));
  });

  it("reports every new failure, not just the first, when multiple 7th+ specs fail", () => {
    const results = mainStateResults().map((r) =>
      r.spec === "open-dir.smoke.ts" || r.spec === "vault.smoke.ts" ? { ...r, status: "failed" as const } : r,
    );

    const result = evaluate({ results, knownFailing: KNOWN_FAILING, expectedSpecCount: 40 });

    assert.equal(result.ok, false);
    assert.deepEqual(result.newFailures, ["open-dir.smoke.ts", "vault.smoke.ts"]);
  });
});

describe("evaluate — clause 2: one-way ratchet (a known-failing spec starts passing)", () => {
  it("goes red when a listed spec passes, even though nothing else regressed", () => {
    const results = mainStateResults().map((r) =>
      r.spec === "archive-browse.smoke.ts" ? { ...r, status: "passed" as const } : r,
    );

    const result = evaluate({ results, knownFailing: KNOWN_FAILING, expectedSpecCount: 40 });

    assert.equal(result.ok, false);
    assert.deepEqual(result.newFailures, []);
    assert.deepEqual(result.fixedButStillListed, ["archive-browse.smoke.ts"]);
    assert.equal(result.incomplete, false);
    assert.ok(
      result.messages.some(
        (m) => m.includes("RATCHET") && m.includes("archive-browse.smoke.ts") && m.includes("delete its entry"),
      ),
    );
  });

  it("passes clean once the fixed spec's entry is actually removed from known-failing.json", () => {
    const { "archive-browse.smoke.ts": removed, ...rest } = KNOWN_FAILING.specs;
    void removed;
    const trimmed: KnownFailingFile = { specs: rest };

    const results = mainStateResults().map((r) =>
      r.spec === "archive-browse.smoke.ts" ? { ...r, status: "passed" as const } : r,
    );

    const result = evaluate({ results, knownFailing: trimmed, expectedSpecCount: 40 });
    assert.equal(result.ok, true);
  });
});

describe("evaluate — clause 3: SUITE DID NOT COMPLETE (the CPE-1594 regression this exists to catch)", () => {
  it("goes red when fewer specs report than expected, even if every reported spec looks fine", () => {
    // Simulates the exact CPE-1594 failure mode: a 45-min timeout kills the job after ~33 of 40 specs,
    // and every spec that DID get to run happened to pass or match known-failing — a naive "any
    // failures?" check would call this GREEN. It must not.
    const truncated = mainStateResults().slice(0, 33);

    const result = evaluate({ results: truncated, knownFailing: KNOWN_FAILING, expectedSpecCount: 40 });

    assert.equal(result.ok, false);
    assert.equal(result.incomplete, true);
    assert.ok(result.messages.some((m) => m.includes("SUITE DID NOT COMPLETE") && m.includes("40") && m.includes("33")));
  });

  it("incomplete alone is enough to fail even with zero newFailures/fixedButStillListed", () => {
    const truncated = mainStateResults()
      .filter((r) => r.status === "passed") // only the known-good specs reported
      .slice(0, 20);

    const result = evaluate({ results: truncated, knownFailing: KNOWN_FAILING, expectedSpecCount: 40 });

    assert.equal(result.incomplete, true);
    assert.deepEqual(result.newFailures, []);
    assert.deepEqual(result.fixedButStillListed, []);
    assert.equal(result.ok, false);
  });

  it("a run with MORE reported specs than expected is not flagged incomplete (defensive, not the real case)", () => {
    const result = evaluate({
      results: mainStateResults(),
      knownFailing: KNOWN_FAILING,
      expectedSpecCount: 39, // fewer than the 40 that actually reported
    });

    assert.equal(result.incomplete, false);
  });
});

describe("evaluate — combined failure modes in one run", () => {
  it("reports incomplete + newFailures + fixedButStillListed all at once when all three happen together", () => {
    const results = mainStateResults()
      .map((r) => (r.spec === "archive-browse.smoke.ts" ? { ...r, status: "passed" as const } : r)) // ratchet fires
      .map((r) => (r.spec === "open-dir.smoke.ts" ? { ...r, status: "failed" as const } : r)) // new regression
      .slice(0, 39); // and the run didn't complete

    const result = evaluate({ results, knownFailing: KNOWN_FAILING, expectedSpecCount: 40 });

    assert.equal(result.ok, false);
    assert.equal(result.incomplete, true);
    assert.deepEqual(result.fixedButStillListed, ["archive-browse.smoke.ts"]);
    assert.deepEqual(result.newFailures, ["open-dir.smoke.ts"]);
    assert.equal(result.messages.length, 3);
  });
});

describe("evaluate — empty/edge inputs", () => {
  it("an empty known-failing list still passes cleanly when everything passes and the count matches", () => {
    const results: SpecResult[] = [
      { spec: "a.smoke.ts", status: "passed" },
      { spec: "b.smoke.ts", status: "passed" },
    ];
    const result = evaluate({ results, knownFailing: { specs: {} }, expectedSpecCount: 2 });
    assert.equal(result.ok, true);
  });

  it("zero results against a nonzero expectedSpecCount is incomplete, not vacuously true", () => {
    const result = evaluate({ results: [], knownFailing: { specs: {} }, expectedSpecCount: 40 });
    assert.equal(result.ok, false);
    assert.equal(result.incomplete, true);
  });
});
