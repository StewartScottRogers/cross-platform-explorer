import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import CompareDialog from "./CompareDialog.svelte";

// Per-test fixtures, keyed by path ("/left" / "/right"). A value of `Error` means the backend call
// rejects for that path — mirrors scan_tree's real "not a folder" error when the path is a file, and
// read_file_text's real UTF-8-decode failure for a binary file.
let scanTreeLeft: any = [];
let scanTreeRight: any = [];
let textLeft: string | Error = "";
let textRight: string | Error = "";
let rangeLeft: number[] | Error = [];
let rangeRight: number[] | Error = [];
let imageDiffResult: any | Error = null;

// `commands.*` in bindings.gen.ts call `TAURI_INVOKE(cmd, args)`, which is `invoke` re-exported from
// `./invoke`, itself wrapping `@tauri-apps/api/core`'s `invoke` — so that's the seam to mock (same
// recipe as DuplicatesDialog.test.ts).
const invoke = vi.fn(async (cmd: string, args?: any) => {
  if (cmd === "scan_tree") {
    const v = args.path === "/left" ? scanTreeLeft : scanTreeRight;
    if (v instanceof Error) throw v;
    return v;
  }
  if (cmd === "read_file_text") {
    const v = args.path === "/left" ? textLeft : textRight;
    if (v instanceof Error) throw v;
    return v;
  }
  if (cmd === "read_file_range") {
    const v = args.path === "/left" ? rangeLeft : rangeRight;
    if (v instanceof Error) throw v;
    return v;
  }
  if (cmd === "diff_images") {
    if (imageDiffResult instanceof Error) throw imageDiffResult;
    return imageDiffResult;
  }
  throw new Error(`unexpected command ${cmd}`);
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: unknown) => invoke(cmd, args) }));

async function typePaths(left = "/left", right = "/right") {
  await fireEvent.input(screen.getByLabelText("Left path"), { target: { value: left } });
  await fireEvent.input(screen.getByLabelText("Right path"), { target: { value: right } });
}

