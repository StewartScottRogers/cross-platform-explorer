import { describe, it, expect } from "vitest";
import {
  addRule,
  updateRule,
  removeRule,
  toggleRule,
  moveRule,
  serializeRules,
  parseRules,
} from "./colorRulesStore";
import type { ColorRule } from "./colorRules";

const ids = (list: ColorRule[]) => list.map((r) => r.id);

describe("colorRulesStore CRUD (CPE-776)", () => {
  it("adds an enabled rule with a generated id, immutably", () => {
    const before: ColorRule[] = [];
    const after = addRule(before, { kind: "ext", exts: ["ts"] }, { color: "#f00", label: "code" });
    expect(before).toEqual([]); // original untouched
    expect(after).toHaveLength(1);
    expect(after[0].id).toMatch(/^cr_/);
    expect(after[0].enabled).toBe(true);
    expect(after[0].color).toBe("#f00");
  });

  it("updates by id without touching others or the id", () => {
    let list = addRule([], { kind: "isDir", value: true });
    list = addRule(list, { kind: "glob", pattern: "*.log" });
    const targetId = list[0].id;
    const updated = updateRule(list, targetId, { label: "folder", enabled: false });
    expect(updated[0].label).toBe("folder");
    expect(updated[0].enabled).toBe(false);
    expect(updated[0].id).toBe(targetId); // id immutable
    expect(updated[1]).toEqual(list[1]); // sibling untouched
    expect(updateRule(list, "nope", { label: "x" })).toEqual(list); // unknown id → no-op copy
  });

  it("removes by id", () => {
    const list = addRule(addRule([], { kind: "ext", exts: ["a"] }), { kind: "ext", exts: ["b"] });
    expect(removeRule(list, list[0].id)).toEqual([list[1]]);
    expect(removeRule(list, "nope")).toEqual(list);
  });

  it("toggles enable state (explicit and flip)", () => {
    const list = addRule([], { kind: "ext", exts: ["a"] }); // enabled: true
    const id = list[0].id;
    expect(toggleRule(list, id).at(0)?.enabled).toBe(false); // flip
    expect(toggleRule(list, id, true).at(0)?.enabled).toBe(true); // explicit
  });
});

describe("colorRulesStore reorder (CPE-776)", () => {
  const build = () => {
    let list: ColorRule[] = [];
    for (const p of ["a", "b", "c"]) list = addRule(list, { kind: "glob", pattern: p });
    return list;
  };

  it("moves a rule earlier/later and clamps at the ends", () => {
    const list = build(); // a, b, c
    const [a, b, c] = ids(list);
    expect(ids(moveRule(list, b, -1))).toEqual([b, a, c]); // b up
    expect(ids(moveRule(list, b, 1))).toEqual([a, c, b]); // b down
    expect(ids(moveRule(list, a, -1))).toEqual([a, b, c]); // first up → clamp (no-op)
    expect(ids(moveRule(list, c, 1))).toEqual([a, b, c]); // last down → clamp (no-op)
    expect(ids(moveRule(list, "nope", -1))).toEqual([a, b, c]); // unknown id → no-op
    expect(moveRule(list, b, -1)).not.toBe(list); // returns a new array
  });
});

describe("colorRulesStore persistence (CPE-776)", () => {
  it("round-trips through serialize/parse", () => {
    const list = addRule([], { kind: "size", min: 100, max: 200 }, { color: "#0f0" });
    expect(parseRules(serializeRules(list))).toEqual(list);
  });

  it("tolerates null/garbage and drops malformed rules", () => {
    expect(parseRules(null)).toEqual([]);
    expect(parseRules("not json")).toEqual([]);
    expect(parseRules(JSON.stringify({ not: "an array" }))).toEqual([]);
    const good: ColorRule = { id: "cr_1", when: { kind: "ext", exts: ["ts"] }, enabled: true };
    const mixed = JSON.stringify([
      good,
      { id: "cr_2" }, // no `when`
      { when: { kind: "ext", exts: [] } }, // no id
      { id: "cr_3", when: { kind: "bogus" } }, // unknown condition kind
    ]);
    expect(parseRules(mixed)).toEqual([good]);
  });

  it("drops a known-kind condition with malformed fields (would crash the renderer)", () => {
    const good: ColorRule = { id: "cr_ok", when: { kind: "size", min: 1 }, enabled: true };
    const mixed = JSON.stringify([
      good,
      { id: "cr_1", when: { kind: "ext" } }, // no exts → matchesCondition would throw on .some
      { id: "cr_2", when: { kind: "ext", exts: [1, 2] } }, // exts not strings
      { id: "cr_3", when: { kind: "glob" } }, // no pattern
      { id: "cr_4", when: { kind: "size", min: "big" } }, // min not a number
      { id: "cr_5", when: { kind: "olderThan" } }, // no days
      { id: "cr_6", when: { kind: "isDir", value: "yes" } }, // value not boolean
    ]);
    expect(parseRules(mixed)).toEqual([good]);
  });
});
