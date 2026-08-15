/**
 * Component tests for the "find files by name" dialog (CPE-603, streaming CPE-666, recents CPE-558,
 * toolbar pre-fill CPE-866) — untested behaviors flagged by CPE-1401: recents localStorage load/save,
 * streaming-channel batch accumulation, and the generation-token supersede guard (the key regression
 * risk: a stale batch from a superseded search must be dropped, not rendered).
 *
 * Mirrors the repo's established component-test mocking (`InstantSearch.test.ts` /
 * `ContentSearchDialog.test.ts`): mock `@tauri-apps/api/core`'s `invoke` + `Channel`, since the
 * `rawInvoke`/`createChannel` streaming seam (`../invoke`) ultimately flows through that module.
 * `find_files_by_name_stream` calls are queued as manually-resolvable deferreds so the supersede test
 * can control arrival ORDER independently of call order — the whole point of that guard.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import { tick } from "svelte";
import FileNameSearchDialog from "./FileNameSearchDialog.svelte";
import type { NameMatch, NameSearchResult } from "../fileNameSearch";

interface Deferred {
  args: any;
  resolve: (v: NameSearchResult) => void;
  reject: (e: unknown) => void;
}

let calls: Deferred[] = [];

const invoke = vi.fn((cmd: string, args?: any) => {
  if (cmd === "find_files_by_name_stream") {
    return new Promise<NameSearchResult>((resolve, reject) => calls.push({ args, resolve, reject }));
  }
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

const RECENTS_KEY = "cpe.nameSearchRecents";
const PLACEHOLDER = "Name to find — try *.png or report?";
const input = () => screen.getByPlaceholderText(PLACEHOLDER) as HTMLInputElement;
const form = () => document.querySelector("form") as HTMLFormElement;
const hitPaths = () => Array.from(document.querySelectorAll(".hit")).map((el) => el.getAttribute("title"));
const recentOptions = () =>
  Array.from(document.querySelectorAll("#ns-recents option")).map((o) => o.getAttribute("value"));
const match = (path: string, name: string, is_dir = false): NameMatch => ({ path, name, is_dir });

beforeEach(() => {
  invoke.mockClear();
  calls = [];
  try {
    localStorage.clear();
  } catch {
    /* ignore */
  }
});

describe("FileNameSearchDialog — recents persistence (CPE-558)", () => {
  it("loads recent searches from localStorage on mount and offers them in the datalist", () => {
    localStorage.setItem(RECENTS_KEY, JSON.stringify(["old-query", "another"]));
    render(FileNameSearchDialog, { root: "Z:\\repos\\cpe" });
    expect(recentOptions()).toEqual(["old-query", "another"]);
  });

  it("tolerates missing/corrupt localStorage and starts with an empty recents list", () => {
    localStorage.setItem(RECENTS_KEY, "{not json");
    render(FileNameSearchDialog, { root: "Z:\\repos\\cpe" });
    expect(recentOptions()).toEqual([]);
  });

  it("saves the query to recents (newest first) when a search runs", async () => {
    localStorage.setItem(RECENTS_KEY, JSON.stringify(["older"]));
    render(FileNameSearchDialog, { root: "Z:\\repos\\cpe" });
    await fireEvent.input(input(), { target: { value: "report" } });
    await fireEvent.click(screen.getByText("Search"));
    expect(calls).toHaveLength(1);
    expect(JSON.parse(localStorage.getItem(RECENTS_KEY) ?? "[]")).toEqual(["report", "older"]);
    // The live `recents` list re-renders the datalist immediately too (not just persisted storage).
    expect(recentOptions()).toEqual(["report", "older"]);
  });
});

describe("FileNameSearchDialog — onMount pre-fill + auto-run (CPE-866)", () => {
  it("pre-fills the query and runs the search immediately when opened with initialQuery", async () => {
    render(FileNameSearchDialog, { root: "Z:\\repos\\cpe", initialQuery: "report" });
    await settle();
    expect(calls).toHaveLength(1);
    expect(calls[0].args).toEqual(expect.objectContaining({ root: "Z:\\repos\\cpe", query: "report" }));
    expect(input().value).toBe("report");
  });

  it("does not auto-run when no initialQuery is given", async () => {
    render(FileNameSearchDialog, { root: "Z:\\repos\\cpe" });
    await settle();
    expect(calls).toHaveLength(0);
    expect(input().value).toBe("");
  });
});

