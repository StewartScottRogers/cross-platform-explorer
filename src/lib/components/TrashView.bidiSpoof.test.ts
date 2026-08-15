/**
 * CPE-1712 review round 2 — coverage regression guard.
 *
 * Round 1 fixed TrashView's visible name span but left its `title`/`aria-label` and the
 * original-location column raw — the row's tooltip and a screen reader would still get the spoofed
 * text even though the row's own text read safely. Mocking strategy mirrors `TrashView.test.ts`'s own
 * (mock `@tauri-apps/api/core`'s `invoke` + `Channel` directly, since `rawInvoke`/`createChannel`
 * flow through that module).
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, waitFor } from "@testing-library/svelte";
import TrashView from "./TrashView.svelte";

// Built from a decimal code point, not a literal character — see filename.ts's own doc comment for why.
const RLO = String.fromCharCode(0x202e);

interface StreamCall {
  args: { onEntry: { onmessage: (v: unknown) => void } };
  resolve: (v: unknown) => void;
}
let streamCalls: StreamCall[] = [];
const invoke = vi.fn((cmd: string, args?: unknown) => {
  if (cmd === "list_trash_stream") {
    return new Promise((resolve) => streamCalls.push({ args: args as StreamCall["args"], resolve }));
  }
  return Promise.reject(new Error("unhandled: " + cmd));
});
vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

beforeEach(() => {
  invoke.mockClear();
  streamCalls = [];
});

describe("TrashView — tooltip, aria-label, AND the original-location column (CPE-1712 round 2 blocker)", () => {
  it("escapes the name's title/aria-label and the original-path column, not just the visible name", async () => {
    const { container } = render(TrashView, {});
    await Promise.resolve();
    await Promise.resolve();
    streamCalls[0].args.onEntry.onmessage([
      { id: "1", name: `${RLO}gnp.txt`, original_path: `C:\\Users\\alice\\${RLO}gnp.txt`, size: 10, time_deleted: 0 },
    ]);
    await waitFor(() => expect(container.querySelector(".tv-row")).toBeTruthy());

    expect(container.textContent).not.toContain("txt.png");
    // Scoped to `.tv-row` (not the bare `.tv-cell.tv-name`/`.tv-cell.tv-path` selector): the header row
    // above the data rows carries the SAME classes on its column-label cells, which have no `title`.
    const row = container.querySelector(".tv-row");
    const nameCell = row?.querySelector(".tv-name");
    expect(nameCell?.getAttribute("title")).toBe("[RLO]gnp.txt");
    const checkbox = row?.querySelector(".tv-check input");
    expect(checkbox?.getAttribute("aria-label")).toBe("[RLO]gnp.txt");
    const pathCell = row?.querySelector(".tv-path");
    expect(pathCell?.textContent).toBe("C:\\Users\\alice\\[RLO]gnp.txt");
    expect(pathCell?.getAttribute("title")).toBe("C:\\Users\\alice\\[RLO]gnp.txt");
  });
});
