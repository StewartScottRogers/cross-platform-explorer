#!/usr/bin/env node
// CPE-1956 — THE CI GATE. `ci.yml`'s five Rust jobs (`backend`, `crates`, `net-e2e`, `sidecar`,
// `msrv`) all sit behind `needs: lockfile-preflight`. When the preflight fails, GitHub does not fail
// them: it SKIPS them. A skipped job is grey, not red — it satisfies a required status check, it is
// neither `pending` nor `failure` to `scripts/ci-poll.mjs`, and in a PR's checks list it reads
// exactly like a job that had nothing to do this run. The entire Rust test suite can vanish from a
// PR's verdict while the PR still looks like it ran CI.
//
// This script is the only place in `ci.yml` where "every one of the five REPORTED" is asked, as
// opposed to "the ones that reported were happy". It is the same shape as `gui-smoke.yml`'s
// `gui-smoke-linux-verdict` (CPE-1753), for the same reason.
//
// Input: `CI_VERDICT_NEEDS`, the workflow's own `${{ toJSON(needs) }}` — an object of
// `{ "<job>": { "result": "success" | "failure" | "cancelled" | "skipped", "outputs": {...} } }`.
// Anything that is not `success` is a red, and `skipped` gets its own message because that is the
// case a human is most likely to misread.
//
// Exit 0 = every needed job ran and succeeded. Exit 1 = it did not, with a `::error::` naming which.
// Anything unexpected (missing env, unparseable JSON, an empty object) is ALSO exit 1: a verdict job
// that cannot see its inputs must not report success, which is the whole defect class this exists
// to close (CLAUDE.md, "Never treat 'npm said nothing' as 'nothing is wrong'").

import { fileURLToPath } from "node:url";

/**
 * Enumeration floor. `toJSON(needs)` coming back empty or near-empty means the job's `needs:` list
 * was gutted (or the expression was mistyped and evaluated to `{}`), and a verdict over zero jobs
 * would otherwise print a comfortable "0 of 0 failed" and exit 0 — a vacuous green over a suite
 * that did not run, which is precisely what this file exists to prevent. Set deliberately BELOW
 * today's real count of five so removing one job over time does not need this touched, while a
 * near-total loss (0/1/2 jobs) still reds.
 *
 * The complementary check — that the `needs:` list still names EVERY job behind
 * `lockfile-preflight`, so a sixth one cannot be added and silently left uncovered — is derived
 * from `ci.yml` itself in `src/lib/ciVerdict.test.ts`, not hard-coded here (CPE-1932).
 */
export const MIN_DEPENDENT_JOBS = 3;

/**
 * GitHub's job results, and what each one means for this gate.
 * @type {Record<string, string | null>}
 */
const RESULT_MEANING = {
  success: null,
  failure: "FAILED — the job ran and reported a failure.",
  cancelled: "CANCELLED — the job did not finish, so it proved nothing.",
  skipped:
    "SKIPPED — the job never ran at all. This is the CPE-1956 shape: a job behind a failed `needs:` " +
    "is skipped, not failed, so it shows grey in the checks list, satisfies a required status check, " +
    "and is invisible to a merge gate that counts pending/failure. It did NOT pass; it did not run.",
};

/**
 * @param {unknown} needs the parsed `toJSON(needs)` payload
 * @returns {{ ok: boolean, messages: string[], errors: string[] }}
 */
export function judge(needs) {
  /** @type {string[]} */ const messages = [];
  /** @type {string[]} */ const errors = [];

  if (needs === null || typeof needs !== "object" || Array.isArray(needs)) {
    return {
      ok: false,
      messages,
      errors: [
        `CI_VERDICT_NEEDS did not parse to an object (got ${Array.isArray(needs) ? "an array" : typeof needs}). ` +
          `This verdict job cannot see its inputs, so it must not report success.`,
      ],
    };
  }

  const entries = Object.entries(/** @type {Record<string, unknown>} */ (needs)).sort(([a], [b]) =>
    a < b ? -1 : a > b ? 1 : 0,
  );

  messages.push(`${entries.length} needed job(s) reported to the verdict:`);
  for (const [name, value] of entries) {
    const result =
      value !== null && typeof value === "object" && typeof (/** @type {any} */ (value).result) === "string"
        ? /** @type {any} */ (value).result
        : null;
    messages.push(`  ${name}: ${result ?? "<no result field>"}`);
    if (result === "success") continue;
    if (result === null) {
      errors.push(
        `${name} reported no \`result\` field — the verdict cannot tell whether it ran. Treated as a failure.`,
      );
      continue;
    }
    const meaning =
      Object.prototype.hasOwnProperty.call(RESULT_MEANING, result) && RESULT_MEANING[result]
        ? RESULT_MEANING[result]
        : `reported \`${result}\`, which is not \`success\`.`;
    errors.push(`${name}: ${meaning}`);
  }

  if (entries.length < MIN_DEPENDENT_JOBS) {
    errors.push(
      `only ${entries.length} job(s) reported to this verdict — expected at least ${MIN_DEPENDENT_JOBS}. ` +
        `That almost always means the ci-verdict job's \`needs:\` list was emptied or the \`toJSON(needs)\` ` +
        `expression stopped evaluating, not that CI genuinely shrank. A gate that judges nothing and reports ` +
        `success is the failure this gate exists to prevent — fix the wiring, do not lower this floor.`,
    );
  }

  return { ok: errors.length === 0, messages, errors };
}

function main() {
  const raw = process.env.CI_VERDICT_NEEDS;
  if (typeof raw !== "string" || raw.trim() === "") {
    console.error("::error::CI_VERDICT_NEEDS is unset or empty — this job was mis-wired. Failing closed.");
    process.exit(1);
  }

  let parsed;
  try {
    parsed = JSON.parse(raw);
  } catch (e) {
    console.error(
      `::error::CI_VERDICT_NEEDS is not valid JSON (${e instanceof Error ? e.message : String(e)}). Failing closed.`,
    );
    process.exit(1);
  }

  const verdict = judge(parsed);
  for (const m of verdict.messages) console.log(m);
  if (!verdict.ok) {
    console.log("");
    for (const e of verdict.errors) console.error(`::error::${e}`);
    console.error(
      "::error::CI VERDICT: the Rust test suite did not fully run and pass. Every job listed above must " +
        "report `success`; a `skipped` job is NOT a pass. If lockfile-preflight failed, fix the stale " +
        "lockfile it named and re-run — the five jobs behind it were never executed.",
    );
    process.exit(1);
  }
  console.log("");
  console.log("CI VERDICT: every job behind lockfile-preflight ran and succeeded.");
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  try {
    main();
  } catch (e) {
    // Never let an unexpected throw reach the runner as a bare stack trace: CI reads `::error::`.
    console.error(`::error::ci-verdict failed unexpectedly: ${e instanceof Error ? e.stack : String(e)}`);
    process.exit(1);
  }
}
