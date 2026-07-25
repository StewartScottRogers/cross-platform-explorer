import { describe, it, expect } from "vitest";
import {
  parseWorkspaces,
  serializeWorkspaces,
  addWorkspace,
  renameWorkspace,
  removeWorkspace,
  updateWorkspace,
  pruneMissing,
  type Workspace,
  type WorkspaceTab,
} from "./workspaces";

const tab = (path: string, over: Partial<WorkspaceTab> = {}): WorkspaceTab => ({ path, ...over });

describe("parse/serialize (CPE-787)", () => {
  it("round-trips a workspace list", () => {
    const list: Workspace[] = [{ id: "a", name: "Taxes", tabs: [tab("/docs", { view: "details", filter: "*.pdf" })] }];
    expect(parseWorkspaces(serializeWorkspaces(list))).toEqual(list);
  });

  it("tolerates malformed input and drops invalid entries/tabs", () => {
    expect(parseWorkspaces(null)).toEqual([]);
    expect(parseWorkspaces("not json")).toEqual([]);
    expect(parseWorkspaces("{}")).toEqual([]); // not an array
    const messy = JSON.stringify([
      { id: "ok", name: "Good", tabs: [{ path: "/a" }, { path: "" }, { nope: 1 }, "bad"] },
      { id: "x", name: "no tabs" }, // missing tabs → dropped
      { name: "no id", tabs: [] }, // missing id → dropped
    ]);
    expect(parseWorkspaces(messy)).toEqual([{ id: "ok", name: "Good", tabs: [{ path: "/a" }] }]);
  });
});

describe("CRUD (CPE-787)", () => {
  it("add / rename / remove / update are immutable and correct", () => {
    let list: Workspace[] = [];
    list = addWorkspace(list, "Work", [tab("/proj")]);
    expect(list).toHaveLength(1);
    expect(list[0].name).toBe("Work");
    expect(list[0].id).toMatch(/^ws_/);
    const id = list[0].id;

    const renamed = renameWorkspace(list, id, "Project");
    expect(renamed[0].name).toBe("Project");
    expect(list[0].name).toBe("Work"); // original untouched (immutable)

    const updated = updateWorkspace(renamed, id, [tab("/proj"), tab("/proj/sub")]);
    expect(updated[0].tabs).toHaveLength(2);

    expect(removeWorkspace(updated, id)).toEqual([]);
    expect(removeWorkspace(updated, "nope")).toHaveLength(1); // no-op for unknown id
  });
});

describe("pruneMissing (CPE-787)", () => {
  it("keeps only tabs whose path still exists", () => {
    const ws: Workspace = { id: "a", name: "W", tabs: [tab("/here"), tab("/gone"), tab("/also-here")] };
    const present = new Set(["/here", "/also-here"]);
    const pruned = pruneMissing(ws, (p) => present.has(p));
    expect(pruned.tabs.map((t) => t.path)).toEqual(["/here", "/also-here"]);
    expect(ws.tabs).toHaveLength(3); // original untouched
  });
});
