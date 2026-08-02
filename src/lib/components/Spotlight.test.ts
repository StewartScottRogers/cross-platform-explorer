/**
 * Component tests for the Spotlight overlay (CPE-1216, epic CPE-704) — the sectioned, highlighted
 * quick-launch overlay over `spotlight_search`/`spotlight_frecent` (CPE-1214). Mirrors the repo's
 * established component-test mocking (`InstantSearch.test.ts`): mock `@tauri-apps/api/core`'s
 * `invoke`/`Channel`, since the typed `commands.*` client and the raw streaming seam both flow
 * through it.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import { tick } from "svelte";
import Spotlight from "./Spotlight.svelte";
import * as settings from "../settings";
import type { Command } from "../commandPalette";

interface Deferred {
  args: any;
  resolve: (v: unknown) => void;
  reject: (e: unknown) => void;
}

let searchCalls: Deferred[] = [];
let streamCalls: Deferred[] = [];
let frecentResponse: string[] = [];

const invoke = vi.fn((cmd: string, args?: any) => {
  if (cmd === "spotlight_frecent") return Promise.resolve(frecentResponse);
  if (cmd === "spotlight_search") return new Promise((resolve, reject) => searchCalls.push({ args, resolve, reject }));
  if (cmd === "find_files_by_name_stream")
    return new Promise((resolve, reject) => streamCalls.push({ args, resolve, reject }));
  if (cmd === "write_settings") return Promise.resolve(null);
  if (cmd === "read_settings") return Promise.resolve("{}");
  return Promise.reject(new Error(`unexpected command: ${cmd}`));
});

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

/** Flush the microtask hops a manually-resolved deferred needs before Svelte's DOM-update cycle
 *  (InstantSearch.test.ts's precedent — fake timers only fake timer callbacks, not microtasks). */
async function settle() {
  for (let i = 0; i < 5; i++) await Promise.resolve();
  await tick();
}

const input = () => screen.getByPlaceholderText("Search everywhere…") as HTMLInputElement;
const rowTitles = () => Array.from(document.querySelectorAll(".sp-row")).map((el) => el.getAttribute("title"));

const paletteCommands: Command[] = [
  { id: "nav.home", label: "Home", run: vi.fn() },
  { id: "file.rename", label: "Rename", run: vi.fn() },
];

beforeEach(() => {
  vi.useFakeTimers();
  invoke.mockClear();
  searchCalls = [];
  streamCalls = [];
  frecentResponse = [];
  settings.resetSettings();
  Element.prototype.scrollIntoView = vi.fn(); // jsdom doesn't implement it (App.test.ts precedent)
});
afterEach(() => {
  vi.useRealTimers();
});

describe("Spotlight — empty-query default view (CPE-1216)", () => {
  it("shows the type hint and never calls spotlight_frecent when nothing has been visited yet", async () => {
    render(Spotlight, { root: "/root", paletteCommands });
    await settle();
    expect(screen.getByText(/Type to search/)).toBeTruthy();
    expect(invoke).not.toHaveBeenCalledWith("spotlight_frecent", expect.anything());
  });

  it("renders the frecency default view (most-frecent first) when usage history exists", async () => {
    settings.saveSpotlightFrecency([{ path: "/hot", count: 5, last_used_s: 900 }]);
    frecentResponse = ["/hot", "/warm"];
    render(Spotlight, { root: "/root", paletteCommands });
    await settle();
    expect(rowTitles()).toEqual(["/hot", "/warm"]);
  });
});

describe("Spotlight — search debounce + sectioned/highlighted rendering (CPE-1216)", () => {
  it("debounces typing, calls spotlight_search with the built kind-tagged sources, and renders results", async () => {
    render(Spotlight, { root: "/root", paletteCommands });
    await settle();

    await fireEvent.input(input(), { target: { value: "r" } });
    await fireEvent.input(input(), { target: { value: "re" } });
    expect(searchCalls).toHaveLength(0); // still inside the debounce window

    await vi.advanceTimersByTimeAsync(150);
    expect(searchCalls).toHaveLength(1);
    expect(searchCalls[0].args.query).toBe("re");
    // Only the action source is non-empty (no places/favorites/history/file-hits were given) — the
    // empty sources are dropped by buildSources.
    expect(searchCalls[0].args.sources).toEqual([["action", ["Home", "Rename"]]]);

    searchCalls[0].resolve([
      { kind: "action", results: [{ text: "Rename", kind: "action", score: 10, positions: [0, 1] }] },
    ]);
    await settle();

    expect(rowTitles()).toEqual(["Rename"]);
    expect(screen.getByText("Actions")).toBeTruthy(); // section header
    // Matched-position highlighting (positions [0,1] → "Re" highlighted, "name" plain).
    const marks = document.querySelectorAll(".sp-row mark.sp-hl");
    expect(marks).toHaveLength(1);
    expect(marks[0].textContent).toBe("Re");
  });

  it("also streams file hits from root for the query, once the first (action/folder/recent) slice lands", async () => {
    render(Spotlight, { root: "/root", paletteCommands });
    await settle();
    await fireEvent.input(input(), { target: { value: "re" } });
    await vi.advanceTimersByTimeAsync(150);
    expect(searchCalls).toHaveLength(1);
    // The first slice (action/folder/recent) resolves before the (potentially slow) recursive file
    // walk is even started — don't block the first paint on it (STREAMING.md).
    expect(streamCalls).toHaveLength(0);

    searchCalls[0].resolve([]);
    await settle();
    expect(streamCalls).toHaveLength(1);
    expect(streamCalls[0].args).toEqual(expect.objectContaining({ root: "/root", query: "re" }));
  });
});

