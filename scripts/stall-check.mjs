#!/usr/bin/env node
// CPE-1880 — mechanical detection of a STALLED sub-agent report, and a bounded response to it.
//
// THE FAILURE THIS CLOSES
//   A dispatched sub-agent receives no background task notifications. When a call it made is
//   auto-backgrounded by the harness (see `scripts/ci-poll.mjs` for the measurement), the agent has a
//   pending background task and no way to be woken, so it returns a report that LOOKS like a status
//   update and is in fact a dead end: "a background monitor is now polling …, waiting for that event."
//
//   That is this repo's most-repeated defect shape — a step that fails while looking exactly like a step
//   that succeeded. The Foreman reads "a monitor is armed", concludes "in flight and fine", and waits on
//   an agent that will never speak again. Worse, it LOOPS: each stale wake produces another
//   "still waiting", which produces another wake. Run `batched-2026-08-23-1124` recorded one agent doing
//   this four times before it had to be killed.
//
//   CPE-1848 addressed it with prose telling agents not to do this. The agents complied and stalled
//   anyway, because the command that prose PRESCRIBED was itself unbounded. So the response cannot rest
//   on the agent noticing its own stall; it has to be checked on arrival, by the Foreman, mechanically.
//
// WHAT THIS IS
//   A pure classifier over a returned report plus the count of stalls that agent has already produced.
//   It answers one question with three possible values:
//
//     accept     — a real report; proceed.
//     re-invoke  — stall-shaped, first offence. SendMessage the same agent: "report now, synchronously,
//                  with what you have. I own CI." (Observed live: the material was always already there.)
//     take-over  — stall-shaped a SECOND time from the same agent. Do NOT re-invoke again. Kill it and
//                  read its PR yourself. This bound is what stops the loop; it is the whole point.
//
// BIAS
//   Deliberately tuned to over-flag rather than under-flag. A false positive costs one re-invoke (the
//   agent restates a report it already has). A false negative costs a hung agent, a stalled batch
//   counter, and a full Foreman round-trip to recover. Those are not close.
//
// QUOTING — fences only, and that boundary was moved deliberately
//   FENCED CODE BLOCKS are stripped before matching, so a report that quotes the dispatch contract (or
//   this file) verbatim is not mistaken for one committing the offence. `>` BLOCKQUOTES are NOT stripped
//   any more: the review measured that exemption swallowing **all five** recorded stalls the moment a
//   status was written as a blockquote, which is a routine formatting choice rather than a deliberate
//   "this is an artefact" marker. To quote banned phrasing safely, put it in a fence — the dispatch
//   contract says so.
//
// USAGE
//   node scripts/stall-check.mjs report.txt            # classify a file
//   … | node scripts/stall-check.mjs                   # classify stdin
//   node scripts/stall-check.mjs report.txt --prior 1  # this agent has already stalled once
//
// EXIT CODES
//   0 accept · 3 re-invoke · 4 take-over · 64 bad usage
//
// The pure functions are exported and unit-tested by `src/lib/sprintStallControls.test.ts` against the
// VERBATIM returns recorded in CPE-1880 and CPE-1848, plus a benign corpus that must not trip.

import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

/**
 * A `hard` family is the offence itself — an agent that says it has put a monitor/watch/poll into the
 * background has already lost the ability to finish, and no amount of surrounding detail changes that.
 * A `soft` family is contentless deferral, which a real handoff line can legitimately explain away.
 *
 * @typedef {object} StallPattern
 * @property {string} id
 * @property {"hard"|"soft"} severity
 * @property {RegExp} re
 * @property {string} why
 */

