/**
 * CheckpointDialog (CPE-1125, epic CPE-732). The palette-driven command surface over the CPE-1123
 * checkpoint_* commands. These assert: the dialog lists checkpoints for the root on open, creates a
 * checkpoint (create → refresh list), previews a revert, and — the guardrail this ticket calls out —
 * a revert (whole-tree or single-path) never fires on one click: it must be confirmed first. The typed
 * `commands.*` client routes through the mocked `../invoke`, so mocking `invoke` here drives it.
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

import CheckpointDialog from "./CheckpointDialog.svelte";

const CHECKPOINTS = [
  { manifest_id: "m-2", label: "before refactor", ts: 2000 },
  { manifest_id: "m-1", label: "", ts: 1000 },
];

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "checkpoint_list") return CHECKPOINTS;
    return null;
  });
});

describe("CheckpointDialog (CPE-1125)", () => {
  it("lists checkpoints for the root on open", async () => {
    render(CheckpointDialog, { initialPath: "/work/proj" });
    expect(await screen.findByTestId("cp-m-2")).toBeTruthy();
    expect(screen.getByTestId("cp-m-1")).toBeTruthy();
    expect(invokeMock).toHaveBeenCalledWith("checkpoint_list", { root: "/work/proj" });
  });

  it("creates a checkpoint via checkpoint_create, then refreshes the list", async () => {
    render(CheckpointDialog, { initialPath: "/work/proj" });
    await screen.findByTestId("cp-m-2");

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "checkpoint_create") {
        return {
          checkpoint: { manifest_id: "m-3", label: "clean", ts: 3000 },
          new_blobs: 4,
          reused_blobs: 1,
          added_bytes: 128,
          skipped: [],
        };
      }
      if (cmd === "checkpoint_list") return [{ manifest_id: "m-3", label: "clean", ts: 3000 }, ...CHECKPOINTS];
      return null;
    });

    await fireEvent.input(screen.getByTestId("checkpoint-label"), { target: { value: "clean" } });
    await fireEvent.click(screen.getByTestId("create-btn"));

    expect(invokeMock).toHaveBeenCalledWith("checkpoint_create", { root: "/work/proj", label: "clean" });
    expect(await screen.findByTestId("cp-m-3")).toBeTruthy();
    expect(await screen.findByTestId("note")).toBeTruthy();
  });

  it("previews a revert via checkpoint_preview_revert and surfaces the drift count", async () => {
    render(CheckpointDialog, { initialPath: "/work/proj" });
    await screen.findByTestId("cp-m-2");

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "checkpoint_preview_revert") {
        return { creates: 1, overwrites: 2, deletes: 0, bytes_written: 4096, total: 3, drift_count: 1, drift_paths: ["/work/proj/a.txt"] };
      }
      if (cmd === "checkpoint_list") return CHECKPOINTS;
      return null;
    });

    await fireEvent.click(screen.getByTestId("preview-btn-m-2"));

    expect(invokeMock).toHaveBeenCalledWith("checkpoint_preview_revert", { root: "/work/proj", manifestId: "m-2" });
    const panel = await screen.findByTestId("preview-panel");
    expect(panel.textContent).toContain("drift 1");
    expect(await screen.findByTestId("drift-list")).toBeTruthy();
  });

  it("does NOT call checkpoint_revert on the first click — it arms a confirmation panel instead", async () => {
    render(CheckpointDialog, { initialPath: "/work/proj" });
    await screen.findByTestId("cp-m-2");

    await fireEvent.click(screen.getByTestId("revert-btn-m-2"));

    expect(invokeMock).not.toHaveBeenCalledWith("checkpoint_revert", expect.anything());
    expect(await screen.findByTestId("confirm-revert")).toBeTruthy();
  });

  it("cancelling the confirmation panel never reverts", async () => {
    render(CheckpointDialog, { initialPath: "/work/proj" });
    await screen.findByTestId("cp-m-2");

    await fireEvent.click(screen.getByTestId("revert-btn-m-2"));
    await screen.findByTestId("confirm-revert");
    await fireEvent.click(screen.getByTestId("confirm-cancel-btn"));

    expect(screen.queryByTestId("confirm-revert")).toBeNull();
    expect(invokeMock).not.toHaveBeenCalledWith("checkpoint_revert", expect.anything());
  });

  it("reverts (whole tree) only after the confirm panel is accepted, then emits reverted", async () => {
    const { component } = render(CheckpointDialog, { initialPath: "/work/proj" });
    await screen.findByTestId("cp-m-2");

    const reverted = vi.fn();
    component.$on("reverted", reverted);

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "checkpoint_revert") return { applied: 3, skipped: [] };
      if (cmd === "checkpoint_list") return CHECKPOINTS;
      return null;
    });

    await fireEvent.click(screen.getByTestId("revert-btn-m-2"));
    await screen.findByTestId("confirm-revert");
    await fireEvent.click(screen.getByTestId("confirm-yes-btn"));

    expect(invokeMock).toHaveBeenCalledWith("checkpoint_revert", { root: "/work/proj", manifestId: "m-2" });
    expect(await screen.findByTestId("outcome-panel")).toBeTruthy();
    expect(reverted).toHaveBeenCalled();
  });

  it("reverts a single path via checkpoint_revert_one only after confirming, scoped to the typed path", async () => {
    render(CheckpointDialog, { initialPath: "/work/proj" });
    await screen.findByTestId("cp-m-2");

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "checkpoint_preview_revert") return { creates: 0, overwrites: 1, deletes: 0, bytes_written: 10, total: 1, drift_count: 0, drift_paths: [] };
      if (cmd === "checkpoint_revert_one") return { applied: 1, skipped: [] };
      if (cmd === "checkpoint_list") return CHECKPOINTS;
      return null;
    });

    // Select the checkpoint (Preview also selects it) so the revert-one row appears.
    await fireEvent.click(screen.getByTestId("preview-btn-m-2"));
    await screen.findByTestId("revert-one-path");

    await fireEvent.input(screen.getByTestId("revert-one-path"), { target: { value: "/work/proj/a.txt" } });
    await fireEvent.click(screen.getByTestId("revert-one-btn"));

    // Still armed, not yet applied.
    expect(invokeMock).not.toHaveBeenCalledWith("checkpoint_revert_one", expect.anything());
    await fireEvent.click(await screen.findByTestId("confirm-yes-btn"));

    expect(invokeMock).toHaveBeenCalledWith("checkpoint_revert_one", { root: "/work/proj", manifestId: "m-2", path: "/work/proj/a.txt" });
  });
});
