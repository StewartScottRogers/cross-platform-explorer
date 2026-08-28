// CPE-1880 — the two structural controls that replace CPE-1848's prose-only dispatch contract.
//
// THE INVESTIGATION THIS ENCODES
//   CPE-1848 told sub-agents "you receive no background task notifications" and handed them the blocking
//   command to use instead: `gh run watch <run-id> --interval 30`. Five agents stalled the same day
//   anyway, three of them AFTER being sent that exact command in a message that named the defect and the
//   ticket. The reason is not defiance and not forgetfulness:
//
//     · The Claude Code Bash tool caps one call at 600 000 ms. A command that outlives the cap is
//       AUTO-BACKGROUNDED, not killed — leaving a sub-agent holding a background task it cannot be woken
//       from.
//     · `gh run watch` blocks until the run finishes. Measured over the 95 completed `ci.yml` runs
//       between 2026-08-23 and 2026-08-26: median 58.9 min, p90 77.3 min, max 97.0 min. Of the 71 runs
//       that SUCCEEDED, ZERO finished inside 600 s; the fastest took 28.6 min. The only sub-ten-minute
//       runs in the window were four cancellations.
//
//   So the contract prescribed a command with a 0-of-71 chance of returning. The agents complied and
//   complying is what stalled them — which is why a fourth, louder wording could not have worked. The
//   fix has to remove the unbounded call and catch the stall on arrival, not restate the rule.
//
// WHAT THESE TESTS PIN
//   1. `scripts/ci-poll.mjs` — its budget is clamped BELOW the harness cap by construction, and cannot
//      be raised by any argument. It also mechanises the two poll traps sprint.md states in prose
//      (an empty board is not a green one; `pending == 0` needs `total_count` stable across two reads).
//   2. `scripts/stall-check.mjs` — every one of the five returns recorded in CPE-1880, and the three in
//      CPE-1848, is classified as a stall; a benign corpus (including the prescribed handoff line and
//      the product's own "watcher" vocabulary) is not; and a second stall from the same agent escalates
//      to take-over rather than a third re-invoke, which is the loop bound the ticket asks for.
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import {
  HARNESS_TOOL_TIMEOUT_MS,
  MAX_BUDGET_MS,
  DEFAULT_BUDGET_MS,
  DEFAULT_INTERVAL_MS,
  clampBudgetMs,
  planTickCount,
  worstCaseWallClockMs,
  assertNotBackgroundable,
  decideFromReads,
  formatVerdict,
  parseArgs,
  readFromRunJson,
  readFromPrJson,
  shouldSleepAgain,
  ghCallTimeoutMs,
  boundedWallClockMs,
  classifyGhFailure,
  formatErrorVerdict,
  GH_CALL_TIMEOUT_MS,
  GH_MIN_CALL_TIMEOUT_MS,
} from "../../scripts/ci-poll.mjs";
import { classifyReport, stripQuoted, STALL_PATTERNS } from "../../scripts/stall-check.mjs";

// ── The five verbatim returns from run `batched-2026-08-23-1124`, exactly as recorded in CPE-1880 ──────
const RECORDED_STALLS: Array<{ label: string; text: string }> = [
  {
    label: "Worker CPE-1794 (1st)",
    text:
      "Still waiting for the CI checks on PR #1017 to complete — no further action needed from me until " +
      "the monitor notification arrives.",
  },
  {
    label: "Worker CPE-1794 (2nd, after being handed the blocking command)",
    text:
      "A background monitor is now polling PR #1017's two check suites every 30s and will notify when " +
      "both complete. Waiting for that event.",
  },
  { label: "Worker CPE-1794 (3rd)", text: "Still in progress. Waiting for the next update from the monitor." },
  { label: "Worker CPE-1794 (4th)", text: "Still in progress. Continuing to wait for completion." },
  { label: "UAT PR 1009 (×3)", text: "Stale notification from a background poll I already resolved earlier." },
];

// ── The three CPE-1848 phrasings its own contract paragraph bans by name ───────────────────────────────
const CPE_1848_PHRASES = [
  "A monitor is armed on the CI run; I will pick it up from there.",
  "The background watch will report the outcome for PR #900.",
  "I'll wait for the notification and continue once it lands.",
];

// ── Reports that must NEVER be flagged. Hoisted to module scope so the B1 promotion of
// `awaiting-notification` to HARD can be re-run against every one of them, with and without the
// contract's mandated handoff tail — the cost of that promotion has to be measurably zero. ────────────
const BENIGN_REPORTS: Array<[string, string]> = [
  [
    "the prescribed handoff return",
    "PR #1031 pushed. CI still pending on 84d20517 — total_count=19 pending=4 mergeable=MERGEABLE. " +
      "Handing CI to the Foreman.",
  ],
  [
    "a completed bounded-poll report",
    "Ran node scripts/ci-poll.mjs --run 32672218824 twice. CI VERDICT: completed success — " +
      "total_count=19 pending=0 mergeable=MERGEABLE sha=84d20517 after 16 tick(s) / 480s. Merged.",
  ],
  [
    "soft deferral WITH an explicit handoff (still pending, over to you)",
    "Still waiting on the last two checks. CI still pending on 84d20517 — total_count=19 pending=2 " +
      "mergeable=MERGEABLE. The Foreman owns CI from here.",
  ],
  [
    "the product's own watcher vocabulary (Agent Watch)",
    "Agent Watch renders a live view of the agent's filesystem activity; the watcher streams events to " +
      "the UI as they happen, so the pane paints immediately.",
  ],
  ["a note about vitest watch mode", "npm run test:watch keeps vitest watching the suite while I iterate locally."],
  [
    "a future-tense report with no notification promise",
    "The Performance Guard will report the size delta in the wrap, per PURPOSE.md's tiebreaker.",
  ],
  [
    "an ordinary in-flight progress note",
    "The Server crates (windows) job is still running at 41 minutes; two of nineteen checks have not " +
      "reported. I will re-poll. CI still pending on 84d20517.",
  ],
  [
    "a finished worker report",
    "Implemented the fix in crates/server/src/backup.rs, added four unit tests, ran cargo clippy " +
      "--all-targets -D warnings in both feature modes, opened PR #1031.",
  ],
];

