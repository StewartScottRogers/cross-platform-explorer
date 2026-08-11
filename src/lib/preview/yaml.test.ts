// CPE-1617 (epic CPE-1568 slice 7): bounded-subset YAML parser — ordinary nested maps/sequences/scalars
// parse correctly; anything outside the documented subset degrades EXPLICITLY (unsupported:true) rather
// than being silently mis-parsed, and genuine syntax errors are structurally distinct (unsupported:false).
import { describe, it, expect } from "vitest";
import { parseYaml } from "./yaml";

function ok(raw: string): unknown {
  const r = parseYaml(raw);
  expect(r.ok, r.ok ? "" : `${(r as { error: string }).error}`).toBe(true);
  return (r as { ok: true; value: unknown }).value;
}

function unsupported(raw: string): string {
  const r = parseYaml(raw);
  expect(r.ok).toBe(false);
  expect((r as { unsupported: boolean }).unsupported, "expected unsupported:true").toBe(true);
  return (r as { error: string }).error;
}

function syntaxError(raw: string): string {
  const r = parseYaml(raw);
  expect(r.ok).toBe(false);
  expect((r as { unsupported: boolean }).unsupported, "expected unsupported:false (a real error)").toBe(false);
  return (r as { error: string }).error;
}

describe("parseYaml — happy path (ordinary config shapes)", () => {
  it("parses a flat mapping of scalars", () => {
    expect(ok("name: demo\nport: 8080\nratio: 1.5\nenabled: true\ndisabled: false\nnote: ~")).toEqual({
      name: "demo",
      port: 8080,
      ratio: 1.5,
      enabled: true,
      disabled: false,
      note: null,
    });
  });

  it("parses nested mappings by indentation", () => {
    expect(ok(["server:", "  host: localhost", "  port: 8080", "  tls:", "    enabled: true"].join("\n"))).toEqual({
      server: { host: "localhost", port: 8080, tls: { enabled: true } },
    });
  });

  it("parses a block sequence of scalars", () => {
    expect(ok(["fruits:", "  - apple", "  - banana", "  - cherry"].join("\n"))).toEqual({
      fruits: ["apple", "banana", "cherry"],
    });
  });

  it("parses a block sequence of mappings (the `- key: value` shorthand)", () => {
    const v = ok(
      ["users:", "  - name: ann", "    age: 30", "  - name: bo", "    age: 25"].join("\n"),
    );
    expect(v).toEqual({
      users: [
        { name: "ann", age: 30 },
        { name: "bo", age: 25 },
      ],
    });
  });

  it("parses a sequence nested under a bare dash", () => {
    const v = ok(["matrix:", "  -", "    - 1", "    - 2", "  -", "    - 3", "    - 4"].join("\n"));
    expect(v).toEqual({ matrix: [[1, 2], [3, 4]] });
  });

  it("parses single-line flow sequences and mappings", () => {
    expect(ok("nums: [1, 2, 3]")).toEqual({ nums: [1, 2, 3] });
    expect(ok("point: {x: 1, y: 2}")).toEqual({ point: { x: 1, y: 2 } });
  });

  it("parses quoted scalars with escapes", () => {
    expect(ok(String.raw`msg: "line1\nline2"`)).toEqual({ msg: "line1\nline2" });
    expect(ok("path: 'it''s a test'")).toEqual({ path: "it's a test" });
  });

  it("parses comments and blank lines", () => {
    expect(ok(["# leading comment", "", "a: 1  # trailing", "", "# another"].join("\n"))).toEqual({ a: 1 });
  });

  it("parses a single leading '---' document-start marker", () => {
    expect(ok(["---", "a: 1", "b: 2"].join("\n"))).toEqual({ a: 1, b: 2 });
  });

  it("parses a top-level scalar document", () => {
    expect(ok("42")).toBe(42);
    expect(ok('"hello"')).toBe("hello");
  });

  it("parses an empty document as null", () => {
    expect(ok("")).toBe(null);
  });

  it("a key whose value merely mentions '&' or '*' mid-string is fine (only a LEADING sigil is special)", () => {
    expect(ok('note: "call me * 2, or maybe & something"')).toEqual({
      note: "call me * 2, or maybe & something",
    });
  });
});

