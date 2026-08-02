/**
 * Component tests for the file-content search dialog (CPE-1263, epic CPE-976) — the UI over the local
 * content index wired by CPE-1262 (`content_search` / `content_index_build`).
 *
 * Mirrors the repo's established component-test mocking (`InstantSearch.test.ts` /
 * `ContentSearchDialog.test.ts`): mock `@tauri-apps/api/core`'s `invoke` + `Channel`, since both the
 * typed `commands.*` client (`../bindings.gen`, used for `content_search`) and the raw
 * `rawInvoke`/`createChannel` streaming seam (`../invoke`, used for `content_index_build`) ultimately flow
 * through that module. `content_search` calls are queued as manually-resolvable deferreds so the
 * supersede test can control arrival ORDER independently of call order.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import { tick } from "svelte";
import ContentIndexSearchDialog from "./ContentIndexSearchDialog.svelte";
import type { ContentHit, ContentSearchOutcome } from "../bindings.gen";

interface Deferred {
  args: any;
  resolve: (v: unknown) => void;
  reject: (e: unknown) => void;
}

let searchCalls: Deferred[] = [];
let buildCalls: Deferred[] = [];

const hit = (path: string, score: number, snippet = "some matching text"): ContentHit => ({ path, score, snippet });
const outcome = (hits: ContentHit[], index_exists = true): ContentSearchOutcome => ({ hits, index_exists });

const invoke = vi.fn((cmd: string, _args?: any) => {
  if (cmd === "content_search") return new Promise((resolve, reject) => searchCalls.push({ args: _args, resolve, reject }));
  if (cmd === "content_index_build") return new Promise((resolve, reject) => buildCalls.push({ args: _args, resolve, reject }));
  return Promise.reject(new Error(`unexpected command: ${cmd}`));
});

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

/** Flush the microtask hops a manually-resolved deferred needs before Svelte's DOM-update cycle. */
async function settle() {
  for (let i = 0; i < 5; i++) await Promise.resolve();
  await tick();
}

const input = () => screen.getByPlaceholderText("Search what's inside your files…") as HTMLInputElement;
const hitPaths = () => Array.from(document.querySelectorAll(".hit")).map((el) => el.getAttribute("title"));

/** Resolve the opening probe (`content_search(root, "", 0)`) with `indexExists`, and settle. */
async function settleProbe(indexExists = true) {
  await settle(); // the onMount probe call is queued
  expect(searchCalls).toHaveLength(1);
  searchCalls[0].resolve(outcome([], indexExists));
  await settle();
}

beforeEach(() => {
  vi.useFakeTimers();
  invoke.mockClear();
  searchCalls = [];
  buildCalls = [];
  Element.prototype.scrollIntoView = vi.fn();
});
afterEach(() => {
  vi.useRealTimers();
});

describe("ContentIndexSearchDialog — needs-build state (CPE-1263)", () => {
  it("shows a 'build the index' prompt, not a raw error, when index_exists is false", async () => {
    render(ContentIndexSearchDialog, { root: "Z:\\repos\\cpe" });
    await settleProbe(false);
    expect(screen.getByText("No content index yet")).toBeTruthy();
    expect(screen.getByText("Build content index")).toBeTruthy();
    // The query input is disabled until an index exists.
    expect(input().disabled).toBe(true);
  });

  it("does not call content_search again while typing before an index exists", async () => {
    render(ContentIndexSearchDialog, { root: "Z:\\repos\\cpe" });
    await settleProbe(false);
    searchCalls = [];
    await fireEvent.input(input(), { target: { value: "abc" } });
    await vi.advanceTimersByTimeAsync(300);
    expect(searchCalls).toHaveLength(0);
  });

  it("building streams progress, doesn't block the UI, and reveals the search once done", async () => {
    render(ContentIndexSearchDialog, { root: "Z:\\repos\\cpe" });
    await settleProbe(false);

    await fireEvent.click(screen.getByText("Build content index"));
    expect(buildCalls).toHaveLength(1);
    expect(buildCalls[0].args).toEqual(expect.objectContaining({ root: "Z:\\repos\\cpe" }));

    buildCalls[0].args.onProgress.onmessage({ files_indexed: 12, files_skipped: 1, current_path: "Z:\\repos\\cpe\\a.txt" });
    await settle();
    expect(screen.getByText(/12 files indexed/)).toBeTruthy();

    buildCalls[0].resolve({ files_indexed: 40, files_skipped: 2, truncated: false });
    await settle();
    expect(screen.queryByText("No content index yet")).toBeNull();
    expect(input().disabled).toBe(false);
  });

  it("surfaces a build error instead of silently staying broken", async () => {
    render(ContentIndexSearchDialog, { root: "Z:\\repos\\cpe" });
    await settleProbe(false);
    await fireEvent.click(screen.getByText("Build content index"));
    buildCalls[0].reject("permission denied");
    await settle();
    expect(screen.getByText("permission denied")).toBeTruthy();
  });
});

