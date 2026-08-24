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
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import TrashView from "./TrashView.svelte";

// Built from a decimal code point, not a literal character — see filename.ts's own doc comment for why.
const RLO = String.fromCharCode(0x202e);

interface StreamCall {
  args: { onEntry: { onmessage: (v: unknown) => void } };
  resolve: (v: unknown) => void;
}
let streamCalls: StreamCall[] = [];
let restoreImpl: ((ids: string[]) => Promise<Array<{ ok: boolean; error: string }>>) | null = null;
const invoke = vi.fn((cmd: string, args?: any) => {
  if (cmd === "list_trash_stream") {
    return new Promise((resolve) => streamCalls.push({ args: args as StreamCall["args"], resolve }));
  }
  if (cmd === "restore_trash_items" && restoreImpl) return restoreImpl(args.ids);
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
  restoreImpl = null;
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

  // CPE-1757 round 2: `f.name` was passed raw into `$t("trash.restoreFailed", { name: f.name, ... })` —
  // an i18n interpolation PARAMETER, not a template-literal or property-access shape, a class of miss
  // round 1's guard never considered. Asserts on the rendered banner text, not the i18n call's arguments.
  it("escapes a bidi override in a restore-failure banner's name (CPE-1757)", async () => {
    restoreImpl = async () => [{ ok: false, error: "in use" }];
    const { container } = render(TrashView, {});
    await Promise.resolve();
    await Promise.resolve();
    streamCalls[0].args.onEntry.onmessage([
      { id: "1", name: `${RLO}gnp.txt`, original_path: `C:\\x\\${RLO}gnp.txt`, size: 10, time_deleted: 0 },
    ]);
    await waitFor(() => expect(container.querySelector(".tv-row")).toBeTruthy());

    await fireEvent.click(container.querySelector(".tv-check input") as HTMLInputElement);
    // CPE-1827: "Restore selected" now lives behind the titlebar's "…" overflow menu — open it first.
    await fireEvent.click(screen.getByTitle("More actions"));
    await fireEvent.click(screen.getByText("Restore selected"));

    await waitFor(() => expect(container.textContent).toContain("Couldn't restore"));
    expect(container.textContent).not.toContain("txt.png");
    expect(container.textContent).toContain("[RLO]gnp.txt");
  });
});