describe("parseYaml — deliberately unsupported constructs degrade explicitly with a reason", () => {
  it("anchors", () => {
    expect(unsupported("base: &anchor\n  a: 1")).toMatch(/anchor/i);
  });

  it("aliases", () => {
    expect(unsupported("a: &x 1\nb: *x")).toMatch(/anchor|alias/i);
  });

  it("explicit tags", () => {
    expect(unsupported("a: !!str 123")).toMatch(/tag/i);
  });

  it("a block scalar's explicit numeric indentation indicator (|2) remains unsupported", () => {
    // The plain `|`/`>` forms are now SUPPORTED (finding #8, PR #833 review) — only this rarer explicit
    // form still degrades. See the "block scalars" describe block below for the supported forms.
    expect(unsupported(["desc: |2", "    line one"].join("\n"))).toMatch(/block scalar/i);
  });

  it("complex mapping keys", () => {
    expect(unsupported("? complex key\n: value")).toMatch(/complex/i);
  });

  it("multiple documents", () => {
    expect(unsupported(["a: 1", "---", "b: 2"].join("\n"))).toMatch(/multiple.*document/i);
  });

  it("tab-indented content", () => {
    expect(unsupported("a:\n\tb: 1")).toMatch(/tab/i);
  });

  it("a flow collection spanning multiple lines", () => {
    expect(unsupported("nums: [1, 2,")).toMatch(/multiple lines|multi-line/i);
  });
});

describe("parseYaml — genuine syntax errors are structurally distinct from 'unsupported'", () => {
  it("bad indentation reports a real error, not unsupported", () => {
    const err = syntaxError(["a: 1", "  b: 2"].join("\n"));
    expect(err).toMatch(/unexpected indentation/i);
  });

  it("an unterminated quoted scalar reports a real error", () => {
    expect(syntaxError('a: "unterminated')).toMatch(/unterminated/i);
  });

  it("a line that isn't a mapping entry inside a mapping block reports a real error", () => {
    expect(syntaxError(["a: 1", "not a mapping line"].join("\n"))).toMatch(/expected "key: value"/i);
  });

  it("a duplicate key reports a real error", () => {
    expect(syntaxError("a: 1\na: 2")).toMatch(/duplicate key/i);
  });
});

// CPE-1617 PR #833 review, finding #1: `key: value: value` PREVIOUSLY parsed as `{ok:true, value:{key:
// "value: value"}}` — the classic YAML gotcha every real tool (js-yaml, PyYAML) rejects. Negative
// control: the assertions below fail against the pre-fix code.
describe("parseYaml — finding #1: a bare mapping colon inside a value is rejected", () => {
  it("'a: b: c' is a real syntax error, not a silently accepted string value", () => {
    // Pre-fix: `parseYaml("a: b: c")` returned `{ok:true, value:{a:"b: c"}}`.
    expect(syntaxError("a: b: c")).toMatch(/mapping values are not allowed/i);
  });

  it("a trailing bare colon in a value is also rejected", () => {
    expect(syntaxError("a: ends with colon:")).toMatch(/mapping values are not allowed/i);
  });

  it("quoting the value sidesteps the rejection, as real YAML requires", () => {
    expect(ok('a: "b: c"')).toEqual({ a: "b: c" });
  });

  it("a colon NOT followed by a space (a URL) is unaffected", () => {
    expect(ok("url: http://example.com:8080/path")).toEqual({ url: "http://example.com:8080/path" });
  });

  it("a time-like plain scalar (colon-digit, no space) is unaffected", () => {
    expect(ok("time: 07:32:00")).toEqual({ time: "07:32:00" });
  });
});

// CPE-1617 PR #833 review, finding #6: an indentless sequence PREVIOUSLY reported "unexpected
// indentation" — standard, spec-legal YAML, found on a real 53-line file
// (`gateway/assets/status_phrases.yaml`). Negative control: the first assertion fails against the
// pre-fix code with that exact message.
describe("parseYaml — finding #6: indentless sequences", () => {
  it("a sequence at the SAME indent as its own key parses instead of erroring", () => {
    // Pre-fix: `parseYaml("status:\n- item\n- item")` returned a syntax error, "Line 2: unexpected
    // indentation".
    expect(ok(["status:", "- item", "- item"].join("\n"))).toEqual({ status: ["item", "item"] });
  });

  it("a sibling key at the same indent after the indentless sequence continues the mapping", () => {
    expect(ok(["status:", "- a", "- b", "next_key: 5"].join("\n"))).toEqual({
      status: ["a", "b"],
      next_key: 5,
    });
  });

  it("an indentless sequence of mappings (the dash-shorthand) also parses", () => {
    const v = ok(["items:", "- name: x", "  qty: 1", "- name: y", "  qty: 2"].join("\n"));
    expect(v).toEqual({
      items: [
        { name: "x", qty: 1 },
        { name: "y", qty: 2 },
      ],
    });
  });

  it("a normally-indented sequence is unaffected by the fix", () => {
    expect(ok(["fruits:", "  - apple", "  - banana"].join("\n"))).toEqual({ fruits: ["apple", "banana"] });
  });

  it("a sibling key after an indentless sequence nested inside another mapping still works", () => {
    const v = ok(["outer:", "  status:", "  - a", "  - b", "  other: 1"].join("\n"));
    expect(v).toEqual({ outer: { status: ["a", "b"], other: 1 } });
  });
});