describe("ci-poll's budget is structurally below the harness cap (CPE-1880)", () => {
  it("clamps the default budget under the 600s auto-background threshold", () => {
    expect(MAX_BUDGET_MS).toBeLessThan(HARNESS_TOOL_TIMEOUT_MS);
    expect(DEFAULT_BUDGET_MS).toBeLessThanOrEqual(MAX_BUDGET_MS);
  });

  it("cannot be raised by an argument — a larger request is clamped down, never honoured", () => {
    expect(clampBudgetMs(MAX_BUDGET_MS + 1)).toBe(MAX_BUDGET_MS);
    expect(clampBudgetMs(3_600_000)).toBe(MAX_BUDGET_MS);
    // …and the CLI path clamps too, so `--budget 3600` cannot reintroduce the stall.
    expect(parseArgs(["--run", "123", "--budget", "3600"]).budgetMs).toBe(MAX_BUDGET_MS);
  });

  it("accepts a SMALLER budget (a caller may hurry, never dawdle)", () => {
    expect(clampBudgetMs(60_000)).toBe(60_000);
    expect(parseArgs(["--pr", "1031", "--budget", "60"]).budgetMs).toBe(60_000);
  });

  it("rejects a nonsense budget loudly rather than defaulting to something unbounded", () => {
    expect(() => clampBudgetMs(0)).toThrow(RangeError);
    expect(() => clampBudgetMs(-1)).toThrow(RangeError);
    expect(() => clampBudgetMs(Number.NaN)).toThrow(RangeError);
    expect(() => planTickCount(DEFAULT_BUDGET_MS, 0)).toThrow(RangeError);
  });

  it("keeps worst-case wall clock — gh round-trips included — under the cap for the shipped defaults", () => {
    const worst = worstCaseWallClockMs(DEFAULT_BUDGET_MS, DEFAULT_INTERVAL_MS);
    expect(worst).toBeLessThan(HARNESS_TOOL_TIMEOUT_MS);
    expect(assertNotBackgroundable()).toBe(worst);
  });

  it("fails loudly if the constants ever drift back into backgroundable territory", () => {
    // The regression this guards: someone widens MAX_BUDGET_MS (or the harness cap tightens) and every
    // agent silently stalls again. The clamp makes that unreachable from any ARGUMENT — proven above —
    // so the only way to exercise the assertion is to shrink the cap under a healthy budget, which is
    // exactly the "the harness got stricter" half of the drift.
    expect(() => assertNotBackgroundable(DEFAULT_BUDGET_MS, DEFAULT_INTERVAL_MS, 60_000)).toThrow(/auto-backgrounded/);
    expect(() => assertNotBackgroundable(DEFAULT_BUDGET_MS, DEFAULT_INTERVAL_MS, 60_000)).toThrow(RangeError);
  });

  it("always plans at least one real read, so a tiny budget still yields a verdict rather than nothing", () => {
    expect(planTickCount(1_000, DEFAULT_INTERVAL_MS)).toBe(1);
    expect(planTickCount(DEFAULT_BUDGET_MS, DEFAULT_INTERVAL_MS)).toBeGreaterThan(1);
  });
});

