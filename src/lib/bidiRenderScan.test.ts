// CPE-1757 round 2: direct unit evidence that the whitelist engine (bidiRenderScan.ts) catches every
// shape the review's 17-shape probe component exercised — not asserted, reproduced. Each `it` mirrors
// one probe shape verbatim (or as close as a standalone expression allows) so a future reader can check
// the claim against the actual behavior, not against prose.
import { describe, it, expect } from "vitest";
import { isSafeExpr, findUnsafeRenderLines, RenderScanError } from "./bidiRenderScan";

describe("isSafeExpr — the whitelist: literal, displaySafe* call, or unsafe", () => {
  it("accepts plain literals", () => {
    expect(isSafeExpr('"hello"')).toBe(true);
    expect(isSafeExpr("'hello'")).toBe(true);
    expect(isSafeExpr("42")).toBe(true);
    expect(isSafeExpr("true")).toBe(true);
    expect(isSafeExpr("false")).toBe(true);
    expect(isSafeExpr("null")).toBe(true);
    expect(isSafeExpr("undefined")).toBe(true);
  });

  it("accepts a bare displaySafeName/displaySafePath call", () => {
    expect(isSafeExpr("displaySafeName(entry.name)")).toBe(true);
    expect(isSafeExpr("displaySafePath(entry.path)")).toBe(true);
    expect(isSafeExpr("displaySafeName(baseName(root))")).toBe(true);
  });

  it("accepts an ||/?? OR-chain of safe calls (FileNameSearchDialog's own fallback shape)", () => {
    expect(isSafeExpr("displaySafeName(baseName(root)) || displaySafePath(root)")).toBe(true);
    expect(isSafeExpr('displaySafePath(opTo(op) ?? "")')).toBe(true);
  });

  // Known, deliberate limitation (documented in bidiRenderScan.ts's header): this engine does not parse
  // ternary structure, so a condition that references a real identifier (not just literals) makes the
  // WHOLE expression register as "still has an identifier outside a safe call" — a false POSITIVE, not a
  // false negative. That is the safe direction to be wrong in: it costs an allowlist entry instead of a
  // missed spoof. None of this PR's fixes need an inline ternary with a variable condition; if one ever
  // does, it either gets rewritten as `{#if}`/`{:else}` (which this engine handles natively, since each
  // branch is scanned as its own separate render) or the line is allowlisted with that reason recorded.
  it("documents that a ternary's CONDITION identifier also trips the check (false positive, not a miss)", () => {
    expect(isSafeExpr('kind === "dir" ? displaySafeName(a) : displaySafePath(b)')).toBe(false);
    // Rewritten as Svelte control flow instead of an inline ternary, each branch is its own mustache and
    // is checked independently — the idiom this repo already uses (ConflictDialog/CheckpointDialog).
    expect(findUnsafeRenderLines(`{#if kind === "dir"}{displaySafeName(a)}{:else}{displaySafePath(b)}{/if}`)).toEqual([]);
  });

  // --- The 17-shape probe, reproduced -----------------------------------------------------------
  it("shape: template-literal interpolation — `Deleting ${entry.name}`", () => {
    expect(isSafeExpr("`Deleting ${entry.name}`")).toBe(false);
  });

  it("shape: an intermediate variable — const n = entry.name; {n}", () => {
    expect(isSafeExpr("n")).toBe(false);
  });

  it("shape: a differently-named local — {fileName} (InspectCryptoDialog's own variable name)", () => {
    expect(isSafeExpr("fileName")).toBe(false);
  });

  it("shape: {@html entry.name} — handled by findUnsafeRenderLines's @html branch; the bare expr is unsafe", () => {
    expect(isSafeExpr("entry.name")).toBe(false);
  });

  it("shape: destructured each-binding — {#each files as { name }} then {name}", () => {
    expect(isSafeExpr("name")).toBe(false);
  });

  it("shape: parentDir(...)", () => {
    expect(isSafeExpr("parentDir(x)")).toBe(false);
  });

  it("shape: splitPath(p).at(-1)", () => {
    expect(isSafeExpr("splitPath(p).at(-1)")).toBe(false);
  });

  it("shape: p.split('/').pop()", () => {
    expect(isSafeExpr('p.split("/").pop()')).toBe(false);
  });

  it("shape: a locally-named helper — baseOf(...) / base(p) (not literally 'baseName'/'basename')", () => {
    expect(isSafeExpr("baseOf(path)")).toBe(false);
    expect(isSafeExpr("base(p)")).toBe(false);
  });

  it("shape: entry.oldName / entry.fullPath — any property name, not just .name/.path", () => {
    expect(isSafeExpr("entry.oldName")).toBe(false);
    expect(isSafeExpr("entry.fullPath")).toBe(false);
  });

  it("a displaySafe call nested inside a template literal IS safe; an unwrapped one beside it is not", () => {
    expect(isSafeExpr("`${displaySafeName(entry.name)}`")).toBe(true);
    expect(isSafeExpr("`${displaySafeName(entry.name)} (${entry.path})`")).toBe(false);
  });
});