// CPE-1617 PR #833 review, finding #7: a double-quoted value legitimately wrapping across two physical
// lines (YAML line-folding) PREVIOUSLY reported "unterminated double-quoted string" — found on a real
// 403-line Afrikaans locale file (`locales/af.yaml`), which PyYAML parses to a clean 2-key dict.
// Negative control: the first assertion fails against the pre-fix code.
describe("parseYaml — finding #7: multi-line double-quoted scalar folding", () => {
  it("a double-quoted value wrapping across two physical lines folds instead of erroring", () => {
    // Pre-fix: reported "unterminated double-quoted string" on this ordinary YAML shape.
    const v = ok(['msg: "some very long text', 'that continues on the next line"'].join("\n"));
    expect(v).toEqual({ msg: "some very long text that continues on the next line" });
  });

  it("folds across more than two lines, stripping each continuation line's leading whitespace", () => {
    const v = ok(['msg: "line one', '  line two', '  line three"'].join("\n"));
    expect(v).toEqual({ msg: "line one line two line three" });
  });

  it("a '#' inside a folded quote is literal content, never treated as a comment", () => {
    const v = ok(['msg: "text with a # inside', 'more text"'].join("\n"));
    expect(v).toEqual({ msg: "text with a # inside more text" });
  });

  it("an ordinary single-line quoted value is unaffected", () => {
    expect(ok('a: "one line"')).toEqual({ a: "one line" });
  });

  it("a genuinely unterminated quote (never closes) still reports a real error, not a silent guess", () => {
    expect(syntaxError('a: "never closes\nb: 2')).toMatch(/unterminated/i);
  });
});

// CPE-1617 PR 833 review round 2: `foldMultilineDoubleQuoted` originally joined EVERY physical line
// with a plain space, including blank ones — a silent WRONG VALUE (not an error, not a degrade), since
// real YAML folds a blank line to a newline, not a space. Cross-checked against PyYAML: N consecutive
// blank continuation lines fold to exactly N newlines (an ordinary non-blank break still folds to one
// space). Each test below is a negative control: with the pre-fix `parts.join(" ")` behaviour, every
// expected `\n` below would instead be a plain `" "` (a run of spaces for N>1), which is what shipped
// untested and is exactly the scenario (a multi-paragraph translation-catalog string) that justified
// adding line-folding support in the first place.
describe("parseYaml — finding #7 follow-up: blank-line folding inside a double-quoted scalar", () => {
  it("one blank line folds to exactly one newline (not a space)", () => {
    // Pre-fix (`parts.join(" ")`): "line one line two" (a single space, no newline at all).
    const v = ok(['msg: "line one', "", 'line two"'].join("\n"));
    expect(v).toEqual({ msg: "line one\nline two" });
  });

  it("two consecutive blank lines fold to exactly two newlines", () => {
    // Pre-fix: "line one  line two" (two spaces, no newlines).
    const v = ok(['msg: "line one', "", "", 'line two"'].join("\n"));
    expect(v).toEqual({ msg: "line one\n\nline two" });
  });

  it("three consecutive blank lines fold to exactly three newlines", () => {
    // Pre-fix: "line one   line two" (three spaces, no newlines).
    const v = ok(['msg: "line one', "", "", "", 'line two"'].join("\n"));
    expect(v).toEqual({ msg: "line one\n\n\nline two" });
  });

  it("a blank line at the very START of the folded region (right after the opening quote) folds correctly", () => {
    // Pre-fix: " text" (a leading space, not a leading newline).
    const v = ok(['msg: "', "", 'text"'].join("\n"));
    expect(v).toEqual({ msg: "\ntext" });
  });

  it("a blank line at the very END of the folded region (right before the closing quote) folds correctly", () => {
    // Pre-fix: "text " (a trailing space, not a trailing newline).
    const v = ok(['msg: "text', "", '"'].join("\n"));
    expect(v).toEqual({ msg: "text\n" });
  });

  it("a whitespace-only line (spaces, no other characters) counts as blank, same as a truly empty line", () => {
    // Decision (documented in foldMultilineDoubleQuoted's doc comment): a line that's ALL whitespace is
    // indistinguishable from a zero-length line once its (only) leading whitespace is stripped — real
    // YAML/PyYAML treats them identically, so this parser does too.
    const v = ok(['msg: "line one', "   ", 'line two"'].join("\n"));
    expect(v).toEqual({ msg: "line one\nline two" });
  });

  it("an ordinary non-blank-adjacent break still folds to a single space (not disturbed by the fix)", () => {
    const v = ok(['msg: "line one', 'line two"'].join("\n"));
    expect(v).toEqual({ msg: "line one line two" });
  });
});

