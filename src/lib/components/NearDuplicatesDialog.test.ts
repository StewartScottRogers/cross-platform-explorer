import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import NearDuplicatesDialog from "./NearDuplicatesDialog.svelte";

// The dialog wraps the modest collect-to-vec `find_similar_documents` / `find_similar_folders` commands
// (CPE-1204, epic CPE-997 stretch) — a plain awaited `commands.*` call through the typed client, not a
// streaming Channel like SimilarImagesDialog/DuplicatesDialog. So the mock just resolves/rejects the raw
// backend return value; the generated bindings client wraps it into the `Result` shape.
let calls: Array<{ cmd: string; args: unknown }> = [];
let responder: ((cmd: string, args: unknown) => unknown) | null = null;

const invoke = vi.fn(async (cmd: string, args?: any) => {
  calls.push({ cmd, args });
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
