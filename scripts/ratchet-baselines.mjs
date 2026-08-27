#!/usr/bin/env node
// CPE-1934 — the guard that guards the ratchets.
//
// This repo uses one-way RATCHETS to stop a defect class from growing: a stored count (or a stored
// allowlist of tolerated offenders) that a test compares today's measurement against. The stored
// number is supposed to only ever go DOWN. Every one of them lives as a plain literal inside the very
// file it guards, so until this script existed, a PR that added a new offender AND edited the literal
// upward in the same diff passed trivially — the ratchet cannot see its own baseline move. The only
// backstop was a reviewer noticing that a number went up. That is precisely the move a one-way ratchet
// exists to prevent.
//
// What this does: measures every registered baseline at HEAD and at a base revision (the merge base /
// the branch point), and fails when one INCREASED. A raise stays possible — it is occasionally
// legitimate — but never quiet: it must be accompanied, in the same diff, by a row in the raise ledger
// (`docs/design/RATCHETS.md`) naming the baseline, the exact old and new values, the owning ticket and
// the reason. A ledger row that does not match the actual movement does not authorise it.
//
// Enumeration, not recall (CPE-1932): `REGISTRY` below is the enumerated list, and
// `src/lib/ratchetBaselines.test.ts` fails CI if a file in the tree declares something ratchet-shaped
// and is neither registered here nor listed in `NOT_A_RATCHET` with a stated reason. A new ratchet
// therefore cannot be added without either getting this guard for free or saying out loud why it does
// not need it.
//
// No dependencies on purpose: CI runs this with the runner's preinstalled `node`, with no `npm ci` and
// no toolchain, so the whole job is a checkout plus a few hundred milliseconds.
//
// CLI:
//   node scripts/ratchet-baselines.mjs print              measure every baseline in the working tree
//   node scripts/ratchet-baselines.mjs compare <base-ref> compare the working tree against <base-ref>

