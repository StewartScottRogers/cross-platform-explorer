/**
 * CPE-1627/CPE-1634 regrowth guard: keeps `showNotice()` calls in App.svelte routed through `$t()`, so
 * the localisation debt those tickets paid off (45 raw-literal calls, then 62 templated calls + one
 * literal hiding in a multi-line ternary) can't silently regrow.
 *
 * CPE-1627's original guard was a single-line regex (`/showNotice\(\s*["']/`) that only caught a raw
 * string literal sitting immediately after the open paren — by design it never looked at a template
 * literal (`` showNotice(`...`) ``), and being line-based it could never see a literal hiding behind a
 * ternary or spanning multiple lines (exactly the CPE-1634 bug at `src/App.svelte` — the
 * `applySelectBy` "No items match that criterion." literal survived BOTH CPE-1627's own sweep and its
 * guard because it wasn't the first token after `(`). CPE-1634 replaces the regex with a real AST walk
 * (via the `typescript` compiler API — already a devDependency, no new one added) so both literal forms
 * are caught regardless of how they're nested in the expression tree.
 *
 * The walk only descends into subtrees that can actually BECOME the runtime message value — a
 * conditional's two branches, `+`/`||`/`??`/`&&` operands, parenthesized/`as`/`!` wrappers — and treats
 * any `$t(...)` call as an opaque, already-translated leaf it never opens. It deliberately does NOT
 * descend into a comparison's operands (`typeof e === "string"` must never flag "string") or into an
 * arbitrary function call's arguments (`String(e)` must never flag its input) — those aren't the
 * expression's own literal value.
 *
 * Escape hatch: a call that is genuinely not user-facing can be exempted by appending
 * `// i18n-exempt: <reason>` on the SAME line as the `showNotice(` call. CPE-1627's escape hatch was a
 * bare `!line.includes("i18n-exempt")` substring test — defeatable by embedding the marker INSIDE the
 * user-facing string itself (`showNotice("real text i18n-exempt smuggled in string")` passed the old
 * guard outright, PR #825 review). This version strips every string/template literal's contents out of
 * the line FIRST, then looks for the marker in what's left — so a marker can only count if it sits in
 * real `//` comment syntax with a non-empty reason, never inside a string.
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import ts from "typescript";

const appSveltePath = path.join(path.dirname(fileURLToPath(import.meta.url)), "App.svelte");

interface Offender {
  line: number;
  text: string;
}

/** Pull the <script> block out of a .svelte file, padding everything else to blank so line numbers in
 *  the parsed AST line up 1:1 with the original file (good enough for a `lang="ts"` script block). */
function extractScript(source: string): string {
  const openTag = source.indexOf("<script");
  const closeTag = source.indexOf("</script>");
  const contentStart = source.indexOf(">", openTag) + 1;
  return source.slice(0, contentStart).replace(/[^\n]/g, " ") + source.slice(contentStart, closeTag);
}

function isDollarT(node: ts.Node): boolean {
  return ts.isCallExpression(node) && ts.isIdentifier(node.expression) && node.expression.text === "$t";
}

const VALUE_FLOW_BINARY_OPS = new Set<ts.SyntaxKind>([
  ts.SyntaxKind.PlusToken,
  ts.SyntaxKind.BarBarToken,
  ts.SyntaxKind.QuestionQuestionToken,
  ts.SyntaxKind.AmpersandAmpersandToken,
]);

/** Collect every "naked" (not `$t(...)`-wrapped) string/template literal that could actually become
 *  `node`'s runtime value — recursing only through value-preserving constructs. */
