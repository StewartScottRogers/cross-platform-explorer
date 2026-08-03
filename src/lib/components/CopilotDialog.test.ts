/**
 * CopilotDialog (CPE-1276, epic CPE-977). The human-in-the-loop surface over the safe CPE-1275 backend:
 * instruction → plan preview (counts + ordered op list) → explicit Confirm → execute → per-op results →
 * Undo via the returned checkpoint. Mirrors CheckpointDialog.test.ts's mocking: `../invoke`'s `invoke`/
 * `unwrap` are mocked, since the typed `commands.*` client (`../bindings.gen`) — and `settings.ts`'s
 * persistence — both route through it.
 *
 * The load-bearing guarantee this ticket calls out: `copilot_execute` is NEVER called except from the
 * Confirm button's own click handler — no auto-run on a successful plan, no retry path that skips it.
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

import CopilotDialog from "./CopilotDialog.svelte";
import {
  saveCopilotEnabled,
  saveCopilotBaseUrl,
  saveCopilotModel,
} from "../settings";

const ROOT = "/work/proj";

const PLAN_RESULT = {
  plan: {
    ops: [
      { move: { src: "/work/proj/shot1.png", dst: "/work/proj/Screenshots/shot1.png" } },
      { mkdir: { path: "/work/proj/Screenshots" } },
    ],
  },
  summary: { moves: 1, renames: 0, deletes: 0, mkdirs: 1, copies: 0 },
  violations: [] as string[],
};

const VIOLATING_PLAN_RESULT = {
  plan: { ops: [{ delete: { path: "/etc/passwd" } }] },
  summary: { moves: 0, renames: 0, deletes: 1, mkdirs: 0, copies: 0 },
  violations: ["path escapes the scope root"],
};

const EXEC_RESULT = {
  checkpoint: {
    checkpoint: { manifest_id: "cp-1", label: "", ts: 1000 },
    new_blobs: 2,
    reused_blobs: 0,
    added_bytes: 100,
    skipped: [],
  },
  results: [
    { path: "/work/proj/Screenshots", ok: true, error: "" },
    { path: "/work/proj/shot1.png", ok: true, error: "" },
  ],
  violations: [] as string[],
};

function configure() {
  saveCopilotEnabled(true);
  saveCopilotBaseUrl("http://localhost:1234/v1");
  saveCopilotModel("m");
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async () => null);
  saveCopilotEnabled(false);
  saveCopilotBaseUrl("");
  saveCopilotModel("");
});

describe("CopilotDialog (CPE-1276)", () => {
  it("shows a needs-config prompt (not an error) when the copilot is unconfigured", () => {
    render(CopilotDialog, { root: ROOT });
    expect(screen.getByTestId("needs-config")).toBeTruthy();
    expect(screen.queryByTestId("instruction-input")).toBeNull();
  });

  it("the needs-config prompt's button dispatches openSettings, not a plan call", async () => {
    const { component } = render(CopilotDialog, { root: ROOT });
    const openSettings = vi.fn();
    component.$on("openSettings", openSettings);

    await fireEvent.click(screen.getByTestId("open-settings-btn"));

    expect(openSettings).toHaveBeenCalled();
    expect(invokeMock).not.toHaveBeenCalledWith("copilot_plan", expect.anything());
  });

  it("plan → preview renders the per-kind counts and the ordered op list", async () => {
    configure();
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "copilot_plan") return PLAN_RESULT;
      return null;
    });

    render(CopilotDialog, { root: ROOT });
    await fireEvent.input(screen.getByTestId("instruction-input"), {
      target: { value: "move screenshots into a folder" },
    });
    await fireEvent.click(screen.getByTestId("plan-btn"));

    expect(invokeMock).toHaveBeenCalledWith(
      "copilot_plan",
      expect.objectContaining({ root: ROOT, instruction: "move screenshots into a folder" }),
    );

    const summary = await screen.findByTestId("summary");
    expect(summary.textContent).toContain("1 move");
    expect(summary.textContent).toContain("1 new folder");

    const opList = screen.getByTestId("op-list");
    expect(opList.textContent).toContain("shot1.png");
    expect(opList.textContent).toContain("Screenshots");
    expect(screen.getByTestId("confirm-btn")).toBeTruthy();
  });

  it("a plan with violations shows them and offers NO Confirm button", async () => {
    configure();
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "copilot_plan") return VIOLATING_PLAN_RESULT;
      return null;
    });

    render(CopilotDialog, { root: ROOT });
    await fireEvent.input(screen.getByTestId("instruction-input"), { target: { value: "delete /etc/passwd" } });
    await fireEvent.click(screen.getByTestId("plan-btn"));

    const violations = await screen.findByTestId("violations");
    expect(violations.textContent).toContain("path escapes the scope root");
    expect(screen.queryByTestId("confirm-btn")).toBeNull();
  });

  it("NEVER calls copilot_execute without an explicit Confirm click", async () => {
    configure();
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "copilot_plan") return PLAN_RESULT;
      return null;
    });

    render(CopilotDialog, { root: ROOT });
    await fireEvent.input(screen.getByTestId("instruction-input"), { target: { value: "organize this" } });
    await fireEvent.click(screen.getByTestId("plan-btn"));
    await screen.findByTestId("confirm-btn");

    // A plan was produced and previewed — but nothing has executed.
    expect(invokeMock).not.toHaveBeenCalledWith("copilot_execute", expect.anything());
  });

  it("Confirm calls copilot_execute with exactly the previewed plan, then shows per-op results + Undo", async () => {
    configure();
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "copilot_plan") return PLAN_RESULT;
      if (cmd === "copilot_execute") return EXEC_RESULT;
      return null;
    });

    render(CopilotDialog, { root: ROOT });
    await fireEvent.input(screen.getByTestId("instruction-input"), { target: { value: "organize this" } });
    await fireEvent.click(screen.getByTestId("plan-btn"));
    await screen.findByTestId("confirm-btn");

    await fireEvent.click(screen.getByTestId("confirm-btn"));

    expect(invokeMock).toHaveBeenCalledWith("copilot_execute", { root: ROOT, plan: PLAN_RESULT.plan });

    const results = await screen.findByTestId("exec-results");
    expect(results.textContent).toContain("2 of 2 ops succeeded");
    expect(screen.getByTestId("op-result-0")).toBeTruthy();
    expect(screen.getByTestId("undo-btn")).toBeTruthy();
  });

  it("Undo calls checkpoint_revert with the checkpoint from execute, then reports the outcome", async () => {
    configure();
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "copilot_plan") return PLAN_RESULT;
      if (cmd === "copilot_execute") return EXEC_RESULT;
      if (cmd === "checkpoint_revert") return { applied: 2, skipped: [] };
      return null;
    });

    const { component } = render(CopilotDialog, { root: ROOT });
    const reverted = vi.fn();
    component.$on("reverted", reverted);

    await fireEvent.input(screen.getByTestId("instruction-input"), { target: { value: "organize this" } });
    await fireEvent.click(screen.getByTestId("plan-btn"));
    await screen.findByTestId("confirm-btn");
    await fireEvent.click(screen.getByTestId("confirm-btn"));
    await screen.findByTestId("undo-btn");

    await fireEvent.click(screen.getByTestId("undo-btn"));

    expect(invokeMock).toHaveBeenCalledWith("checkpoint_revert", { root: ROOT, manifestId: "cp-1" });
    const outcome = await screen.findByTestId("undo-outcome");
    expect(outcome.textContent).toContain("2");
    expect(reverted).toHaveBeenCalled();
  });
});