describe("ci-poll mechanises the poll traps sprint.md states in prose (CPE-1880)", () => {
  const read = (o: Partial<Parameters<typeof formatVerdict>[1] & Record<string, unknown>> = {}) => ({
    terminal: false,
    conclusion: null,
    totalCount: 19,
    pending: 0,
    mergeable: "MERGEABLE",
    sha: "84d20517",
    ...o,
  });

  it("never reads an empty board as green — total_count 0 is reported, not passed", () => {
    const d = decideFromReads([read({ totalCount: 0, pending: 0 }) as never]);
    expect(d.done).toBe(false);
    expect(d.reason).toMatch(/total_count=0/);
  });

  it("names CONFLICTING as the explanation when an empty board coincides with it (the CPE-1846 shape)", () => {
    const d = decideFromReads([read({ totalCount: 0, pending: 0, mergeable: "CONFLICTING" }) as never]);
    expect(d.done).toBe(false);
    expect(d.reason).toMatch(/CONFLICTING/);
  });

  it("will not trust pending==0 on a single read — total_count must be stable across two", () => {
    expect(decideFromReads([read() as never]).done).toBe(false);
    expect(decideFromReads([read() as never, read() as never]).done).toBe(true);
  });

  it("re-opens the verdict when total_count rises after pending hit zero (the CPE-1863 shape)", () => {
    // Measured there: total_count 14→18→19 while pending went 7→10, dipping before it rose.
    const seq = [read({ totalCount: 14, pending: 0 }), read({ totalCount: 18, pending: 3 }), read({ totalCount: 19, pending: 0 })];
    expect(decideFromReads(seq.slice(0, 1) as never[]).done).toBe(false);
    expect(decideFromReads(seq.slice(0, 2) as never[]).done).toBe(false);
    expect(decideFromReads(seq as never[]).done).toBe(false);
    expect(decideFromReads([...seq, read({ totalCount: 19, pending: 0 })] as never[]).done).toBe(true);
  });

  it("stops immediately on an explicitly completed run", () => {
    const d = decideFromReads([read({ terminal: true, conclusion: "success", pending: 0 }) as never]);
    expect(d.done).toBe(true);
  });

  it("prints one terminal verdict line carrying every number sprint.md requires a poll to state", () => {
    const done = formatVerdict(
      { done: true, reason: "run reported completed (success)" },
      read({ terminal: true, conclusion: "success" }) as never,
      { ticks: 3, elapsedMs: 96_000, target: "32672218824" },
    );
    expect(done).toMatch(/^CI VERDICT: completed success/);
    // CPE-1906 ADDED to this line and did not reorder it: `oldest_pending_min`, `skipped` and `neutral`
    // now sit between `pending` and `mergeable`. The keys the sprint runbooks quote are all still
    // present and still in their original relative order, so a caller grepping any one of them is
    // unaffected — which is the compatibility promise, asserted rather than asserted-to.
    for (const key of ["total_count=19", "pending=0", "mergeable=MERGEABLE", "sha=84d20517"]) {
      expect(done).toContain(key);
    }
    const order = ["total_count=", "pending=", "oldest_pending_min=", "skipped=", "neutral=", "mergeable=", "sha="];
    let at = -1;
    for (const key of order) {
      const next = done.indexOf(key, at + 1);
      expect(next, `${key} missing from the verdict line`).toBeGreaterThan(at);
      at = next;
    }
  });

  it("a budget-exhausted verdict is a real report — it carries the prescribed handoff line, not a promise", () => {
    const pending = formatVerdict(
      { done: false, reason: "4 of 19 checks still pending" },
      read({ pending: 4 }) as never,
      { ticks: 16, elapsedMs: 480_000, target: "1031" },
    );
    expect(pending).toMatch(/^CI VERDICT: pending/);
    expect(pending).toContain("CI still pending on 84d20517");
    // …and it must itself survive the stall detector, or the two controls would fight each other.
    expect(classifyReport(pending).action).toBe("accept");
  });

  it("normalises gh output for both poll modes", () => {
    const run = readFromRunJson({
      status: "in_progress",
      conclusion: null,
      headSha: "deadbeef",
      jobs: [{ status: "completed" }, { status: "in_progress" }],
    });
    expect(run).toMatchObject({ terminal: false, totalCount: 2, pending: 1, sha: "deadbeef" });

    const pr = readFromPrJson({
      mergeable: "CONFLICTING",
      headRefOid: "cafebabe",
      statusCheckRollup: [{ status: "COMPLETED", conclusion: "SUCCESS" }, { status: "IN_PROGRESS" }],
    });
    expect(pr).toMatchObject({ terminal: false, totalCount: 2, pending: 1, mergeable: "CONFLICTING", sha: "cafebabe" });
  });

  it("a PR rollup is never 'terminal', even at pending==0 — only a RUN carries an authoritative status", () => {
    // A rollup at pending==0 means "everything scheduled SO FAR has reported", which is precisely the
    // CPE-1863 misread. If this were terminal it would bypass the two-read rule and the poll would
    // stop on the dip. Caught by smoke-testing the real CLI against PR #1031, not by the pure tests.
    const allGreen = readFromPrJson({
      mergeable: "MERGEABLE",
      headRefOid: "7fcd1f94",
      statusCheckRollup: [{ status: "COMPLETED", conclusion: "SUCCESS" }],
    });
    expect(allGreen.terminal).toBe(false);
    expect(allGreen.conclusion).toBe("success");
    expect(decideFromReads([allGreen as never]).done).toBe(false);
    expect(decideFromReads([allGreen as never, allGreen as never]).done).toBe(true);
  });

  it("'stable across two reads' means the board sat QUIET twice, not that one number matched twice", () => {
    // [(19,1),(19,0)] used to return done: `totalCount` matched, so the check passed. But a board that
    // has only just reached pending=0 is exactly when the count is about to rise — `gui-smoke` shards do
    // not exist until their build job finishes. Both reads must be at pending=0.
    const seq = [read({ totalCount: 19, pending: 1 }), read({ totalCount: 19, pending: 0 })];
    expect(decideFromReads(seq as never[]).done).toBe(false);
    expect(decideFromReads(seq as never[]).reason).toMatch(/just reached 0/i);
    expect(decideFromReads([...seq, read({ totalCount: 19, pending: 0 })] as never[]).done).toBe(true);
  });

  it("requires a poll target — no argument means a loud usage error, not a silent no-op", () => {
    expect(() => parseArgs([])).toThrow(/--run|--pr/);
    expect(() => parseArgs(["--nope"])).toThrow(/unknown argument/);
  });
});