// CPE-1617 PR #833 review, finding #8: block scalars were assessed as tractable (unlike anchors/
// aliases/tags/complex-keys) and implemented — the single biggest real-world driver of "valid YAML
// degrades to plain text" (every GitHub Actions `run: |` step).
describe("parseYaml — finding #8: block scalars (| literal, > folded, chomping indicators)", () => {
  it("a literal block scalar (|) preserves internal line breaks, with default clip chomping", () => {
    const v = ok(["run: |", "  echo hello", "  echo world"].join("\n"));
    expect(v).toEqual({ run: "echo hello\necho world\n" });
  });

  it("a folded block scalar (>) joins lines with a single space", () => {
    const v = ok(["desc: >", "  line one", "  line two"].join("\n"));
    expect(v).toEqual({ desc: "line one line two\n" });
  });

  it("strip chomping (|-) removes the trailing newline entirely", () => {
    expect(ok(["run: |-", "  echo hello"].join("\n"))).toEqual({ run: "echo hello" });
  });

  it("a block scalar's more-indented lines keep their extra leading whitespace as literal text", () => {
    const v = ok(["run: |", "  if true; then", "    echo yes", "  fi"].join("\n"));
    expect(v).toEqual({ run: "if true; then\n  echo yes\nfi\n" });
  });

  it("an internal blank line inside a literal block scalar is preserved", () => {
    const v = ok(["text: |", "  para one", "", "  para two"].join("\n"));
    expect(v).toEqual({ text: "para one\n\npara two\n" });
  });

  it("a folded block scalar's internal blank line becomes a real (paragraph-break) newline, not a space", () => {
    // PR 833 review round 3: this expectation was WRONG until now — it asserted "para one\n\npara two\n"
    // (two newlines), matching the pre-fix `foldBlockLines` bug (which double-counted: once entering the
    // blank line, once leaving it) rather than the real YAML rule. A test written by reading the buggy
    // code instead of checking against PyYAML/the spec just protects the bug — see the dedicated
    // describe block below for the full negative-control coverage this shipped without.
    const v = ok(["text: >", "  para one", "", "  para two"].join("\n"));
    expect(v).toEqual({ text: "para one\npara two\n" });
  });

  it("a block scalar is correctly followed by a sibling key at the header's own indent", () => {
    expect(ok(["run: |", "  echo hi", "next: 5"].join("\n"))).toEqual({ run: "echo hi\n", next: 5 });
  });

  it("a bare dash sequence item can itself be a block scalar", () => {
    const v = ok(["items:", "  - |", "    line1", "    line2", "  - |", "    other"].join("\n"));
    expect(v).toEqual({ items: ["line1\nline2\n", "other\n"] });
  });

  it("the dash-shorthand's inline key can itself be block-scalar-valued", () => {
    const v = ok(["steps:", "  - run: |", "      echo one", "    name: step1"].join("\n"));
    expect(v).toEqual({ steps: [{ run: "echo one\n", name: "step1" }] });
  });

  it("an empty block scalar (no indented content follows) yields an empty string", () => {
    expect(ok(["a: |", "b: 2"].join("\n"))).toEqual({ a: "", b: 2 });
  });

  it("a real GitHub Actions 'run: |' shape parses end to end", () => {
    const v = ok(
      ["name: CI", "jobs:", "  build:", "    steps:", "      - run: |", "          npm ci", "          npm test"].join(
        "\n",
      ),
    );
    expect(v).toEqual({ name: "CI", jobs: { build: { steps: [{ run: "npm ci\nnpm test\n" }] } } });
  });
});

