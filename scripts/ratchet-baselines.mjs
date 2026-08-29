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
// legitimate — but never quiet: it must be accompanied, IN THIS DIFF, by a NEW row in the raise ledger
// (`docs/design/RATCHETS.md`) naming the baseline, the exact old and new values, the owning ticket and
// the reason.
//
// THE GOVERNING RULE, because every real hole found in review broke it: **a measurement this guard
// cannot make must be RED, never a number and never a skip.** A measurer that returns the wrong value
// passes a raise, which is the entire defect. So:
//   - a baseline constant that stops being a bare integer literal (`= 200 + 78`) throws, it does not
//     take the first integer it sees;
//   - an allowlist whose entries include a spread (`...MORE_GAPS`) throws, it does not count the
//     spread as one element;
//   - a literal that is not the whole initialiser (`[...].concat(X)`) throws;
//   - a baseline that cannot be measured at the BASE revision is an error, not a free pass — git's own
//     rename detection is followed first, and anything still unresolved must be declared;
//   - a ledger row that already existed at the base revision authorises nothing: a row is a one-time
//     licence for the raise made in its own diff, not a standing permit.
//
// Enumeration, not recall (CPE-1932): `REGISTRY` below is the enumerated list, and
// `src/lib/ratchetBaselines.test.ts` fails CI if a file in the tree declares something ratchet-shaped
// and is neither registered here nor listed in `NOT_A_RATCHET` with a stated reason.
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

/**
 * A measurement that could not be made. Never coerced to a number anywhere — it travels as itself all
 * the way to `evaluate`, which turns it into a red.
 * @typedef {{ failed: string }} Unmeasurable
 */

/**
 * @param {unknown} v
 * @returns {v is Unmeasurable}
 */
export function isUnmeasurable(v) {
  return typeof v === "object" && v !== null && typeof (/** @type {Unmeasurable} */ (v).failed) === "string";
}

// ---------------------------------------------------------------------------------------------
// A source scanner just big enough to count the entries of a literal without a real parser.
//
// Counting entries with a regex is what makes this class of tooling lie: a `,` inside a string, a
// nested object, or a `//` comment all break naive counting, and the failure mode is a count that
// silently drifts rather than an error. So: a small state machine that knows about strings (all three
// kinds, escapes and `${}` substitutions included) and both comment forms, and nothing else — plus a
// hard refusal on any construct whose element count it cannot know.
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
 *
 * **Refuses a spread.** `[...MORE_GAPS, "x"]` has an element count this scanner cannot know — the
 * spread might contribute six entries or none. Counting it as one is how a real 14 -> 17 raise
 * reported itself as `14 -> 12 LOWERED` and exited 0 (CPE-1934 review, F1). Unknowable must be red.
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
  const elements = parts.map(stripComments).filter((p) => p.trim().length > 0);
  for (const el of elements) {
    if (el.trim().startsWith("...")) {
      throw new Error(
        `this literal spreads another value into itself (${JSON.stringify(el.trim().slice(0, 40))}), so its real ` +
          `entry count is not knowable from this file alone. A spread would be counted as ONE entry, which ` +
          `under-reports the debt and can turn a raise into a "LOWERED" pass. Write the entries out literally.`,
      );
    }
  }
  return elements;
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
 * Blank out everything in `src` that is not live code — comment bodies, and the INTERIOR of every
 * string/template literal — replacing each character with a space so that **every index stays valid
 * against the original source**. Newlines are preserved so line numbers survive.
 *
 * Why this exists (CPE-1934 review round 3, R2-F1c): the declaration *search* used to run on raw
 * source and take the first match. `stripComments` was applied only to the captured initialiser, never
 * before the search, so a decoy inside a `/* … *\/` block or a template literal outranked the live
 * constant:
 *
 *     /* Historical note: const BASELINE_TOTAL_HEX_OCCURRENCES = 277; *\/
 *     const BASELINE_TOTAL_HEX_OCCURRENCES = 278;   // the live value — a real raise
 *
 * measured 277 and exited 0. Masking first is half the fix; [`findSoleDeclaration`] is the other,
 * durable half.
 *
 * String quotes are KEPT (only the interior is blanked) so the literal scanners still see a
 * well-delimited, inert string where a string used to be. An unterminated `'`/`"` is bounded at the
 * end of its line, so a stray quote cannot silently swallow the rest of the file.
 *
 * Known gap, deliberately not closed: **regex literals are not tracked.** Telling `/` division from a
 * regex needs real parsing, and getting it wrong here is the safe direction — an unmasked regex can
 * only ADD apparent entries (`["a", /x,y/, "b"]` counts 4, not 3), which over-reports debt and so
 * fails closed on a raise; and a quote inside a regex masks at most to end of line, which yields a
 * "no declaration found" red rather than a wrong number. See `docs/design/RATCHETS.md`.
 *
 * @param {string} src
 * @returns {string}
 */
