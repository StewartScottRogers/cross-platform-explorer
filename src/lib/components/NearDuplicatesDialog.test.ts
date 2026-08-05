import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import NearDuplicatesDialog from "./NearDuplicatesDialog.svelte";

// The dialog wraps the modest collect-to-vec `find_similar_documents` / `find_similar_folders` commands
// (CPE-1204, epic CPE-997 stretch) — a plain awaited `commands.*` call through the typed client, not a
// streaming Channel like SimilarImagesDialog/DuplicatesDialog. So the mock just resolves/rejects the raw
// backend return value; the generated bindings client wraps it into the `Result` shape.
//
// CPE-1324 adds keeper-guarded cleanup (mirroring SimilarImagesDialog), so the mock also handles
// `delete_to_trash` / `checkpoint_create` the same way the SimilarImagesDialog test does.
let calls: Array<{ cmd: string; args: unknown }> = [];
let responder: ((cmd: string, args: unknown) => unknown) | null = null;
let trashCalls: string[][] = [];
let checkpointCalls: Array<[string, string]> = [];

const invoke = vi.fn(async (cmd: string, args?: any) => {
  calls.push({ cmd, args });
  if (cmd === "delete_to_trash") {
    trashCalls.push(args.paths);
    return [];
  }
  if (cmd === "checkpoint_create") {
    checkpointCalls.push([args.root, args.label]);
    return { status: "ok", data: { id: "cp1", label: args.label } };
  }
  if (!responder) throw new Error(`no responder set for ${cmd}`);
  return responder(cmd, args);
});

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

beforeEach(() => {
  invoke.mockClear();
  calls = [];
  responder = null;
  trashCalls = [];
  checkpointCalls = [];
});

describe("NearDuplicatesDialog — documents kind (CPE-1204)", () => {
  it("does not scan until the user clicks Scan, then calls find_similar_documents with root", async () => {
    responder = () => ({ groups: [], files_scanned: 0, truncated: false });
    render(NearDuplicatesDialog, { root: "/repo", kind: "documents" });
    expect(invoke).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByTestId("nd-scan-btn"));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("find_similar_documents", { root: "/repo" }));
  });

  it("renders a returned group of near-duplicate documents with paths split into name + location", async () => {
    responder = () => ({
      groups: [{ paths: ["/repo/note.txt", "/repo/sub/note-edited.md"] }],
      files_scanned: 3,
      truncated: false,
    });
    render(NearDuplicatesDialog, { root: "/repo", kind: "documents" });
    await fireEvent.click(screen.getByTestId("nd-scan-btn"));

    await waitFor(() => expect(screen.getByTestId("nd-group")).toBeTruthy());
    expect(screen.getByText("note.txt")).toBeTruthy();
    expect(screen.getByText("note-edited.md")).toBeTruthy();
    expect(screen.getByText(/1 group found/)).toBeTruthy();
  });

  it("shows the empty state when nothing groups, without a stuck spinner", async () => {
    responder = () => ({ groups: [], files_scanned: 12, truncated: false });
    render(NearDuplicatesDialog, { root: "/repo", kind: "documents" });
    await fireEvent.click(screen.getByTestId("nd-scan-btn"));

    await waitFor(() => expect(screen.getByTestId("nd-none")).toBeTruthy());
    expect(screen.getByText(/No matches found/)).toBeTruthy();
  });

  it("surfaces a backend error instead of hanging in the loading state", async () => {
    responder = () => {
      throw "not a folder"; // a plain (non-Error) rejection — mirrors a Tauri command's Err(String)
    };
    render(NearDuplicatesDialog, { root: "/repo", kind: "documents" });
    await fireEvent.click(screen.getByTestId("nd-scan-btn"));

    await waitFor(() => expect(screen.getByText("not a folder")).toBeTruthy());
    expect(screen.getByTestId("nd-rescan-btn")).toBeTruthy();
  });

  it("clicking an item dispatches navigate with the item's path and closes", async () => {
    responder = () => ({
      groups: [{ paths: ["/repo/a.txt", "/repo/b.txt"] }],
      files_scanned: 2,
      truncated: false,
    });
    const { component } = render(NearDuplicatesDialog, { root: "/repo", kind: "documents" });
    const navigated: string[] = [];
    let closed = false;
    component.$on("navigate", (e: CustomEvent<string>) => navigated.push(e.detail));
    component.$on("close", () => (closed = true));

    await fireEvent.click(screen.getByTestId("nd-scan-btn"));
    await waitFor(() => expect(screen.getByTestId("nd-group")).toBeTruthy());
    await fireEvent.click(screen.getAllByTestId("nd-item")[0]);

    expect(navigated).toEqual(["/repo/a.txt"]);
    expect(closed).toBe(true);
  });

  it("rescanning replaces the previous results rather than appending to them", async () => {
    let callIndex = 0;
    responder = () => {
      callIndex += 1;
      if (callIndex === 1) {
        return { groups: [{ paths: ["/repo/old1.txt", "/repo/old2.txt"] }], files_scanned: 2, truncated: false };
      }
      return { groups: [{ paths: ["/repo/new1.txt", "/repo/new2.txt"] }], files_scanned: 2, truncated: false };
    };
    render(NearDuplicatesDialog, { root: "/repo", kind: "documents" });
    await fireEvent.click(screen.getByTestId("nd-scan-btn"));
    await waitFor(() => expect(screen.getByText("old1.txt")).toBeTruthy());

    await fireEvent.click(screen.getByTestId("nd-rescan-btn"));
    await waitFor(() => expect(screen.getByText("new1.txt")).toBeTruthy());
    expect(screen.queryByText("old1.txt")).toBeNull();
    expect(invoke).toHaveBeenCalledTimes(2);
  });
});

