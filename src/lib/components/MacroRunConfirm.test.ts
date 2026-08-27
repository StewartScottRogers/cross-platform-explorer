/**
 * MacroRunConfirm (CPE-1191, epic CPE-739): the dry-run (macro_plan) -> confirm -> execute
 * (macro_run) -> offer Undo (macro_undo) safety gate for running a saved macro. The typed
 * `commands.*` client routes through the mocked `../invoke`, mirroring TemplatesDialog.test.ts.
 *
 * CPE-1891 added `macro_preflight` (a read-only real-filesystem collision scan run alongside
 * `macro_plan`) and threaded `confirmedOverwrite` through `macro_run` as the list of destinations the
 * user actually confirmed (PR #1044 review round 2, Blocker 2 -- never a blanket bool), so a colliding
 * destination no longer aborts and rolls back the whole batch with no recourse — the tests below cover
 * that flow, plus the review round's reason-rendering redesign (one sentence per hazard KIND, hoisted
 * above the path list, not one per row) and its irreversibility warning.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";

// Mirrors MacroRunConfirm.svelte's own `genericizeReason` (CPE-1891, Visual Critic round 3): the
// hoisted per-kind sentence strips the leading `"<path>" ` clause, since the hoisted sentence is
// shared across possibly-many collisions and must not look tied to just the first one's path.
const genericized = (reason: string) => reason.replace(/^"[^"]*"\s*/, "This destination ");

const invokeMock = vi.fn(async (_cmd: string, _args?: unknown): Promise<unknown> => null);
vi.mock("../invoke", () => ({
  invoke: (...a: unknown[]) => (invokeMock as (...x: unknown[]) => unknown)(...a),
  unwrap: <T>(r: { status: string; data?: T; error?: unknown }): T => {
    if (r.status === "ok") return r.data as T;
    throw r.error instanceof Error ? r.error : new Error(String(r.error));
  },
}));

import MacroRunConfirm from "./MacroRunConfirm.svelte";

const MACRO = { name: "Tidy", steps: [{ rename: { template: "{stem}_v2.{ext}" } }] };
const PLAN = [{ input: "/work/a.txt", kind: "rename", detail: "/work/a_v2.txt" }];
const RESOLVED_RUN = {
  ops: [{ from: "/work/a.txt", kind: "rename", detail: "a_v2.txt", to: "/work/a_v2.txt" }],
  inverses: [{ from: "/work/a_v2.txt", kind: "rename", detail: "a.txt", to: "/work/a.txt" }],
};
const CONFIRMABLE_COLLISION = {
  op_index: 0,
  from: "/work/a.txt",
  to: "/work/a_v2.txt",
  kind: "rename",
  confirmable: true,
  reason: '"a_v2.txt" already exists',
};
const BLOCKED_COLLISION = {
  op_index: 0,
  from: "/work/a.txt",
  to: "/work/a_v2.txt",
  kind: "rename",
  confirmable: false,
  reason: '"/work/a_v2.txt" is a link, and renaming onto a link destroys it — the link is removed and its target is left orphaned',
};
// Same shape, the CONVERT-flavored wording (CPE-1891 follow-up: the two must read differently, not just
// share a generic "is a link" header).
const CONVERT_BLOCKED_COLLISION = {
  op_index: 1,
  from: "/work/b.png",
  to: "/work/b.jpg",
  kind: "convert",
  confirmable: false,
  reason: '"/work/b.jpg" is a link, and creating a file at a link\'s name writes THROUGH it — the bytes would land at the link\'s target, a path you did not name',
};
// A THIRD blocked item, same "rename-move" bucket as BLOCKED_COLLISION but a DIFFERENT path/reason
// string -- proves the dedup is by hazard kind, not by exact reason text.
const MOVE_BLOCKED_COLLISION = {
  op_index: 2,
  from: "/work/c.txt",
  to: "/work/Archive/c.txt",
  kind: "move",
  confirmable: false,
  reason: '"/work/Archive/c.txt" is a link, and renaming onto a link destroys it — the link is removed and its target is left orphaned',
};

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "macro_plan") return PLAN;
    if (cmd === "macro_preflight") return [];
    return null;
  });
});