function collectNakedLiterals(node: ts.Node, out: ts.Node[]): void {
  if (isDollarT(node)) return; // translated — opaque, never descend
  if (ts.isStringLiteralLike(node) || ts.isTemplateExpression(node)) {
    out.push(node);
    return;
  }
  if (ts.isConditionalExpression(node)) {
    collectNakedLiterals(node.whenTrue, out);
    collectNakedLiterals(node.whenFalse, out);
    return; // the condition itself never carries message text
  }
  if (ts.isParenthesizedExpression(node)) {
    collectNakedLiterals(node.expression, out);
    return;
  }
  if (ts.isBinaryExpression(node) && VALUE_FLOW_BINARY_OPS.has(node.operatorToken.kind)) {
    collectNakedLiterals(node.left, out);
    collectNakedLiterals(node.right, out);
    return;
  }
  if (ts.isAsExpression(node) || ts.isNonNullExpression(node)) {
    collectNakedLiterals(node.expression, out);
    return;
  }
  // Anything else — identifiers, property/element access, arbitrary call expressions (e.g. `String(e)`,
  // `typeof e === "string"`'s comparison itself, `.toLowerCase()`) — is opaque: it isn't statically the
  // argument's literal value, so don't descend (this is exactly what keeps `typeof e === "string"` from
  // false-positiving on the word "string").
}

/** True when `line` exempts an offender on it via a REAL trailing `// i18n-exempt: <reason>` comment —
 *  not a marker merely sitting inside a string/template literal on that line (the CPE-1634 bypass). */