export function maskNonCode(src) {
  const out = src.split("");
  /**
   * @param {number} from
   * @param {number} to
   * @returns {void}
   */
  const blank = (from, to) => {
    for (let k = Math.max(0, from); k < to && k < out.length; k++) if (out[k] !== "\n") out[k] = " ";
  };

  let i = 0;
  while (i < src.length) {
    const c = src[i];
    if (c === "/" && src[i + 1] === "/") {
      const nl = src.indexOf("\n", i);
      const end = nl === -1 ? src.length : nl;
      blank(i, end);
      i = end;
      continue;
    }
    if (c === "/" && src[i + 1] === "*") {
      const e = src.indexOf("*/", i + 2);
      const end = e === -1 ? src.length : e + 2;
      blank(i, end);
      i = end;
      continue;
    }
    if (c === '"' || c === "'" || c === "`") {
      let j = i + 1;
      while (j < src.length) {
        if (src[j] === "\\") {
          j += 2;
          continue;
        }
        if (src[j] === c) break;
        if (c !== "`" && src[j] === "\n") break; // an unterminated quote stops at its own line
        j++;
      }
      const closed = j < src.length && src[j] === c;
      blank(i + 1, closed ? j : j); // keep the delimiters, blank the interior
      i = closed ? j + 1 : j;
      continue;
    }
    i++;
  }
  return out.join("");
}

/** @param {string} s @param {number} idx @returns {number} */
function lineOf(s, idx) {
  return s.slice(0, idx).split("\n").length;
}

/**
 * Find the ONE `const <name>` declaration in already-masked source and return the index just past the
 * name. **Two or more matches is an error in itself** — that is the durable half of the R2-F1c fix: it
 * turns "which of these did I pick?" into a question that cannot arise, so a decoy cannot outrank the
 * live constant even in a form the masker does not understand.
 * @param {string} masked
 * @param {string} name
 * @returns {number}
 */
function findSoleDeclaration(masked, name) {
  const re = new RegExp(`(?:^|\\n)[ \\t]*(?:export[ \\t]+)?const[ \\t]+${name}\\b`, "g");
  /** @type {number[]} */
  const ends = [];
  /** @type {number[]} */
  const lines = [];
  for (let m = re.exec(masked); m !== null; m = re.exec(masked)) {
    ends.push(m.index + m[0].length);
    lines.push(lineOf(masked, m.index + m[0].length));
    re.lastIndex = m.index + m[0].length;
  }
  if (ends.length === 0) throw new Error(`no \`const ${name}\` declaration found`);
  if (ends.length > 1) {
    throw new Error(
      `${ends.length} declarations of \`const ${name}\` found (lines ${lines.join(", ")}) — this guard cannot ` +
        `tell which one is live, and picking the first is exactly how a decoy outranks the real baseline. ` +
        `There must be exactly one.`,
    );
  }
  return ends[0];
}

/**
 * Find the literal assigned to `const <name>` and return its `[`/`{` index. Operates on MASKED source
 * (see [`maskNonCode`]) — indices are valid against the original too.
 * @param {string} masked
 * @param {string} name
 * @returns {number}
 */
function literalStart(masked, name) {
  const at = findSoleDeclaration(masked, name);
  const eq = masked.indexOf("=", at);
  if (eq === -1) throw new Error(`\`const ${name}\` has no initialiser`);
  let i = eq + 1;
  while (i < masked.length && /\s/.test(masked[i])) i++;
  if (!CLOSERS[masked[i]]) throw new Error(`\`const ${name}\` is not initialised with an array/object literal`);
  return i;
}