describe("findUnsafeRenderLines — render-position gating + the {#if}-adjacency fix", () => {
  it("flags plain text content between tags", () => {
    const src = `<span>{entry.name}</span>`;
    expect(findUnsafeRenderLines(src)).toEqual(["1:entry.name"]);
  });

  it("flags title=/aria-label=/alt= given directly as attr={expr}", () => {
    expect(findUnsafeRenderLines(`<span title={entry.path}>x</span>`)).toEqual(["1:entry.path"]);
    expect(findUnsafeRenderLines(`<img alt={entry.name} />`)).toEqual(["1:entry.name"]);
    expect(findUnsafeRenderLines(`<div aria-label={entry.name}>x</div>`)).toEqual(["1:entry.name"]);
  });

  it("flags a raw name/path embedded in a quoted title=\"…{expr}…\" value", () => {
    expect(findUnsafeRenderLines('<span aria-label="Diff for {baseOf(path)}">x</span>')).toEqual(["1:baseOf(path)"]);
  });

  // Regression: a component's shorthand-prop attribute list (extremely common in this codebase, e.g.
  // `<Sidebar {density} {currentPath} … />`) must NOT be mistaken for a run of text-content renders just
  // because each `{…}` sits right after the previous one's `}` — the very ambiguity the {#if}-adjacency
  // fix (above) introduced, caught by dry-running this engine over App.svelte before wiring it into CI.
  it("does NOT flag consecutive shorthand props inside a component's attribute list", () => {
    const src = [
      `<Sidebar`,
      `  {density}`,
      `  {currentPath}`,
      `  {entry}`,
      `/>`,
    ].join("\n");
    expect(findUnsafeRenderLines(src)).toEqual([]);
  });

  it("still flags title=/aria-label=/alt= given as attr={expr} even among other shorthand props on the same tag", () => {
    const src = `<Sidebar {density} title={entry.path} {currentPath} />`;
    expect(findUnsafeRenderLines(src)).toEqual(["1:entry.path"]);
  });

  it("flags {@html entry.name}", () => {
    expect(findUnsafeRenderLines(`<div>{@html entry.name}</div>`)).toEqual(["1:entry.name"]);
  });

  // The exact miss named in review: a render sitting right after {#if}/{:else}/{/each} rather than `>`.
  it("flags a render immediately adjacent to {#if}'s own closing brace, not just after `>`", () => {
    const src = `<div>{#if show}{entry.name}{/if}</div>`;
    expect(findUnsafeRenderLines(src).length).toBeGreaterThan(0);
  });

  it("flags a render immediately adjacent to {:else}", () => {
    const src = `<div>{#if a}{b}{:else}{entry.path}{/if}</div>`;
    expect(findUnsafeRenderLines(src).length).toBeGreaterThan(0);
  });

  it("does NOT flag the #if/#each/@const control tags themselves (their condition/binding isn't drawn)", () => {
    const src = `{#if entry.path}<span>ok</span>{/if}\n{#each list as entry (entry.path)}<span>{entry.id}</span>{/each}`;
    // `entry.id` (not name/path-shaped, but still a bare, unwrapped identifier) IS correctly flagged —
    // proves the control-tag EXEMPTION is real, not just "this file happens to have nothing risky".
    expect(findUnsafeRenderLines(src)).toEqual(["2:entry.id"]);
  });

  it("does NOT flag bind:value, a component prop pass-through, or an event handler (documented boundary, UAT-confirmed)", () => {
    const src = [
      `<input bind:value={path} />`,
      `<Foo path={sbs.path} />`,
      `<Bar {path} />`,
      `<button on:click={() => select(f.path)}>x</button>`,
      `<span data-fullpath={e.path}>x</span>`,
    ].join("\n");
    expect(findUnsafeRenderLines(src)).toEqual([]);
  });

  it("does NOT flag script/style block content", () => {
    const src = [
      `<script>`,
      `  const entry = { name: "x" };`,
      `  function f() { return entry.name; }`,
      `</script>`,
      `<style>.x { content: "entry.name"; }</style>`,
      `<span>ok</span>`,
    ].join("\n");
    expect(findUnsafeRenderLines(src)).toEqual([]);
  });

  it("does NOT flag a wrapped render, and reports the correct line for a real spoof case", () => {
    const src = [
      `<div>`,
      `  <span title={displaySafePath(f.path)}>{displaySafeName(f.name)}</span>`,
      `  <span title={g.path}>{g.name}</span>`,
      `</div>`,
    ].join("\n");
    // Both offenders on line 3 are now distinct entries (line:expr), not collapsed into a single "3" —
    // a second, previously-silent consequence of the line-number-only Set fixed by CPE-1761 #2.
    expect(findUnsafeRenderLines(src)).toEqual(["3:g.name", "3:g.path"]);
  });
});

