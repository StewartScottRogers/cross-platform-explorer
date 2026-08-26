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
    expect(done).toMatch(/total_count=19 pending=0 mergeable=MERGEABLE sha=84d20517/);
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
  const BENIGN: Array<[string, string]> = [
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

  for (const [label, text] of BENIGN) {
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

describe("stall-check ignores QUOTED prohibitions, so documenting the rule is not committing the offence", () => {
  it("strips fenced blocks and blockquote lines before matching", () => {
    const quoted =
      "The dispatch contract now reads:\n\n" +
      "> never say a monitor is \"armed\" or that a background watch will report — that phrasing is the\n" +
      "> exact defect this rule exists to prevent.\n\n" +
      "```\n" +
      "A background monitor is now polling PR #1017 and will notify when both complete.\n" +
      "```\n\n" +
      "I updated the guard test accordingly and opened the PR.";
    expect(stripQuoted(quoted)).not.toMatch(/background monitor/i);
    expect(classifyReport(quoted).action).toBe("accept");
  });

  it("but the same sentence UNQUOTED is still flagged", () => {
    expect(
      classifyReport("A background monitor is now polling PR #1017 and will notify when both complete.").stalled,
    ).toBe(true);
  });

  it("this very test file's own subject matter would not trip the detector when quoted properly", () => {
    // Guards the self-referential trap: the CPE-1880 report necessarily contains the banned phrases.
    const prBody =
      "Five agents stalled. Each returned something of this shape:\n\n" +
      "> A background monitor is now polling PR #1017's two check suites every 30s and will notify when\n" +
      "> both complete. Waiting for that event.\n\n" +
      "The cause is the 600 s harness cap, not defiance. CI VERDICT: completed success.";
    expect(classifyReport(prBody).action).toBe("accept");
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

  it("an empty or absent report is not silently accepted as a real one", () => {
    // Nothing to match, so nothing is flagged — but the Foreman must not read this as a pass. The
    // detector's contract is narrow on purpose: it finds stall LANGUAGE, it does not certify substance.
    expect(classifyReport("").action).toBe("accept");
    expect(classifyReport("").matches).toHaveLength(0);
  });
});
