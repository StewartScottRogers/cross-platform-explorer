/**
 * MacroRunConfirm (CPE-1191, epic CPE-739): the dry-run (macro_plan) -> confirm -> execute
 * (macro_run) -> offer Undo (macro_undo) safety gate for running a saved macro. The typed
 * `commands.*` client routes through the mocked `../invoke`, mirroring TemplatesDialog.test.ts.
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

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => (cmd === "macro_plan" ? PLAN : null));
});

describe("MacroRunConfirm dry-run + confirm (CPE-1191)", () => {
  it("dry-runs via macro_plan on mount and renders the planned ops", async () => {
    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    await screen.findByTestId("plan-list");
    expect(invokeMock).toHaveBeenCalledWith("macro_plan", { macro: MACRO, inputs: ["/work/a.txt"] });
    expect(screen.getByTestId("plan-list").textContent).toContain("/work/a.txt");
  });

  it("Run is disabled while the plan is still loading, and calls macro_run with macro/inputs/root", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_run") return RESOLVED_RUN;
      if (cmd === "macro_plan") return PLAN;
      return null;
    });

    const { component } = render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    await screen.findByTestId("plan-list");
    expect((screen.getByTestId("run-btn") as HTMLButtonElement).disabled).toBe(false);

    const ran = vi.fn();
    component.$on("ran", (e: CustomEvent) => ran(e.detail));

    await fireEvent.click(screen.getByTestId("run-btn"));

    expect(invokeMock).toHaveBeenCalledWith("macro_run", { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    expect(ran).toHaveBeenCalledWith(RESOLVED_RUN);
  });

  it("shows an empty-plan message and disables Run when there's nothing to do", async () => {
    invokeMock.mockImplementation(async (cmd: string) => (cmd === "macro_plan" ? [] : null));
    render(MacroRunConfirm, { macro: MACRO, inputs: [], root: "/work" });
    await screen.findByTestId("plan-list");
    expect(screen.getByTestId("plan-list").textContent).toContain("Nothing to run");
    expect((screen.getByTestId("run-btn") as HTMLButtonElement).disabled).toBe(true);
  });

  it("a macro_plan failure shows the error and keeps Run disabled", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") throw new Error("boom");
      return null;
    });
    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    expect(await screen.findByTestId("plan-error")).toHaveProperty("textContent", "boom");
    expect((screen.getByTestId("run-btn") as HTMLButtonElement).disabled).toBe(true);
  });
});

describe("MacroRunConfirm run results + undo (CPE-1191)", () => {
  it("after a successful run, shows the applied step count and offers Undo -> macro_undo", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return PLAN;
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