/**
 * The literal must be the WHOLE initialiser. `[...] .concat(MORE)` / `[...].slice(1)` measure the
 * bracket span and silently ignore the rest, which under-reports. Anything but `;` (optionally after
 * `as const`) after the closing bracket is refused.
 * @param {string} src
 * @param {number} end   index just past the literal's closing bracket
 * @param {string} name
 * @returns {void}
 */
function assertLiteralIsWholeInitialiser(src, end, name) {
  const tail = src.slice(end);
  const skipped = /^\s*(?:as\s+const\s*)?/.exec(tail);
  const next = tail[(skipped ? skipped[0].length : 0)];
  if (next !== ";") {
    throw new Error(
      `\`const ${name}\`'s literal is not the whole initialiser — it is followed by ` +
        `${JSON.stringify(tail.slice(0, 30).replace(/\s+/g, " "))} rather than \`;\`. Whatever follows would be ` +
        `ignored, so the measured count could be lower than the real one. Keep the baseline a plain literal.`,
    );
  }
}

// --- the measurement shapes the baselines in this repo actually take ---------------------------

/**
 * A baseline stored as a bare integer, e.g. `const BASELINE_FILES_WITH_HEX = 85;`.
 *
 * Deliberately strict: the ENTIRE initialiser must be one integer. An earlier version took the first
 * integer after `=`, so `= 200 + 78` (a real 277 -> 278 raise) measured as `200` and reported
 * `277 -> 200 LOWERED`, exit 0 — a complete all-green bypass from a one-line edit (CPE-1934 review, F1).
 * @param {string} name
 * @returns {(src: string) => number}
 */
export function numericConst(name) {
  return (src) => {
    const masked = maskNonCode(src);
    const at = findSoleDeclaration(masked, name);
    const nl = masked.indexOf("\n", at);
    const lineEnd = nl === -1 ? masked.length : nl;
    const eq = masked.indexOf("=", at);
    if (eq === -1 || eq > lineEnd) {
      throw new Error(`\`const ${name}\` has no initialiser on its own declaration line`);
    }
    const raw = masked
      .slice(eq + 1, lineEnd)
      .trim()
      .replace(/;\s*$/, "")
      .trim();
    if (!/^\d[\d_]*$/.test(raw)) {
      throw new Error(
        `\`const ${name}\` is no longer a plain integer literal — its initialiser reads ${JSON.stringify(raw)}. ` +
          `This guard refuses to guess a number out of an expression: a measurer that returns the WRONG value ` +
          `passes a raise, which is exactly the defect it exists to stop. Keep the baseline a bare integer.`,
      );
    }
    return Number(raw.replace(/_/g, ""));
  };
}

/**
 * A baseline stored as an allowlist array — its length is the debt.
 * @param {string} name
 * @returns {(src: string) => number}
 */
