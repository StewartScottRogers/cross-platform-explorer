/**
 * WorkbenchView collapse (CPE-568) — clicking a file header folds its hunks away on big diffs.
 * The diff parsing/stats are pure-tested in diff.test.ts; this covers the collapse interaction.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";

const invokeMock = vi.fn(async (_cmd: string, _args?: unknown): Promise<unknown> => ({}));
vi.mock("../invoke", () => ({ invoke: (...a: unknown[]) => (invokeMock as (...x: unknown[]) => unknown)(...a) }));

import WorkbenchView from "./WorkbenchView.svelte";

const DIFF = `diff --git a/src/app.ts b/src/app.ts
--- a/src/app.ts
+++ b/src/app.ts
@@ -1,2 +1,2 @@
 const x = 1;
-const y = 2;
+const y = 3;`;

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) =>
    cmd === "workbench_diff" ? { is_repo: true, branch: "main", diff: DIFF } : {});
});

describe("WorkbenchView collapse (CPE-568)", () => {
  it("collapses a file's hunks on header click, and expands again", async () => {
    render(WorkbenchView, { root: "/repo" });
    await waitFor(() => expect(screen.getByText("const y = 3;")).toBeTruthy());

    const head = screen.getByText("src/app.ts").closest(".file-head") as HTMLElement;
    await fireEvent.click(head);
    expect(screen.queryByText("const y = 3;")).toBeNull(); // hunks folded away

    await fireEvent.click(head);
    expect(await screen.findByText("const y = 3;")).toBeTruthy(); // expanded again
  });
});
