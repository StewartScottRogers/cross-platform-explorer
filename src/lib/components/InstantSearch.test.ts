/**
 * Component tests for the Instant Search overlay (CPE-1139, epic CPE-703) — the keyboard-first global
 * search over the resident cross-volume index (`index_search`/`index_build`/`index_status`, CPE-1137).
 *
 * Mirrors the repo's established component-test mocking (`BatchMediaDialog.test.ts` /
 * `ContentSearchDialog.test.ts`): mock `@tauri-apps/api/core`'s `invoke` + `Channel`, since both the
 * typed `commands.*` client (`../bindings.gen`) and the raw `rawInvoke`/`createChannel` streaming seam
 * (`../invoke`) ultimately flow through that module. `index_search`/`index_build` calls are queued as
 * manually-resolvable deferreds so the supersede test can control arrival ORDER independently of call
 * order — the whole point of that guard.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import { tick } from "svelte";
import InstantSearch from "./InstantSearch.svelte";

interface Deferred {
  args: any;
  resolve: (v: unknown) => void;
  reject: (e: unknown) => void;
}

let searchCalls: Deferred[] = [];
let buildCalls: Deferred[] = [];
/** `index_status`'s canned response — a non-empty array means a resident volume (index "on"). */
let statusResponse: unknown[] = [];

const invoke = vi.fn((cmd: string, args?: any) => {
  if (cmd === "index_status") return Promise.resolve(statusResponse);
  if (cmd === "home_dir") return Promise.resolve("C:\\Users\\test");
  if (cmd === "index_search") return new Promise((resolve, reject) => searchCalls.push({ args, resolve, reject }));
  if (cmd === "index_build") return new Promise((resolve, reject) => buildCalls.push({ args, resolve, reject }));
  return Promise.reject(new Error(`unexpected command: ${cmd}`));
});

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

/** Flush the microtask hops a manually-resolved deferred needs before Svelte's DOM-update cycle. Fake
 *  timers only fake timer callbacks, not promise microtasks, so this works regardless of `useFakeTimers`. */
async function settle() {
  for (let i = 0; i < 5; i++) await Promise.resolve();
  await tick();
}

const input = () => screen.getByPlaceholderText("Search everywhere…") as HTMLInputElement;
/** Result rows keyed by `title` (the full path) — highlighted names split across a `<mark>` + text
 *  node, so `getByText` can't reliably match them; `title` is a single attribute string instead. */
const rowPaths = () => Array.from(document.querySelectorAll(".is-row")).map((el) => el.getAttribute("title"));

beforeEach(() => {
  vi.useFakeTimers();
  invoke.mockClear();
  searchCalls = [];
  buildCalls = [];
  statusResponse = [{ volume_id: 1, entries: 100, truncated: false }]; // index resident by default
  Element.prototype.scrollIntoView = vi.fn(); // jsdom doesn't implement it (App.test.ts precedent)
});
afterEach(() => {
  vi.useRealTimers();
});

describe("InstantSearch debounce (CPE-1139)", () => {
  it("waits out the debounce window and searches once for the final typed query", async () => {
    render(InstantSearch, { root: "Z:\\repos\\cpe" });
    await settle(); // resolve the onMount index_status check

    await fireEvent.input(input(), { target: { value: "re" } });
    await fireEvent.input(input(), { target: { value: "rep" } });
    await fireEvent.input(input(), { target: { value: "repo" } });
    expect(searchCalls).toHaveLength(0); // still inside the debounce window

    await vi.advanceTimersByTimeAsync(150);
    expect(searchCalls).toHaveLength(1);
    expect(searchCalls[0].args).toEqual(expect.objectContaining({ query: "repo", limit: 200 }));
  });

  it("clearing the query back to empty cancels the pending search", async () => {
    render(InstantSearch, { root: "Z:\\repos\\cpe" });
    await settle();
    await fireEvent.input(input(), { target: { value: "abc" } });
    await fireEvent.input(input(), { target: { value: "" } });
    await vi.advanceTimersByTimeAsync(150);
    expect(searchCalls).toHaveLength(0);
  });
});