describe("Spotlight — highlight scoped to the basename (CPE-1223)", () => {
  it("renders the FULL query highlighted within the basename, not a partial run, when a path-prefix char confuses the backend's full-path match (PR #529's failure case)", async () => {
    render(Spotlight, { root: "/root", paletteCommands });
    await settle();

    await fireEvent.input(input(), { target: { value: "marker" } });
    await vi.advanceTimersByTimeAsync(150);
    // Backend scores/highlights over the full path: its greedy match consumes the stray "m" of "Temp"
    // (index 14) for the query's leading "m", so the positions it reports only cover "arker" within the
    // basename (indices 18-22) — exactly PR #529's partial-highlight bug. The basename-direct rematch
    // must recover the WHOLE "marker" run, none of it in the "Temp" prefix.
    const text = "C:\\Users\\me\\Temp\\marker.txt";
    searchCalls[0].resolve([
      {
        kind: "file",
        results: [{ text, kind: "file", score: 20, positions: [14, 18, 19, 20, 21, 22] }],
      },
    ]);
    await settle();

    const marks = document.querySelectorAll(".sp-row mark.sp-hl");
    expect(marks).toHaveLength(1); // no separate stray mark in the "Temp" prefix
    expect(marks[0].textContent).toBe("marker"); // the full run, not "arker"
  });

  it("still highlights every run of a genuine scattered in-filename match (rdme -> README.md)", async () => {
    render(Spotlight, { root: "/root", paletteCommands });
    await settle();

    await fireEvent.input(input(), { target: { value: "rdme" } });
    await vi.advanceTimersByTimeAsync(150);
    const text = "/home/dev/project/README.md";
    const basenameStart = text.lastIndexOf("/") + 1;
    // r(0) d(3) m(4) e(5) relative to the basename — mirrors spotlight.rs's own multi-run shape.
    searchCalls[0].resolve([
      {
        kind: "file",
        results: [
          {
            text,
            kind: "file",
            score: 15,
            positions: [basenameStart + 0, basenameStart + 3, basenameStart + 4, basenameStart + 5],
          },
        ],
      },
    ]);
    await settle();

    const marks = document.querySelectorAll(".sp-row mark.sp-hl");
    expect(Array.from(marks).map((m) => m.textContent)).toEqual(["R", "DME"]);
  });
});

describe("Spotlight — keyboard activation (CPE-1216)", () => {
  it("Enter on a folder/file/recent row dispatches activate, records a frecency visit, and closes", async () => {
    const { component } = render(Spotlight, { root: "/root", paletteCommands });
    await settle();
    const onClose = vi.fn();
    const onActivate = vi.fn();
    component.$on("close", onClose);
    component.$on("activate", (e: CustomEvent<{ path: string; kind: string }>) => onActivate(e.detail));

    await fireEvent.input(input(), { target: { value: "re" } });
    await vi.advanceTimersByTimeAsync(150);
    searchCalls[0].resolve([{ kind: "folder", results: [{ text: "/repo", kind: "folder", score: 5, positions: [] }] }]);
    await settle();

    await fireEvent.keyDown(input(), { key: "Enter" });
    expect(onActivate).toHaveBeenCalledWith({ path: "/repo", kind: "folder" });
    expect(onClose).toHaveBeenCalled();

    const stored = settings.loadSpotlightFrecency();
    expect(stored.find((v) => v.path === "/repo")?.count).toBe(1);
  });

  it("Enter on an action row runs the command directly — no activate event, no frecency recorded", async () => {
    const runRename = vi.fn();
    const commands: Command[] = [{ id: "file.rename", label: "Rename", run: runRename }];
    const { component } = render(Spotlight, { root: "/root", paletteCommands: commands });
    await settle();
    const onActivate = vi.fn();
    component.$on("activate", onActivate);

    await fireEvent.input(input(), { target: { value: "re" } });
    await vi.advanceTimersByTimeAsync(150);
    searchCalls[0].resolve([
      { kind: "action", results: [{ text: "Rename", kind: "action", score: 10, positions: [0, 1] }] },
    ]);
    await settle();

    await fireEvent.keyDown(input(), { key: "Enter" });
    expect(runRename).toHaveBeenCalled();
    expect(onActivate).not.toHaveBeenCalled();
    expect(settings.loadSpotlightFrecency()).toEqual([]);
  });

  it("ArrowDown moves the active row before Enter activates it", async () => {
    const { component } = render(Spotlight, { root: "/root", paletteCommands });
    await settle();
    const onActivate = vi.fn();
    component.$on("activate", (e: CustomEvent<{ path: string; kind: string }>) => onActivate(e.detail));

    await fireEvent.input(input(), { target: { value: "r" } });
    await vi.advanceTimersByTimeAsync(150);
    searchCalls[0].resolve([
      {
        kind: "folder",
        results: [
          { text: "/r1", kind: "folder", score: 2, positions: [] },
          { text: "/r2", kind: "folder", score: 1, positions: [] },
        ],
      },
    ]);
    await settle();

    await fireEvent.keyDown(input(), { key: "ArrowDown" });
    await fireEvent.keyDown(input(), { key: "Enter" });
    expect(onActivate).toHaveBeenCalledWith({ path: "/r2", kind: "folder" });
  });

  it("Escape closes without activating", async () => {
    const { component } = render(Spotlight, { root: "/root", paletteCommands });
    await settle();
    const onClose = vi.fn();
    const onActivate = vi.fn();
    component.$on("close", onClose);
    component.$on("activate", onActivate);

    await fireEvent.keyDown(input(), { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
    expect(onActivate).not.toHaveBeenCalled();
  });
});
