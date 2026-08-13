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
import fs from "node:fs";
import { describe, it } from "node:test";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { caseKey, evaluate, suggestedKnownFailingEntry, type CaseResult, type KnownFailingFile } from "./ratchet.js";

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

describe("evaluate — the `intermittent` escape hatch (CPE-1677)", () => {
  // A genuinely flaky case is a coin-flip gate under case granularity: clause 1 reds the runs where it
  // fails, clause 2 reds the runs where it passes. `intermittent: true` exempts BOTH directions — but
  // only both directions, and only for a case that still exists.
  const FLAKY = "opens samples/audio/track.ogg: no crash + preview renders or gracefully degrades";
  const WITH_FLAKY: KnownFailingFile = {
    cases: [
      ...KNOWN_FAILING.cases,
      { spec: SAMPLES, test: FLAKY, reason: "proven flaky across runs A/B/C", ticket: "CPE-1677", intermittent: true },
    ],
  };

  it("stays green when the intermittent case FAILS", () => {
    const results: CaseResult[] = [...mainStateResults(), { spec: SAMPLES, test: FLAKY, status: "failed" }];
    const result = evaluate({ results, knownFailing: WITH_FLAKY, expectedSpecCount: EXPECTED_SPECS });

    assert.equal(result.ok, true);
    assert.deepEqual(result.newFailures, []);
  });

  it("stays green when the intermittent case PASSES, and does NOT demand its removal", () => {
    const results: CaseResult[] = [...mainStateResults(), { spec: SAMPLES, test: FLAKY, status: "passed" }];
    const result = evaluate({ results, knownFailing: WITH_FLAKY, expectedSpecCount: EXPECTED_SPECS });

    assert.equal(result.ok, true);
    assert.deepEqual(result.fixedButStillListed, []);
  });

  it("reports the intermittent entry with its observed status either way, so it can't go quiet", () => {
    const results: CaseResult[] = [...mainStateResults(), { spec: SAMPLES, test: FLAKY, status: "passed" }];
    const result = evaluate({ results, knownFailing: WITH_FLAKY, expectedSpecCount: EXPECTED_SPECS });

    assert.deepEqual(result.intermittentListings, [{ key: caseKey(SAMPLES, FLAKY), statuses: ["passed"] }]);
  });

  it("still reds the job when the intermittent case stops existing (clause 3 is NOT waived)", () => {
    const result = evaluate({
      results: mainStateResults(), // no case with that title at all
      knownFailing: WITH_FLAKY,
      expectedSpecCount: EXPECTED_SPECS,
    });

    assert.equal(result.ok, false);
    assert.deepEqual(result.unmatchedListings, [caseKey(SAMPLES, FLAKY)]);
  });

  it("exempts only itself — a different case failing in the same spec still reds the job", () => {
    const results: CaseResult[] = [
      ...withStatus(mainStateResults(), SAMPLES, FONT_CASE, "failed"),
      { spec: SAMPLES, test: FLAKY, status: "failed" },
    ];
    const result = evaluate({ results, knownFailing: WITH_FLAKY, expectedSpecCount: EXPECTED_SPECS });

    assert.equal(result.ok, false);
    assert.deepEqual(result.newFailures, [caseKey(SAMPLES, FONT_CASE)]);
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

// CPE-1680 finding 1: an unrecognised wdio state used to fall through to "skipped", and a skipped case
// is exempt from every clause above — so a state this ratchet has never seen silently became "safe to
// ignore" instead of the honest "I don't know" it actually is. `evaluate()` now treats `"unknown"` as its
// own outcome (clause 6) that reds the run unconditionally; see the module header and the `CaseStatus`
// doc comment in ratchet.ts for why RED (not "warn but stay green") was the chosen default.
describe("evaluate — clause 6: UNRECOGNISED TEST STATE (CPE-1680)", () => {
  it("reds the run when a case reports an unknown state, even though nothing else looks wrong", () => {
    const target = "some case wdio reported a state we've never seen for";
    const results: CaseResult[] = [{ spec: SAMPLES, test: target, status: "unknown" }];

    const result = evaluate({ results, knownFailing: { cases: [] }, expectedSpecCount: 1 });

    assert.equal(result.ok, false);
    assert.deepEqual(result.unknownStates, [caseKey(SAMPLES, target)]);
    assert.ok(result.messages.some((m) => m.includes("UNRECOGNISED TEST STATE") && m.includes(target)));
    // Critically: NOT folded into "skipped" and therefore NOT silently ignored — it's its own outcome,
    // not reachable through the newFailures/fixedButStillListed clauses at all.
    assert.deepEqual(result.newFailures, []);
    assert.deepEqual(result.fixedButStillListed, []);
  });

  it("still reds the run even when the unknown-state case IS listed in known-failing.json", () => {
    // The exact hole this closes: an "I don't know" result can't be verified to be the legitimate
    // "still failing, exemption doing its job" case either — listing it must not launder it back to green.
    const target = "a listed case that mysteriously reports an unrecognised state";
    const knownFailing: KnownFailingFile = {
      cases: [{ spec: SAMPLES, test: target, reason: "usually fails on WebKitGTK", ticket: "CPE-1507" }],
    };
    const results: CaseResult[] = [{ spec: SAMPLES, test: target, status: "unknown" }];

    const result = evaluate({ results, knownFailing, expectedSpecCount: 1 });

    assert.equal(result.ok, false);
    assert.deepEqual(result.unknownStates, [caseKey(SAMPLES, target)]);
  });

  it("an unknown status still proves 'this title exists' for clause 3, same as skipped/pending", () => {
    const target = "opens samples/crypto/cert.der: no crash + preview renders or gracefully degrades";
    const results = withStatus(mainStateResults(), SAMPLES, target, "unknown");

    const result = evaluate({ results, knownFailing: KNOWN_FAILING, expectedSpecCount: EXPECTED_SPECS });

    // Clause 6 reds it, but clause 3 (STALE EXEMPTION) must NOT also fire — the title still exists.
    assert.equal(result.ok, false);
    assert.deepEqual(result.unmatchedListings, []);
    assert.ok(result.unknownStates.includes(caseKey(SAMPLES, target)));
  });
});

// CPE-1680 finding 2 (reviewer nit 3 on PR #864): the NEW GUI REGRESSION message's paste-this-in
// known-failing.json entry used to be a generic ellipsis placeholder, but the risk it was called out for
// is real: any code that string-concatenates a test title inside literal quote marks breaks the moment
// the title itself contains a double quote — exactly what wdio's own global-hook titles do (e.g.
// `"before all" hook"`), which `run-ratchet.ts#loadCaseResults` further wraps as `<hook> "..."` when a
// hook throws, producing `<hook> ""before all" hook""`. `suggestedKnownFailingEntry()` now builds the
// snippet with `JSON.stringify`, so it stays valid JSON no matter how many quotes the title carries.
describe("evaluate — clause 1's suggested entry is valid, pasteable JSON even for a wdio hook-shaped quoted title (CPE-1680)", () => {
  const HOOK_SPEC = "network.smoke.ts";
  // wdio's own literal hook title — the quotes are PART of the string, not added by us.
  const HOOK_TITLE = `"before all" hook`;
  // Exactly what run-ratchet.ts#loadCaseResults synthesises for a THROWING "before all" hook:
  // record(spec, `<hook> "${hook.title}"`, "failed") => `<hook> ""before all" hook""`.
  const HOOK_CASE = `<hook> "${HOOK_TITLE}"`;

  it("suggestedKnownFailingEntry round-trips a quote-containing title as valid JSON", () => {
    const snippet = suggestedKnownFailingEntry(HOOK_SPEC, HOOK_CASE);
    const parsed = JSON.parse(snippet) as { spec: string; test: string };
    assert.equal(parsed.spec, HOOK_SPEC);
    assert.equal(parsed.test, HOOK_CASE);
  });

  it("the NEW GUI REGRESSION message embeds that exact valid JSON snippet for a real failing-hook title", () => {
    const results: CaseResult[] = [{ spec: HOOK_SPEC, test: HOOK_CASE, status: "failed" }];

    const result = evaluate({ results, knownFailing: { cases: [] }, expectedSpecCount: 1 });

    assert.equal(result.ok, false);
    const message = result.messages.find((m) => m.includes("NEW GUI REGRESSION"));
    assert.ok(message, "expected a NEW GUI REGRESSION message");
    const expectedSnippet = suggestedKnownFailingEntry(HOOK_SPEC, HOOK_CASE);
    assert.ok(message!.includes(expectedSnippet));

    // This is the actual bug (CPE-1680): the old ellipsis-placeholder text never embedded the real
    // title, so a quote-containing title never got the chance to break it. Prove the embedded snippet
    // itself is valid, parseable JSON and that it round-trips the exact garbled-looking hook title.
    const snippetStart = message!.indexOf("{");
    assert.ok(snippetStart >= 0);
    const parsed = JSON.parse(message!.slice(snippetStart)) as { spec: string; test: string };
    assert.equal(parsed.spec, HOOK_SPEC);
    assert.equal(parsed.test, HOOK_CASE);
  });
});

// CPE-1680 finding 3 (reviewer nit 5 + independent UAT): `intermittent: true` had no machine-checkable
// bar — the UAT proved it by probing `evaluate()` directly with an entry carrying an EMPTY `reason` and
// EMPTY `ticket`, which sailed through exactly like the four real, well-evidenced entries. A case that
// fails on EVERY run is a permanent break, not intermittent — clause 7 refuses only the structurally
// checkable failure mode (no evidence attached at all); it cannot and does not try to verify flakiness.
describe("evaluate — clause 7: UNEVIDENCED INTERMITTENT (CPE-1680)", () => {
  const SPEC = "probe.smoke.ts";
  const TEST = "a case marked intermittent with no evidence";

  it("rejects the UAT-proven exploit: intermittent:true with an EMPTY reason and EMPTY ticket", () => {
    const knownFailing: KnownFailingFile = {
      cases: [{ spec: SPEC, test: TEST, reason: "", ticket: "", intermittent: true }],
    };
    // Probed directly, exactly as the UAT did — no matching test result needed to prove the hole exists.
    const result = evaluate({ results: [], knownFailing, expectedSpecCount: 0 });

    assert.equal(result.ok, false);
    assert.deepEqual(result.unevidencedIntermittent, [caseKey(SPEC, TEST)]);
    assert.ok(result.messages.some((m) => m.includes("UNEVIDENCED INTERMITTENT") && m.includes(TEST)));
  });

  it("rejects a reason too short to be real evidence, even with a well-formed ticket id", () => {
    const knownFailing: KnownFailingFile = {
      cases: [{ spec: SPEC, test: TEST, reason: "flaky", ticket: "CPE-9999", intermittent: true }],
    };
    const result = evaluate({ results: [], knownFailing, expectedSpecCount: 0 });

    assert.equal(result.ok, false);
    assert.deepEqual(result.unevidencedIntermittent, [caseKey(SPEC, TEST)]);
  });

  it("rejects a real-looking reason with no owning ticket", () => {
    const knownFailing: KnownFailingFile = {
      cases: [
        {
          spec: SPEC,
          test: TEST,
          reason: "observed passing and failing across several unrelated runs",
          ticket: "",
          intermittent: true,
        },
      ],
    };
    const result = evaluate({ results: [], knownFailing, expectedSpecCount: 0 });

    assert.equal(result.ok, false);
    assert.deepEqual(result.unevidencedIntermittent, [caseKey(SPEC, TEST)]);
  });

  it("accepts an entry once it clears the bar: a real-looking reason and a real-looking ticket", () => {
    const knownFailing: KnownFailingFile = {
      cases: [
        {
          spec: SPEC,
          test: TEST,
          reason: "observed passing and failing across several unrelated runs",
          ticket: "CPE-9999",
          intermittent: true,
        },
      ],
    };
    const result = evaluate({
      results: [{ spec: SPEC, test: TEST, status: "failed" }],
      knownFailing,
      expectedSpecCount: 1,
    });

    assert.equal(result.ok, true);
    assert.deepEqual(result.unevidencedIntermittent, []);
  });

  it("does not apply the evidence bar to a plain (non-intermittent) entry", () => {
    const knownFailing: KnownFailingFile = { cases: [{ spec: SPEC, test: TEST, reason: "", ticket: "" }] };
    const result = evaluate({
      results: [{ spec: SPEC, test: TEST, status: "failed" }],
      knownFailing,
      expectedSpecCount: 1,
    });

    assert.deepEqual(result.unevidencedIntermittent, []); // clause 7 is scoped to `intermittent: true` only
    assert.equal(result.ok, true);
  });

  it("the real known-failing.json's four intermittent entries clear the bar unchanged, as committed", () => {
    // The ticket's explicit constraint: this fix must NOT force the four current entries to be rewritten
    // dishonestly to satisfy a format. Load the REAL committed file and prove it as-is.
    const knownFailingPath = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../known-failing.json");
    const knownFailing = JSON.parse(fs.readFileSync(knownFailingPath, "utf-8")) as KnownFailingFile;
    const intermittentEntries = knownFailing.cases.filter((c) => c.intermittent);
    assert.equal(
      intermittentEntries.length,
      4,
      "expected the four documented media-flake entries (CPE-1679) — update this test if that count changes",
    );

    // Feed every listed case as still-failing so only clause 7 (not 1/2/3, which this test isn't about)
    // can produce a message.
    const results: CaseResult[] = knownFailing.cases.map((c) => ({ spec: c.spec, test: c.test, status: "failed" }));
    const specCount = new Set(knownFailing.cases.map((c) => c.spec)).size;

    const result = evaluate({ results, knownFailing, expectedSpecCount: specCount });

    assert.deepEqual(result.unevidencedIntermittent, []);
    assert.ok(!result.messages.some((m) => m.includes("UNEVIDENCED INTERMITTENT")));
  });
});
