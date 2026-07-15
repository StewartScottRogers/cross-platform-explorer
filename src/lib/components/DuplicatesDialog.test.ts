import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import DuplicatesDialog from "./DuplicatesDialog.svelte";

let response: any = {
  groups: [{ size: 1024, hash: "abc", paths: ["/repo/a.txt", "/repo/sub/b.txt"] }],
  files_scanned: 42,
  truncated: false,
};
const invoke = vi.fn(async (_cmd: string, _args?: unknown) => response);
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: unknown) => invoke(cmd, args) }));

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
    expect(invoke).toHaveBeenCalledWith("find_duplicates", { root: "/repo" });
    // 2 copies × 1 KB, so ~1 KB reclaimable (formatSize → "1.0 KB"; text is split by an inline icon).
    expect(screen.getByText(/reclaimable/)).toBeTruthy();
    expect(screen.getByText(/2 copies/)).toBeTruthy();

    await fireEvent.click(screen.getByText("b.txt"));
    expect(onNavigate).toHaveBeenCalledWith("/repo/sub");
  });

  it("reports when there are no duplicates", async () => {
    response = { groups: [], files_scanned: 10, truncated: false };
    render(DuplicatesDialog, { root: "/repo" });
    await fireEvent.click(screen.getByText("Scan for duplicates"));
    await waitFor(() => expect(screen.getByText(/No duplicate files found/)).toBeTruthy());
  });
});