/** @type {StallPattern[]} */
export const STALL_PATTERNS = [
  {
    id: "backgrounded-watcher",
    severity: "hard",
    // Three word orders, because English supplies all three and the review found this file over-flagging
    // on one while UNDER-flagging on the others:
    //   adjective — "A background monitor is now polling…" · "a background poll I already resolved"
    //   predicate — "put the CI watch in the background" · "was moved to the background by the harness"
    //   detached  — "polling continues in a detached shell"
    // The predicate form matters most: it is close to what the harness itself prints when it backgrounds
    // a call ("moved to background, task bfr274ats"), so it is the single highest-value string in the
    // set — and the original regex, which required `background` immediately before the noun, missed
    // every instance of it.
    re: /\bbackground(ed)?\s+(monitor|watch(er)?|poll(er|ing)?|task|job|process)\b|(?=[^.\n]*\b(monitor|watch|watcher|poll|poller|polling|task|job|process|call|command)\b)(?=[^.\n]*\b(in|to|into)\s+the\s+background\b)(?=[^.\n]*\b(put|move[ds]?|left|kept|spawned|started|parked|backgrounded|running|runs|continues?|continuing|is|are|was|were|been)\b)[^.\n]+|\bdetached\s+(shell|process|task|job|terminal)\b|\b(run(ning)?|continu(e|es|ing)|poll(ing)?|watch(ing)?)\b[^.\n]{0,30}\bdetached\b/i,
    why: "names a monitor/watch/poll running in the background — a sub-agent cannot be woken by one",
  },
  {
    id: "armed-monitor",
    severity: "hard",
    // "a monitor is armed" · "the monitor has been armed" · "armed a background watch"
    re: /\b(monitor|watch(er)?|poll(er)?)\b[^.\n]{0,30}\b(is|has been|was|now)\s+armed\b|\barmed\s+(a|the)\s+(monitor|watch(er)?|poll(er)?)\b/i,
    why: "describes arming a monitor — arming feels like progress and produces nothing",
  },
  {
    id: "monitor-will-notify",
    severity: "hard",
    // "…and will notify when both complete" · "will report when the checks finish"
    re: /\b(will|i'?ll|it'?ll)\s+(notify|report|update|let you know|ping)\b[^.\n]{0,60}\b(when|once|as soon as|after)\b/i,
    why: "promises a future notification the agent has no mechanism to deliver",
  },
  {
    id: "awaiting-notification",
    // HARD, and this was the review's blocking finding — the two controls were disarming each other.
    // The contract this ships MANDATES that every worker append `CI still pending on <SHA>`, and that
    // string is a HANDOFF_PATTERN. So every compliant report carried the exact token that excused every
    // soft match, and recorded stalls #1, #3 and #4 flipped from `re-invoke` to `accept` the moment the
    // mandated tail was appended. The bare-text tests were green while the thing they guard was broken
    // — the same shape this ticket inverted CPE-1848's guard test for.
    //
    // A wait keyed to a notification/monitor/event/wake-up is something a sub-agent structurally cannot
    // receive, so no amount of good write-up around it changes the outcome. Nothing excuses it. Verified
    // against all eight benign corpus entries: zero trip, including "Still waiting on the last two
    // checks. CI still pending on 84d20517…", which carries no such keyword.
    severity: "hard",
    // "until the monitor notification arrives" · "Waiting for that event." ·
    // "Waiting for the next update from the monitor." · "I'll wait for the notification"
    re: /\b(wait(ing)?|await(ing)?)\b[^.\n]{0,50}\b(notification|monitor|that event|the event|wake-?up|signal|the next update)\b|\b(monitor|background|harness|poll|watch)\s+notification\b|\bnotification\s+(arrives|lands|fires|comes|returns|shows up)\b/i,
    why: "defers to a signal a sub-agent cannot receive — nothing elsewhere in a report excuses it",
  },
  {
    id: "contentless-progress",
    severity: "soft",
    // "Still in progress." · "Still waiting for the CI checks … to complete"
    // NOT "still running" — a suite legitimately still running is a fact an agent reports mid-work, and
    // flagging it would fire on ordinary progress notes rather than on the stall.
    re: /\bstill\s+(in progress|waiting)\b/i,
    why: "reports progress with no result — the shape a stall takes once the agent has nothing left to do",
  },
  {
    id: "continuing-to-wait",
    // HARD. Recorded stall #4 — "Still in progress. Continuing to wait for completion." — contains no
    // notification keyword at all, so promoting `awaiting-notification` did not reach it, and with the
    // contract's mandated handoff tail appended it classified `accept`. A sub-agent cannot "continue to
    // wait": it has no loop of its own to continue in and no signal to be resumed by. The phrase is
    // categorically a stall, whatever else the report says.
    severity: "hard",
    // "Continuing to wait for completion." · "I'll continue waiting."
    re: /\bcontinu(e|es|ing)\s+to\s+wait\b|\bcontinu(e|es|ing)\s+waiting\b|\bkeep\s+waiting\b/i,
    why: "an unbounded wait with no exit condition the agent controls — it has no loop to continue in",
  },
  {
    id: "no-further-action",
    // Stays SOFT, deliberately: "the lockfile already matches, so no further action is needed" is an
    // ordinary and correct thing for a worker to report. It is only damning next to a wait, and the
    // hard families above already catch those.
    severity: "soft",
    // "no further action needed from me until…"
    re: /\bno further action\s+(is\s+)?(needed|required)\b/i,
    why: "declares itself finished without producing a result — suspicious beside any wait language",
  },
];

/**
 * Markers that a report handed CI off explicitly instead of parking on it. These are exactly the shapes
 * `scripts/ci-poll.mjs` prints and the dispatch contract prescribes, so a compliant report is never
 * mistaken for a stalled one.
 *
 * @type {RegExp[]}
 */
export const HANDOFF_PATTERNS = [
  /\bCI VERDICT:/,
  /\bCI still pending on\s+\S+/i,
  /\bhand(ing|ed)?\s+CI\s+(back\s+)?(over\s+)?to\s+the\s+Foreman\b/i,
  /\bthe\s+Foreman\s+owns\s+CI\b/i,
];

