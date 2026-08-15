// CPE-1757 round 2: direct unit evidence that the whitelist engine (bidiRenderScan.ts) catches every
// shape the review's 17-shape probe component exercised — not asserted, reproduced. Each `it` mirrors
// one probe shape verbatim (or as close as a standalone expression allows) so a future reader can check
// the claim against the actual behavior, not against prose.
import { describe, it, expect } from "vitest";
import { isSafeExpr, findUnsafeRenderLines } from "./bidiRenderScan";

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
    expect(findUnsafeRenderLines(src)).toEqual([1]);
  });

  it("flags title=/aria-label=/alt= given directly as attr={expr}", () => {
    expect(findUnsafeRenderLines(`<span title={entry.path}>x</span>`)).toEqual([1]);
    expect(findUnsafeRenderLines(`<img alt={entry.name} />`)).toEqual([1]);
    expect(findUnsafeRenderLines(`<div aria-label={entry.name}>x</div>`)).toEqual([1]);
  });

  it("flags a raw name/path embedded in a quoted title=\"…{expr}…\" value", () => {
    expect(findUnsafeRenderLines('<span aria-label="Diff for {baseOf(path)}">x</span>')).toEqual([1]);
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
    expect(findUnsafeRenderLines(src)).toEqual([1]);
  });

  it("flags {@html entry.name}", () => {
    expect(findUnsafeRenderLines(`<div>{@html entry.name}</div>`)).toEqual([1]);
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
    expect(findUnsafeRenderLines(src)).toEqual([2]);
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
    expect(findUnsafeRenderLines(src)).toEqual([3]);
  });
});
