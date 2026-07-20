import { describe, it, expect } from "vitest";
import {
  addCommand,
  updateCommand,
  removeCommand,
  moveCommand,
  commandsForSurface,
  resolveCommand,
  serializeCommands,
  parseCommands,
  type UserCommand,
} from "./userCommands";
import type { DirEntry } from "./types";

const e = (path: string, name: string): DirEntry =>
  ({ name, path, is_dir: false, size: 0, modified: 0 }) as DirEntry;

const ids = (list: UserCommand[]) => list.map((c) => c.id);

describe("userCommands CRUD (CPE-783)", () => {
  it("adds with defaults (mode each, context surface) and a generated id, immutably", () => {
    const before: UserCommand[] = [];
    const after = addCommand(before, "Git add", "git add {path}");
    expect(before).toEqual([]);
    expect(after[0].id).toMatch(/^uc_/);
    expect(after[0].mode).toBe("each");
    expect(after[0].surfaces).toEqual(["context"]);
  });

  it("updates by id without touching the id or siblings", () => {
    let list = addCommand([], "A", "a {path}");
    list = addCommand(list, "B", "b {path}");
    const id = list[0].id;
    const out = updateCommand(list, id, { name: "A2", surfaces: ["toolbar", "palette"] });
    expect(out[0].name).toBe("A2");
    expect(out[0].surfaces).toEqual(["toolbar", "palette"]);
    expect(out[0].id).toBe(id);
    expect(out[1]).toEqual(list[1]);
    expect(updateCommand(list, "nope", { name: "x" })).toEqual(list);
  });

  it("removes by id", () => {
    const list = addCommand(addCommand([], "A", "a"), "B", "b");
    expect(removeCommand(list, list[0].id)).toEqual([list[1]]);
    expect(removeCommand(list, "nope")).toEqual(list);
  });
});

describe("userCommands reorder + surface filter (CPE-783)", () => {
  it("reorders one step, clamped at the ends", () => {
    let list: UserCommand[] = [];
    for (const n of ["a", "b", "c"]) list = addCommand(list, n, n);
    const [a, b, c] = ids(list);
    expect(ids(moveCommand(list, b, -1))).toEqual([b, a, c]);
    expect(ids(moveCommand(list, b, 1))).toEqual([a, c, b]);
    expect(ids(moveCommand(list, a, -1))).toEqual([a, b, c]); // clamp
    expect(ids(moveCommand(list, c, 1))).toEqual([a, b, c]); // clamp
    expect(moveCommand(list, a, 1)).not.toBe(list); // new array
  });

  it("filters by surface in list order", () => {
    let list = addCommand([], "tool", "t", { surfaces: ["toolbar", "palette"] });
    list = addCommand(list, "ctx", "c", { surfaces: ["context"] });
    list = addCommand(list, "pal", "p", { surfaces: ["palette"] });
    expect(commandsForSurface(list, "palette").map((c) => c.name)).toEqual(["tool", "pal"]);
    expect(commandsForSurface(list, "context").map((c) => c.name)).toEqual(["ctx"]);
    expect(commandsForSurface(list, "toolbar").map((c) => c.name)).toEqual(["tool"]);
  });
});

describe("userCommands resolve (CPE-783)", () => {
  const sel = [e("C:\\pics\\a.jpg", "a.jpg"), e("C:\\pics\\b.jpg", "b.jpg")];

  it("each mode → one line per entry; joined mode → a single line", () => {
    const each = addCommand([], "conv", "convert {name}", { mode: "each" })[0];
    expect(resolveCommand(each, sel)).toEqual(["convert a.jpg", "convert b.jpg"]);
    const joined = addCommand([], "zip", "zip out.zip {path}", { mode: "joined" })[0];
    // joined mode space-joins double-quoted values per token (see cmdTemplate.expandForSelection)
    expect(resolveCommand(joined, sel)).toEqual(['zip out.zip "C:\\pics\\a.jpg" "C:\\pics\\b.jpg"']);
  });
});

describe("userCommands persistence (CPE-783)", () => {
  it("round-trips through serialize/parse", () => {
    const list = addCommand([], "A", "a {path}", { mode: "joined", surfaces: ["toolbar"] });
    expect(parseCommands(serializeCommands(list))).toEqual(list);
  });

  it("tolerates null/garbage and drops malformed commands", () => {
    expect(parseCommands(null)).toEqual([]);
    expect(parseCommands("nope")).toEqual([]);
    expect(parseCommands(JSON.stringify({ not: "array" }))).toEqual([]);
    const good: UserCommand = { id: "uc_1", name: "A", template: "a", mode: "each", surfaces: ["context"] };
    const mixed = JSON.stringify([
      good,
      { id: "uc_2", name: "B", template: "b", mode: "each" }, // no surfaces
      { id: "uc_3", name: "C", template: "c", mode: "bogus", surfaces: ["context"] }, // bad mode
      { id: "uc_4", name: "D", template: "d", mode: "each", surfaces: ["nowhere"] }, // bad surface
    ]);
    expect(parseCommands(mixed)).toEqual([good]);
  });
});