/**
 * Remove FENCED CODE BLOCKS only, so quoting the banned phrasing verbatim is not itself an offence.
 *
 * It used to strip `>` blockquote lines too, and the review showed that was a hole big enough to drive
 * the whole ticket through: writing your status as a blockquote — a completely routine formatting
 * choice, and the shape a report quoting a prior message naturally takes — made **all five** recorded
 * stalls classify `accept`. The exemption was wider than the thing it was exempting.
 *
 * A fence is a deliberate "this is a verbatim artefact, not my own words" marker; a blockquote is not,
 * and agents reach for it to format ordinary prose. So the rule is now: **to quote banned phrasing
 * without tripping the detector, put it in a code fence.** That is stated in the dispatch contract.
 * Inline backtick quotes are NOT stripped and still trip — deliberately: a one-line inline quote is
 * indistinguishable from committing the offence, and the bias here is toward flagging.
 *
 * @param {string} text
 * @returns {string}
 */
export function stripQuoted(text) {
  return text.replace(/^[ \t]*(```|~~~)[\s\S]*?^[ \t]*\1[ \t]*$/gm, "\n");
}

/**
 * @typedef {object} StallMatch
 * @property {string} id
 * @property {"hard"|"soft"} severity
 * @property {string} why
 * @property {string} excerpt the matched text, for the Foreman's re-invoke message
 */

/**
 * @typedef {object} ReportVerdict
 * @property {boolean} stalled
 * @property {StallMatch[]} matches
 * @property {boolean} handoff  a real CI handoff/verdict line is present
 * @property {"accept"|"re-invoke"|"take-over"} action
 * @property {string} message   what the Foreman should do, in one line
 */

/**
 * Classify a returned sub-agent report.
 *
 * @param {string} report
 * @param {{priorStalls?: number}} [opts] how many stall-shaped reports this SAME agent already returned
 * @returns {ReportVerdict}
 */
export function classifyReport(report, opts = {}) {
  const priorStalls = opts.priorStalls ?? 0;
  const text = stripQuoted(String(report ?? ""));
  const handoff = HANDOFF_PATTERNS.some((re) => re.test(text));

  /** @type {StallMatch[]} */
  const matches = [];
  for (const p of STALL_PATTERNS) {
    const m = text.match(p.re);
    if (m) matches.push({ id: p.id, severity: p.severity, why: p.why, excerpt: m[0].trim() });
  }

  const hard = matches.some((m) => m.severity === "hard");
  // A handoff line excuses SOFT deferral ("still pending, over to you") but never a hard one: an agent
  // that backgrounded a watcher is stuck regardless of how well it wrote up the rest.
  const stalled = hard || (matches.length > 0 && !handoff);

  if (!stalled) {
    return {
      stalled: false,
      matches,
      handoff,
      action: "accept",
      message: handoff
        ? "real report; CI handed off explicitly"
        : "no stall language",
    };
  }
  if (priorStalls >= 1) {
    return {
      stalled: true,
      matches,
      handoff,
      action: "take-over",
      message:
        `this agent has now returned ${priorStalls + 1} stall-shaped reports. Do NOT re-invoke ` +
        "again: kill it, read its PR yourself, and run the gauntlet by hand. Re-invoking a third time is " +
        "the loop CPE-1880 exists to bound.",
    };
  }
  return {
    stalled: true,
    matches,
    handoff,
    action: "re-invoke",
    message:
      "SendMessage this agent: \"I own CI; do not watch, poll, or monitor it. Report now, " +
      "synchronously, with the work you already have.\" Do not wait on it.",
  };
}

function main() {
  const argv = process.argv.slice(2);
  let priorStalls = 0;
  /** @type {string|null} */ let file = null;
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === "--prior") {
      priorStalls = Number(argv[i + 1]);
      i += 1;
      if (!Number.isFinite(priorStalls) || priorStalls < 0) {
        console.error("stall-check: --prior needs a non-negative number");
        process.exit(64);
      }
    } else if (file === null) {
      file = argv[i];
    } else {
      console.error(`stall-check: unexpected argument ${argv[i]}`);
      process.exit(64);
    }
  }

  const report = file ? readFileSync(file, "utf8") : readFileSync(0, "utf8");
  const verdict = classifyReport(report, { priorStalls });
  console.log(`stall-check: ${verdict.action.toUpperCase()} — ${verdict.message}`);
  for (const m of verdict.matches) {
    console.log(`  · [${m.severity}] ${m.id}: "${m.excerpt}" — ${m.why}`);
  }
  process.exit(verdict.action === "accept" ? 0 : verdict.action === "re-invoke" ? 3 : 4);
}

if (process.argv[1] !== undefined && pathToFileURL(process.argv[1]).href === import.meta.url) {
  main();
}