export function arrayLength(name) {
  return (src) => {
    const masked = maskNonCode(src);
    const open = literalStart(masked, name);
    if (masked[open] !== "[") throw new Error(`\`const ${name}\` is not an array literal`);
    const end = endOfSpan(masked, open);
    assertLiteralIsWholeInitialiser(masked, end, name);
    return splitTopLevel(masked.slice(open + 1, end - 1)).length;
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
    const masked = maskNonCode(src);
    const open = literalStart(masked, name);
    if (masked[open] !== "{") throw new Error(`\`const ${name}\` is not an object literal`);
    const end = endOfSpan(masked, open);
    assertLiteralIsWholeInitialiser(masked, end, name);
    let total = 0;
    // Masking also fixes a subtler hazard here: a `[` inside a KEY string can no longer be mistaken
    // for the value array's opening bracket, because key interiors are blank by the time we look.
    for (const entry of splitTopLevel(masked.slice(open + 1, end - 1))) {
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
// shrink. Found 2026-08-27 by four independent sweeps, not from memory (see docs/design/RATCHETS.md),
// and independently re-derived by the CPE-1934 Reviewer with a wider vocabulary — nothing missed.
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
    id: "launcher-contrast-not-flat-exemptions",
    file: "scripts/dev-harness/launcher-contrast/run.mjs",
    what: "launcher sites exempted from the pixel leg's flat-ground condition (CPE-1966)",
    measure: arrayLength("NOT_FLAT_BY_DESIGN_EXEMPTIONS"),
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
  {
    file: "scripts/dev-harness/js-strip-sweep.mjs",
    reason:
      "CAP/PRE_EXISTING_CAP are how many desyncs the listing PRINTS (lifted by `--all`) — a console " +
      "display limit on an exploration tool, not a stored count of debt. Nothing is excused by being " +
      "past the cap: the desync total above it is always printed in full (CPE-1966).",
  },
  {
    file: "scripts/organize-done.mjs",
    reason: "THRESHOLD is how many Done tickets trigger a reorganise — a workflow trigger, not debt owed.",
  },
  {
    file: "scripts/ratchet-baselines.mjs",
    reason: "This guard's own REGISTRY: the list OF ratchets, not a ratchet. Its completeness is tested separately.",
  },
  {
    file: "src/lib/ratchetBaselines.test.ts",
    reason:
      "This guard's own test. Its R2-F1c decoy fixtures contain literal `const BASELINE_...` text inside " +
      "template literals, which is the point of those fixtures — none of it is a real stored baseline.",
  },
  {
    file: "src/lib/jsSource.test.ts",
    reason:
      "KNOWN_GAPS is a CASE TABLE, not a suppression list: each entry asserts the stripper's exact " +
      "output on an adversarial shape, so removing one removes coverage rather than paying down debt. " +
      "Nothing is excused by being in it — the `vm.Script` oracle in the same file runs over every " +
      "entry regardless (CPE-1966).",
  },
];

// ---------------------------------------------------------------------------------------------
// The raise ledger.
// ---------------------------------------------------------------------------------------------

/**
 * A declared raise. `from === null` means "this baseline is new/unmeasurable at the base revision",
 * written in the ledger as `| id | new -> N | CPE-NNNN | why |`.
 * @typedef {object} LedgerRow
 * @property {string} id
 * @property {number | null} from
 * @property {number} to
 * @property {string} ticket
 * @property {string} reason
 */

/**
 * Parse the raise ledger's table rows out of `docs/design/RATCHETS.md`.
 * Row shape: `| <id> | <from> -> <to> | CPE-NNNN | reason |`, where `<from>` is an integer or `new`.
 * @param {string} md
 * @returns {LedgerRow[]}
 */
export function parseLedger(md) {
  /** @type {LedgerRow[]} */
  const rows = [];
  const rowRe = /^\|\s*`?([a-z0-9-]+)`?\s*\|\s*(\d+|new)\s*(?:->|→)\s*(\d+)\s*\|\s*(CPE-\d+)\s*\|\s*(.+?)\s*\|$/;
  for (const line of md.split(/\r?\n/)) {
    const m = rowRe.exec(line.trim());
    if (m) {
      rows.push({ id: m[1], from: m[2] === "new" ? null : Number(m[2]), to: Number(m[3]), ticket: m[4], reason: m[5] });
    }
  }
  return rows;
}

/**
 * A row of the ENUMERATION table in `docs/design/RATCHETS.md` — the human-readable list of every
 * ratchet and where it stands today. Distinct from a `LedgerRow`, which authorises a raise.
 * @typedef {object} DocRow
 * @property {string} id
 * @property {string} file
 * @property {number} value
 * @property {boolean} gated  false when the row declares itself "enumerated, not gated"
 * @property {number} line    1-based line in the document, so a failure can point at it
 */

/**
 * The exact header cells the enumeration table must carry. Anchoring on the HEADER, and matching the
 * whole cell, is what stops a number quoted in a surrounding paragraph — or a row of the raise ledger
 * further down this same file — from ever satisfying the check (CPE-1933 rule 2: anchor on structure,
 * never on prose).
 */
const ENUMERATION_HEADER = ["id", "file", "what the number counts", "today"];

/** @param {string} s */
const isTableRow = (s) => s.trim().startsWith("|") && s.trim().endsWith("|") && s.trim().length > 1;

/**
 * @param {string} s
 * @returns {string[]}
 */
const tableCells = (s) =>
  s
    .trim()
    .slice(1, -1)
    .split("|")
    .map((c) => c.trim());

/**
 * Parse the enumeration table out of `docs/design/RATCHETS.md` (CPE-1948).
 *
 * That table used to be hand-kept: twelve baselines with their current values written out as literals
 * that nothing checked, inside the one document explaining why an unchecked literal is a defect. It
 * went stale within an hour of landing, and a second row (`bidi-render-registry`, 1552 → 1553 via
 * PR #1056) was already stale when CPE-1948 was picked up. `src/lib/ratchetsDoc.test.ts` asserts what
 * this returns against `measureWorkingTree()`, so the table is derived rather than remembered.
 *
 * Every refusal below is deliberate rather than lenient, for the reason the rest of this file exists:
 * a parser that GUESSES turns a stale table into a green test, which is strictly worse than a stale
 * table nobody claimed was checked.
 *
 * @param {string} md the document's full text
 * @returns {DocRow[]}
 */
export function parseEnumerationTable(md) {
  const lines = md.split(/\r?\n/);
  /** @type {number[]} */
  const headers = [];
  for (let i = 0; i < lines.length; i++) {
    if (!isTableRow(lines[i])) continue;
    const c = tableCells(lines[i]);
    if (c.length === ENUMERATION_HEADER.length && c.every((v, k) => v.toLowerCase() === ENUMERATION_HEADER[k])) {
      headers.push(i);
    }
  }
  if (headers.length !== 1) {
    throw new Error(
      `expected exactly ONE enumeration table in ${LEDGER_PATH} headed \`| ${ENUMERATION_HEADER.join(" | ")} |\`, ` +
        `found ${headers.length}. Two would make "which one is live" a question — the same defect this file ` +
        `refuses for a doubly-declared constant.`,
    );
  }
  const start = headers[0];
  const sep = lines[start + 1] ?? "";
  if (!isTableRow(sep) || !tableCells(sep).every((c) => /^:?-{2,}:?$/.test(c))) {
    throw new Error(`${LEDGER_PATH}:${start + 2}: the enumeration table header is not followed by a separator row`);
  }
  // The row scan takes every consecutive table-shaped line and stops at the first that is not one.
  // Two shapes slip past it, and neither is caught HERE: a blank line mid-table silently TRUNCATES
  // the row list, and a second four-column table butted straight against this one (no blank line)
  // has its rows ABSORBED into this one. Both are caught downstream instead, by the ORDERED id
  // comparison against REGISTRY in `src/lib/ratchetsDoc.test.ts`. Measured, not assumed: a blank
  // line injected before the `mojibake-allowlist` row cut this parse from 12 rows to 6 and reddened
  // that comparison, with the not-gated non-vacuity check firing as a second net. The `today`
  // assertion did NOT fire, because the six surviving rows were each still correct. So this parser
  // is not airtight on its own, and the id comparison is not scaffolding around a check that
  // already works — it IS the check doing the work here; do not delete it as redundant.
  // (Absorption needs an intruder whose cells are themselves row-shaped: the raise ledger header,
  // with its bare `baseline` cell, throws rather than being swallowed.)
  /** @type {DocRow[]} */
  const rows = [];
  for (let i = start + 2; i < lines.length && isTableRow(lines[i]); i++) {
    const c = tableCells(lines[i]);
    const where = `${LEDGER_PATH}:${i + 1}`;
    if (c.length !== ENUMERATION_HEADER.length) {
      throw new Error(`${where}: enumeration row has ${c.length} cells, expected ${ENUMERATION_HEADER.length}`);
    }
    const id = /^`([a-z0-9-]+)`$/.exec(c[0]);
    if (!id) throw new Error(`${where}: id cell is ${JSON.stringify(c[0])}, expected exactly one backticked id`);
    const file = /^`([^`]+)`$/.exec(c[1]);
    if (!file) throw new Error(`${where}: file cell is ${JSON.stringify(c[1])}, expected exactly one backticked path`);
    // The value cell is matched WHOLE, never "the first number in the cell". A cell trailing a
    // parenthetical history — `14 (13 -> 14 on 2026-08-27)`, which is what this row actually said —
    // would otherwise be read as its leading digits with the rest unasserted, which is exactly how a
    // scanner ends up agreeing with a sentence instead of with a measurement. The one permitted
    // suffix is the not-gated marker, and that marker is itself asserted against `unenforced`.
    const value = /^(\d+)( — \*\*enumerated, not gated\*\*)?$/.exec(c[3]);
    if (!value) {
      throw new Error(
        `${where}: the \`today\` cell is ${JSON.stringify(c[3])}. It must be exactly a number, optionally ` +
          `followed by " — **enumerated, not gated**" — nothing else, so no digit here can go unasserted.`,
      );
    }
    rows.push({ id: id[1], file: file[1], value: Number(value[1]), gated: value[2] === undefined, line: i + 1 });
  }
  if (rows.length === 0) throw new Error(`the enumeration table in ${LEDGER_PATH} has no rows`);
  return rows;
}

