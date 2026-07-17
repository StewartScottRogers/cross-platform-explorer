import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import ContentSearchDialog from "./ContentSearchDialog.svelte";

// The dialog persists recent queries + the Match-case toggle (CPE-558/576); isolate localStorage.
beforeEach(() => { try { localStorage.clear(); } catch { /* ignore */ } });

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
    // The line text is split across <mark>/text nodes now (CPE-557 highlighting), so match on the
    // <code> element's full textContent.
    const codeLine = (text: string) => (_: string, el: Element | null) =>
      el?.tagName.toLowerCase() === "code" && el.textContent === text;
    expect(screen.getByText(codeLine("the needle is here"))).toBeTruthy();
    // The query is highlighted: one <mark>needle</mark> per result line (CPE-557).
    expect(screen.getAllByText("needle")).toHaveLength(2);

    // Clicking a hit dispatches navigate with the FILE path (the host reveals + selects it, CPE-423).
    await fireEvent.click(screen.getByText(codeLine("found the needle deep")));
    expect(onNavigate).toHaveBeenCalledWith("/repo/sub/b.md");
  });

  it("collapses a file's matches when its chevron is clicked (CPE-574)", async () => {
    const codeLine = (text: string) => (_: string, el: Element | null) =>
      el?.tagName.toLowerCase() === "code" && el.textContent === text;
    render(ContentSearchDialog, { root: "/repo" });
    await fireEvent.input(screen.getByPlaceholderText("Text to find inside files"), { target: { value: "needle" } });
    await fireEvent.click(screen.getByText("Search"));
    await waitFor(() => expect(screen.getByText(codeLine("the needle is here"))).toBeTruthy());

    const chevrons = screen.getAllByLabelText("Toggle file");
    await fireEvent.click(chevrons[0]); // collapse the first file (a.txt)
    expect(screen.queryByText(codeLine("the needle is here"))).toBeNull();
    // the other file's match still shows
    expect(screen.getByText(codeLine("found the needle deep"))).toBeTruthy();
  });

  it("restores the saved Match-case toggle on open (CPE-576)", async () => {
    localStorage.setItem("cpe.contentSearchCase", "1");
    render(ContentSearchDialog, { root: "/repo" });
    const caseBox = screen.getByRole("checkbox") as HTMLInputElement;
    expect(caseBox.checked).toBe(true);
  });

  it("shows a no-matches message when nothing is found", async () => {
    render(ContentSearchDialog, { root: "/repo" });
    await fireEvent.input(screen.getByPlaceholderText("Text to find inside files"), { target: { value: "zzz" } });
    await fireEvent.click(screen.getByText("Search"));
    await waitFor(() => expect(screen.getByText("No matches in this folder.")).toBeTruthy());
  });
});
