// CPE-1594 / CPE-1677 — headless unit tests for the ratchet verdict function. Runs under Node's
// built-in test runner via `tsx` (same convention as `compare.test.ts`), so it's verifiable WITHOUT a
// `tauri build` or `tauri-driver` session. Run with:
//   npm run test:unit          (from gui-smoke/)
//
// CPE-1677 rewrote these from spec-file granularity to CASE granularity. The headline test is
// "clause 1 — a regression INSIDE an already-listed spec file": the exact experiment the CPE-1639 worker
// ran on real CI, where the old ratchet printed a byte-identical green verdict for a clean run and for a
// run with a deliberately broken font-preview case inside `samples.smoke.ts`.
import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { caseKey, evaluate, type CaseResult, type KnownFailingFile } from "./ratchet.js";

const SAMPLES = "samples.smoke.ts";
const SAVED_SEARCH = "saved-search.smoke.ts";
const OPEN_DIR = "open-dir.smoke.ts";

/** A miniature but structurally faithful stand-in for the real suite: one multi-case spec that is
 *  PARTIALLY known-failing (like `samples.smoke.ts`: 24 passing + 22 listed-failing cases), one
 *  single-case spec that is wholly known-failing, and one spec that is entirely green. */
const KNOWN_FAILING: KnownFailingFile = {
  cases: [
    {
      spec: SAMPLES,
      test: "opens samples/crypto/cert.der: no crash + preview renders or gracefully degrades",
      reason: "preview pane never settles on WebKitGTK",
      ticket: "CPE-1507",
    },
    {
      spec: SAMPLES,
      test: "opens samples/text/data.json: no crash + preview renders or gracefully degrades",
      reason: "preview pane never settles on WebKitGTK",
      ticket: "CPE-1507",
    },
    {
      spec: SAVED_SEARCH,
      test: "saves a search from the palette, shows it in the sidebar, and opens the filtered view",
      reason: "sidebar header never renders on Linux",
      ticket: "CPE-1507",
    },
  ],
};

const FONT_CASE = "opens samples/fonts/mini.ttf: no crash + preview renders or gracefully degrades";

/** The real, current `main` shape: every listed case fails, everything else passes, all 3 specs report. */
function mainStateResults(): CaseResult[] {
  return [
    { spec: SAMPLES, test: "the seeded samples copy is non-empty (coverage sanity)", status: "passed" },
    { spec: SAMPLES, test: FONT_CASE, status: "passed" },
    {
      spec: SAMPLES,
      test: "opens samples/images/pixel.png: no crash + preview renders or gracefully degrades",
      status: "passed",
    },
    {
      spec: SAMPLES,
      test: "opens samples/crypto/cert.der: no crash + preview renders or gracefully degrades",
      status: "failed",
    },
    {
      spec: SAMPLES,
      test: "opens samples/text/data.json: no crash + preview renders or gracefully degrades",
      status: "failed",
    },
    {
      spec: SAVED_SEARCH,
      test: "saves a search from the palette, shows it in the sidebar, and opens the filtered view",
      status: "failed",
    },
    { spec: OPEN_DIR, test: "the app window launched and <body> rendered non-empty content", status: "passed" },
    { spec: OPEN_DIR, test: "--open <tmpdir> navigated: the breadcrumb shows the folder name", status: "passed" },
  ];
}

const EXPECTED_SPECS = 3;

function withStatus(results: CaseResult[], spec: string, test: string, status: CaseResult["status"]): CaseResult[] {
  return results.map((r) => (r.spec === spec && r.test === test ? { ...r, status } : r));
}

describe("evaluate — the known-failing baseline (current main state)", () => {
  it("is green: every listed case fails, every unlisted case passes, all specs report", () => {
    const result = evaluate({
      results: mainStateResults(),
      knownFailing: KNOWN_FAILING,
      expectedSpecCount: EXPECTED_SPECS,
    });

    assert.equal(result.ok, true);
    assert.deepEqual(result.newFailures, []);
    assert.deepEqual(result.fixedButStillListed, []);
    assert.deepEqual(result.unmatchedListings, []);
    assert.deepEqual(result.duplicateListings, []);
    assert.equal(result.incomplete, false);
    assert.deepEqual(result.messages, []);
    assert.equal(result.reportedSpecCount, EXPECTED_SPECS);
  });
});

