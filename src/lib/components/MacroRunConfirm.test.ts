/**
 * MacroRunConfirm (CPE-1191, epic CPE-739): the dry-run (macro_plan) -> confirm -> execute
 * (macro_run) -> offer Undo (macro_undo) safety gate for running a saved macro. The typed
 * `commands.*` client routes through the mocked `../invoke`, mirroring TemplatesDialog.test.ts.
 *
 * CPE-1891 added `macro_preflight` (a read-only real-filesystem collision scan run alongside
 * `macro_plan`) and threaded `confirmedOverwrite` through `macro_run`, so a colliding destination no
 * longer aborts and rolls back the whole batch with no recourse — the tests below cover that flow.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";

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
  reason: '"/work/a_v2.txt" is a link, and renaming onto a link destroys it',
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

  it("Run is disabled while the plan is still loading, and calls macro_run with macro/inputs/root/confirmedOverwrite", async () => {
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
      confirmedOverwrite: false,
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
  it("lists a confirmable collision, disables Run until the overwrite box is checked, then runs confirmed", async () => {
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

    const runBtn = screen.getByTestId("run-btn") as HTMLButtonElement;
    expect(runBtn.disabled).toBe(true);
    expect(runBtn.textContent).toContain("Run");

    await fireEvent.click(screen.getByTestId("confirm-overwrite"));
    expect(runBtn.disabled).toBe(false);
    expect(runBtn.textContent).toContain("Overwrite 1 and Run");

    await fireEvent.click(runBtn);
    expect(invokeMock).toHaveBeenCalledWith("macro_run", {
      macro: MACRO,
      inputs: ["/work/a.txt"],
      root: "/work",
      confirmedOverwrite: true,
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

  it("a mix of confirmable and blocked collisions still refuses Run even once the confirmable one is checked", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") return [CONFIRMABLE_COLLISION, { ...BLOCKED_COLLISION, op_index: 1 }];
      return null;
    });

    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    await screen.findByTestId("confirmable-collisions");
    await screen.findByTestId("blocked-collisions");

    await fireEvent.click(screen.getByTestId("confirm-overwrite"));
    expect((screen.getByTestId("run-btn") as HTMLButtonElement).disabled).toBe(true);
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