// CPE-1617 PR 833 review round 3: `foldBlockLines` (the `>` folded-style joiner) emitted TWO newlines
// per blank line instead of one — double-counting: once "entering" the blank line, once "leaving" it.
// Verified wrong against PyYAML on two real files in this repo (`hermes-agent/plugins/platforms/{ntfy,
// photon}/plugin.yaml`, both real `description: >` fields with one blank line between two paragraphs).
// The bug was compounded by a test (fixed above, in the "finding #8" block) that had been written to
// match the buggy output rather than the spec, so the suite actively protected it. Now unified with
// double-quoted flow-scalar folding behind one shared `foldYamlLines` (see its own doc comment) — each
// test below is a negative control: with the pre-fix `foldBlockLines`, every expected single `\n` below
// would instead be `\n\n` (one extra).
describe("parseYaml — finding #8 follow-up: blank-line folding inside a folded (>) block scalar", () => {
  it("two consecutive blank lines fold to exactly two newlines", () => {
    // Pre-fix (`foldBlockLines`): "para one\n\n\npara two\n" (three newlines, one too many).
    const v = ok(["text: >", "  para one", "", "", "  para two"].join("\n"));
    expect(v).toEqual({ text: "para one\n\npara two\n" });
  });

  it("three consecutive blank lines fold to exactly three newlines", () => {
    // Pre-fix: "para one\n\n\n\npara two\n" (four newlines, one too many).
    const v = ok(["text: >", "  para one", "", "", "", "  para two"].join("\n"));
    expect(v).toEqual({ text: "para one\n\n\npara two\n" });
  });

  it("a blank line at the very START of the folded region (before any content) folds correctly", () => {
    // Reachable for a block scalar (unlike a double-quoted scalar, whose first "line" is always the
    // non-blank header) — a blank line right after the `>` header, before the first indented line.
    // Pre-fix: "\n\npara one\n" (a leading blank contributed via `foldBlockLines`'s own logic, but
    // still double-counted relative to the single blank line actually present).
    const v = ok(["text: >", "", "  para one"].join("\n"));
    expect(v).toEqual({ text: "\npara one\n" });
  });

  it("a trailing blank line is trimmed before folding, not turned into an extra newline", () => {
    // Trailing blank lines were already correctly stripped before chomping (pre-existing, confirmed-good
    // behaviour) — this pins that it stays correct after unifying the fold logic.
    const v = ok(["text: >", "  para one", "  para two", ""].join("\n"));
    expect(v).toEqual({ text: "para one para two\n" });
  });

  it("a whitespace-only line (spaces, no other characters) counts as blank, same as a truly empty line", () => {
    const v = ok(["text: >", "  para one", "  ", "  para two"].join("\n"));
    expect(v).toEqual({ text: "para one\npara two\n" });
  });

  it("an ordinary non-blank-adjacent break still folds to a single space (not disturbed by the fix)", () => {
    const v = ok(["text: >", "  line one", "  line two"].join("\n"));
    expect(v).toEqual({ text: "line one line two\n" });
  });
});

describe("parseYaml — untrusted-input safety", () => {
  it("a '__proto__' key never pollutes Object.prototype", () => {
    const v = ok("__proto__:\n  polluted: true") as Record<string, unknown>;
    expect(({} as Record<string, unknown>).polluted).toBeUndefined();
    expect(v["__proto__"]).toEqual({ polluted: true });
  });

  it("deeply nested block sequences fail cleanly instead of overflowing the stack", () => {
    const depth = 3000;
    const lines: string[] = [];
    for (let i = 0; i < depth; i++) lines.push("  ".repeat(i) + "-");
    lines.push("  ".repeat(depth) + "1");
    expect(() => parseYaml(lines.join("\n"))).not.toThrow();
    const r = parseYaml(lines.join("\n"));
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error).toMatch(/nested too deeply/i);
  });

  it("deeply nested flow collections on one line fail cleanly instead of overflowing the stack", () => {
    const depth = 5000;
    const raw = "a: " + "[".repeat(depth) + "1" + "]".repeat(depth);
    expect(() => parseYaml(raw)).not.toThrow();
    const r = parseYaml(raw);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error).toMatch(/nested too deeply/i);
  });

  it("a large flat sequence (many siblings, not nesting) parses without error", () => {
    const lines = ["items:"];
    for (let i = 0; i < 20000; i++) lines.push(`  - ${i}`);
    const r = parseYaml(lines.join("\n"));
    expect(r.ok).toBe(true);
    if (r.ok) expect((r.value as { items: unknown[] }).items.length).toBe(20000);
  });
});