describe("evaluate — clause 1: NEW GUI REGRESSION", () => {
  // THE CPE-1677 test. Under the old spec-file ratchet this input produced a byte-identical green
  // verdict to the baseline above, because samples.smoke.ts was already "the failing spec".
  it("goes red when a case regresses INSIDE an already-known-failing spec file (the CPE-1639 experiment)", () => {
    const results = withStatus(mainStateResults(), SAMPLES, FONT_CASE, "failed");

    const result = evaluate({ results, knownFailing: KNOWN_FAILING, expectedSpecCount: EXPECTED_SPECS });

    assert.equal(result.ok, false);
    assert.deepEqual(result.newFailures, [caseKey(SAMPLES, FONT_CASE)]);
    assert.deepEqual(result.fixedButStillListed, []);
    assert.deepEqual(result.unmatchedListings, []);
    assert.equal(result.incomplete, false);
    assert.ok(result.messages.some((m) => m.includes("NEW GUI REGRESSION") && m.includes(FONT_CASE)));
  });

  it("still goes red when a case fails in a spec file with no entries at all", () => {
    const target = "--open <tmpdir> navigated: the breadcrumb shows the folder name";
    const results = withStatus(mainStateResults(), OPEN_DIR, target, "failed");

    const result = evaluate({ results, knownFailing: KNOWN_FAILING, expectedSpecCount: EXPECTED_SPECS });

    assert.equal(result.ok, false);
    assert.deepEqual(result.newFailures, [caseKey(OPEN_DIR, target)]);
  });

  it("reports every new failure, not just the first", () => {
    const results = withStatus(
      withStatus(mainStateResults(), SAMPLES, FONT_CASE, "failed"),
      OPEN_DIR,
      "the app window launched and <body> rendered non-empty content",
      "failed",
    );

    const result = evaluate({ results, knownFailing: KNOWN_FAILING, expectedSpecCount: EXPECTED_SPECS });

    assert.equal(result.newFailures.length, 2);
    assert.deepEqual(
      result.newFailures,
      [
        caseKey(OPEN_DIR, "the app window launched and <body> rendered non-empty content"),
        caseKey(SAMPLES, FONT_CASE),
      ].sort((a, b) => a.localeCompare(b)),
    );
  });

  it("an exemption is scoped to its own spec file — the same title failing elsewhere is still a regression", () => {
    const results: CaseResult[] = [
      ...mainStateResults(),
      {
        spec: "preview-pane.smoke.ts",
        test: "opens samples/crypto/cert.der: no crash + preview renders or gracefully degrades",
        status: "failed",
      },
    ];

    const result = evaluate({ results, knownFailing: KNOWN_FAILING, expectedSpecCount: 4 });

    assert.equal(result.ok, false);
    assert.deepEqual(result.newFailures, [
      caseKey("preview-pane.smoke.ts", "opens samples/crypto/cert.der: no crash + preview renders or gracefully degrades"),
    ]);
  });
});

describe("evaluate — clause 2: one-way ratchet (a listed case starts passing)", () => {
  it("goes red when a listed case passes, even though nothing else regressed and its spec still fails elsewhere", () => {
    const target = "opens samples/text/data.json: no crash + preview renders or gracefully degrades";
    const results = withStatus(mainStateResults(), SAMPLES, target, "passed");

    const result = evaluate({ results, knownFailing: KNOWN_FAILING, expectedSpecCount: EXPECTED_SPECS });

    assert.equal(result.ok, false);
    assert.deepEqual(result.newFailures, []);
    assert.deepEqual(result.fixedButStillListed, [caseKey(SAMPLES, target)]);
    assert.equal(result.incomplete, false);
    assert.ok(
      result.messages.some((m) => m.includes("RATCHET") && m.includes(target) && m.includes("delete its entry")),
    );
  });

  it("passes clean once the fixed case's entry is actually removed from the list", () => {
    const target = "opens samples/text/data.json: no crash + preview renders or gracefully degrades";
    const trimmed: KnownFailingFile = {
      cases: KNOWN_FAILING.cases.filter((c) => !(c.spec === SAMPLES && c.test === target)),
    };
    const results = withStatus(mainStateResults(), SAMPLES, target, "passed");

    const result = evaluate({ results, knownFailing: trimmed, expectedSpecCount: EXPECTED_SPECS });

    assert.equal(result.ok, true);
    assert.deepEqual(result.messages, []);
  });

  it("a listed case that is SKIPPED neither retires the entry nor reds the job", () => {
    const target = "opens samples/text/data.json: no crash + preview renders or gracefully degrades";
    const results = withStatus(mainStateResults(), SAMPLES, target, "skipped");

    const result = evaluate({ results, knownFailing: KNOWN_FAILING, expectedSpecCount: EXPECTED_SPECS });

    assert.equal(result.ok, true);
    assert.deepEqual(result.fixedButStillListed, []);
    assert.deepEqual(result.unmatchedListings, []);
  });
});