describe("CompareDialog (CPE-1393)", () => {
  it("folder mode: scans both sides via scan_tree, diffs, and renders the summary + rows", async () => {
    invoke.mockClear();
    scanTreeLeft = [
      {
        name: "sub",
        isDir: true,
        children: [{ name: "x.txt", isDir: false, size: 1, modified: 1 }],
      },
      { name: "only-left.txt", isDir: false, size: 10, modified: 1 },
    ];
    scanTreeRight = [
      {
        name: "sub",
        isDir: true,
        children: [{ name: "x.txt", isDir: false, size: 2, modified: 2 }],
      },
    ];

    render(CompareDialog, {});
    await typePaths();
    await fireEvent.click(screen.getByTestId("compare-btn"));

    await waitFor(() => expect(screen.getByTestId("compare-summary")).toBeTruthy());
    expect(invoke).toHaveBeenCalledWith("scan_tree", { path: "/left", maxDepth: 32 });
    expect(invoke).toHaveBeenCalledWith("scan_tree", { path: "/right", maxDepth: 32 });

    // x.txt differs by size+modified under "sub" (changed), "sub" itself rolls up to changed since a
    // descendant differs, "only-left.txt" only exists on the left (removed). Leaves: 1 changed, 1 removed.
    const summary = screen.getByTestId("compare-summary");
    expect(summary.textContent).toContain("+0");
    expect(summary.textContent).toContain("−1"); // removed uses U+2212 minus sign
    expect(summary.textContent).toContain("~1");
    expect(summary.textContent).toContain("=0");

    // Dirs sort first (`ordered`), so "sub" (with its child expanded) comes before "only-left.txt".
    let nodes = screen.getAllByTestId("compare-node");
    expect(nodes.map((n) => n.textContent?.trim())).toEqual([
      expect.stringContaining("sub"),
      expect.stringContaining("x.txt"),
      expect.stringContaining("only-left.txt"),
    ]);
    expect(nodes[0].dataset.status).toBe("changed");
    expect(nodes[1].dataset.status).toBe("changed");
    expect(nodes[2].dataset.status).toBe("removed");

    // Collapsing "sub" hides its child row (toggle() + flattenDiff via the collapsed set).
    await fireEvent.click(nodes[0]);
    nodes = screen.getAllByTestId("compare-node");
    expect(nodes.map((n) => n.textContent?.trim())).toEqual([
      expect.stringContaining("sub"),
      expect.stringContaining("only-left.txt"),
    ]);

    // Expanding again brings the child row back.
    await fireEvent.click(nodes[0]);
    nodes = screen.getAllByTestId("compare-node");
    expect(nodes).toHaveLength(3);
  });

  it("image mode: two image paths call diff_images and render the image-compare pane (CPE-1508)", async () => {
    invoke.mockClear();
    scanTreeLeft = new Error("not a folder");
    scanTreeRight = new Error("not a folder");
    imageDiffResult = {
      width: 10,
      height: 10,
      changedPixels: 3,
      totalPixels: 100,
      percentDifferent: 3,
      bbox: { x: 1, y: 1, width: 2, height: 2 },
      sizeMismatch: false,
      maskPng: [137, 80, 78, 71],
    };

    render(CompareDialog, {});
    await typePaths("/left/a.png", "/right/b.png");
    await fireEvent.click(screen.getByTestId("compare-btn"));

    await waitFor(() => expect(screen.getByTestId("image-compare")).toBeTruthy());
    expect(invoke).toHaveBeenCalledWith("diff_images", { a: "/left/a.png", b: "/right/b.png" });
    // Neither the text nor the byte fallback should run for an image pair.
    expect(invoke).not.toHaveBeenCalledWith("read_file_text", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("read_file_range", expect.anything());
    expect(screen.getByTestId("ic-subview-side-by-side")).toBeTruthy();
    expect(screen.queryByTestId("ic-size-mismatch")).toBeNull();
  });

  it("image mode: shows the size-mismatch note when the backend reports differing dimensions", async () => {
    invoke.mockClear();
    scanTreeLeft = new Error("not a folder");
    scanTreeRight = new Error("not a folder");
    imageDiffResult = {
      width: 20,
      height: 10,
      changedPixels: 50,
      totalPixels: 200,
      percentDifferent: 25,
      bbox: null,
      sizeMismatch: true,
      maskPng: [1, 2, 3],
    };

    render(CompareDialog, {});
    await typePaths("/left/a.jpg", "/right/b.jpg");
    await fireEvent.click(screen.getByTestId("compare-btn"));

    await waitFor(() => expect(screen.getByTestId("ic-size-mismatch")).toBeTruthy());
  });

  it("file mode (text): falls back to a line diff when scan_tree fails but both files decode as UTF-8", async () => {
    invoke.mockClear();
    scanTreeLeft = new Error("not a folder");
    scanTreeRight = new Error("not a folder");
    textLeft = "line1\nline2\nline3";
    textRight = "line1\nlineX\nline3";

    render(CompareDialog, {});
    await typePaths();
    await fireEvent.click(screen.getByTestId("compare-btn"));

    await waitFor(() => expect(screen.getByTestId("text-compare")).toBeTruthy());
    expect(invoke).toHaveBeenCalledWith("read_file_text", { path: "/left", maxBytes: 4 * 1024 * 1024 });
    expect(invoke).toHaveBeenCalledWith("read_file_text", { path: "/right", maxBytes: 4 * 1024 * 1024 });
    // read_file_range must NOT be reached — the text path takes precedence once both decode.
    expect(invoke).not.toHaveBeenCalledWith("read_file_range", expect.anything());

    expect(screen.getByTestId("text-summary").textContent).toBe("+1 added · −1 removed");
    expect(screen.queryByTestId("text-equal")).toBeNull();
  });

  it("file mode (text, equal): renders the identical state when both files' lines match", async () => {
    invoke.mockClear();
    scanTreeLeft = new Error("not a folder");
    scanTreeRight = new Error("not a folder");
    textLeft = "same\ncontent";
    textRight = "same\ncontent";

    render(CompareDialog, {});
    await typePaths();
    await fireEvent.click(screen.getByTestId("compare-btn"));

    await waitFor(() => expect(screen.getByTestId("text-equal")).toBeTruthy());
    expect(screen.getByTestId("text-equal").textContent).toBe("✓ Files are identical.");
  });

  it("file mode (byte): falls back to a byte compare when the files aren't valid UTF-8", async () => {
    invoke.mockClear();
    scanTreeLeft = new Error("not a folder");
    scanTreeRight = new Error("not a folder");
    textLeft = new Error("invalid utf-8");
    textRight = new Error("invalid utf-8");
    rangeLeft = [1, 2, 3, 4];
    rangeRight = [1, 2, 9, 4];

    render(CompareDialog, {});
    await typePaths();
    await fireEvent.click(screen.getByTestId("compare-btn"));

    await waitFor(() => expect(screen.getByTestId("file-compare")).toBeTruthy());
    expect(invoke).toHaveBeenCalledWith("read_file_range", { path: "/left", offset: 0, len: 4 * 1024 * 1024 });
    expect(invoke).toHaveBeenCalledWith("read_file_range", { path: "/right", offset: 0, len: 4 * 1024 * 1024 });

    expect(screen.queryByTestId("file-equal")).toBeNull();
    expect(screen.getByText(/at offset 0x2 \(2\)/)).toBeTruthy();
    expect(screen.getByTestId("file-ranges").textContent).toBe("1");
    expect(screen.getByText("match")).toBeTruthy(); // lengths match — only content differs
  });

  it("file mode (byte, equal): renders the byte-identical state", async () => {
    invoke.mockClear();
    scanTreeLeft = new Error("not a folder");
    scanTreeRight = new Error("not a folder");
    textLeft = new Error("invalid utf-8");
    textRight = new Error("invalid utf-8");
    rangeLeft = [1, 2, 3];
    rangeRight = [1, 2, 3];

    render(CompareDialog, {});
    await typePaths();
    await fireEvent.click(screen.getByTestId("compare-btn"));

    await waitFor(() => expect(screen.getByTestId("file-equal")).toBeTruthy());
    expect(screen.getByTestId("file-equal").textContent).toBe("✓ Files are byte-for-byte identical.");
  });

  it("error state: renders the folder-scan error when neither the folder nor the file fallback pans out", async () => {
    invoke.mockClear();
    scanTreeLeft = new Error("not a folder or file too large");
    scanTreeRight = new Error("not a folder or file too large");
    textLeft = new Error("invalid utf-8");
    textRight = new Error("invalid utf-8");
    rangeLeft = new Error("read failed");
    rangeRight = new Error("read failed");

    render(CompareDialog, {});
    await typePaths();
    await fireEvent.click(screen.getByTestId("compare-btn"));

    await waitFor(() =>
      expect(screen.getByText(/not a folder or file too large/)).toBeTruthy(),
    );
    // The reported error is the original scan_tree failure, not the file-fallback errors.
    expect(screen.getByText("Error: not a folder or file too large")).toBeTruthy();
    // No folder summary is shown once compare() lands in the error branch (compared stays false).
    expect(screen.queryByTestId("compare-summary")).toBeNull();
  });

  it("seeds the path inputs from initialLeft/initialRight and dispatches cancel on Close", async () => {
    invoke.mockClear();
    const onCancel = vi.fn();
    const { component } = render(CompareDialog, { initialLeft: "/seed-left", initialRight: "/seed-right" });
    component.$on("cancel", onCancel);

    expect((screen.getByLabelText("Left path") as HTMLInputElement).value).toBe("/seed-left");
    expect((screen.getByLabelText("Right path") as HTMLInputElement).value).toBe("/seed-right");
    // Nothing is compared until the user presses the button.
    expect(invoke).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByText("Close"));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });
});