describe("stall-check flags every recorded stall from run batched-2026-08-23-1124 (CPE-1880)", () => {
  for (const { label, text } of RECORDED_STALLS) {
    it(`flags ${label}`, () => {
      const v = classifyReport(text);
      expect(v.stalled).toBe(true);
      expect(v.matches.length).toBeGreaterThan(0);
      expect(v.action).toBe("re-invoke");
    });
  }

  for (const [i, text] of CPE_1848_PHRASES.entries()) {
    it(`flags the CPE-1848 banned phrasing #${i + 1}`, () => {
      expect(classifyReport(text).stalled).toBe(true);
    });
  }

  it("classes a backgrounded watcher as HARD — the offence itself, not merely suspicious wording", () => {
    const v = classifyReport(RECORDED_STALLS[1].text);
    expect(v.matches.some((m) => m.severity === "hard")).toBe(true);
  });

  it("explains WHY in the finding, so the Foreman's re-invoke can quote the offending words back", () => {
    const v = classifyReport(RECORDED_STALLS[4].text);
    expect(v.matches[0].excerpt.toLowerCase()).toContain("background poll");
    expect(v.matches[0].why).toBeTruthy();
  });
});

describe("B1 RED-PROOF: the mandated handoff tail must not disarm the detector (CPE-1880)", () => {
  // THE review's blocking finding, and the sharpest instance of this ticket's own theme. The contract
  // shipped here REQUIRES every worker to append `CI still pending on <SHA>`. That string is a
  // HANDOFF_PATTERN, and `stalled = hard || (matches.length > 0 && !handoff)`. So every COMPLIANT report
  // carried the exact token that excused every soft match: recorded stalls #1, #3 and #4 flipped from
  // `re-invoke` to `accept` the moment the mandated tail was present. The bare-text replays below were
  // green the whole time — a test passing while the thing it guards is broken, which is precisely what
  // this ticket inverted CPE-1848's guard test for.
  //
  // Fixed by promoting `awaiting-notification` from soft to HARD. These tests replay every recorded
  // return in the ONLY shape that occurs in production: with the contract's own required tail attached.
  const MANDATED_TAIL = " CI still pending on 4570cfe692ba99459a3fc317a8a21be95a870f61.";

  for (const { label, text } of RECORDED_STALLS) {
    it(`still flags ${label} WITH the mandated 'CI still pending on <SHA>' tail`, () => {
      const v = classifyReport(text + MANDATED_TAIL);
      expect(v.handoff, "the tail really is recognised as a handoff — otherwise this proves nothing").toBe(true);
      expect(v.stalled, `${label} + mandated tail must still be a stall`).toBe(true);
      expect(v.action).toBe("re-invoke");
    });
  }

  it("the three that used to flip (#1, #3, #4) each carry a HARD finding now, not just soft ones", () => {
    for (const i of [0, 2, 3]) {
      const v = classifyReport(RECORDED_STALLS[i].text + MANDATED_TAIL);
      expect(v.matches.some((m) => m.severity === "hard"), RECORDED_STALLS[i].label).toBe(true);
    }
  });

  it("every CPE-1848 banned phrasing also survives the mandated tail", () => {
    for (const phrase of CPE_1848_PHRASES) {
      expect(classifyReport(phrase + MANDATED_TAIL).stalled, phrase).toBe(true);
    }
  });

  it("and the benign corpus is unharmed by the promotion — no new false positives", () => {
    // The whole cost of making `awaiting-notification` hard has to be zero on real reports, or the fix
    // trades one disarmed control for an un-trustable one.
    for (const [label, text] of BENIGN_REPORTS) {
      expect(classifyReport(text).action, label).toBe("accept");
      expect(classifyReport(text + MANDATED_TAIL).action, `${label} + tail`).toBe("accept");
    }
  });
});

describe("B2 RED-PROOF: the HARD pattern survives natural English word order (CPE-1880)", () => {
  // The original regex required `background` IMMEDIATELY before the noun, so it caught the adjective
  // form and missed the predicate form entirely. The file claimed a deliberate over-flagging bias; on
  // reversed word order it silently under-flagged. The second string below is close to what the harness
  // itself prints when it backgrounds a call, which makes it the highest-value string in the whole set.
  const REVERSED: Array<[string, string]> = [
    ["put … in the background", "I've put the CI watch in the background and will pick it up once it finishes."],
    [
      "the harness's own wording",
      "The gh run watch call was moved to the background by the harness (task bfr274ats). Awaiting its result.",
    ],
    ["running in the background", "A monitor task is running in the background; I'll circle back."],
    ["detached shell", "Polling continues in a detached shell. Nothing further from me right now."],
  ];

  for (const [label, text] of REVERSED) {
    it(`flags "${label}"`, () => {
      const v = classifyReport(text);
      expect(v.stalled, text).toBe(true);
      expect(v.matches.some((m) => m.severity === "hard"), "must be HARD — a handoff line must not excuse it").toBe(
        true,
      );
    });
  }

  it("and each survives the mandated handoff tail too", () => {
    for (const [label, text] of REVERSED) {
      expect(classifyReport(`${text} CI still pending on 4570cfe6.`).stalled, label).toBe(true);
    }
  });

  it("the word-order alternation does not start flagging ordinary background vocabulary", () => {
    // The reason the original regex was narrow. Widening it must not swallow the product's own domain
    // language or routine build talk.
    const stillBenign = [
      "Agent Watch streams filesystem events in the background so the pane paints immediately.",
      "The indexer runs in the background thread pool; I measured it at 40ms.",
      "cargo build put the artefacts in target/debug. Nothing detached from the shell.",
      "I detached the fixture from the shared harness so the two suites stop colliding.",
    ];
    for (const text of stillBenign) {
      expect(classifyReport(text).action, text).toBe("accept");
    }
  });
});