describe("NearDuplicatesDialog — folders kind (CPE-1204)", () => {
  it("calls find_similar_folders and reads folders_scanned (not files_scanned) for the count", async () => {
    responder = () => ({
      groups: [{ paths: ["/repo/Photos", "/repo/Photos (backup)"] }],
      folders_scanned: 5,
      files_scanned: 40,
      truncated: false,
    });
    render(NearDuplicatesDialog, { root: "/repo", kind: "folders" });
    await fireEvent.click(screen.getByTestId("nd-scan-btn"));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("find_similar_folders", { root: "/repo" }));
    await waitFor(() => expect(screen.getByText("Photos")).toBeTruthy());
    expect(screen.getByText("Photos (backup)")).toBeTruthy();
    // scanned count comes from folders_scanned (5), not files_scanned (40).
    expect(screen.getByText(/5 scanned/)).toBeTruthy();
  });

  it("uses the folders-specific title", async () => {
    responder = () => ({ groups: [], folders_scanned: 0, files_scanned: 0, truncated: false });
    render(NearDuplicatesDialog, { root: "/repo", kind: "folders" });
    expect(screen.getByText("Find near-identical folders")).toBeTruthy();
  });
});

// CPE-1324: keeper-guarded cleanup, mirroring SimilarImagesDialog's safety rails via the SAME
// `keepsOnePerGroup` / `pruneGroups` helpers from ../duplicates (not re-implemented here).
describe("NearDuplicatesDialog — keeper-guarded cleanup (CPE-1324)", () => {
  it("SAFETY: Move to Bin is disabled whenever the selection would delete every copy of a group", async () => {
    responder = () => ({
      groups: [{ paths: ["/repo/a.txt", "/repo/b.txt"] }],
      files_scanned: 2,
      truncated: false,
    });
    render(NearDuplicatesDialog, { root: "/repo", kind: "documents" });
    await fireEvent.click(screen.getByTestId("nd-scan-btn"));
    await waitFor(() => expect(screen.getByTestId("nd-group")).toBeTruthy());

    const moveBtn = screen.getByTestId("nd-move-btn") as HTMLButtonElement;
    // Default selects NOTHING → disabled.
    expect(moveBtn.disabled).toBe(true);

    const boxes = screen.getAllByRole("checkbox") as HTMLInputElement[];
    // Select the FIRST item only → one copy still survives → enabled.
    await fireEvent.click(boxes[0]);
    await waitFor(() => expect(moveBtn.disabled).toBe(false));
    // Select the SECOND too → now the whole group would be wiped → disabled by the keeper guard.
    await fireEvent.click(boxes[1]);
    await waitFor(() => expect(moveBtn.disabled).toBe(true));
  });

  it("SAFETY: cleanup checkpoints first, then trashes ONLY the selected paths, then prunes the group", async () => {
    responder = () => ({
      groups: [{ paths: ["/repo/a.txt", "/repo/sub/b.txt"] }],
      files_scanned: 2,
      truncated: false,
    });
    render(NearDuplicatesDialog, { root: "/repo", kind: "documents" });
    await fireEvent.click(screen.getByTestId("nd-scan-btn"));
    await waitFor(() => expect(screen.getByTestId("nd-group")).toBeTruthy());

    // "Select extras" keeps the first, marks only the second copy.
    await fireEvent.click(screen.getByText("Select extras"));
    await fireEvent.click(await screen.findByText("Move 1 to Recycle Bin"));

    await waitFor(() => expect(trashCalls).toEqual([["/repo/sub/b.txt"]]));
    // Recoverable Bin only — never a permanent delete.
    expect(invoke).not.toHaveBeenCalledWith("delete_permanently", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("delete", expect.anything());
    // A checkpoint was taken first, scoped to the folder.
    expect(checkpointCalls).toEqual([["/repo", "Before removing near-duplicates"]]);
    // Checkpoint call happened before the trash call (call order).
    const checkpointIdx = calls.findIndex((c) => c.cmd === "checkpoint_create");
    const trashIdx = calls.findIndex((c) => c.cmd === "delete_to_trash");
    expect(checkpointIdx).toBeGreaterThanOrEqual(0);
    expect(trashIdx).toBeGreaterThan(checkpointIdx);
    // The group dropped to one copy → no longer a near-duplicate → pruned out → the list empties.
    await waitFor(() => expect(screen.getByTestId("nd-none")).toBeTruthy());
  });

  it("a checkpoint failure does not block the (already recoverable) trash move", async () => {
    responder = (cmd) => {
      if (cmd === "checkpoint_create") throw new Error("disk full");
      return { groups: [{ paths: ["/repo/a.txt", "/repo/b.txt"] }], files_scanned: 2, truncated: false };
    };
    render(NearDuplicatesDialog, { root: "/repo", kind: "documents" });
    await fireEvent.click(screen.getByTestId("nd-scan-btn"));
    await waitFor(() => expect(screen.getByTestId("nd-group")).toBeTruthy());

    await fireEvent.click(screen.getByText("Select extras"));
    await fireEvent.click(await screen.findByText("Move 1 to Recycle Bin"));

    // Checkpoint failed but the trash move still went through — a checkpoint is a bonus, not a gate.
    await waitFor(() => expect(trashCalls).toEqual([["/repo/b.txt"]]));
    await waitFor(() => expect(screen.getByTestId("nd-none")).toBeTruthy());
  });

  it("shows a live selected count in the Move to Bin label", async () => {
    responder = () => ({
      groups: [{ paths: ["/repo/a.txt", "/repo/b.txt", "/repo/c.txt"] }],
      files_scanned: 3,
      truncated: false,
    });
    render(NearDuplicatesDialog, { root: "/repo", kind: "documents" });
    await fireEvent.click(screen.getByTestId("nd-scan-btn"));
    await waitFor(() => expect(screen.getByTestId("nd-group")).toBeTruthy());

    expect(screen.getByText("Move 0 to Recycle Bin")).toBeTruthy();
    const boxes = screen.getAllByRole("checkbox") as HTMLInputElement[];
    await fireEvent.click(boxes[1]);
    await waitFor(() => expect(screen.getByText("Move 1 to Recycle Bin")).toBeTruthy());
    await fireEvent.click(boxes[2]);
    await waitFor(() => expect(screen.getByText("Move 2 to Recycle Bin")).toBeTruthy());
  });

  it("selection resets on rescan", async () => {
    let callIndex = 0;
    responder = () => {
      callIndex += 1;
      return { groups: [{ paths: ["/repo/a.txt", "/repo/b.txt"] }], files_scanned: 2, truncated: false };
    };
    render(NearDuplicatesDialog, { root: "/repo", kind: "documents" });
    await fireEvent.click(screen.getByTestId("nd-scan-btn"));
    await waitFor(() => expect(screen.getByTestId("nd-group")).toBeTruthy());

    await fireEvent.click(screen.getByText("Select extras"));
    await waitFor(() => expect(screen.getByText("Move 1 to Recycle Bin")).toBeTruthy());

    await fireEvent.click(screen.getByTestId("nd-rescan-btn"));
    await waitFor(() => expect(screen.getByTestId("nd-group")).toBeTruthy());
    expect(screen.getByText("Move 0 to Recycle Bin")).toBeTruthy();
    const moveBtn = screen.getByTestId("nd-move-btn") as HTMLButtonElement;
    expect(moveBtn.disabled).toBe(true);
  });
});