describe("InstantSearch supersede — stale streams are dropped (CPE-1139)", () => {
  it("drops a stale search's late-arriving batch once a newer query has superseded it", async () => {
    render(InstantSearch, { root: "Z:\\repos\\cpe" });
    await settle();

    await fireEvent.input(input(), { target: { value: "abc" } });
    await vi.advanceTimersByTimeAsync(150);
    expect(searchCalls).toHaveLength(1);
    const stale = searchCalls[0];
    stale.args.onHit.onmessage([{ path: "Z:\\abc-old.txt", name: "abc-old.txt", is_dir: false, score: 1 }]);
    await settle();
    expect(rowPaths()).toEqual(["Z:\\abc-old.txt"]);

    // A newer keystroke supersedes it before the stale call resolves.
    await fireEvent.input(input(), { target: { value: "abcdef" } });
    await vi.advanceTimersByTimeAsync(150);
    expect(searchCalls).toHaveLength(2);
    const fresh = searchCalls[1];
    // The fresh search resets the list immediately, before any batch has arrived for it.
    expect(rowPaths()).toEqual([]);

    // The STALE call's late batch must be dropped, not appended onto the fresh (reset) list.
    stale.args.onHit.onmessage([{ path: "Z:\\abc-late.txt", name: "abc-late.txt", is_dir: false, score: 1 }]);
    await settle();
    expect(rowPaths()).toEqual([]);

    fresh.args.onHit.onmessage([{ path: "Z:\\abcdef.txt", name: "abcdef.txt", is_dir: false, score: 1 }]);
    await settle();
    expect(rowPaths()).toEqual(["Z:\\abcdef.txt"]);

    stale.resolve(1);
    fresh.resolve(1);
  });
});

describe("InstantSearch keyboard navigation (CPE-1139)", () => {
  it("ArrowDown moves the active row and Enter reveals it; Escape closes the overlay", async () => {
    const { component } = render(InstantSearch, { root: "Z:\\repos\\cpe" });
    await settle();
    const onNavigate = vi.fn();
    const onClose = vi.fn();
    component.$on("navigate", (e: CustomEvent<string>) => onNavigate(e.detail));
    component.$on("close", onClose);

    await fireEvent.input(input(), { target: { value: "x" } });
    await vi.advanceTimersByTimeAsync(150);
    searchCalls[0].args.onHit.onmessage([
      { path: "Z:\\x1.txt", name: "x1.txt", is_dir: false, score: 2 },
      { path: "Z:\\x2.txt", name: "x2.txt", is_dir: false, score: 1 },
    ]);
    await settle();

    await fireEvent.keyDown(input(), { key: "ArrowDown" }); // active moves from row 0 to row 1
    await fireEvent.keyDown(input(), { key: "Enter" });
    expect(onNavigate).toHaveBeenCalledWith("Z:\\x2.txt");
    expect(onClose).toHaveBeenCalled();
  });

  it("Escape closes without a navigate", async () => {
    const { component } = render(InstantSearch, { root: "Z:\\repos\\cpe" });
    await settle();
    const onNavigate = vi.fn();
    const onClose = vi.fn();
    component.$on("navigate", onNavigate);
    component.$on("close", onClose);

    await fireEvent.keyDown(input(), { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
    expect(onNavigate).not.toHaveBeenCalled();
  });
});

describe("InstantSearch build-index affordance — off-means-off (CPE-1139)", () => {
  it("shows a 'Build index' affordance, not a blank list, when no volume is resident", async () => {
    statusResponse = [];
    render(InstantSearch, { root: "Z:\\repos\\cpe" });
    await settle();
    expect(screen.getByText("Instant search is off")).toBeTruthy();
    expect(screen.getByText("Build index")).toBeTruthy();
  });

  it("does not call index_search while the index is off, even if the user types", async () => {
    statusResponse = [];
    render(InstantSearch, { root: "Z:\\repos\\cpe" });
    await settle();
    await fireEvent.input(input(), { target: { value: "abc" } });
    await vi.advanceTimersByTimeAsync(150);
    expect(searchCalls).toHaveLength(0);
  });

  it("Build index streams progress and reveals the search once built", async () => {
    statusResponse = [];
    render(InstantSearch, { root: "Z:\\repos\\cpe" });
    await settle();

    await fireEvent.click(screen.getByText("Build index"));
    expect(buildCalls).toHaveLength(1);
    // The build targets the whole DRIVE owning the current folder (Z:\), not just the open folder.
    expect(buildCalls[0].args).toEqual(expect.objectContaining({ root: "Z:\\" }));

    buildCalls[0].args.onProgress.onmessage({ dirs_scanned: 12, truncated: false });
    await settle();
    expect(screen.getByText(/12 folders scanned/)).toBeTruthy();

    buildCalls[0].resolve({ dirs_scanned: 40, truncated: false });
    await settle();
    expect(screen.queryByText("Instant search is off")).toBeNull();
  });

  it("surfaces a build error instead of silently staying off", async () => {
    statusResponse = [];
    render(InstantSearch, { root: "Z:\\repos\\cpe" });
    await settle();
    await fireEvent.click(screen.getByText("Build index"));
    buildCalls[0].reject("permission denied");
    await settle();
    expect(screen.getByText("permission denied")).toBeTruthy();
  });
});
