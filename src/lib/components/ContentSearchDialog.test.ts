import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import ContentSearchDialog from "./ContentSearchDialog.svelte";

const invoke = vi.fn(async (_cmd: string, args: any) => {
  if (args.query === "needle")
    return {
      matches: [
        { path: "/repo/a.txt", line_number: 3, line: "the needle is here" },
        { path: "/repo/sub/b.md", line_number: 7, line: "found the needle deep" },
      ],
      files_scanned: 2,
      truncated: false,
    };
  return { matches: [], files_scanned: 5, truncated: false };
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...(a as [string, any])) }));

describe("ContentSearchDialog (CPE-417)", () => {
  it("searches the current folder and groups results; a hit navigates to its folder", async () => {
    const onNavigate = vi.fn();
    const { component } = render(ContentSearchDialog, { root: "/repo" });
    component.$on("navigate", (e: CustomEvent<string>) => onNavigate(e.detail));

    await fireEvent.input(screen.getByPlaceholderText("Text to find inside files"), { target: { value: "needle" } });
    await fireEvent.click(screen.getByText("Search"));

    await waitFor(() => expect(screen.getByText(/2 matches in 2 files/)).toBeTruthy());
    // Passes the current folder as root + the camelCase caseSensitive arg.
    expect(invoke).toHaveBeenCalledWith("search_file_contents", { root: "/repo", query: "needle", caseSensitive: false });
    expect(screen.getByText("a.txt")).toBeTruthy();
    expect(screen.getByText("the needle is here")).toBeTruthy();

    // Clicking a hit dispatches navigate to the file's PARENT folder.
    await fireEvent.click(screen.getByText("found the needle deep"));
    expect(onNavigate).toHaveBeenCalledWith("/repo/sub");
  });

  it("shows a no-matches message when nothing is found", async () => {
    render(ContentSearchDialog, { root: "/repo" });
    await fireEvent.input(screen.getByPlaceholderText("Text to find inside files"), { target: { value: "zzz" } });
    await fireEvent.click(screen.getByText("Search"));
    await waitFor(() => expect(screen.getByText("No matches in this folder.")).toBeTruthy());
  });
});