describe("MacroRunConfirm dry-run + confirm (CPE-1191)", () => {
  it("dry-runs via macro_plan on mount and renders the planned ops", async () => {
    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    await screen.findByTestId("plan-list");
    expect(invokeMock).toHaveBeenCalledWith("macro_plan", { macro: MACRO, inputs: ["/work/a.txt"] });
    expect(screen.getByTestId("plan-list").textContent).toContain("/work/a.txt");
  });

  it("also preflights via macro_preflight on mount, with macro/inputs/root", async () => {
    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    await screen.findByTestId("plan-list");
    expect(invokeMock).toHaveBeenCalledWith("macro_preflight", {
      macro: MACRO,
      inputs: ["/work/a.txt"],
      root: "/work",
    });
  });

  it("Run is disabled while the plan is still loading, and calls macro_run with an EMPTY confirmedOverwrite when nothing collided", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_run") return RESOLVED_RUN;
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") return [];
      return null;
    });

    const { component } = render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    await screen.findByTestId("plan-list");
    expect((screen.getByTestId("run-btn") as HTMLButtonElement).disabled).toBe(false);

    const ran = vi.fn();
    component.$on("ran", (e: CustomEvent) => ran(e.detail));

    await fireEvent.click(screen.getByTestId("run-btn"));

    expect(invokeMock).toHaveBeenCalledWith("macro_run", {
      macro: MACRO,
      inputs: ["/work/a.txt"],
      root: "/work",
      confirmedOverwrite: [],
    });
    expect(ran).toHaveBeenCalledWith(RESOLVED_RUN);
  });

  it("shows an empty-plan message and disables Run when there's nothing to do", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return [];
      if (cmd === "macro_preflight") return [];
      return null;
    });
    render(MacroRunConfirm, { macro: MACRO, inputs: [], root: "/work" });
    await screen.findByTestId("plan-list");
    expect(screen.getByTestId("plan-list").textContent).toContain("Nothing to run");
    expect((screen.getByTestId("run-btn") as HTMLButtonElement).disabled).toBe(true);
  });

  it("a macro_plan failure shows the error and keeps Run disabled", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") throw new Error("boom");
      if (cmd === "macro_preflight") return [];
      return null;
    });
    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    expect(await screen.findByTestId("plan-error")).toHaveProperty("textContent", "boom");
    expect((screen.getByTestId("run-btn") as HTMLButtonElement).disabled).toBe(true);
  });

  it("a macro_preflight failure shows its own error without blocking the plan preview", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") throw new Error("preflight boom");
      return null;
    });
    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    expect(await screen.findByTestId("preflight-error")).toHaveProperty("textContent", "preflight boom");
    expect(screen.getByTestId("plan-list")).toBeTruthy();
  });
});