describe("evaluate — clause 3: STALE EXEMPTION (a listed title matches no test)", () => {
  // The trap CPE-1677 called out: titles are strings and drift. If a rename silently dropped the
  // exemption, it would drop the coverage with it — the rename would read as "case gone, nothing to
  // check" and CI would stay green with one fewer guarded surface.
  it("goes red when a listed case is renamed out from under the list", () => {
    const oldTitle = "opens samples/crypto/cert.der: no crash + preview renders or gracefully degrades";
    const renamed = mainStateResults().map((r) =>
      r.spec === SAMPLES && r.test === oldTitle ? { ...r, test: `${oldTitle} (v2)` } : r,
    );

    const result = evaluate({ results: renamed, knownFailing: KNOWN_FAILING, expectedSpecCount: EXPECTED_SPECS });

    assert.equal(result.ok, false);
    assert.deepEqual(result.unmatchedListings, [caseKey(SAMPLES, oldTitle)]);
    // ...and the renamed case is now an unlisted failure, so it reds the job twice over. Both are true.
    assert.deepEqual(result.newFailures, [caseKey(SAMPLES, `${oldTitle} (v2)`)]);
    assert.ok(result.messages.some((m) => m.includes("STALE EXEMPTION") && m.includes(oldTitle)));
  });

  it("goes red when a listed case is deleted outright (exemption now matches nothing)", () => {
    const target = "opens samples/crypto/cert.der: no crash + preview renders or gracefully degrades";
    const results = mainStateResults().filter((r) => !(r.spec === SAMPLES && r.test === target));

    const result = evaluate({ results, knownFailing: KNOWN_FAILING, expectedSpecCount: EXPECTED_SPECS });

    assert.equal(result.ok, false);
    assert.deepEqual(result.unmatchedListings, [caseKey(SAMPLES, target)]);
    assert.deepEqual(result.newFailures, []);
  });

  it("goes red when a whole listed spec dies before reporting its cases (every entry matches nothing)", () => {
    const results = mainStateResults().filter((r) => r.spec !== SAVED_SEARCH);

    const result = evaluate({ results, knownFailing: KNOWN_FAILING, expectedSpecCount: EXPECTED_SPECS });

    assert.equal(result.ok, false);
    assert.equal(result.incomplete, true); // 2 of 3 spec files reported
    assert.deepEqual(result.unmatchedListings, [
      caseKey(SAVED_SEARCH, "saves a search from the palette, shows it in the sidebar, and opens the filtered view"),
    ]);
  });
});

describe("evaluate — clause 4: SUITE DID NOT COMPLETE (the CPE-1594 regression this exists to catch)", () => {
  it("goes red when fewer spec files report than expected, even if every reported case looks fine", () => {
    // The exact CPE-1594 failure mode: a timeout kills the job partway, and everything that DID run
    // happened to pass or match the list — a naive "any unlisted failures?" check would call this GREEN.
    const result = evaluate({
      results: mainStateResults(),
      knownFailing: KNOWN_FAILING,
      expectedSpecCount: 41,
    });

    assert.equal(result.ok, false);
    assert.equal(result.incomplete, true);
    assert.ok(result.messages.some((m) => m.includes("SUITE DID NOT COMPLETE") && m.includes("41") && m.includes("3")));
  });

  it("counts DISTINCT spec files, not cases, so a many-case run doesn't mask a missing spec", () => {
    const result = evaluate({
      results: mainStateResults(), // 8 cases, but only 3 spec files
      knownFailing: KNOWN_FAILING,
      expectedSpecCount: 5,
    });

    assert.equal(result.reportedSpecCount, 3);
    assert.equal(result.incomplete, true);
  });

  it("a run with MORE spec files than expected is not flagged incomplete (defensive, not the real case)", () => {
    const result = evaluate({ results: mainStateResults(), knownFailing: KNOWN_FAILING, expectedSpecCount: 2 });
    assert.equal(result.incomplete, false);
  });
});