describe("FileNameSearchDialog — streaming batches (CPE-666)", () => {
  it("accumulates streamed batches into the results list and flips loading off on the first batch", async () => {
    render(FileNameSearchDialog, { root: "Z:\\repos\\cpe" });
    await fireEvent.input(input(), { target: { value: "report" } });
    await fireEvent.click(screen.getByText("Search"));
    await settle();
    expect(calls).toHaveLength(1);
    expect(screen.getByText("Searching…")).toBeTruthy();

    // First batch arrives: loading flips off and the hit renders immediately.
    calls[0].args.onMatch.onmessage([match("Z:\\repos\\cpe\\alpha.txt", "alpha.txt")]);
    await settle();
    expect(screen.queryByText("Searching…")).toBeNull();
    expect(hitPaths()).toEqual(["Z:\\repos\\cpe\\alpha.txt"]);

    // A second batch accumulates onto the first rather than replacing it.
    calls[0].args.onMatch.onmessage([match("Z:\\repos\\cpe\\zebra.txt", "zebra.txt")]);
    await settle();
    expect(hitPaths()).toEqual(["Z:\\repos\\cpe\\alpha.txt", "Z:\\repos\\cpe\\zebra.txt"]);
    expect(screen.getByText(/2 matches/)).toBeTruthy();

    // The final resolve applies the terminal truncated/dirs_scanned stats.
    calls[0].resolve({ matches: [], dirs_scanned: 10, truncated: true });
    await settle();
    expect(screen.getByText("(showing the first results)")).toBeTruthy();
  });

  it("shows a no-matches message when the search completes with zero hits", async () => {
    render(FileNameSearchDialog, { root: "Z:\\repos\\cpe" });
    await fireEvent.input(input(), { target: { value: "zzz" } });
    await fireEvent.click(screen.getByText("Search"));
    await settle();
    calls[0].resolve({ matches: [], dirs_scanned: 3, truncated: false });
    await settle();
    expect(screen.getByText("No files or folders match under this folder.")).toBeTruthy();
  });

  it("surfaces a rejected search as an error, not a silent hang", async () => {
    render(FileNameSearchDialog, { root: "Z:\\repos\\cpe" });
    await fireEvent.input(input(), { target: { value: "report" } });
    await fireEvent.click(screen.getByText("Search"));
    await settle();
    calls[0].reject(new Error("permission denied"));
    await settle();
    expect(screen.getByText("Error: permission denied")).toBeTruthy();
  });
});

describe("FileNameSearchDialog — generation-token supersede (CPE-666, the key regression risk)", () => {
  it("drops a stale batch (and stale final stats) from a superseded search; only the newer search's results show", async () => {
    render(FileNameSearchDialog, { root: "Z:\\repos\\cpe" });
    const formEl = form();

    // Search A starts and is still in flight (no batch delivered yet).
    await fireEvent.input(input(), { target: { value: "aaa" } });
    await fireEvent.submit(formEl);
    await settle();
    expect(calls).toHaveLength(1);
    const searchA = calls[0];

    // Search B supersedes A before A delivers anything — this bumps the generation token.
    await fireEvent.input(input(), { target: { value: "bbb" } });
    await fireEvent.submit(formEl);
    await settle();
    expect(calls).toHaveLength(2);
    const searchB = calls[1];

    // A's late-arriving batch must be dropped, not appended to the (reset) list.
    searchA.args.onMatch.onmessage([match("Z:\\repos\\cpe\\a-stale.txt", "a-stale.txt")]);
    await settle();
    expect(hitPaths()).toEqual([]);

    // B's own batch renders normally.
    searchB.args.onMatch.onmessage([match("Z:\\repos\\cpe\\b-fresh.txt", "b-fresh.txt")]);
    await settle();
    expect(hitPaths()).toEqual(["Z:\\repos\\cpe\\b-fresh.txt"]);

    // A resolving late (with its own dirs_scanned/truncated stats) must also be dropped — it must not
    // clobber B's stats or reappear in the list.
    searchA.resolve({ matches: [], dirs_scanned: 999, truncated: true });
    await settle();
    expect(hitPaths()).toEqual(["Z:\\repos\\cpe\\b-fresh.txt"]);
    expect(screen.queryByText("(showing the first results)")).toBeNull();

    // B resolving normally still works after A's stale resolution landed.
    searchB.resolve({ matches: [], dirs_scanned: 5, truncated: false });
    await settle();
    expect(hitPaths()).toEqual(["Z:\\repos\\cpe\\b-fresh.txt"]);
    expect(screen.queryByText("(showing the first results)")).toBeNull();
  });

  it("drops a stale search's rejected error from clobbering a newer search still in flight", async () => {
    render(FileNameSearchDialog, { root: "Z:\\repos\\cpe" });
    const formEl = form();

    await fireEvent.input(input(), { target: { value: "aaa" } });
    await fireEvent.submit(formEl);
    await settle();
    const searchA = calls[0];

    await fireEvent.input(input(), { target: { value: "bbb" } });
    await fireEvent.submit(formEl);
    await settle();
    expect(calls).toHaveLength(2);
    const searchB = calls[1];

    // A rejects after being superseded — must not surface as an error over B's still-loading state.
    searchA.reject(new Error("stale failure"));
    await settle();
    expect(screen.queryByText("Error: stale failure")).toBeNull();
    expect(screen.getByText("Searching…")).toBeTruthy();

    searchB.args.onMatch.onmessage([match("Z:\\repos\\cpe\\b-fresh.txt", "b-fresh.txt")]);
    await settle();
    expect(hitPaths()).toEqual(["Z:\\repos\\cpe\\b-fresh.txt"]);
  });
});

