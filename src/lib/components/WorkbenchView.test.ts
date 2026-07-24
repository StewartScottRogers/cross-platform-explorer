/**
 * WorkbenchView collapse (CPE-568) — clicking a file header folds its hunks away on big diffs.
 * The diff parsing/stats are pure-tested in diff.test.ts; this covers the collapse interaction.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";

const invokeMock = vi.fn(async (_cmd: string, _args?: unknown): Promise<unknown> => ({}));
// The typed `commands.*` client (bindings.gen) routes through this same `invoke`, so mocking it here also
// drives the typed workbench_diff call. `unwrap` is the throw-on-error helper the Result-returning typed
// calls use — provide the real behaviour.
vi.mock("../invoke", () => ({
  invoke: (...a: unknown[]) => (invokeMock as (...x: unknown[]) => unknown)(...a),
  unwrap: <T>(r: { status: string; data?: T; error?: unknown }): T => {
    if (r.status === "ok") return r.data as T;
    throw r.error instanceof Error ? r.error : new Error(String(r.error));
  },
}));

import WorkbenchView from "./WorkbenchView.svelte";

const DIFF = `diff --git a/src/app.ts b/src/app.ts
--- a/src/app.ts
+++ b/src/app.ts
@@ -1,2 +1,2 @@
 const x = 1;
-const y = 2;
+const y = 3;`;

beforeEach(() => {
  try { localStorage.clear(); } catch { /* ignore */ } // WorkbenchView persists the browser URL (CPE-575)
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) =>
    cmd === "workbench_diff" ? { is_repo: true, branch: "main", diff: DIFF } : {});
});

const DIFF2 = `diff --git a/a.ts b/a.ts
--- a/a.ts
+++ b/a.ts
@@ -1 +1 @@
-old a
+new a
diff --git a/b.ts b/b.ts
--- a/b.ts
+++ b/b.ts
@@ -1 +1 @@
-old b
+new b`;

// A modified line's text is split across <span class="chg"> nodes (CPE-570 inline highlight), so match
// on the .code element's full textContent.
const codeLine = (text: string) => (_: string, el: Element | null) =>
  (el?.classList?.contains("code") ?? false) && el!.textContent === text;

describe("WorkbenchView collapse (CPE-568)", () => {
  it("collapses a file's hunks on header click, and expands again", async () => {
    render(WorkbenchView, { root: "/repo" });
    await waitFor(() => expect(screen.getByText(codeLine("const y = 3;"))).toBeTruthy());

    const head = screen.getByText("src/app.ts").closest(".file-head") as HTMLElement;
    await fireEvent.click(head);
    expect(screen.queryByText(codeLine("const y = 3;"))).toBeNull(); // hunks folded away

    await fireEvent.click(head);
    expect(await screen.findByText(codeLine("const y = 3;"))).toBeTruthy(); // expanded again
  });

  it("collapse all / expand all fold every file (CPE-569)", async () => {
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "workbench_diff" ? { is_repo: true, branch: "main", diff: DIFF2 } : {});
    render(WorkbenchView, { root: "/repo" });
    await waitFor(() => expect(screen.getByText(codeLine("new a"))).toBeTruthy());

    await fireEvent.click(screen.getByText("Collapse all"));
    expect(screen.queryByText(codeLine("new a"))).toBeNull();
    expect(screen.queryByText(codeLine("new b"))).toBeNull();

    await fireEvent.click(screen.getByText("Expand all"));
    expect(await screen.findByText(codeLine("new a"))).toBeTruthy();
    expect(screen.getByText(codeLine("new b"))).toBeTruthy();
  });

  it("highlights the changed span within a modified line (CPE-570)", async () => {
    const { container } = render(WorkbenchView, { root: "/repo" });
    await waitFor(() => expect(screen.getByText(codeLine("const y = 3;"))).toBeTruthy());
    // the add line's "3" and the del line's "2" are wrapped in .chg highlights
    const chg = [...container.querySelectorAll(".line.add .chg")].map((e) => e.textContent);
    expect(chg).toContain("3");
  });

  it("remembers the browser URL across opens (CPE-575)", async () => {
    localStorage.setItem("cpe.workbenchUrl", "localhost:5173");
    render(WorkbenchView, { root: "/repo" });
    const input = screen.getByPlaceholderText(/open the running app in a browser/i) as HTMLInputElement;
    expect(input.value).toBe("localhost:5173");
  });

  it("copies a file's reconstructed diff to the clipboard (CPE-572)", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    render(WorkbenchView, { root: "/repo" });
    await waitFor(() => expect(screen.getByText("Copy")).toBeTruthy());

    await fireEvent.click(screen.getByText("Copy"));
    expect(writeText).toHaveBeenCalledTimes(1);
    expect(writeText.mock.calls[0][0]).toContain("diff --git a/src/app.ts b/src/app.ts");
    expect(writeText.mock.calls[0][0]).toContain("+const y = 3;");
  });
});
