import { describe, it, expect } from "vitest";
import { extractAskLabels, hasAskParams, resolveAskParams } from "./macroParams";
import type { ActionMacro } from "./bindings.gen";

const m = (steps: ActionMacro["steps"]): ActionMacro => ({ name: "m", steps });

describe("macroParams extractAskLabels / hasAskParams (CPE-1190 UI half)", () => {
  it("finds no labels and reports hasAskParams=false for a macro with no {ask:...} tokens", () => {
    const macro = m([{ rename: { template: "{stem}_v2.{ext}" } }]);
    expect(extractAskLabels(macro)).toEqual([]);
    expect(hasAskParams(macro)).toBe(false);
  });

  it("collects distinct labels across all four step kinds, in first-appearance order", () => {
    const macro = m([
      { rename: { template: "{ask:prefix}_{stem}.{ext}" } },
      { move: { dest: "/archive/{ask:folder}" } },
      { tag: { label: "{ask:label}" } },
      { convert: { to_ext: "{ask:format}" } },
    ]);
    expect(extractAskLabels(macro)).toEqual(["prefix", "folder", "label", "format"]);
    expect(hasAskParams(macro)).toBe(true);
  });

  it("de-duplicates a label referenced in more than one step", () => {
    const macro = m([
      { rename: { template: "{ask:tag}_{stem}.{ext}" } },
      { tag: { label: "{ask:tag}" } },
    ]);
    expect(extractAskLabels(macro)).toEqual(["tag"]);
  });
});

describe("macroParams resolveAskParams (CPE-1190 UI half)", () => {
  it("substitutes an answered label in a rename template", () => {
    const macro = m([{ rename: { template: "{ask:prefix}_{stem}.{ext}" } }]);
    const resolved = resolveAskParams(macro, { prefix: "vacation" });
    expect(resolved.steps[0]).toEqual({ rename: { template: "vacation_{stem}.{ext}" } });
  });

  it("substitutes in a move dest, a tag label, and a convert extension", () => {
    const macro = m([
      { move: { dest: "/archive/{ask:folder}" } },
      { tag: { label: "{ask:label}" } },
      { convert: { to_ext: "{ask:format}" } },
    ]);
    const resolved = resolveAskParams(macro, { folder: "2026", label: "reviewed", format: "webp" });
    expect(resolved.steps).toEqual([
      { move: { dest: "/archive/2026" } },
      { tag: { label: "reviewed" } },
      { convert: { to_ext: "webp" } },
    ]);
  });

  it("drops an unanswered {ask:label} cleanly — never a literal token leaking through", () => {
    const macro = m([{ rename: { template: "{ask:missing}{stem}.{ext}" } }]);
    const resolved = resolveAskParams(macro, {});
    expect(resolved.steps[0]).toEqual({ rename: { template: "{stem}.{ext}" } });
  });

  it("a partially-answered params map resolves the answered label and drops the rest", () => {
    const macro = m([{ rename: { template: "{ask:a}-{ask:b}-{stem}.{ext}" } }]);
    const resolved = resolveAskParams(macro, { a: "X" });
    expect(resolved.steps[0]).toEqual({ rename: { template: "X--{stem}.{ext}" } });
  });

  it("a macro with no {ask:...} tokens round-trips unchanged (new object, same shape)", () => {
    const macro = m([{ rename: { template: "{stem}_v2.{ext}" } }]);
    const resolved = resolveAskParams(macro, { unused: "x" });
    expect(resolved).toEqual(macro);
    expect(resolved).not.toBe(macro);
  });
});