function isGenuinelyExempt(line: string): boolean {
  const codeOnly = line.replace(/"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|`(?:[^`\\]|\\.)*`/g, '""');
  const commentIdx = codeOnly.indexOf("//");
  if (commentIdx === -1) return false;
  return /i18n-exempt:\s*\S/.test(codeOnly.slice(commentIdx));
}

/** Walk `source` (a full .svelte file's text) and return every un-exempted `showNotice(...)` call whose
 *  first argument resolves to a naked string/template literal. */
function findOffenders(source: string): Offender[] {
  const script = extractScript(source);
  const sourceFile = ts.createSourceFile(appSveltePath, script, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const lines = source.split("\n");
  const offenders: Offender[] = [];

  function visit(node: ts.Node): void {
    if (
      ts.isCallExpression(node) &&
      ts.isIdentifier(node.expression) &&
      node.expression.text === "showNotice" &&
      node.arguments.length > 0
    ) {
      const naked: ts.Node[] = [];
      collectNakedLiterals(node.arguments[0], naked);
      if (naked.length > 0) {
        const lineNo = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1;
        const lineText = lines[lineNo - 1] ?? "";
        if (!isGenuinelyExempt(lineText)) {
          offenders.push({ line: lineNo, text: node.getText(sourceFile).replace(/\s+/g, " ").slice(0, 160) });
        }
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(sourceFile);
  return offenders;
}

/** CPE-1627's ORIGINAL line-based regex check, kept here only to demonstrate — in the tests below — that
 *  it's blind to exactly the cases the AST walk above catches (template literals, ternary-hidden
 *  literals, and the exemption-marker-inside-a-string bypass). Not used as the real guard anymore. */
function findOffendersOldRegex(source: string): Offender[] {
  const offenders: Offender[] = [];
  source.split("\n").forEach((line, i) => {
    if (/showNotice\(\s*["']/.test(line) && !line.includes("i18n-exempt")) {
      offenders.push({ line: i + 1, text: line.trim() });
    }
  });
  return offenders;
}

describe("showNotice() i18n regrowth guard (CPE-1627/CPE-1634)", () => {
  it("App.svelte has no un-exempted showNotice(...) call carrying a raw string or template literal", () => {
    const source = readFileSync(appSveltePath, "utf-8");
    const offenders = findOffenders(source);
    expect(offenders).toEqual([]);
  });

  it("flags a newly-added raw string literal (RED case, the original CPE-1627 shape)", () => {
    const snippet = `
      <script lang="ts">
        function f() {
          showNotice("Brand new hardcoded English text");
        }
      </script>
    `;
    const offenders = findOffenders(snippet);
    expect(offenders).toHaveLength(1);
    expect(offenders[0].text).toContain("Brand new hardcoded English text");
  });

  it("flags a newly-added TEMPLATE literal (the CPE-1634 extension — old regex guard misses this)", () => {
    const snippet = `
      <script lang="ts">
        function f(name: string) {
          showNotice(\`Couldn't copy \${name} to the destination.\`);
        }
      </script>
    `;
    // Demonstrates the gap this ticket closes: the pre-CPE-1634 guard is structurally blind to a
    // template literal (it only matches a quote character right after the open paren).
    expect(findOffendersOldRegex(snippet)).toEqual([]);
    // The hardened guard catches it.
    const offenders = findOffenders(snippet);
    expect(offenders).toHaveLength(1);
    expect(offenders[0].text).toContain("Couldn't copy");
  });

  it("flags a literal hidden behind a ternary, even split across lines (the CPE-1634 review-found bug's exact shape)", () => {
    const snippet = `
      <script lang="ts">
        function f(idx: number[]) {
          showNotice(
            idx.length === 0
              ? "No items match that criterion."
              : \`Selected \${idx.length} items.\`,
          );
        }
      </script>
    `;
    // The pre-CPE-1634 line-based guard can't see either branch: the condition, not a quote, follows
    // "showNotice(" on its own line, and the literal itself is several lines down.
    expect(findOffendersOldRegex(snippet)).toEqual([]);
    const offenders = findOffenders(snippet);
    expect(offenders).toHaveLength(1); // one showNotice() call, flagged once, even though BOTH branches are naked
  });

  it("does not false-positive on a typeof/string comparison next to a real $t()-wrapped branch", () => {
    // Real shape from App.svelte's ejectDrive(): the backend can throw a plain string; when it does,
    // it's shown as-is (not translatable — it's dynamic backend text), otherwise a translated fallback
    // fires. A naive "any string literal near showNotice" scan would wrongly flag the `"string"` in the
    // typeof check.
    const snippet = `
      <script lang="ts">
        function f(e: unknown, name: string) {
          showNotice(typeof e === "string" ? e : $t("notice.ejectFailed", { name }), true);
        }
      </script>
    `;
    expect(findOffenders(snippet)).toEqual([]);
  });

  it("closes the reported bypass: an i18n-exempt marker EMBEDDED IN THE STRING no longer exempts it (RED -> GREEN)", () => {
    // The exact bypass PR #825's reviewer demonstrated against CPE-1627's guard: smuggling the marker
    // inside the user-facing string itself, rather than a real trailing comment.
    const snippet = `
      <script lang="ts">
        function f() {
          showNotice("This is real English user-facing text i18n-exempt smuggled in string");
        }
      </script>
    `;
    // RED (documented, not asserted as desirable): the OLD guard's bare substring test was fooled —
    // it saw "i18n-exempt" anywhere on the line, including inside the string, and stayed silent.
    expect(findOffendersOldRegex(snippet)).toEqual([]);
    // GREEN: the hardened guard strips string contents before looking for the marker, so the embedded
    // text no longer counts as an exemption — the offender is still caught.
    const offenders = findOffenders(snippet);
    expect(offenders).toHaveLength(1);
  });

  it("still honors a GENUINE // i18n-exempt: <reason> trailing comment", () => {
    const snippet = `
      <script lang="ts">
        function f(debugMsg: string) {
          showNotice("Internal debug state, never shown to a real user", true); // i18n-exempt: debug-only path, dev builds only
        }
      </script>
    `;
    expect(findOffenders(snippet)).toEqual([]);
  });

  it("rejects an exemption comment with no actual reason (loud escape hatch, not a silent one)", () => {
    const snippet = `
      <script lang="ts">
        function f() {
          showNotice("Some text"); // i18n-exempt
        }
      </script>
    `;
    // No ":" + reason after the marker — doesn't count. Keeps the hatch from becoming a bare toggle.
    expect(findOffenders(snippet)).toHaveLength(1);
  });
});
