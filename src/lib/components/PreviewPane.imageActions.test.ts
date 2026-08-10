/**
 * CPE-1576 (epic CPE-1568 slice 3) — single-file image actions (Rotate left/right, Convert…, Copy
 * image) on the preview action bar, declared on the `image`/`decoded-image`/`raw-image`/`heic`
 * providers (`preview/provider.ts`) and rendered by `PreviewPane`'s generic action bar (CPE-1570).
 * Rotate/Convert reuse the EXISTING CPE-1093 batch-media backend (`batch_media_plan` +
 * `batch_media_execute_stream`) for a one-file job — mirrors `BatchMediaDialog.test.ts`'s established
 * mock of `@tauri-apps/api/core`'s `invoke`/`Channel`, since both the raw `invoke`/`createChannel`
 * streaming seam (`../invoke`) the action calls through ultimately flow through that module. Copy image
 * needs no backend: it fetches the pane's own already-resolved image URL and writes it to the clipboard
 * via the Clipboard API's image `write`, which jsdom doesn't implement — `fetch`/`ClipboardItem` are
 * stubbed per-test.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import PreviewPane from "./PreviewPane.svelte";
import type { DirEntry } from "../types";

interface Deferred {
  args: any;
  resolve: (v: unknown) => void;
  reject: (e: unknown) => void;
}

let planCalls: Deferred[] = [];
let execCalls: Deferred[] = [];

const invoke = vi.fn((cmd: string, args?: any) => {
  if (cmd === "batch_media_plan") {
    return new Promise((resolve, reject) => planCalls.push({ args, resolve, reject }));
  }
  if (cmd === "batch_media_execute_stream") {
    return new Promise((resolve, reject) => execCalls.push({ args, resolve, reject }));
  }
  return Promise.reject(new Error(`unexpected command: ${cmd}`));
});

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

const entry = (over: Partial<DirEntry>): DirEntry => ({
  name: "x",
  path: "/x",
  is_dir: false,
  size: 1,
  modified: 0,
  extension: "",
  hidden: false,
  is_symlink: false,
  ...over,
});

/** Resolve a queued plan/exec call and let the microtask chain settle. */
async function resolvePlan(planned: unknown) {
  const call = planCalls.shift();
  if (!call) throw new Error("no pending batch_media_plan call");
  call.resolve(planned);
  for (let i = 0; i < 5; i++) await Promise.resolve();
}
async function resolveExec(report: unknown) {
  const call = execCalls.shift();
  if (!call) throw new Error("no pending batch_media_execute_stream call");
  call.resolve(report);
  for (let i = 0; i < 5; i++) await Promise.resolve();
}

function actionBarLabels(container: Element): (string | undefined)[] {
  const bar = container.querySelector('[data-testid="preview-action-bar"]')!;
  return [...bar.querySelectorAll("button")].map((b) => b.textContent?.trim());
}

beforeEach(() => {
  invoke.mockClear();
  planCalls = [];
  execCalls = [];
});