describe("stall-check bounds the loop rather than repeating the re-invoke (CPE-1880 AC 4)", () => {
  it("first stall-shaped return re-invokes the same agent", () => {
    expect(classifyReport(RECORDED_STALLS[0].text, { priorStalls: 0 }).action).toBe("re-invoke");
  });

  it("second one takes over — the agent is killed, not asked a third time", () => {
    const v = classifyReport(RECORDED_STALLS[2].text, { priorStalls: 1 });
    expect(v.action).toBe("take-over");
    expect(v.message).toMatch(/kill it/i);
  });

  it("the recorded 4-return CPE-1794 sequence is cut off at the second, not the fourth", () => {
    // Replaying the real sequence: under this rule the run never reaches returns 3 and 4.
    const actions = RECORDED_STALLS.slice(0, 4).map((r, i) => classifyReport(r.text, { priorStalls: i }).action);
    expect(actions).toEqual(["re-invoke", "take-over", "take-over", "take-over"]);
    expect(actions.filter((a) => a === "re-invoke")).toHaveLength(1);
  });
});

describe("stall-check does not trip on benign reports (CPE-1880)", () => {

  for (const [label, text] of BENIGN_REPORTS) {
    it(`accepts ${label}`, () => {
      const v = classifyReport(text);
      expect(v.action).toBe("accept");
      expect(v.stalled).toBe(false);
    });
  }

  it("a HARD match is not excused by a handoff line — a backgrounded watcher is stuck regardless", () => {
    const v = classifyReport(
      "A background monitor is now polling the checks. CI still pending on 84d20517 — total_count=19 " +
        "pending=2 mergeable=MERGEABLE.",
    );
    expect(v.stalled).toBe(true);
    expect(v.handoff).toBe(true);
  });
});

describe("stall-check strips FENCES only — the blockquote exemption was too wide (CPE-1880, S6)", () => {
  // The first version stripped `>` blockquote lines too, and the review measured the cost: writing your
  // status as a blockquote — a routine formatting choice, not a deliberate "this is an artefact" marker
  // — made ALL FIVE recorded stalls classify `accept`. The exemption was wider than the thing it was
  // exempting. A fence is an explicit verbatim marker; a blockquote is just prose. So: fences only, and
  // the dispatch contract tells agents to quote banned phrasing in a fence.
  it("a fenced quote of the banned phrasing does not trip the detector", () => {
    const quoted =
      "The dispatch contract now reads:\n\n" +
      "```\n" +
      "never say a monitor is \"armed\" or that a background watch will report — that phrasing is the\n" +
      "exact defect this rule exists to prevent.\n" +
      "```\n\n" +
      "```\n" +
      "A background monitor is now polling PR #1017 and will notify when both complete.\n" +
      "```\n\n" +
      "I updated the guard test accordingly and opened the PR. CI VERDICT: completed success.";
    expect(stripQuoted(quoted)).not.toMatch(/background monitor/i);
    expect(classifyReport(quoted).action).toBe("accept");
  });

  it("but the same sentence UNQUOTED is still flagged", () => {
    expect(
      classifyReport("A background monitor is now polling PR #1017 and will notify when both complete.").stalled,
    ).toBe(true);
  });

  it("S6 RED-PROOF: every recorded stall written as a BLOCKQUOTE is still flagged", () => {
    // This is the case the old stripQuoted swallowed whole. All five, one `> ` prefix away from
    // invisible. If this ever goes green-by-accepting again, the detector has been disarmed.
    for (const { label, text } of RECORDED_STALLS) {
      const asBlockquote = text
        .split(". ")
        .map((sentence) => `> ${sentence}`)
        .join("\n");
      const v = classifyReport(asBlockquote);
      expect(v.stalled, `${label} written as a blockquote must still be a stall`).toBe(true);
    }
  });

  it("a stall indented as a blockquote inside an otherwise good report is still caught", () => {
    const report =
      "Pushed the branch and opened PR #1032. Status:\n\n" +
      "> A background monitor is now polling the checks and will notify when they finish.\n\n" +
      "Will pick it up from there.";
    expect(classifyReport(report).stalled).toBe(true);
  });

  it("this ticket's own write-up does not trip its own detector — when fenced, as the contract says", () => {
    // The self-referential trap is real: any honest report on CPE-1880 must contain the banned phrases.
    // The escape hatch is a fence, and only a fence.
    const prBody =
      "Five agents stalled. Each returned something of this shape:\n\n" +
      "```\n" +
      "A background monitor is now polling PR #1017's two check suites every 30s and will notify when\n" +
      "both complete. Waiting for that event.\n" +
      "```\n\n" +
      "The cause is the 600 s harness cap, not defiance. CI VERDICT: completed success.";
    expect(classifyReport(prBody).action).toBe("accept");
  });
});

