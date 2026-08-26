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
// QUOTING
//   Fenced code blocks and `>` blockquote lines are stripped before matching, so a report that QUOTES
//   the dispatch contract (or this file) is not mistaken for one that is committing the offence. That is
//   not a nicety — the report on CPE-1880 itself would otherwise trip its own detector.
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
    // "A background monitor is now polling…" · "the background watch will report" ·
    // "a background poll I already resolved"
    re: /\bbackground(ed)?\s+(monitor|watch(er)?|poll(er|ing)?|task|job|process)\b/i,
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
    severity: "soft",
    // "until the monitor notification arrives" · "Waiting for that event." ·
    // "Waiting for the next update from the monitor." · "I'll wait for the notification"
    re: /\b(wait(ing)?|await(ing)?)\b[^.\n]{0,50}\b(notification|monitor|that event|the event|wake-?up|signal|the next update)\b/i,
    why: "defers to a signal a sub-agent cannot receive",
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
    id: "open-ended-wait",
    severity: "soft",
    // "Continuing to wait for completion." · "no further action needed from me until…"
    re: /\bcontinu(e|ing)\s+to\s+wait\b|\bno further action\s+(is\s+)?(needed|required)\b/i,
    why: "an unbounded wait with no exit condition the agent controls",
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
 * Remove fenced code blocks and blockquote lines, so QUOTING the banned phrasing is not itself an
 * offence. Everything else is kept verbatim.
 *
 * @param {string} text
 * @returns {string}
 */
export function stripQuoted(text) {
  const withoutFences = text.replace(/^[ \t]*(```|~~~)[\s\S]*?^[ \t]*\1[ \t]*$/gm, "\n");
  return withoutFences
    .split(/\r?\n/)
    .filter((line) => !/^\s{0,3}>/.test(line))
    .join("\n");
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
        ? "accept — real report; CI handed off explicitly"
        : "accept — no stall language",
    };
  }
  if (priorStalls >= 1) {
    return {
      stalled: true,
      matches,
      handoff,
      action: "take-over",
      message:
        `take-over — this agent has now returned ${priorStalls + 1} stall-shaped reports. Do NOT re-invoke ` +
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
      "re-invoke — SendMessage this agent: \"I own CI; do not watch, poll, or monitor it. Report now, " +
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