import { execFileSync } from "node:child_process";
import { readFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const REPO_ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");

/** Where a legitimate raise has to be written down, loudly, in the same diff that raises it. */
export const LEDGER_PATH = "docs/design/RATCHETS.md";

// ---------------------------------------------------------------------------------------------
// A source scanner just big enough to count the entries of a literal without a real parser.
//
// Counting entries with a regex is what makes this class of tooling lie: a `,` inside a string, a
// nested object, or a `//` comment all break naive counting, and the failure mode is a count that
// silently drifts rather than an error. So: a small state machine that knows about strings (all three
// kinds, escapes and `${}` substitutions included) and both comment forms, and nothing else.
// ---------------------------------------------------------------------------------------------

/** The closing bracket for each opening one this scanner tracks. */
const CLOSERS = /** @type {Record<string, string | undefined>} */ ({ "[": "]", "{": "}", "(": ")" });

/**
 * Return the index just past the balanced bracket span that starts at `open` in `src`.
 * `src[open]` must be one of `[`, `{`, `(`.
 * @param {string} src
 * @param {number} open
 * @returns {number}
 */
export function endOfSpan(src, open) {
  const wanted = CLOSERS[src[open]];
  if (!wanted) throw new Error(`endOfSpan: index ${open} is ${JSON.stringify(src[open])}, not an opening bracket`);
  /** @type {string[]} */
  const stack = [wanted];
  let i = open + 1;
  while (i < src.length) {
    const c = src[i];
    // Comments first — a bracket or quote inside one must not move the stack.
    if (c === "/" && src[i + 1] === "/") {
      const nl = src.indexOf("\n", i);
      i = nl === -1 ? src.length : nl + 1;
      continue;
    }
    if (c === "/" && src[i + 1] === "*") {
      const end = src.indexOf("*/", i + 2);
      i = end === -1 ? src.length : end + 2;
      continue;
    }
    if (c === '"' || c === "'" || c === "`") {
      i = endOfString(src, i);
      continue;
    }
    const opened = CLOSERS[c];
    if (opened) {
      stack.push(opened);
      i++;
      continue;
    }
    if (c === stack[stack.length - 1]) {
      stack.pop();
      i++;
      if (stack.length === 0) return i;
      continue;
    }
    i++;
  }
  throw new Error(`endOfSpan: unterminated ${src[open]} starting at index ${open}`);
}

/**
 * Skip the string literal starting at `open` (a quote character) and return the index just past it.
 * A template literal's `${...}` substitution is real code, so it is scanned as code by recursing back
 * through `endOfSpan`.
 * @param {string} src
 * @param {number} open
 * @returns {number}
 */
function endOfString(src, open) {
  const quote = src[open];
  let i = open + 1;
  while (i < src.length) {
    const c = src[i];
    if (c === "\\") {
      i += 2;
      continue;
    }
    if (c === quote) return i + 1;
    if (quote === "`" && c === "$" && src[i + 1] === "{") {
      i = endOfSpan(src, i + 1); // the substitution is code; recurse through the same scanner
      continue;
    }
    i++;
  }
  throw new Error(`endOfString: unterminated ${quote} starting at index ${open}`);
}

/**
 * Split the INSIDE of an array/object literal into its top-level elements. Trailing commas and
 * comment-only tails produce no element.
 * @param {string} inner
 * @returns {string[]}
 */
export function splitTopLevel(inner) {
  /** @type {string[]} */
  const parts = [];
  let start = 0;
  let i = 0;
  while (i < inner.length) {
    const c = inner[i];
    if (c === "/" && inner[i + 1] === "/") {
      const nl = inner.indexOf("\n", i);
      i = nl === -1 ? inner.length : nl + 1;
      continue;
    }
    if (c === "/" && inner[i + 1] === "*") {
      const end = inner.indexOf("*/", i + 2);
      i = end === -1 ? inner.length : end + 2;
      continue;
    }
    if (c === '"' || c === "'" || c === "`") {
      i = endOfString(inner, i);
      continue;
    }
    if (CLOSERS[c]) {
      i = endOfSpan(inner, i);
      continue;
    }
    if (c === ",") {
      parts.push(inner.slice(start, i));
      start = i + 1;
    }
    i++;
  }
  parts.push(inner.slice(start));
  return parts.map(stripComments).filter((p) => p.trim().length > 0);
}

/**
 * Remove comments from one element's text so a comment-only tail doesn't count as an element.
 * @param {string} s
 * @returns {string}
 */
function stripComments(s) {
  let out = "";
  let i = 0;
  while (i < s.length) {
    const c = s[i];
    if (c === "/" && s[i + 1] === "/") {
      const nl = s.indexOf("\n", i);
      i = nl === -1 ? s.length : nl + 1;
      continue;
    }
    if (c === "/" && s[i + 1] === "*") {
      const end = s.indexOf("*/", i + 2);
      i = end === -1 ? s.length : end + 2;
      continue;
    }
    if (c === '"' || c === "'" || c === "`") {
      const end = endOfString(s, i);
      out += s.slice(i, end);
      i = end;
      continue;
    }
    out += c;
    i++;
  }
  return out;
}

/**
 * Find the literal assigned to `const <name>` and return its `[`/`{` index.
 * @param {string} src
 * @param {string} name
 * @returns {number}
 */
function literalStart(src, name) {
  const decl = new RegExp(`(?:^|\\n)\\s*(?:export\\s+)?const\\s+${name}\\b`);
  const m = decl.exec(src);
  if (!m) throw new Error(`no \`const ${name}\` declaration found`);
  const eq = src.indexOf("=", m.index + m[0].length);
  if (eq === -1) throw new Error(`\`const ${name}\` has no initialiser`);
  let i = eq + 1;
  while (i < src.length && /\s/.test(src[i])) i++;
  if (!CLOSERS[src[i]]) throw new Error(`\`const ${name}\` is not initialised with an array/object literal`);
  return i;
}

// --- the measurement shapes the baselines in this repo actually take ---------------------------

/**
 * A baseline stored as a bare integer, e.g. `const BASELINE_FILES_WITH_HEX = 85;`.
 * @param {string} name
 * @returns {(src: string) => number}
 */
export function numericConst(name) {
  return (src) => {
    const m = new RegExp(`(?:^|\\n)\\s*(?:export\\s+)?const\\s+${name}\\b[^=\\n]*=\\s*(\\d[\\d_]*)`).exec(src);
    if (!m) throw new Error(`no numeric \`const ${name}\` found`);
    return Number(m[1].replace(/_/g, ""));
  };
}

/**
 * A baseline stored as an allowlist array — its length is the debt.
 * @param {string} name
 * @returns {(src: string) => number}
 */
export function arrayLength(name) {
  return (src) => {
    const open = literalStart(src, name);
    if (src[open] !== "[") throw new Error(`\`const ${name}\` is not an array literal`);
    return splitTopLevel(src.slice(open + 1, endOfSpan(src, open) - 1)).length;
  };
}

/**
 * A baseline stored as `Record<string, T[]>` — the debt is the total number of recorded entries
 * across every key, not the number of keys (a file with ten offenders is ten pieces of debt).
 * @param {string} name
 * @returns {(src: string) => number}
 */
export function recordOfArraysTotal(name) {
  return (src) => {
    const open = literalStart(src, name);
    if (src[open] !== "{") throw new Error(`\`const ${name}\` is not an object literal`);
    const inner = src.slice(open + 1, endOfSpan(src, open) - 1);
    let total = 0;
    for (const entry of splitTopLevel(inner)) {
      const bracket = entry.indexOf("[");
      if (bracket === -1) throw new Error(`\`const ${name}\` entry has no array value: ${entry.slice(0, 60)}`);
      total += splitTopLevel(entry.slice(bracket + 1, endOfSpan(entry, bracket) - 1)).length;
    }
    return total;
  };
}

/**
 * A baseline stored in a JSON data file — the length of one array property.
 * @param {string} prop
 * @returns {(src: string) => number}
 */
export function jsonArrayLength(prop) {
  return (src) => {
    const parsed = JSON.parse(src);
    const arr = parsed[prop];
    if (!Array.isArray(arr)) throw new Error(`JSON property ${JSON.stringify(prop)} is not an array`);
    return arr.length;
  };
}

// ---------------------------------------------------------------------------------------------
// THE ENUMERATION.
//
// Every ratchet-style baseline in the tree: a stored count or allowlist that is only ever supposed to
// shrink. Found 2026-08-27 by four independent sweeps, not from memory (see docs/design/RATCHETS.md).
// `unenforced: true` means the baseline is real but deliberately not gated here — with the reason
// written down, because an unexplained omission is how an enumeration rots.
// ---------------------------------------------------------------------------------------------

/**
 * @typedef {object} Baseline
 * @property {string} id            stable name used in the raise ledger
 * @property {string} file          repo-relative path holding the literal
 * @property {string} what          one line: what the number counts
 * @property {(src: string) => number} measure
 * @property {boolean} [unenforced] true = enumerated but not gated (reason required)
 * @property {string} [unenforcedReason]
 */

/** @type {Baseline[]} */
export const REGISTRY = [
  {
    id: "hex-files",
    file: "src/app.css.test.ts",
    what: ".svelte files carrying a hard-coded hex colour in a style position (CPE-1534)",
    measure: numericConst("BASELINE_FILES_WITH_HEX"),
  },
  {
    id: "hex-occurrences",
    file: "src/app.css.test.ts",
    what: "total hard-coded hex colour occurrences in style positions (CPE-1534)",
    measure: numericConst("BASELINE_TOTAL_HEX_OCCURRENCES"),
  },
  {
    id: "gui-smoke-known-failing",
    file: "gui-smoke/known-failing.json",
    what: "GUI smoke test cases allowed to fail today (CPE-1594 / CPE-1677)",
    measure: jsonArrayLength("cases"),
  },
  {
    id: "docs-known-gaps",
    file: "src/docs.coverage.test.ts",
    what: "shipped surfaces with no documentation yet (CPE-1571; CPE-1569 burns them down)",
    measure: arrayLength("KNOWN_GAPS_ALLOWLIST"),
  },
  {
    // Only ALLOWLIST is debt. The same file's LOCAL_CUSTOM_PROPERTIES is a permanent by-design list of
    // non-theme custom properties — its own comment says there is nothing there to fix — so it is not a
    // ratchet and is deliberately not measured.
    id: "warn-token-allowlist",
    file: "src/app.css.warn-token.test.ts",
    what: "semantic tokens allowed not to resolve to a hex (CPE-1875)",
    measure: arrayLength("ALLOWLIST"),
  },
  {
    id: "invoke-optout-allowlist",
    file: "src/lib/invoke.guard.test.ts",
    what: "modules allowed to import invoke straight from @tauri-apps/api/core (BUSY-CURSOR.md)",
    measure: arrayLength("INVOKE_OPTOUT_ALLOWLIST"),
  },
  {
    id: "mojibake-allowlist",
    file: "src/lib/mojibakeGuard.test.ts",
    what: "source lines allowed to look like mojibake (CPE-1723)",
    measure: arrayLength("ALLOWLIST"),
  },
  {
    id: "pwsh-encoding-allowed-lines",
    file: "src/lib/workflowPwshFileEncoding.test.ts",
    what: "workflow PowerShell lines allowed to write a file without an explicit encoding (CPE-1842)",
    measure: arrayLength("ALLOWED_LINES"),
  },
  {
    id: "bidi-render-registry",
    file: "src/lib/bidiEscape.guard.test.ts",
    what: "component render sites still showing a raw, unescaped filesystem name (CPE-1757 / CPE-1885)",
    measure: recordOfArraysTotal("REGISTRY"),
  },
  {
    id: "bidi-app-markup-offenders",
    file: "src/lib/bidiEscape.guard.test.ts",
    what: "App.svelte markup render sites still showing a raw filesystem name (CPE-1757)",
    measure: arrayLength("APP_MARKUP_OFFENDERS"),
  },
  {
    id: "bidi-app-script-basename-allowlist",
    file: "src/lib/bidiEscape.guard.test.ts",
    what: "App.svelte <script> baseName() calls allowed to skip displaySafeName (CPE-1757)",
    measure: arrayLength("APP_SCRIPT_BASENAME_ALLOWLIST"),
  },
  {
    id: "manual-test-mvd",
    file: ".claude/qa-architecture/MANUAL-TEST-BURNDOWN.md",
    what: "still-manual verification surfaces (MVD) the QA Architect drives toward zero",
    measure: (src) => {
      const m = /\*\*MVD \(still-manual surfaces\):[^*]*?=\s*(\d+)\s*total\*\*/.exec(src);
      if (!m) throw new Error("no MVD total found in the burndown header");
      return Number(m[1]);
    },
    unenforced: true,
    unenforcedReason:
      "The MVD legitimately RISES whenever a QA-Architect audit discovers pre-existing unlogged debt — " +
      "the ledger's own history records several such shifts (+5 on 2026-08-11), and that discovery is the " +
      "behaviour we want, not the behaviour we want to gate. Gating it would push audits toward not " +
      "logging what they find, which is the opposite of the point. Enumerated here so it is visibly a " +
      "decision rather than an oversight; the stored-vs-real drift in this ledger is CPE-1922's.",
  },
];

/**
 * Files that declare something ratchet-SHAPED but are not ratchets. Keeps the completeness check in
 * `src/lib/ratchetBaselines.test.ts` honest: a new match is either a real ratchet that must be
 * registered above, or it lands here with a reason.
 * @type {{ file: string; reason: string }[]}
 */
export const NOT_A_RATCHET = [
  {
    file: "gui-smoke/lib/compare.ts",
    reason: "BASELINES_DIR is a directory path for screenshot comparison — not a stored count of debt.",
  },
  {
    file: "gui-smoke/lib/ratchet.test.ts",
    reason: "KNOWN_FAILING/EXPECTED_SPECS are synthetic fixtures for the ratchet's own unit tests.",
  },
  {
    file: "gui-smoke/scripts/run-ratchet.ts",
    reason: "KNOWN_FAILING_PATH is the path to the real ratchet file (registered as gui-smoke-known-failing).",
  },
  {
    file: "gui-smoke/wdio.conf.ts",
    reason: "REPLAY_BASELINE_NAME is a test fixture filename for the activity-replay baseline feature.",
  },
];

// ---------------------------------------------------------------------------------------------
// The raise ledger.
// ---------------------------------------------------------------------------------------------

/**
 * @typedef {object} LedgerRow
 * @property {string} id
 * @property {number} from
 * @property {number} to
 * @property {string} ticket
 * @property {string} reason
 */

/**
 * Parse the raise ledger's table rows out of `docs/design/RATCHETS.md`.
 * Row shape: `| <id> | <from> -> <to> | CPE-NNNN | reason |`
 * @param {string} md
 * @returns {LedgerRow[]}
 */
export function parseLedger(md) {
  /** @type {LedgerRow[]} */
  const rows = [];
  const rowRe = /^\|\s*`?([a-z0-9-]+)`?\s*\|\s*(\d+)\s*(?:->|→)\s*(\d+)\s*\|\s*(CPE-\d+)\s*\|\s*(.+?)\s*\|$/;
  for (const line of md.split(/\r?\n/)) {
    const m = rowRe.exec(line.trim());
    if (m) rows.push({ id: m[1], from: Number(m[2]), to: Number(m[3]), ticket: m[4], reason: m[5] });
  }
  return rows;
}

// ---------------------------------------------------------------------------------------------
// Comparison — the actual verdict, kept pure so the unit test can drive BOTH directions without git.
// ---------------------------------------------------------------------------------------------

/**
 * @typedef {object} Verdict
 * @property {boolean} ok
 * @property {string[]} messages   human lines for the CI log
 * @property {string[]} errors     the reasons this is red (empty when ok)
 */

/**
 * Decide whether the movement between `base` and `head` is allowed.
 * Lowering is always fine. Raising needs an exact matching ledger row.
 * @param {Baseline[]} registry
 * @param {Record<string, number | null | undefined>} base  null/absent = did not exist at the base revision
 * @param {Record<string, number | undefined>} head
 * @param {LedgerRow[]} ledger
 * @returns {Verdict}
 */
export function evaluate(registry, base, head, ledger) {
  /** @type {string[]} */
  const messages = [];
  /** @type {string[]} */
  const errors = [];

  for (const b of registry) {
    const before = base[b.id];
    const after = head[b.id];
    if (after === undefined || after === null) {
      errors.push(
        `${b.id}: could not be measured in the working tree — the guard cannot verify it, which is a red, not a pass.`,
      );
      continue;
    }
    if (before === undefined || before === null) {
      messages.push(`  ${b.id}: new at this revision (${after}) — nothing to compare against.`);
      continue;
    }
    if (after < before) {
      messages.push(`  ${b.id}: ${before} -> ${after} LOWERED (${b.what})`);
      continue;
    }
    if (after === before) {
      messages.push(`  ${b.id}: ${before} unchanged`);
      continue;
    }

    if (b.unenforced) {
      messages.push(
        `  ${b.id}: ${before} -> ${after} RAISED — enumerated but deliberately not gated. ${b.unenforcedReason ?? ""}`,
      );
      continue;
    }
    const row = ledger.find((r) => r.id === b.id && r.from === before && r.to === after);
    if (row) {
      messages.push(
        `  ${b.id}: ${before} -> ${after} RAISED, and declared in ${LEDGER_PATH} under ${row.ticket}: ${row.reason}`,
      );
      continue;
    }
    errors.push(
      `${b.id} (${b.file}) went UP: ${before} -> ${after}. This is a one-way ratchet over ${b.what} — ` +
        `the number is not the defect, the thing it counts is. Fix the new offender so the count comes back down. ` +
        `If this raise is genuinely correct it must be LOUD rather than quiet: add a row to ${LEDGER_PATH} in ` +
        `THIS diff — \`| ${b.id} | ${before} -> ${after} | CPE-NNNN | why this raise is right |\` — and this ` +
        `guard will pass while leaving the raise unmistakable in review.`,
    );
  }

  return { ok: errors.length === 0, messages, errors };
}

// ---------------------------------------------------------------------------------------------
// Measuring — against the working tree, or against an arbitrary git revision.
// ---------------------------------------------------------------------------------------------

/**
 * Measure every registered baseline in the working tree.
 * @returns {Record<string, number>}
 */
export function measureWorkingTree() {
  /** @type {Record<string, number>} */
  const out = {};
  for (const b of REGISTRY) {
    const p = join(REPO_ROOT, b.file);
    if (!existsSync(p)) throw new Error(`${b.id}: ${b.file} does not exist — the registry has drifted from the tree.`);
    out[b.id] = b.measure(readFileSync(p, "utf8"));
  }
  return out;
}

/**
 * Measure every registered baseline at a git revision. A baseline whose file did not exist there is
 * reported as `null` (new), never silently as 0 — 0 would read as "it was at zero and you raised it".
 * @param {string} ref
 * @returns {Record<string, number | null>}
 */
export function measureAtRef(ref) {
  /** @type {Record<string, number | null>} */
  const out = {};
  for (const b of REGISTRY) {
    /** @type {string} */
    let src;
    try {
      src = execFileSync("git", ["show", `${ref}:${b.file}`], {
        cwd: REPO_ROOT,
        encoding: "utf8",
        maxBuffer: 64 * 1024 * 1024,
      });
    } catch {
      out[b.id] = null;
      continue;
    }
    out[b.id] = b.measure(src);
  }
  return out;
}

// ---------------------------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------------------------

/** @returns {void} */
function main() {
  const [cmd, arg] = process.argv.slice(2);

  if (cmd === "print") {
    const head = measureWorkingTree();
    for (const b of REGISTRY) {
      const gated = b.unenforced ? "   (enumerated, not gated)" : "";
      console.log(`${b.id.padEnd(36)} ${String(head[b.id]).padStart(5)}   ${b.file}${gated}`);
    }
    return;
  }

  if (cmd === "compare") {
    if (!arg) {
      console.error(
        "::error::ratchet guard: `compare` needs a base revision and was given none. Refusing to pass without comparing anything.",
      );
      process.exit(1);
    }
    let baseRef = arg;
    if (/^0{40}$/.test(baseRef)) baseRef = "HEAD^"; // first push on a branch: GitHub sends an all-zero sha
    try {
      execFileSync("git", ["rev-parse", "--verify", `${baseRef}^{commit}`], { cwd: REPO_ROOT, stdio: "ignore" });
    } catch {
      console.error(
        `::error::ratchet guard: base revision ${JSON.stringify(arg)} could not be resolved, so no comparison happened. ` +
          "A guard that cannot see the base must go red, not green — check the checkout's fetch depth.",
      );
      process.exit(1);
    }

    const head = measureWorkingTree();
    const base = measureAtRef(baseRef);
    const ledgerPath = join(REPO_ROOT, LEDGER_PATH);
    const ledger = existsSync(ledgerPath) ? parseLedger(readFileSync(ledgerPath, "utf8")) : [];

    const verdict = evaluate(REGISTRY, base, head, ledger);
    console.log(`Ratchet baselines, working tree vs ${baseRef} (${REGISTRY.length} enumerated):`);
    for (const m of verdict.messages) console.log(m);
    if (!verdict.ok) {
      console.log("");
      for (const e of verdict.errors) console.error(`::error::${e}`);
      process.exit(1);
    }
    console.log("");
    console.log("No ratchet baseline was raised without being declared.");
    return;
  }

  console.error("usage: ratchet-baselines.mjs print | compare <base-ref>");
  process.exit(2);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) main();