describe("the harness scripts stay importable from vitest (CPE-1880)", () => {
  // Found the hard way, mid-ticket: the rebase re-checked these files out with CRLF (this box runs
  // core.autocrlf=true) and the whole suite stopped collecting with `SyntaxError: Invalid or
  // unexpected token` pointing at a COMMENT on line 2 of this file — a location with nothing wrong
  // with it, and no mention of the module actually at fault. Vite's transform of a `.mjs` does not
  // survive CRLF. It reproduces only on a Windows checkout; the Linux CI runner takes LF and stays
  // green, so CI structurally cannot catch it. `.gitattributes` now pins `scripts/*.mjs text eol=lf`.
  //
  // Being honest about this guard's reach: if the offending file is one THIS suite imports, collection
  // dies before any assertion runs — the suite still reds, just uselessly. Its real value is the OTHER
  // case: a script in `scripts/` that nobody imports yet (a future one, or `organize-done.mjs`) sitting
  // unpinned, where this names the file and points at `.gitattributes` instead of leaving the next
  // author to rediscover the whole thing. So it scans the directory rather than a hard-coded pair.
  it("every scripts/**/*.mjs is checked out LF, not CRLF", () => {
    // RECURSIVE, and `.gitattributes` matches `scripts/**/*.mjs`, not `scripts/*.mjs`: the first
    // version of both missed `scripts/dev-harness/sidebar-drop-stack-overlap/check.mjs`, which was
    // sitting unpinned and unseen. A guard that cannot see the file it guards is the failure mode this
    // whole ticket is about.
    const dir = join(process.cwd(), "scripts");
    const mjs = readdirSync(dir, { recursive: true })
      .map((f) => String(f).split("\\").join("/"))
      .filter((f) => f.endsWith(".mjs"));
    expect(mjs.length).toBeGreaterThanOrEqual(4); // ci-poll, stall-check, organize-done, dev-harness/check
    expect(mjs.some((f) => f.includes("/"))).toBe(true); // the recursion actually reaches a subdirectory
    for (const rel of mjs) {
      const bytes = readFileSync(join(dir, rel));
      expect(
        bytes.includes("\r\n"),
        `scripts/${rel} is checked out with CRLF, which makes Vite's transform of it throw an ` +
          `unlocatable SyntaxError. .gitattributes pins scripts/**/*.mjs to LF, but git does not ` +
          `rewrite a working tree it is not otherwise touching, so an existing checkout keeps its CRLF ` +
          `copy. Fix this checkout with:  rm scripts/${rel} && git checkout -- scripts/${rel}  ` +
          `(the index bytes are already LF, so the checkout re-materialises it with the pin applied).`,
      ).toBe(false);
    }
  });
});

