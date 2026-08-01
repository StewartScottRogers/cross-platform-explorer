/**
 * RepairLinkDialog (CPE-1209, epic CPE-715). Owns its own backend sequence: `suggest_repair` on mount,
 * then Accept → confirm → `delete_permanent` (unlink the dead symlink entry) → `create_symlink` (point
 * it at the accepted target). Mocks the Tauri `invoke` boundary per command.
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import RepairLinkDialog from "./RepairLinkDialog.svelte";

// `delete_permanent` (an `OpResult[]`-returning command, never rejects) reports failure IN the array;
// `create_symlink` (a `Result<T, E>`-returning command) rejects the JS promise on Err — real Tauri IPC
// semantics, same note as NewLinkDialog.test.ts.
let suggestion: string | null = "/repo/found/marker.txt";
let deleteOk = true;
let createOk = true;
const invoke = vi.fn(async (cmd: string, args?: any) => {
  if (cmd === "suggest_repair") return suggestion;
  if (cmd === "delete_permanent") {
    return deleteOk
      ? [{ path: args.paths[0], ok: true, error: "" }]
      : [{ path: args.paths[0], ok: false, error: "permission denied" }];
  }
  if (cmd === "create_symlink") {
    if (!createOk) throw new Error("os error 5");
    return null;
  }
  throw new Error(`unexpected invoke: ${cmd}`);
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: unknown) => invoke(cmd, args) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const base = { linkPath: "/repo/broken-link.txt", linkName: "broken-link.txt", searchRoots: ["/repo"] };

describe("RepairLinkDialog (CPE-1209)", () => {
  it("calls suggest_repair on mount and shows the suggested target", async () => {
    suggestion = "/repo/found/marker.txt";
    invoke.mockClear();
    render(RepairLinkDialog, base);

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("suggest_repair", {
      brokenLink: "/repo/broken-link.txt",
      searchRoots: ["/repo"],
    }));
    await waitFor(() => expect(screen.getByTestId("repair-suggestion").textContent).toContain("/repo/found/marker.txt"));
    expect(screen.getByTestId("repair-accept")).toBeTruthy();
  });

  it("handles 'no suggestion found' gracefully — no Accept button, no crash", async () => {
    suggestion = null;
    render(RepairLinkDialog, base);
    await waitFor(() => expect(screen.getByTestId("repair-no-suggestion")).toBeTruthy());
    expect(screen.queryByTestId("repair-accept")).toBeNull();
    expect(screen.getByTestId("repair-browse")).toBeTruthy();
  });

  it("Accept → confirm calls suggest_repair then delete_permanent then create_symlink to the accepted target, and dispatches repaired", async () => {
    suggestion = "/repo/found/marker.txt";
    deleteOk = true;
    createOk = true;
    invoke.mockClear();
    const { component } = render(RepairLinkDialog, base);
    const repaired = vi.fn();
    component.$on("repaired", (e: CustomEvent<string>) => repaired(e.detail));

    await waitFor(() => expect(screen.getByTestId("repair-accept")).toBeTruthy());
    await fireEvent.click(screen.getByTestId("repair-accept"));

    // Confirm-before-overwrite step (ticket requirement) — the destructive call hasn't happened yet.
    expect(screen.getByTestId("repair-confirm-text").textContent).toContain("/repo/found/marker.txt");
    expect(invoke).not.toHaveBeenCalledWith("delete_permanent", expect.anything());

    await fireEvent.click(screen.getByTestId("repair-confirm-yes"));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("delete_permanent", { paths: ["/repo/broken-link.txt"] }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("create_symlink", {
      target: "/repo/found/marker.txt",
      linkPath: "/repo/broken-link.txt",
    }));
    await waitFor(() => expect(repaired).toHaveBeenCalledWith("/repo/found/marker.txt"));
  });

  it("surfaces a failed re-create via the dispatched error event and keeps the dialog open to retry", async () => {
    suggestion = "/repo/found/marker.txt";
    deleteOk = true;
    createOk = false;
    invoke.mockClear();
    const { component } = render(RepairLinkDialog, base);
    const errorSpy = vi.fn();
    component.$on("error", (e: CustomEvent<string>) => errorSpy(e.detail));

    await waitFor(() => expect(screen.getByTestId("repair-accept")).toBeTruthy());
    await fireEvent.click(screen.getByTestId("repair-accept"));
    await fireEvent.click(screen.getByTestId("repair-confirm-yes"));

    await waitFor(() => expect(errorSpy).toHaveBeenCalledTimes(1));
    expect(errorSpy.mock.calls[0][0]).toContain("os error 5");
    expect(screen.getByTestId("repair-error")).toBeTruthy();
    // Back on the ready screen (not stuck mid-confirm), so the user can browse for another target.
    expect(screen.getByTestId("repair-browse")).toBeTruthy();
  });
});