/**
 * Every row in `rows` authorising exactly this movement. Returned as a LIST, not a first match: the
 * same from -> to move can legitimately happen more than once over a repo's life (burn a baseline
 * down, then re-raise it later), so what matters is whether THIS diff adds one MORE row than the base
 * had — not whether a row for that triple exists at all (CPE-1934 review round 3, R2-F2b).
 * @param {LedgerRow[]} rows
 * @param {string} id
 * @param {number | null} from
 * @param {number} to
 * @returns {LedgerRow[]}
 */
function rowsFor(rows, id, from, to) {
  return rows.filter((r) => r.id === id && r.from === from && r.to === to);
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
 *
 * Lowering is always fine. Raising needs a ledger row that matches exactly AND that is **new in this
 * diff** — a row already present at the base revision is a spent licence, not a standing permit
 * (CPE-1934 review, F2). A baseline that could not be measured at either end is an error, never a
 * pass (F1/F3).
 *
 * @param {Baseline[]} registry
 * @param {Record<string, number | Unmeasurable | null | undefined>} base  null = absent at the base revision
 * @param {Record<string, number | Unmeasurable | undefined>} head
 * @param {LedgerRow[]} ledger        the ledger in the working tree
 * @param {LedgerRow[]} [baseLedger]  the ledger at the base revision
 * @returns {Verdict}
 */
export function evaluate(registry, base, head, ledger, baseLedger = []) {
  /** @type {string[]} */
  const messages = [];
  /** @type {string[]} */
  const errors = [];

  /**
   * A raise is authorised when this diff ADDS a row for the movement — i.e. the working-tree ledger
   * holds strictly more rows for that (id, from, to) than the base revision did. Counting rather than
   * `find`ing is what lets the same movement be authorised twice over the repo's life while still
   * refusing to re-use a historical row (R2-F2b).
   * @param {string} id
   * @param {number | null} from
   * @param {number} to
   * @returns {{ row?: LedgerRow; spent?: LedgerRow; inHead?: number; inBase?: number }}
   */
  function licence(id, from, to) {
    const head = rowsFor(ledger, id, from, to);
    if (head.length === 0) return {};
    const base = rowsFor(baseLedger, id, from, to);
    if (head.length > base.length) return { row: head[head.length - 1], inHead: head.length, inBase: base.length };
    return { spent: base[0], inHead: head.length, inBase: base.length };
  }

  for (const b of registry) {
    const before = base[b.id];
    const after = head[b.id];

    // --- head end -----------------------------------------------------------------------------
    if (after === undefined || after === null) {
      errors.push(
        `${b.id} (${b.file}): could not be measured in the working tree — the guard cannot verify it, ` +
          `which is a red, not a pass.`,
      );
      continue;
    }
    if (isUnmeasurable(after)) {
      errors.push(
        `${b.id} (${b.file}): could not be measured in the working tree — ${after.failed} A guard that ` +
          `cannot measure must go red, never green and never a guessed number.`,
      );
      continue;
    }

    // --- base end -----------------------------------------------------------------------------
    if (isUnmeasurable(before)) {
      errors.push(
        `${b.id} (${b.file}): could not be measured at the BASE revision — ${before.failed} Without a base ` +
          `value the guard cannot tell whether this baseline moved, so it goes red rather than assuming it did not.`,
      );
      continue;
    }
    if (before === undefined || before === null) {
      // The file does not exist at the base revision AND git's rename detection found no predecessor.
      // Treating that as "new, nothing to compare" is how a rename silently resets a ratchet (F3), so
      // it has to be declared like any other raise.
      const { row, spent } = licence(b.id, null, after);
      if (row) {
        messages.push(
          `  ${b.id}: new at this revision (${after}), declared in ${LEDGER_PATH} under ${row.ticket}: ${row.reason}`,
        );
        continue;
      }
      errors.push(
        `${b.id} (${b.file}): has no value at the base revision — the file does not exist there and git's rename ` +
          `detection found no predecessor for it. That would reset the ratchet to whatever it says today. If the ` +
          `file really is new in this diff, declare it: add \`| ${b.id} | new -> ${after} | CPE-NNNN | why |\` to ` +
          `${LEDGER_PATH}. If it was renamed, keep the rename detectable (rename in its own commit, or split the ` +
          `content change out) so the old value can be read.` +
          (spent
            ? ` (A \`new ->\` row for this already existed at the base revision and this diff added no further one; ` +
              `APPEND a row under the ticket that owns this change rather than reusing or editing the old one.)`
            : ""),
      );
      continue;
    }

    // --- movement -----------------------------------------------------------------------------
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

    const { row, spent, inHead, inBase } = licence(b.id, before, after);
    if (row) {
      messages.push(
        `  ${b.id}: ${before} -> ${after} RAISED, and declared in ${LEDGER_PATH} under ${row.ticket}: ${row.reason}`,
      );
      continue;
    }
    if (spent) {
      errors.push(
        `${b.id} (${b.file}) went UP: ${before} -> ${after}, but this diff did not ADD a ledger row for that ` +
          `movement — ${LEDGER_PATH} carries ${inHead} row(s) for it and the base revision already carried ` +
          `${inBase} (the oldest is ${spent.ticket}). A row authorises the raise made in its OWN diff, not every ` +
          `future one: otherwise burning a baseline back down and re-raising it later passes silently under ` +
          `someone else's ticket. APPEND a new row under the ticket that owns THIS raise — keep the historical ` +
          `row, it is counted rather than replaced, and you should never need to delete or edit one to get past ` +
          `this guard.`,
      );
      continue;
    }
    errors.push(
      `${b.id} (${b.file}) went UP: ${before} -> ${after}. This is a one-way ratchet over ${b.what} — ` +
        `the number is not the defect, the thing it counts is. Fix the new offender so the count comes back down. ` +
        `If this raise is genuinely correct it must be LOUD rather than quiet: add a row to ${LEDGER_PATH} in ` +
        `THIS diff — \`| ${b.id} | ${before} -> ${after} | CPE-NNNN | why this raise is right |\` — and this ` +
        `guard will pass while leaving the raise unmistakable in review. That is not the only red a raise ` +
        `produces: the SAME file's enumeration table carries a "today" cell for ${b.id}, and ` +
        `src/lib/ratchetsDoc.test.ts asserts it against this measurer, so set that cell to ${after} in the ` +
        `same edit or you will hit a second, unrelated-looking failure about a stale table.`,
    );
  }

  return { ok: errors.length === 0, messages, errors };
}

// ---------------------------------------------------------------------------------------------
// Measuring — against the working tree, or against an arbitrary git revision.
// ---------------------------------------------------------------------------------------------

/**
 * Measure every registered baseline in the working tree. A measurer that throws yields an
 * `Unmeasurable` for that one baseline rather than aborting the run: the others still get reported,
 * and `evaluate` turns the failure into a named `::error::` instead of a bare Node stack trace.
 * @returns {Record<string, number | Unmeasurable>}
 */
export function measureWorkingTree() {
  /** @type {Record<string, number | Unmeasurable>} */
  const out = {};
  for (const b of REGISTRY) {
    const p = join(REPO_ROOT, b.file);
    if (!existsSync(p)) {
      out[b.id] = { failed: `${b.file} does not exist — the registry has drifted from the tree.` };
      continue;
    }
    try {
      out[b.id] = b.measure(readFileSync(p, "utf8"));
    } catch (e) {
      out[b.id] = { failed: `${b.file}: ${e instanceof Error ? e.message : String(e)}` };
    }
  }
  return out;
}

/**
 * Paths renamed between `ref` and the working tree, as `new path -> old path`. Used so a baseline
 * whose file was renamed is still read at the base revision instead of looking brand new (F3).
 * @param {string} ref
 * @returns {Map<string, string>}
 */
export function renameMapAtRef(ref) {
  /** @type {Map<string, string>} */
  const map = new Map();
  try {
    const out = execFileSync("git", ["diff", "--find-renames", "--diff-filter=R", "--name-status", ref, "--"], {
      cwd: REPO_ROOT,
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
    });
    for (const line of out.split("\n")) {
      const parts = line.split("\t");
      if (parts.length === 3 && parts[0].startsWith("R")) map.set(parts[2].trim(), parts[1].trim());
    }
  } catch {
    /* no rename information available; the caller treats an unresolved file as an error, not a pass */
  }
  return map;
}

/**
 * Read one repo-relative path at a git revision, or `null` if it does not exist there.
 * @param {string} ref
 * @param {string} file
 * @returns {string | null}
 */
function showAtRef(ref, file) {
  try {
    return execFileSync("git", ["show", `${ref}:${file}`], {
      cwd: REPO_ROOT,
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
      // A missing path is an EXPECTED outcome here (renames, genuinely new files), so git's own
      // "fatal: path ... does not exist" must not leak into the CI log alongside the real `::error::`
      // lines. The absence is reported by returning null, and the caller decides what it means.
      stdio: ["ignore", "pipe", "ignore"],
    });
  } catch {
    return null;
  }
}

/**
 * Measure every registered baseline at a git revision. A baseline whose file did not exist there is
 * reported as `null` (and `evaluate` makes that an error, not a free pass); a baseline whose file IS
 * there but does not measure is reported as `Unmeasurable`, never silently as a number.
 * @param {string} ref
 * @returns {Record<string, number | Unmeasurable | null>}
 */
export function measureAtRef(ref) {
  /** @type {Record<string, number | Unmeasurable | null>} */
  const out = {};
  const renames = renameMapAtRef(ref);
  for (const b of REGISTRY) {
    let path = b.file;
    let src = showAtRef(ref, path);
    if (src === null) {
      const previous = renames.get(b.file);
      if (previous) {
        path = previous;
        src = showAtRef(ref, path);
      }
    }
    if (src === null) {
      out[b.id] = null;
      continue;
    }
    try {
      out[b.id] = b.measure(src);
    } catch (e) {
      out[b.id] = { failed: `${path} at ${ref}: ${e instanceof Error ? e.message : String(e)}` };
    }
  }
  return out;
}

/**
 * The raise ledger at a git revision (empty when the file does not exist there — which is correct:
 * nothing was authorised before the ledger existed).
 * @param {string} ref
 * @returns {LedgerRow[]}
 */
export function ledgerAtRef(ref) {
  const src = showAtRef(ref, LEDGER_PATH);
  return src === null ? [] : parseLedger(src);
}

// ---------------------------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------------------------

/** @returns {void} */
function main() {
  const [cmd, arg] = process.argv.slice(2);

  if (cmd === "print") {
    const head = measureWorkingTree();
    let bad = 0;
    for (const b of REGISTRY) {
      const v = head[b.id];
      const gated = b.unenforced ? "   (enumerated, not gated)" : "";
      if (isUnmeasurable(v)) {
        bad++;
        console.error(`::error::${b.id}: ${v.failed}`);
      } else {
        console.log(`${b.id.padEnd(36)} ${String(v).padStart(5)}   ${b.file}${gated}`);
      }
    }
    if (bad > 0) process.exit(1);
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
    const baseLedger = ledgerAtRef(baseRef);

    const verdict = evaluate(REGISTRY, base, head, ledger, baseLedger);
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

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  try {
    main();
  } catch (e) {
    // Never let an unexpected throw reach the runner as a bare stack trace: CI reads `::error::`.
    console.error(`::error::ratchet guard failed unexpectedly: ${e instanceof Error ? e.stack : String(e)}`);
    process.exit(1);
  }
}