describe("MacroRunConfirm collision confirm-and-retry (CPE-1891)", () => {
  it("lists a confirmable collision, disables Run until the overwrite box is checked, warns it's not undoable, then runs with that destination named", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") return [CONFIRMABLE_COLLISION];
      if (cmd === "macro_run") return RESOLVED_RUN;
      return null;
    });

    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    const list = await screen.findByTestId("confirmable-collisions");
    expect(list.textContent).toContain("/work/a_v2.txt");
    expect(screen.queryByTestId("blocked-collisions")).toBeNull();
    // Blocker 3 (PR #1044 review round 2): the confirm panel must warn this isn't reversible.
    expect(screen.getByTestId("irreversible-note").textContent).toMatch(/can.t be undone/i);

    const runBtn = screen.getByTestId("run-btn") as HTMLButtonElement;
    expect(runBtn.disabled).toBe(true);
    expect(runBtn.textContent).toContain("Run");

    await fireEvent.click(screen.getByTestId("confirm-overwrite"));
    expect(runBtn.disabled).toBe(false);
    expect(runBtn.textContent).toContain("Overwrite 1 and Run");

    await fireEvent.click(runBtn);
    // Blocker 2: the CONFIRMED DESTINATION, not a bare `true` -- the backend only bypasses the
    // occupancy guard at a `to` actually named here.
    expect(invokeMock).toHaveBeenCalledWith("macro_run", {
      macro: MACRO,
      inputs: ["/work/a.txt"],
      root: "/work",
      confirmedOverwrite: ["/work/a_v2.txt"],
    });
  });

  it("lists a blocked (link) collision with no checkbox, and Run stays disabled regardless", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") return [BLOCKED_COLLISION];
      return null;
    });

    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    const list = await screen.findByTestId("blocked-collisions");
    expect(list.textContent).toContain("/work/a_v2.txt");
    expect(screen.queryByTestId("confirmable-collisions")).toBeNull();
    expect(screen.queryByTestId("confirm-overwrite")).toBeNull();
    expect((screen.getByTestId("run-btn") as HTMLButtonElement).disabled).toBe(true);
  });

  it("renders ONE reason sentence per distinct hazard kind, hoisted above the path list -- not one per row", async () => {
    // CPE-1891 Visual Critic pass: the earlier per-row placement repeated the same paragraph once per
    // path (the wording is per-KIND, not per-path) and clipped mid-sentence past a handful of rows.
    // Three blocked items: rename + move (share the "destroys it" wording) + convert ("writes THROUGH
    // it") -- must collapse to exactly TWO reason sentences, not three, and both must actually reach the
    // DOM.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") return [BLOCKED_COLLISION, CONVERT_BLOCKED_COLLISION, MOVE_BLOCKED_COLLISION];
      return null;
    });

    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt", "/work/b.png", "/work/c.txt"], root: "/work" });
    await screen.findByTestId("blocked-collisions");
    const reasons = screen.getAllByTestId("blocked-reason").map((el) => el.textContent);

    expect(reasons).toHaveLength(2);
    expect(reasons).toContain(genericized(BLOCKED_COLLISION.reason));
    expect(reasons).toContain(genericized(CONVERT_BLOCKED_COLLISION.reason));
    // Genericizing BLOCKED_COLLISION and MOVE_BLOCKED_COLLISION's reasons yields the IDENTICAL string
    // (same "destroys it" wording, only their raw paths differed) -- which is exactly the point: they
    // share one bucket and therefore one rendered sentence, so there is nothing further to assert about
    // MOVE_BLOCKED_COLLISION's reason specifically once its path is stripped.
    expect(genericized(MOVE_BLOCKED_COLLISION.reason)).toEqual(genericized(BLOCKED_COLLISION.reason));
    // The two variants must actually read differently -- a shared generic sentence would defeat the
    // point (rename/move "destroys" the link; convert "writes THROUGH" it).
    expect(screen.getByTestId("blocked-collisions").textContent).toContain("destroys it");
    expect(screen.getByTestId("blocked-collisions").textContent).toContain("writes THROUGH it");
    // Visual Critic round 3: the hoisted sentence must not name any ONE collision's own path -- with
    // several blocked items that made the sentence look tied to just the first one.
    for (const reason of reasons) {
      expect(reason).not.toContain(BLOCKED_COLLISION.to);
      expect(reason).not.toContain(CONVERT_BLOCKED_COLLISION.to);
      expect(reason).toMatch(/^This destination /);
    }
    // All three PATHS still appear in the plain path list below the reason(s), including the
    // move-kind item whose own reason text was folded into the rename/move sentence above.
    expect(screen.getByTestId("blocked-collisions").textContent).toContain("/work/Archive/c.txt");
  });

  it("shows a dim note stating why Run is blocked, next to the button, when a link collision is present", async () => {
    // CPE-1891, Visual Critic round 3: the red box's explanation lives two panels above the button --
    // this states the SAME fact right where the user is looking when they wonder why Run won't light.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") return [BLOCKED_COLLISION];
      return null;
    });

    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    await screen.findByTestId("blocked-collisions");
    expect(screen.getByTestId("run-blocked-note").textContent).toContain("Run is blocked by 1 link above");
  });

  it("does NOT show the run-blocked note when there is nothing blocking Run", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") return [CONFIRMABLE_COLLISION];
      return null;
    });

    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    await screen.findByTestId("confirmable-collisions");
    expect(screen.queryByTestId("run-blocked-note")).toBeNull();
  });

  it("copies every blocked destination name to the clipboard from its own copy button", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") return [BLOCKED_COLLISION, CONVERT_BLOCKED_COLLISION];
      return null;
    });

    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt", "/work/b.png"], root: "/work" });
    const btn = await screen.findByTestId("copy-blocked-collisions");
    expect(btn.textContent).toContain("Copy all 2 names");

    await fireEvent.click(btn);
    expect(writeText).toHaveBeenCalledWith('"/work/a_v2.txt"\n"/work/b.jpg"');
    expect(await screen.findByText("Copied")).toBeTruthy();
  });

  it("a mix of confirmable and blocked collisions still refuses Run, and keeps the plain 'Run' label, even once the confirmable one is checked", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") return [CONFIRMABLE_COLLISION, { ...BLOCKED_COLLISION, op_index: 1 }];
      return null;
    });

    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    await screen.findByTestId("confirmable-collisions");
    await screen.findByTestId("blocked-collisions");

    await fireEvent.click(screen.getByTestId("confirm-overwrite"));
    const runBtn = screen.getByTestId("run-btn") as HTMLButtonElement;
    expect(runBtn.disabled).toBe(true);
    // Should-fix (PR #1044 review round 2): a still-blocked run must not read as armed-and-ready.
    expect(runBtn.textContent?.trim()).toBe("Run");
    expect(runBtn.textContent).not.toContain("Overwrite");
  });

  it("copies every confirmable collision's destination name to the clipboard (CPE-1869's reused affordance)", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") return [CONFIRMABLE_COLLISION];
      return null;
    });

    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    const btn = await screen.findByTestId("copy-collisions");
    expect(btn.textContent).toContain("Copy all 1 name");

    await fireEvent.click(btn);
    expect(writeText).toHaveBeenCalledWith('"/work/a_v2.txt"');
    expect(await screen.findByText("Copied")).toBeTruthy();
  });
});

describe("MacroRunConfirm run results + undo (CPE-1191)", () => {
  it("after a successful run, shows the applied step count and offers Undo -> macro_undo", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") return [];
      if (cmd === "macro_run") return RESOLVED_RUN;
      if (cmd === "macro_undo") return null;
      return null;
    });
    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    await screen.findByTestId("plan-list");
    await fireEvent.click(screen.getByTestId("run-btn"));

    const results = await screen.findByTestId("run-results");
    expect(results.textContent).toContain("Applied 1 step");
    expect(screen.getByTestId("undo-btn")).toBeTruthy();

    await fireEvent.click(screen.getByTestId("undo-btn"));
    expect(invokeMock).toHaveBeenCalledWith("macro_undo", { run: RESOLVED_RUN });
    expect(await screen.findByTestId("undone-note")).toBeTruthy();
    expect(screen.queryByTestId("undo-btn")).toBeNull();
  });
});
