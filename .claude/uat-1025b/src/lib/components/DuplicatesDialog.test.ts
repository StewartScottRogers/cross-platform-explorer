import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import DuplicatesDialog from "./DuplicatesDialog.svelte";

let response: any = {
  groups: [{ size: 1024, hash: "abc", paths: ["/repo/a.txt", "/repo/sub/b.txt"] }],
  files_scanned: 42,
  truncated: false,
};
// The dialog now streams groups (CPE-420): `find_duplicates_stream` pushes batches through an `onGroup`
// Channel, then returns the terminal `{ files_scanned, truncated }`. The mock feeds `response.groups`
// through that Channel; other commands (delete_to_trash) fall through to `response`.
const invoke = vi.fn(async (cmd: string, args?: any) => {
  if (cmd === "find_duplicates_stream") {
    args?.onGroup?.onmessage?.(response.groups);
    return { files_scanned: response.files_scanned, truncated: response.truncated };
  }
  return response;
});
vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

describe("DuplicatesDialog (CPE-421)", () => {
  it("scans on demand, lists a duplicate group, and navigates on click", async () => {
    response = { groups: [{ size: 1024, hash: "abc", paths: ["/repo/a.txt", "/repo/sub/b.txt"] }], files_scanned: 42, truncated: false };
    const onNavigate = vi.fn();
    const { component } = render(DuplicatesDialog, { root: "/repo" });
    component.$on("navigate", (e: CustomEvent<string>) => onNavigate(e.detail));

    // No automatic scan — an explicit button.
    expect(invoke).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByText("Scan for duplicates"));

    await waitFor(() => expect(screen.getByText(/1 duplicate set/)).toBeTruthy());
    expect(invoke).toHaveBeenCalledWith("find_duplicates_stream", expect.objectContaining({ root: "/repo" }));
    // 2 copies × 1 KB, so ~1 KB reclaimable (formatSize → "1.0 KB"; text is split by an inline icon).
    expect(screen.getByText(/reclaimable/)).toBeTruthy();
    expect(screen.getByText(/2 copies/)).toBeTruthy();

    await fireEvent.click(screen.getByText("b.txt"));
    expect(onNavigate).toHaveBeenCalledWith("/repo/sub/b.txt");
  });

  it("reports when there are no duplicates", async () => {
    response = { groups: [], files_scanned: 10, truncated: false };
    render(DuplicatesDialog, { root: "/repo" });
    await fireEvent.click(screen.getByText("Scan for duplicates"));
    await waitFor(() => expect(screen.getByText(/No duplicate files found/)).toBeTruthy());
  });

  it("cleanup: Select redundant then Move to Recycle Bin trashes the extra copy and prunes (CPE-428)", async () => {
    response = { groups: [{ size: 1024, hash: "abc", paths: ["/repo/a.txt", "/repo/sub/b.txt"] }], files_scanned: 42, truncated: false };
    invoke.mockClear();
    render(DuplicatesDialog, { root: "/repo" });
    await fireEvent.click(screen.getByText("Scan for duplicates"));
    await waitFor(() => expect(screen.getByText(/1 duplicate set/)).toBeTruthy());
    // Safe default keeps the first copy; only /sub/b.txt is marked.
    await fireEvent.click(screen.getByText("Select redundant"));
    await fireEvent.click(await screen.findByText("Move 1 to Recycle Bin"));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("delete_to_trash", { paths: ["/repo/sub/b.txt"] }));
    // The set drops to one copy → no longer a duplicate → the list empties.
    await waitFor(() => expect(screen.getByText(/No duplicate files found/)).toBeTruthy());
  });
});