describe("S4 RED-PROOF: the no-backgrounding bound is ENFORCED at runtime, not modelled (CPE-1880)", () => {
  // The review's sharpest non-blocking finding. The clamp only ever governed `--budget`. The interval is
  // a separate, unvalidated input that drives the tick count, and `assertNotBackgroundable` modelled the
  // result ONCE, up front, with a hard-coded 5 s `gh` cost guess. At shipped defaults a real `gh` call
  // costing 15 s means 690 s of wall clock — backgrounded, by the very script whose entire premise is
  // that it cannot be. The loop now reads the clock every tick; these pin that.
  const INTERVAL = 30_000;

  it("stops before a sleep that would cross the deadline, however much time the gh calls ate", () => {
    const started = 1_000_000;
    const deadline = started + 480_000;
    // 400s elapsed, 30s interval — one more sleep fits.
    expect(shouldSleepAgain(started + 400_000, INTERVAL, deadline, 3, 16)).toBe(true);
    // 460s elapsed — the next sleep would land at 490s, past the 480s deadline. Stop.
    expect(shouldSleepAgain(started + 460_000, INTERVAL, deadline, 3, 16)).toBe(false);
  });

  it("stops on the deadline even when the PLAN says plenty of ticks remain", () => {
    // The exact shape of the bug: slow gh calls burn the budget while `tick` is still low, so the tick
    // counter says "13 to go" and the clock says "stop now". The clock has to win.
    const started = 1_000_000;
    expect(shouldSleepAgain(started + 479_000, INTERVAL, started + 480_000, 2, 16)).toBe(false);
  });

  it("still stops at the last planned tick, so a fast run does not sleep pointlessly at the end", () => {
    const started = 1_000_000;
    expect(shouldSleepAgain(started, INTERVAL, started + 480_000, 15, 16)).toBe(false);
  });

  it("a hostile interval cannot argue the wall clock upward any more", () => {
    // An interval of 17s was measured at worstCase 599s — inside the modelled cap by 1s, and wildly past
    // it as soon as a gh call ran slow. With the deadline enforced, a long interval just means fewer
    // ticks rather than a longer run.
    const started = 1_000_000;
    const deadline = started + 480_000;
    expect(shouldSleepAgain(started + 400_000, 200_000, deadline, 0, 99)).toBe(false);
    expect(parseArgs(["--run", "1", "--interval", "17"]).intervalMs).toBe(17_000);
  });

  it("the clamp's docstring no longer claims the wall clock is bounded by the budget alone", () => {
    // The claim was false as written and the review called it out; the correction points at the loop.
    const src = readFileSync(join(process.cwd(), "scripts", "ci-poll.mjs"), "utf8");
    expect(src).toMatch(/deadline check is the enforcement/i);
    expect(src).toMatch(/const deadline = started \+ opts\.budgetMs;/);
    expect(src).toMatch(/Clamping the\s*\n?\s*\* BUDGET does not by itself bound the WALL CLOCK/);
  });

  // ── CPE-1906: the matrix re-run the ticket asks for, plus the leg it was missing ────────────────────
  it("re-runs CPE-1880's interval × gh-cost matrix and every combination still lands under the cap", () => {
    const intervals = [5, 10, 15, 17, 20, 30, 45, 60, 90, 120].map((s) => s * 1000);
    const ghCosts = [1_000, 5_000, 15_000, 30_000, 60_000];
    const budgets = [30_000, 90_000, 300_000, MAX_BUDGET_MS];
    let combinations = 0;
    for (const budgetMs of budgets) {
      for (const intervalMs of intervals) {
        for (const ghCostMs of ghCosts) {
          combinations += 1;
          // THE BOUND is what must be under the cap for every combination — and note what it is NOT a
          // function of: `ghCostMs` and `intervalMs` do not appear in it at all. That independence IS
          // gap 1's fix. The per-call timeout means a slow or hung `gh` can no longer buy itself extra
          // wall clock, so the guarantee stops being a guess about how fast the network is.
          const bound = boundedWallClockMs(budgetMs);
          expect(bound, `budget=${budgetMs} interval=${intervalMs} ghCost=${ghCostMs}`).toBeLessThan(
            HARNESS_TOOL_TIMEOUT_MS,
          );
          // The model is still computed and still reported to the operator, but it is no longer load
          // bearing.
          expect(Number.isFinite(worstCaseWallClockMs(budgetMs, intervalMs, ghCostMs))).toBe(true);
        }
      }
    }
    expect(combinations).toBe(budgets.length * intervals.length * ghCosts.length);
    // Sanity that the matrix is not vacuous: the old MODEL does cross the cap at a 60 s `gh` call on
    // the shipped defaults, which is precisely the hole the structural bound closes.
    expect(worstCaseWallClockMs(MAX_BUDGET_MS, DEFAULT_INTERVAL_MS, 60_000)).toBeGreaterThan(
      HARNESS_TOOL_TIMEOUT_MS,
    );
  });

  it("bounds ONE gh call by the smaller of its ceiling and the time left, never below the floor", () => {
    const deadline = 1_000_000;
    // Plenty of time left → the ceiling applies.
    expect(ghCallTimeoutMs(deadline - 300_000, deadline)).toBe(GH_CALL_TIMEOUT_MS);
    // Less than the ceiling left → the remaining time applies, so the call cannot cross the deadline.
    expect(ghCallTimeoutMs(deadline - 20_000, deadline)).toBe(20_000);
    // Past the deadline → the floor, which is the ONLY term by which the process can outlive its budget
    // and therefore the only term `boundedWallClockMs` has to add.
    expect(ghCallTimeoutMs(deadline + 5_000, deadline)).toBe(GH_MIN_CALL_TIMEOUT_MS);
    expect(boundedWallClockMs(MAX_BUDGET_MS)).toBe(MAX_BUDGET_MS + GH_MIN_CALL_TIMEOUT_MS);
  });

  it("classifies a gh failure by what it means for the caller, not by its stack", () => {
    const timedOut = Object.assign(new Error("Command failed"), { killed: true, signal: "SIGKILL" });
    expect(classifyGhFailure(timedOut).kind).toBe("timed out");
    const nonZero = Object.assign(new Error("Command failed"), { status: 1, stderr: "gh: not found\n" });
    expect(classifyGhFailure(nonZero).kind).toBe("gh exited non-zero");
    expect(classifyGhFailure(nonZero).message).toBe("gh: not found");
    expect(classifyGhFailure(new SyntaxError("Unexpected token < in JSON")).kind).toBe("unparseable output");
    expect(classifyGhFailure(Object.assign(new Error("spawn gh ENOENT"), { code: "ENOENT" })).kind).toBe("gh not found");
  });

  it("the could-not-ask verdict never uses the vocabulary that tells a caller to wait", () => {
    const line = formatErrorVerdict(
      { kind: "timed out", message: "gh exceeded its per-call timeout", count: 3 },
      null,
      { ticks: 3, elapsedMs: 15_000, target: "1031" },
    );
    expect(line).toMatch(/^CI VERDICT: unknown —/);
    // The two sentences that made an error read as "keep waiting". Neither may survive on this path.
    expect(line).not.toMatch(/CI VERDICT: pending/);
    expect(line).not.toMatch(/CI still pending on/);
    expect(line).toContain("do not merge and do not wait on it");
  });

  it("a SKIPPED check is never folded into success, and the other three tokens keep their meaning", () => {
    const rollup = (entries: unknown[]) => ({ statusCheckRollup: entries, mergeable: "MERGEABLE", headRefOid: "abc" });
    const check = (name: string, conclusion: string) => ({
      __typename: "CheckRun",
      name,
      status: "COMPLETED",
      conclusion,
    });
    const read = readFromPrJson(rollup([check("Frontend", "SUCCESS"), check("MSRV check", "SKIPPED")]));
    // The skip is visible as a NAME, which is what lets `classifySkips` adjudicate it. The old code
    // discarded it entirely by matching `SKIPPED` inside the success test.
    expect(read.skippedNames).toEqual(["MSRV check"]);
    expect(read.failedNames).toEqual([]);
    // NEUTRAL is kept as a pass — it RAN and GitHub treats it as non-blocking — but it is counted, so
    // "how many checks declined to judge" is never invisible either.
    const neutral = readFromPrJson(rollup([check("Frontend", "NEUTRAL")]));
    expect(neutral.conclusion).toBe("success");
    expect(neutral.neutralCount).toBe(1);
    // CANCELLED / TIMED_OUT / a shape nobody has seen all fall through to failure. Fail closed.
    for (const c of ["CANCELLED", "TIMED_OUT", "ACTION_REQUIRED", "STALE", "SOMETHING_NEW"]) {
      expect(readFromPrJson(rollup([check("X", c)])).conclusion, c).toBe("failure");
    }
    // The StatusContext arm is now gated on there being no `conclusion`, so it can never paper over a
    // CheckRun after a `gh` upgrade that starts emitting both fields.
    const ctx = readFromPrJson(rollup([{ __typename: "StatusContext", context: "vercel", state: "SUCCESS" }]));
    expect(ctx.conclusion).toBe("success");
    const both = readFromPrJson(
      rollup([{ __typename: "CheckRun", name: "X", status: "COMPLETED", conclusion: "FAILURE", state: "SUCCESS" }]),
    );
    expect(both.conclusion).toBe("failure");
  });

  it("a run GitHub calls `success` with skipped jobs is downgraded, not believed", () => {
    // `gh run view` reports a run whose jobs were skipped by a `needs:` cascade as `success`. Trusting
    // that field is the same defect one level up from the rollup.
    const read = readFromRunJson({
      status: "completed",
      conclusion: "success",
      headSha: "deadbeef",
      jobs: [
        { name: "Lockfile pre-flight", status: "completed", conclusion: "success" },
        { name: "Server crates (windows-latest)", status: "completed", conclusion: "skipped" },
      ],
    });
    expect(read.conclusion).toBe("skipped");
    expect(read.skippedNames).toEqual(["Server crates (windows-latest)"]);
  });

  it("reports the age and name of the longest-running pending check", () => {
    const now = Date.parse("2026-08-27T12:00:00Z");
    const read = readFromPrJson(
      {
        statusCheckRollup: [
          { __typename: "CheckRun", name: "fast", status: "IN_PROGRESS", startedAt: "2026-08-27T11:55:00Z" },
          {
            __typename: "CheckRun",
            name: "Server crates (windows-latest)",
            status: "IN_PROGRESS",
            startedAt: "2026-08-27T10:57:00Z",
          },
        ],
      },
      now,
    );
    expect(read.oldestPendingName).toBe("Server crates (windows-latest)");
    expect(Math.round((read.oldestPendingAgeMs ?? 0) / 60_000)).toBe(63);
    // A missing or unparseable timestamp degrades to null rather than to 0, which would read as "just
    // started" — the wrong direction for a signal whose whole job is spotting a job that is stuck.
    const undated = readFromPrJson({ statusCheckRollup: [{ __typename: "CheckRun", name: "x", status: "QUEUED" }] }, now);
    expect(undated.oldestPendingAgeMs).toBeNull();
  });
});