describe("FileNameSearchDialog — misc (CPE-603)", () => {
  it("Escape closes without a navigate", async () => {
    const { component } = render(FileNameSearchDialog, { root: "Z:\\repos\\cpe" });
    const onNavigate = vi.fn();
    const onClose = vi.fn();
    component.$on("navigate", onNavigate);
    component.$on("close", onClose);

    await fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
    expect(onNavigate).not.toHaveBeenCalled();
  });

  it("clicking a hit dispatches navigate with the entry path and closes", async () => {
    const { component } = render(FileNameSearchDialog, { root: "Z:\\repos\\cpe" });
    const onNavigate = vi.fn();
    const onClose = vi.fn();
    component.$on("navigate", (e: CustomEvent<string>) => onNavigate(e.detail));
    component.$on("close", onClose);

    await fireEvent.input(input(), { target: { value: "report" } });
    await fireEvent.click(screen.getByText("Search"));
    await settle();
    calls[0].args.onMatch.onmessage([match("Z:\\repos\\cpe\\report.txt", "report.txt")]);
    await settle();

    await fireEvent.click(screen.getByTitle("Z:\\repos\\cpe\\report.txt"));
    expect(onNavigate).toHaveBeenCalledWith("Z:\\repos\\cpe\\report.txt");
    expect(onClose).toHaveBeenCalled();
  });
});

// CPE-1757: the header's `root` span (`{baseName(root) || root}`, `title={root}`) was the ONE place
// still raw while the byte-identical span in ContentSearchDialog/DuplicatesDialog was already fixed in
// CPE-1712 — the inconsistency the follow-up guard test exists to catch. Asserts on the rendered DOM,
// not `displaySafeName`'s return value, per the Evidence Rule FileList.bidiSpoof.test.ts established.
describe("FileNameSearchDialog — bidi/format-character escape on the header root (CPE-1757)", () => {
  const RLO = String.fromCharCode(0x202e); // RIGHT-TO-LEFT OVERRIDE, built from a code point

  it("escapes an override in the search root's folder name, in both the visible label and its tooltip", () => {
    const root = `Z:\\repos\\${RLO}gnp.txt`;
    render(FileNameSearchDialog, { root });

    expect(screen.getByTitle(`Z:\\repos\\[RLO]gnp.txt`)).toBeTruthy();
    expect(screen.getByText("[RLO]gnp.txt")).toBeTruthy();
    expect(document.body.textContent).not.toContain("txt.png");
  });

  it("escapes a bidi override in a streamed hit's name and path", async () => {
    render(FileNameSearchDialog, { root: "Z:\\repos\\cpe" });
    await fireEvent.input(input(), { target: { value: "report" } });
    await fireEvent.click(screen.getByText("Search"));
    await settle();
    const spoofedPath = `Z:\\repos\\cpe\\${RLO}gnp.txt`;
    calls[0].args.onMatch.onmessage([match(spoofedPath, `${RLO}gnp.txt`)]);
    await settle();

    expect(screen.getByTitle(`Z:\\repos\\cpe\\[RLO]gnp.txt`)).toBeTruthy();
    expect(screen.getByText("[RLO]gnp.txt")).toBeTruthy();
    expect(document.body.textContent).not.toContain("txt.png");
  });
});
