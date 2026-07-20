import { describe, it, expect } from "vitest";
import {
  planForEntry,
  addRule,
  removeRule,
  renameRule,
  setRuleEnabled,
  parseRules,
  serializeRules,
  type WatchRule,
  type Action,
} from "./watchRules";
import type { Condition } from "./colorRules";
import type { DirEntry } from "./types";

const NOW = 1_700_000_000_000;
const entry = (name: string, over: Partial<DirEntry> = {}): DirEntry =>
  ({ name, path: "/dl/" + name, is_dir: false, size: 10, modified: NOW, ...(over as object) }) as DirEntry;
const pdf: Condition = { kind: "ext", exts: ["pdf"] };

describe("planForEntry (CPE-793)", () => {
  const rules: WatchRule[] = [
    { id: "1", name: "big pdfs", when: { kind: "size", min: 1000 }, actions: [{ kind: "move", dest: "/big" }], enabled: false },
    { id: "2", name: "pdfs", when: pdf, actions: [{ kind: "move", dest: "/docs" }, { kind: "tag", tag: "invoice" }, { kind: "rename", template: "{stem}-filed.{ext}" }] },
  ];

  it("plans the first enabled matching rule, resolving templates", () => {
    const plan = planForEntry(entry("bill.pdf"), rules, NOW);
    expect(plan?.rule.id).toBe("2");
    expect(plan?.actions.map((a) => a.resolved)).toEqual(["/docs", "invoice", "bill-filed.pdf"]);
  });

  it("skips disabled rules and returns null when nothing matches", () => {
    expect(planForEntry(entry("notes.txt"), rules, NOW)).toBeNull();
    // a big .txt would match rule 1 by size, but it's disabled → no plan
    expect(planForEntry(entry("big.txt", { size: 99999 }), rules, NOW)).toBeNull();
  });
});

describe("CRUD + parse (CPE-793)", () => {
  it("adds/renames/toggles/removes immutably", () => {
    let list = addRule([], "R", pdf, [{ kind: "tag", tag: "x" } as Action]);
    expect(list[0].id).toMatch(/^wr_/);
    expect(list[0].enabled).toBe(true);
    const id = list[0].id;
    expect(renameRule(list, id, "R2")[0].name).toBe("R2");
    expect(setRuleEnabled(list, id, false)[0].enabled).toBe(false);
    expect(list[0].enabled).toBe(true); // original untouched
    expect(removeRule(list, id)).toEqual([]);
  });

  it("parse tolerates malformed input; serialize round-trips", () => {
    const list: WatchRule[] = [{ id: "a", name: "n", when: pdf, actions: [{ kind: "move", dest: "/d" }], enabled: true }];
    expect(parseRules(serializeRules(list))).toEqual(list);
    expect(parseRules(null)).toEqual([]);
    expect(parseRules("nope")).toEqual([]);
    expect(parseRules(JSON.stringify([{ id: "x" }, list[0]]))).toEqual([list[0]]); // drops invalid
  });
});