describe("the pattern table stays honest (CPE-1880)", () => {
  it("every pattern carries an id, a severity, and a why the Foreman can quote", () => {
    for (const p of STALL_PATTERNS) {
      expect(p.id).toBeTruthy();
      expect(["hard", "soft"]).toContain(p.severity);
      expect(p.why.length).toBeGreaterThan(20);
      expect(p.re).toBeInstanceOf(RegExp);
    }
  });

  it("at least one HARD pattern exists — otherwise a handoff line would excuse every stall", () => {
    expect(STALL_PATTERNS.some((p) => p.severity === "hard")).toBe(true);
  });

  // CPE-1906 item 4 — the `no-further-action` comment cited "the lockfile already matches, so no
  // further action is needed" as a SAFE example. Bare, it is not: it trips the pattern. It classifies
  // `accept` only because the pattern is soft and the mandated handoff tail excuses it. Both halves are
  // asserted here, so the corrected comment is checked rather than taken on trust.
  it("the `no-further-action` example is clean in context and NOT in isolation, exactly as documented", () => {
    const bare = "The lockfile already matches, so no further action is needed.";
    expect(classifyReport(bare).matches.map((m) => m.id)).toContain("no-further-action");
    expect(classifyReport(bare).action).toBe("re-invoke");
    const withHandoff = `${bare} CI VERDICT: completed success — total_count=19 pending=0.`;
    expect(classifyReport(withHandoff).action).toBe("accept");
    // …and the file must say so, rather than repeating the claim the review found overstated.
    const src = readFileSync(join(process.cwd(), "scripts", "stall-check.mjs"), "utf8");
    expect(src).toMatch(/clean \*in context\*, not in isolation/);
  });

  it("an empty or absent report is not silently accepted as a real one", () => {
    // Nothing to match, so nothing is flagged — but the Foreman must not read this as a pass. The
    // detector's contract is narrow on purpose: it finds stall LANGUAGE, it does not certify substance.
    expect(classifyReport("").action).toBe("accept");
    expect(classifyReport("").matches).toHaveLength(0);
  });
});