describe("evaluate — clause 5: DUPLICATE EXEMPTION (list hygiene)", () => {
  it("goes red when the same spec+test is listed twice", () => {
    const target = "opens samples/text/data.json: no crash + preview renders or gracefully degrades";
    const dupes: KnownFailingFile = {
      cases: [
        ...KNOWN_FAILING.cases,
        { spec: SAMPLES, test: target, reason: "duplicated by accident", ticket: "CPE-1507" },
      ],
    };

    const result = evaluate({ results: mainStateResults(), knownFailing: dupes, expectedSpecCount: EXPECTED_SPECS });

    assert.equal(result.ok, false);
    assert.deepEqual(result.duplicateListings, [caseKey(SAMPLES, target)]);
    assert.ok(result.messages.some((m) => m.includes("DUPLICATE EXEMPTION") && m.includes(target)));
  });
});

describe("evaluate — combined failure modes in one run", () => {
  it("reports incomplete + newFailures + fixedButStillListed + unmatchedListings all at once", () => {
    const listedThatPasses = "opens samples/text/data.json: no crash + preview renders or gracefully degrades";
    const results = withStatus(
      withStatus(mainStateResults(), SAMPLES, listedThatPasses, "passed"), // ratchet fires
      SAMPLES,
      FONT_CASE,
      "failed", // new regression inside the same spec file
    ).filter((r) => r.spec !== SAVED_SEARCH); // and that spec never reported: incomplete + stale entry

    const result = evaluate({ results, knownFailing: KNOWN_FAILING, expectedSpecCount: EXPECTED_SPECS });

    assert.equal(result.ok, false);
    assert.equal(result.incomplete, true);
    assert.deepEqual(result.newFailures, [caseKey(SAMPLES, FONT_CASE)]);
    assert.deepEqual(result.fixedButStillListed, [caseKey(SAMPLES, listedThatPasses)]);
    assert.deepEqual(result.unmatchedListings, [
      caseKey(SAVED_SEARCH, "saves a search from the palette, shows it in the sidebar, and opens the filtered view"),
    ]);
    assert.equal(result.messages.length, 4);
  });
});

describe("evaluate — empty/edge inputs", () => {
  it("an empty list still passes cleanly when every case passes and the spec count matches", () => {
    const results: CaseResult[] = [
      { spec: "a.smoke.ts", test: "does a thing", status: "passed" },
      { spec: "b.smoke.ts", test: "does another thing", status: "passed" },
    ];
    const result = evaluate({ results, knownFailing: { cases: [] }, expectedSpecCount: 2 });
    assert.equal(result.ok, true);
  });

  it("zero results against a nonzero expectedSpecCount is incomplete, not vacuously true", () => {
    const result = evaluate({ results: [], knownFailing: { cases: [] }, expectedSpecCount: 41 });
    assert.equal(result.ok, false);
    assert.equal(result.incomplete, true);
  });

  it("a duplicated it() title inside one spec is treated as one key: a failure anywhere keeps it listed", () => {
    const target = "opens samples/text/data.json: no crash + preview renders or gracefully degrades";
    const results: CaseResult[] = [
      ...mainStateResults(),
      { spec: SAMPLES, test: target, status: "passed" }, // same title, second occurrence, passing
    ];

    const result = evaluate({ results, knownFailing: KNOWN_FAILING, expectedSpecCount: EXPECTED_SPECS });

    assert.equal(result.ok, true);
    assert.deepEqual(result.fixedButStillListed, []);
  });
});