// CPE-1761 #1 + #3: round 2's engine fell back to `i = markup.length` whenever it hit a `{` it couldn't
// close, silently ending the scan of the WHOLE REST OF THE FILE and reporting whatever was found so far —
// which, for a file whose only offenders sit below the trap, is `[]`: indistinguishable from "clean", the
// most dangerous possible output for a guard whose entire job is to catch what a human missed. The fix
// makes the scan fail LOUDLY (throw `RenderScanError`, naming the file and position) instead of guessing.
// Reproduced with the ticket's own repro table: a clean baseline (2 real offenders, caught), then the
// same file with a trap inserted above them — before the fix these all silently degraded to `[]`.
describe("CPE-1761: the scan fails loudly instead of silently truncating (fail-open) on markup it can't parse", () => {
  const twoRawRendersBelow = (line1: string) => [line1, "{entry.name}", "{other.path}"].join("\n");

  it("control: baseline-two-raw — a normal file with two raw renders is caught, not suppressed", () => {
    const src = twoRawRendersBelow(`<div title="x">`);
    expect(findUnsafeRenderLines(src)).toEqual(["2:entry.name", "3:other.path"]);
  });

  it("#1 brace-in-text: a lone unmatched '{' in ordinary prose above two real offenders must RED (throw), not report []", () => {
    const src = twoRawRendersBelow(`<p>use { as a brace</p>`);
    // The defect this guards against: silently returning [] here is WORSE than doing nothing, because it
    // reads as "clean" to anyone trusting the guard. It must be impossible to get an empty result from
    // this input — either the two real offenders are reported, or the scan must refuse to answer at all.
    let threw: unknown;
    try {
      findUnsafeRenderLines(src, "brace-in-text.svelte");
    } catch (e) {
      threw = e;
    }
    expect(threw, "an unmatched '{' must throw rather than silently return an offender list").toBeInstanceOf(RenderScanError);
    const message = (threw as Error).message;
    // The REASON must be legible: which file, that it's specifically an unterminated brace (not some
    // generic parse failure), and — the AC's explicit requirement — that this does NOT mean "clean".
    expect(message).toContain("brace-in-text.svelte");
    expect(message).toMatch(/unterminated "\{"/);
    expect(message).toMatch(/does NOT mean the file is clean/);
  });

  it('#1 unterminated-mustache: <div title="{"> above two real offenders must RED (throw), not report []', () => {
    const src = twoRawRendersBelow(`<div title="{">`);
    expect(() => findUnsafeRenderLines(src, "unterminated-mustache.svelte")).toThrow(RenderScanError);
    try {
      findUnsafeRenderLines(src, "unterminated-mustache.svelte");
      expect.unreachable("expected findUnsafeRenderLines to throw");
    } catch (e) {
      expect((e as Error).message).toContain("unterminated-mustache.svelte");
      expect((e as Error).message).toMatch(/unterminated "\{"/);
    }
  });

  it("#3 stray '<': a bare '<' in text that is not a real tag/comment/closing-tag must RED (throw), not silently misclassify what follows", () => {
    // `<div>< {entry.name}</div>` — control-proven above (bidiRenderScan.test.ts's earlier suite doesn't
    // cover this, but the module's own scratch investigation for this ticket did): WITHOUT the stray `<`,
    // `<div>{entry.name}</div>` correctly flags line 1; WITH it, round 2's engine silently misread the
    // rest of the tag's attribute list and reported nothing. The fix refuses to guess.
    const strayLtSrc = `<div>< {entry.name}</div>`;
    let threw: unknown;
    try {
      findUnsafeRenderLines(strayLtSrc, "stray-lt.svelte");
    } catch (e) {
      threw = e;
    }
    expect(threw, "a stray '<' not opening a real tag must throw rather than silently misclassify what follows it").toBeInstanceOf(RenderScanError);
    const message = (threw as Error).message;
    expect(message).toContain("stray-lt.svelte");
    expect(message).toMatch(/"<" is not followed by a tag name/);
    expect(message).toMatch(/does NOT mean the file is clean/);

    // Same shape as #1's repro table: the stray '<' sits above two otherwise-real offenders that must
    // never silently vanish into an empty result.
    const src = twoRawRendersBelow(`<div>a < b`);
    expect(() => findUnsafeRenderLines(src, "stray-lt-above-offenders.svelte")).toThrow(RenderScanError);
  });

  it("a real, well-formed comparison/closing-tag/comment is NOT affected — only genuinely unrecognizable markup throws", () => {
    // Guards against an over-eager fix: a `<` that DOES open a real tag, comment, or closing tag must
    // keep working exactly as before.
    expect(() => findUnsafeRenderLines(`<!-- a < b, not a tag --><div>{entry.name}</div>`)).not.toThrow();
    expect(findUnsafeRenderLines(`<!-- a < b, not a tag --><div>{entry.name}</div>`)).toEqual(["1:entry.name"]);
    expect(() => findUnsafeRenderLines(`<div>{entry.name}</div><!-- trailing -->`)).not.toThrow();
  });

  // Reviewer (CPE-1761 attempt 2, F3): reverting either of these two fail-open fixes independently left
  // the guard test suite 39/39 GREEN — untested production code guarding two of the ACs. Each must red on
  // its own, not just ride along with the `{`/`<` tests above.
  it("an unterminated '<!--' comment above a real offender must RED (throw), not silently truncate to EOF", () => {
    const src = `<!-- unterminated\n{entry.name}`;
    let threw: unknown;
    try {
      findUnsafeRenderLines(src, "unterminated-comment.svelte");
    } catch (e) {
      threw = e;
    }
    expect(threw, "an unterminated '<!--' must throw rather than silently truncate the scan").toBeInstanceOf(RenderScanError);
    const message = (threw as Error).message;
    expect(message).toContain("unterminated-comment.svelte");
    expect(message).toMatch(/unterminated comment "<!--"/);
    expect(message).toMatch(/does NOT mean the file is clean/);
  });

  it("an unterminated '</' closing tag above a real offender must RED (throw), not silently truncate to EOF", () => {
    const src = `</div\n{entry.name}`;
    let threw: unknown;
    try {
      findUnsafeRenderLines(src, "unterminated-closing-tag.svelte");
    } catch (e) {
      threw = e;
    }
    expect(threw, "an unterminated '</' must throw rather than silently truncate the scan").toBeInstanceOf(RenderScanError);
    const message = (threw as Error).message;
    expect(message).toContain("unterminated-closing-tag.svelte");
    expect(message).toMatch(/unterminated closing tag "<\/"/);
    expect(message).toMatch(/does NOT mean the file is clean/);
  });

  // Reviewer (CPE-1761 attempt 2, F1 — BLOCKER): `quoteChar`/`inTag` were never checked at EOF, so a tag
  // with an odd number of quote characters swallowed every render below it as an (unflagged) attribute-
  // position mustache. Measured live against DiskSpaceView.svelte: appending `<div title='it's a file'>x
  // </div>` + a bare `{entry.name}` line let a raw filesystem name ship GREEN — this ticket's whole
  // premise, unfixed. Reproduced narrowly here (the mechanism), plus the exact end-to-end probe against
  // the real registered file below.
  it("F1: an unbalanced attribute quote must RED at EOF (throw), not silently suppress every render below it", () => {
    const src = [`<div title='it's a file'>x</div>`, `<span>{entry.name}</span>`].join("\n");
    let threw: unknown;
    try {
      findUnsafeRenderLines(src, "unbalanced-quote.svelte");
    } catch (e) {
      threw = e;
    }
    expect(threw, "an odd number of quote characters in an attribute must throw at EOF, not silently swallow the rest of the file").toBeInstanceOf(RenderScanError);
    const message = (threw as Error).message;
    expect(message).toContain("unbalanced-quote.svelte");
    expect(message).toMatch(/unterminated ' attribute string/);
    expect(message).toMatch(/does NOT mean the file is clean/);
  });

  it("F1: an unterminated tag (never reaches a closing '>') must RED at EOF (throw)", () => {
    const src = `<div title="x"\n{entry.name}`;
    expect(() => findUnsafeRenderLines(src, "unterminated-tag.svelte")).toThrow(/unterminated tag — no closing ">"/);
  });

  // CPE-1761 attempt 3 (reviewer): F1's EOF check only catches a quote that NEVER closes. If a forgotten
  // closing `'` is later re-balanced by an unrelated apostrophe/contraction elsewhere in the file, the
  // scanner never reaches EOF still "inside" a quote — it silently resumes normal parsing. The real bug
  // isn't the EOF check; it's that isRenderPosition's inTag branch only recognized the DOUBLE-quote form
  // (`attr="..{`), so a mustache reached while quoteChar === "'" was classified as a non-render and
  // DROPPED regardless — not deferred, not flagged, just silently invisible.
  it("F1 attempt 3: a forgotten closing quote re-balanced by a later contraction must not silently drop the render in between", () => {
    const src = [
      `<button title='Loading>Spinner</button>`,
      `<span>{entry.name}</span>`,
      `<p>You're all caught up</p>`,
    ].join("\n");
    // Before the fix this returned [] — completely clean — because the single-quoted `'Loading>...` never
    // reaches EOF unterminated (the later "You're" apostrophe re-balances it), and the {entry.name}
    // mustache in between was reached while quoteChar === "'", which isRenderPosition's inTag branch
    // didn't recognize as a render position at all.
    const found = findUnsafeRenderLines(src, "unbalanced-quote-rebalanced.svelte");
    expect(found.some((e) => e.includes("entry.name")), `expected entry.name to be reported as an offender, got: ${JSON.stringify(found)}`).toBe(true);
  });

  // A legitimate single-quoted render position must be classified identically to the double-quoted form —
  // proves the fix is genuine symmetry, not a special-cased patch for the repro shape above.
  it("F1 attempt 3: a legitimate single-quoted mustache is a render position, same as the double-quoted form", () => {
    const doubleQuoted = findUnsafeRenderLines(`<div title="{entry.name}">x</div>`, "double.svelte");
    const singleQuoted = findUnsafeRenderLines(`<div title='{entry.name}'>x</div>`, "single.svelte");
    expect(singleQuoted, `single-quoted title='{entry.name}' must be flagged exactly like the double-quoted form`).toEqual(["1:entry.name"]);
    // Same expression, same line, same shape — the two forms must not be able to drift apart, since
    // that drift IS the vulnerability (a spoof hiding behind whichever quote style isn't checked).
    expect(singleQuoted.map((e) => e.split(":").slice(1).join(":"))).toEqual(doubleQuoted.map((e) => e.split(":").slice(1).join(":")));
  });

  it("F1 attempt 3: the narrower repro — an unterminated single-quoted attribute directly followed by a real offender and a later contraction", () => {
    const src = `<div title='unterminated><span>{entry.name}</span><p>can't see</p>`;
    const found = findUnsafeRenderLines(src, "narrow-repro.svelte");
    expect(found.some((e) => e.includes("entry.name")), `expected entry.name to be reported, got: ${JSON.stringify(found)}`).toBe(true);
  });
});

// F1 (reviewer, CPE-1761 attempt 2): the exact end-to-end probe the reviewer ran against a REAL registered
// component — appending an unbalanced-quote attribute to DiskSpaceView.svelte's real source used to let a
// brand-new raw `{entry.name}` render ship completely undetected (attribute-position misclassification,
// same mechanism as the synthetic F1 test above, demonstrated here on the actual file the reviewer used).
describe("CPE-1761 F1: end-to-end probe against a real registered component (DiskSpaceView.svelte)", () => {
  it("appending an unbalanced-quote attribute + a new raw render reds the scan instead of shipping green", () => {
    const probe = [
      `<div title='it's a file'>x</div>`,
      `<span>{entry.name}</span>`,
    ].join("\n");
    expect(() => findUnsafeRenderLines(probe, "DiskSpaceView.svelte (probe)")).toThrow(RenderScanError);
  });
});
