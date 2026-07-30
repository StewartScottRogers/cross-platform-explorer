/**
 * OrganizeDialog (CPE-1142, epic CPE-979 "rules-based" slice). The preview/approve UI over
 * `organize_plan`/`organize_apply`. These assert: a preview loads for the default rule on mount,
 * switching the rule debounces then reloads the preview, an empty folder shows the empty state, Apply
 * calls `organize_apply` and surfaces the checkpoint + Undo (never before Apply is clicked), and a
 * failed preview surfaces an error instead of a stale/blank list. The typed `commands.*` client routes
 * through the mocked `../invoke`, so mocking `invoke` here drives it (mirrors `CheckpointDialog.test.ts`).
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";

const invokeMock = vi.fn(async (_cmd: string, _args?: unknown): Promise<unknown> => null);
vi.mock("../invoke", () => ({
  invoke: (...a: unknown[]) => (invokeMock as (...x: unknown[]) => unknown)(...a),
  unwrap: <T>(r: { status: string; data?: T; error?: unknown }): T => {
    if (r.status === "ok") return r.data as T;
    throw r.error instanceof Error ? r.error : new Error(String(r.error));
  },
}));

import OrganizeDialog from "./OrganizeDialog.svelte";

const PLAN_BY_KIND = [
  { name: "photo.png", target_subdir: "Images" },
  { name: "report.pdf", target_subdir: "Documents" },
  { name: "cover.png", target_subdir: "Images" },
];

beforeEach(() => {
  vi.useFakeTimers();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "organize_plan") return PLAN_BY_KIND;
    return null;
  });
});
afterEach(() => {
  vi.useRealTimers();
});

describe("OrganizeDialog (CPE-1142)", () => {
  it("previews the default rule (by_kind) on mount, grouped by destination subfolder", async () => {
    render(OrganizeDialog, { path: "/work/proj" });
    await vi.advanceTimersByTimeAsync(150);

    expect(invokeMock).toHaveBeenCalledWith("organize_plan", { dir: "/work/proj", rule: "by_kind" });
    expect(await screen.findByTestId("summary")).toBeTruthy();
    expect(screen.getByTestId("group-Images")).toBeTruthy();
    expect(screen.getByTestId("group-Documents")).toBeTruthy();
  });

  it("switching the rule debounces, then reloads the preview for the new rule", async () => {
    render(OrganizeDialog, { path: "/work/proj" });
    await vi.advanceTimersByTimeAsync(150);
    invokeMock.mockClear();

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "organize_plan") return [{ name: "a.png", target_subdir: "PNG" }];
      return null;
    });

    await fireEvent.click(screen.getByTestId("rule-by_extension"));
    expect(invokeMock).not.toHaveBeenCalled(); // still inside the debounce window

    await vi.advanceTimersByTimeAsync(150);
    expect(invokeMock).toHaveBeenCalledWith("organize_plan", { dir: "/work/proj", rule: "by_extension" });
    expect(await screen.findByTestId("group-PNG")).toBeTruthy();
  });

  it("shows the empty state when the folder has no files to organize", async () => {
    invokeMock.mockImplementation(async (cmd: string) => (cmd === "organize_plan" ? [] : null));
    render(OrganizeDialog, { path: "/work/empty" });
    await vi.advanceTimersByTimeAsync(150);

    expect(await screen.findByTestId("empty-state")).toBeTruthy();
    expect((screen.getByTestId("apply-btn") as HTMLButtonElement).disabled).toBe(true);
  });

  it("surfaces a preview error instead of showing a stale or blank list", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "organize_plan") throw new Error("permission denied");
      return null;
    });
    render(OrganizeDialog, { path: "/work/proj" });
    await vi.advanceTimersByTimeAsync(150);

    expect(await screen.findByTestId("error")).toBeTruthy();
  });

  it("never calls organize_apply before the user clicks Apply", async () => {
    render(OrganizeDialog, { path: "/work/proj" });
    await vi.advanceTimersByTimeAsync(150);
    await screen.findByTestId("group-Images");

    expect(invokeMock).not.toHaveBeenCalledWith("organize_apply", expect.anything());
  });

  it("Apply calls organize_apply, checkpoints, and surfaces the result + an Undo action", async () => {
    const { component } = render(OrganizeDialog, { path: "/work/proj" });
    await vi.advanceTimersByTimeAsync(150);
    await screen.findByTestId("group-Images");

    const applied = vi.fn();
    const undo = vi.fn();
    component.$on("applied", applied);
    component.$on("undo", undo);

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "organize_apply") {
        return {
          checkpoint: {
            checkpoint: { manifest_id: "m-1", label: "Before auto-organize", ts: 1000 },
            new_blobs: 3,
            reused_blobs: 0,
            added_bytes: 100,
            skipped: [],
          },
          results: [
            { path: "/work/proj/Images/photo.png", ok: true, error: "" },
            { path: "/work/proj/Documents/report.pdf", ok: true, error: "" },
            { path: "/work/proj/Images/cover.png", ok: true, error: "" },
          ],
        };
      }
      return null;
    });

    await fireEvent.click(screen.getByTestId("apply-btn"));

    expect(invokeMock).toHaveBeenCalledWith("organize_apply", { dir: "/work/proj", rule: "by_kind" });
    expect(await screen.findByTestId("outcome-panel")).toBeTruthy();
    expect(applied).toHaveBeenCalled();

    await fireEvent.click(screen.getByTestId("undo-btn"));
    expect(undo).toHaveBeenCalled();
  });
});