describe("ContentIndexSearchDialog — query → ranked results (CPE-1263)", () => {
  it("renders ranked hits with filename, relative path, score, and snippet", async () => {
    render(ContentIndexSearchDialog, { root: "Z:\\repos\\cpe" });
    await settleProbe(true);

    await fireEvent.input(input(), { target: { value: "quick fox" } });
    await vi.advanceTimersByTimeAsync(300);
    expect(searchCalls).toHaveLength(2); // probe + this search
    expect(searchCalls[1].args).toEqual(expect.objectContaining({ root: "Z:\\repos\\cpe", query: "quick fox", k: 25 }));

    searchCalls[1].resolve(outcome([hit("Z:\\repos\\cpe\\src\\fox.txt", 0.82, "the quick fox jumps")]));
    await settle();

    expect(hitPaths()).toEqual(["Z:\\repos\\cpe\\src\\fox.txt"]);
    expect(screen.getByText("fox.txt")).toBeTruthy();
    expect(screen.getByText("src/fox.txt")).toBeTruthy();
    expect(screen.getByText("82%")).toBeTruthy();
    // The snippet is split across a <mark> (highlighted match) + plain text nodes, so a string/regex
    // matcher on getByText can't reliably match it (same caveat InstantSearch.test.ts documents for
    // highlighted names) — assert on the rendered container's full text instead.
    expect(document.querySelector(".snippet")?.textContent).toBe("the quick fox jumps");
  });

  it("shows a clean 'no matches' state when the index exists but nothing scores", async () => {
    render(ContentIndexSearchDialog, { root: "Z:\\repos\\cpe" });
    await settleProbe(true);

    await fireEvent.input(input(), { target: { value: "nonexistent" } });
    await vi.advanceTimersByTimeAsync(300);
    searchCalls[1].resolve(outcome([]));
    await settle();

    expect(screen.getByText("No matches in this folder's content index.")).toBeTruthy();
  });

  it("clicking a result dispatches navigate with the file path and closes", async () => {
    const { component } = render(ContentIndexSearchDialog, { root: "Z:\\repos\\cpe" });
    await settleProbe(true);
    const onNavigate = vi.fn();
    const onClose = vi.fn();
    component.$on("navigate", (e: CustomEvent<string>) => onNavigate(e.detail));
    component.$on("close", onClose);

    await fireEvent.input(input(), { target: { value: "fox" } });
    await vi.advanceTimersByTimeAsync(300);
    searchCalls[1].resolve(outcome([hit("Z:\\repos\\cpe\\src\\fox.txt", 0.5)]));
    await settle();

    // Click the whole result row (its title carries the full path — unambiguous, unlike the
    // filename/relative-path spans which can coincide for a file directly under root).
    await fireEvent.click(screen.getByTitle("Z:\\repos\\cpe\\src\\fox.txt"));
    expect(onNavigate).toHaveBeenCalledWith("Z:\\repos\\cpe\\src\\fox.txt");
    expect(onClose).toHaveBeenCalled();
  });

  it("Escape closes without a navigate", async () => {
    const { component } = render(ContentIndexSearchDialog, { root: "Z:\\repos\\cpe" });
    await settleProbe(true);
    const onNavigate = vi.fn();
    const onClose = vi.fn();
    component.$on("navigate", onNavigate);
    component.$on("close", onClose);

    await fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
    expect(onNavigate).not.toHaveBeenCalled();
  });
});

describe("ContentIndexSearchDialog — debounce + generation-token supersede (CPE-1263)", () => {
  it("waits out the debounce window and searches once for the final typed query", async () => {
    render(ContentIndexSearchDialog, { root: "Z:\\repos\\cpe" });
    await settleProbe(true);

    await fireEvent.input(input(), { target: { value: "q" } });
    await fireEvent.input(input(), { target: { value: "qu" } });
    await fireEvent.input(input(), { target: { value: "quick" } });
    expect(searchCalls).toHaveLength(1); // still just the opening probe — inside the debounce window

    await vi.advanceTimersByTimeAsync(250);
    expect(searchCalls).toHaveLength(2);
    expect(searchCalls[1].args).toEqual(expect.objectContaining({ query: "quick" }));
  });

  it("drops a stale search's late-arriving result once a newer query has superseded it", async () => {
    render(ContentIndexSearchDialog, { root: "Z:\\repos\\cpe" });
    await settleProbe(true);

    await fireEvent.input(input(), { target: { value: "abc" } });
    await vi.advanceTimersByTimeAsync(250);
    expect(searchCalls).toHaveLength(2);
    const stale = searchCalls[1];

    // A newer keystroke supersedes it before the stale call resolves.
    await fireEvent.input(input(), { target: { value: "abcdef" } });
    await vi.advanceTimersByTimeAsync(250);
    expect(searchCalls).toHaveLength(3);
    const fresh = searchCalls[2];

    // The STALE call resolving late must be dropped, not rendered.
    stale.resolve(outcome([hit("Z:\\stale.txt", 0.9)]));
    await settle();
    expect(hitPaths()).toEqual([]);

    fresh.resolve(outcome([hit("Z:\\fresh.txt", 0.7)]));
    await settle();
    expect(hitPaths()).toEqual(["Z:\\fresh.txt"]);
  });

  it("clearing the query back to empty cancels the pending search", async () => {
    render(ContentIndexSearchDialog, { root: "Z:\\repos\\cpe" });
    await settleProbe(true);
    await fireEvent.input(input(), { target: { value: "abc" } });
    await fireEvent.input(input(), { target: { value: "" } });
    await vi.advanceTimersByTimeAsync(250);
    expect(searchCalls).toHaveLength(1); // only the opening probe — no search fired for the cleared query
  });
});