describe("PreviewPane — single-file image actions (CPE-1576)", () => {
  it("renders Rotate left/right, Convert…, Copy image for a native encoder-writable image", async () => {
    const { container } = render(PreviewPane, {
      entry: entry({ name: "a.jpg", path: "/photos/a.jpg", extension: "jpg" }),
      assetUrl: (p: string) => `asset://${p}`,
    });
    await waitFor(() => expect(container.querySelector('[data-testid="preview-action-bar"]')).toBeTruthy());
    expect(actionBarLabels(container)).toEqual(["Rotate left", "Rotate right", "Convert…", "Copy image"]);
  });

  it("gates Rotate/Convert off for a format the batch-media encoder can't write (svg), keeps Copy image", async () => {
    const { container } = render(PreviewPane, {
      entry: entry({ name: "a.svg", path: "/photos/a.svg", extension: "svg" }),
      assetUrl: (p: string) => `asset://${p}`,
    });
    await waitFor(() => expect(container.querySelector('[data-testid="preview-action-bar"]')).toBeTruthy());
    expect(actionBarLabels(container)).toEqual(["Copy image"]);
  });

  it("gates Copy image off until a decoded-image URL has actually resolved", async () => {
    let resolveLoad!: (v: string) => void;
    const loadImageData = vi.fn(() => new Promise<string>((resolve) => { resolveLoad = resolve; }));
    const { container } = render(PreviewPane, {
      entry: entry({ name: "a.tif", path: "/photos/a.tif", extension: "tif" }),
      loadImageData,
    });
    // Rotate/Convert ARE encoder-writable for tif — they show immediately; Copy image waits on the load.
    await waitFor(() => expect(container.querySelector('[data-testid="preview-action-bar"]')).toBeTruthy());
    expect(actionBarLabels(container)).toEqual(["Rotate left", "Rotate right", "Convert…"]);

    resolveLoad("data:image/png;base64,AAAA");
    await waitFor(() => expect(actionBarLabels(container)).toContain("Copy image"));
  });

  it("Rotate left runs a single-file, non-destructive batch-media job via the existing CPE-1093 backend", async () => {
    render(PreviewPane, {
      entry: entry({ name: "a.jpg", path: "/photos/a.jpg", extension: "jpg" }),
      assetUrl: (p: string) => `asset://${p}`,
    });

    const btn = await waitFor(() => screen.getByRole("button", { name: /Rotate left/ }));
    await fireEvent.click(btn);

    await waitFor(() => expect(planCalls.length).toBe(1));
    expect(planCalls[0].args).toEqual({
      job: { ops: [{ op: "rotate", degrees: 270 }], non_destructive: true },
      inputs: ["/photos/a.jpg"],
    });

    await resolvePlan([{ input: "/photos/a.jpg", output: "/photos/a-rot270.jpg", summary: "rotate 270°" }]);
    await waitFor(() => expect(execCalls.length).toBe(1));
    expect(execCalls[0].args.job).toEqual({ ops: [{ op: "rotate", degrees: 270 }], non_destructive: true });
    expect(execCalls[0].args.items).toEqual([
      { input: "/photos/a.jpg", output: "/photos/a-rot270.jpg", summary: "rotate 270°" },
    ]);

    await resolveExec({ written: 1, skipped: [] });
    await waitFor(() => expect(screen.getByTestId("preview-action-message").textContent).toContain("Rotated image saved"));
  });

  it("Rotate right uses degrees 90 (the mirror of Rotate left's 270)", async () => {
    render(PreviewPane, {
      entry: entry({ name: "a.jpg", path: "/photos/a.jpg", extension: "jpg" }),
      assetUrl: (p: string) => `asset://${p}`,
    });

    const btn = await waitFor(() => screen.getByRole("button", { name: /Rotate right/ }));
    await fireEvent.click(btn);

    await waitFor(() => expect(planCalls.length).toBe(1));
    expect(planCalls[0].args.job.ops).toEqual([{ op: "rotate", degrees: 90 }]);
  });

  it("surfaces a skipped result as a failure message instead of a false success", async () => {
    render(PreviewPane, {
      entry: entry({ name: "a.jpg", path: "/photos/a.jpg", extension: "jpg" }),
      assetUrl: (p: string) => `asset://${p}`,
    });

    const btn = await waitFor(() => screen.getByRole("button", { name: /Rotate left/ }));
    await fireEvent.click(btn);
    await waitFor(() => expect(planCalls.length).toBe(1));
    await resolvePlan([{ input: "/photos/a.jpg", output: "/photos/a-rot270.jpg", summary: "rotate 270°" }]);
    await waitFor(() => expect(execCalls.length).toBe(1));
    await resolveExec({ written: 0, skipped: [["/photos/a.jpg", "not a valid image"]] });

    await waitFor(() =>
      expect(screen.getByTestId("preview-action-message").textContent).toContain("not a valid image"),
    );
  });

  it("Convert… prompts for a target format, then reuses the same backend with a convert op", async () => {
    const prompt = vi.spyOn(window, "prompt").mockReturnValue("webp");
    render(PreviewPane, {
      entry: entry({ name: "a.jpg", path: "/photos/a.jpg", extension: "jpg" }),
      assetUrl: (p: string) => `asset://${p}`,
    });

    const btn = await waitFor(() => screen.getByRole("button", { name: /Convert…/ }));
    await fireEvent.click(btn);

    expect(prompt).toHaveBeenCalled();
    await waitFor(() => expect(planCalls.length).toBe(1));
    expect(planCalls[0].args).toEqual({
      job: { ops: [{ op: "convert", to_ext: "webp" }], non_destructive: true },
      inputs: ["/photos/a.jpg"],
    });
    prompt.mockRestore();
  });

  it("Convert… does nothing when the prompt is cancelled", async () => {
    const prompt = vi.spyOn(window, "prompt").mockReturnValue(null);
    render(PreviewPane, {
      entry: entry({ name: "a.jpg", path: "/photos/a.jpg", extension: "jpg" }),
      assetUrl: (p: string) => `asset://${p}`,
    });

    const btn = await waitFor(() => screen.getByRole("button", { name: /Convert…/ }));
    await fireEvent.click(btn);
    await new Promise((r) => setTimeout(r, 0));

    expect(invoke).not.toHaveBeenCalled();
    prompt.mockRestore();
  });

  it("Copy image fetches the resolved preview URL and writes it to the clipboard as image bytes", async () => {
    const blob = { type: "image/png" } as Blob;
    const fetchMock = vi.fn().mockResolvedValue({ blob: () => Promise.resolve(blob) });
    vi.stubGlobal("fetch", fetchMock);
    class FakeClipboardItem {
      constructor(public items: Record<string, Blob>) {}
    }
    vi.stubGlobal("ClipboardItem", FakeClipboardItem);
    const write = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { write }, configurable: true });

    render(PreviewPane, {
      entry: entry({ name: "a.jpg", path: "/photos/a.jpg", extension: "jpg" }),
      assetUrl: (p: string) => `asset://${p}`,
    });

    const btn = await waitFor(() => screen.getByRole("button", { name: /Copy image/ }));
    await fireEvent.click(btn);

    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith("asset:///photos/a.jpg"));
    await waitFor(() => expect(write).toHaveBeenCalled());
    const [item] = write.mock.calls[0][0] as [FakeClipboardItem];
    expect(item.items).toEqual({ "image/png": blob });

    vi.unstubAllGlobals();
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});
