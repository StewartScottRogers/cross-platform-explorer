/**
 * CPE-1120: ExplorerPane must forward its live `sessions` prop straight into `<FileList>` so the
 * owner heat-map + legend (CPE-1116) resolve colours via the stable sorted-session index and show
 * agent names, instead of silently defaulting to `[]` and falling back to the per-id hash + a
 * shortened sessionId (the bug this ticket fixes — flagged by the CPE-1116 Reviewer + UAT).
 */
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import ExplorerPane from "./ExplorerPane.svelte";
import { emptySelection } from "../selection";
import { ingestActivity, clearActivity } from "../agentActivity";
import type { AgentSession } from "../sidecar";
import type { DirEntry, RecentFile } from "../types";

// The component tree imports Tauri APIs transitively; stub them so jsdom can render without a
// Tauri runtime (same stub FileList.test.ts uses).
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const entry = (over: Partial<DirEntry> = {}): DirEntry => ({
  name: "changed.rs",
  path: "/x/changed.rs",
  is_dir: false,
  size: 1024,
  modified: Date.now(),
  extension: "rs",
  hidden: false,
  ...over,
});

const session = (over: Partial<AgentSession> = {}): AgentSession => ({
  sessionId: "sess-a",
  agentId: "agent-1",
  agentName: "Claude",
  provider: "anthropic",
  model: "claude-opus",
  cwd: "/x",
  ...over,
});

describe("ExplorerPane -> FileList sessions wiring (CPE-1120)", () => {
  afterEach(() => clearActivity());

  it("forwards a non-empty sessions list so the owner colour resolves via the sorted index, and the legend shows the agent's name", () => {
    ingestActivity([{ kind: "modified", path: "/x/changed.rs", actor: "sess-a" }]);
    // Sorted by sessionId: "sess-a" < "sess-b" -> sess-a is index 0 -> --agent-1. This is the
    // *sorted-session-index* result; it's only reachable if ExplorerPane actually passed `sessions`
    // down (the hash fallback for "sess-a" alone need not land on slot 1).
    const sessions = [session({ sessionId: "sess-b", agentName: "Codex" }), session({ sessionId: "sess-a", agentName: "Claude" })];

    const { container } = render(ExplorerPane, {
      selection: emptySelection(),
      activeWatchCwd: "/x",
      watchedAgentName: "Claude",
      entries: [entry()],
      sessions,
    });

    const row = container.querySelector(".row.agent-active") as HTMLElement;
    expect(row).toBeTruthy();
    expect(row.style.getPropertyValue("--agent-accent")).toBe("var(--agent-1)");

    // Legend renders the agent's friendly name (from the session), not a shortened "sess-a…" id —
    // only possible once `sessions` reaches FileList's `friendlyActor` lookup.
    expect(screen.getByText("Claude")).toBeTruthy();
    expect(screen.queryByText(/sess-a/)).toBeNull();
  });

  it("off means off: no sessions (default prop) leaves the legend on the shortened-id fallback, unchanged from before this wiring", () => {
    ingestActivity([{ kind: "modified", path: "/x/changed.rs", actor: "sess-a-long-id" }]);

    const { container } = render(ExplorerPane, {
      selection: emptySelection(),
      activeWatchCwd: "/x",
      watchedAgentName: "Claude",
      entries: [entry({ path: "/x/changed.rs" })],
      // `sessions` omitted entirely -> ExplorerPane's own default ([]).
    });

    const row = container.querySelector(".row.agent-active") as HTMLElement;
    expect(row).toBeTruthy();
    expect(row.style.getPropertyValue("--agent-accent")).toMatch(/^var\(--agent-[1-6]\)$/);
    // friendlyActor's fallback for an unresolved id: first 10 chars + an ellipsis.
    expect(screen.getByText("sess-a-lon…")).toBeTruthy();
  });
});

describe("ExplorerPane pane-wide right-click → empty-area menu (CPE-1154)", () => {
  // Fire a real, cancelable contextmenu event on the `.filelist-pane` scroll container itself —
  // the blank pane pixels the old `.rows`/`.empty-state` handlers never covered (the confirmed leak).
  const rightClick = (el: Element, x = 7, y = 9): MouseEvent => {
    const ev = new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: x, clientY: y });
    el.dispatchEvent(ev);
    return ev;
  };

  it("an EMPTY folder: right-clicking the blank pane opens the app menu (dispatches contextEmpty) and suppresses the native one", () => {
    const { container, component } = render(ExplorerPane, {
      selection: emptySelection(),
      entries: [], // empty folder — `.empty-state` is a centred box that does NOT fill the pane
    });
    const contextEmpty = vi.fn();
    component.$on("contextEmpty", (e) => contextEmpty(e.detail));

    const pane = container.querySelector(".filelist-pane")!;
    expect(pane).toBeTruthy();
    const ev = rightClick(pane, 7, 9);

    expect(contextEmpty).toHaveBeenCalledWith({ x: 7, y: 9 });
    expect(ev.defaultPrevented).toBe(true); // native WebView2/Edge menu suppressed
  });

  it("a populated folder: right-clicking the blank pane (not a row) still opens the empty-area menu", () => {
    const { container, component } = render(ExplorerPane, {
      selection: emptySelection(),
      entries: [entry()],
    });
    const contextEmpty = vi.fn();
    component.$on("contextEmpty", (e) => contextEmpty(e.detail));

    rightClick(container.querySelector(".filelist-pane")!, 3, 4);
    expect(contextEmpty).toHaveBeenCalledWith({ x: 3, y: 4 });
  });

  it("Home is NOT a folder: the pane catch-all never opens the empty-area menu there", () => {
    const { container, component } = render(ExplorerPane, {
      selection: emptySelection(),
      inHome: true,
    });
    const contextEmpty = vi.fn();
    component.$on("contextEmpty", (e) => contextEmpty(e.detail));

    const ev = rightClick(container.querySelector(".filelist-pane")!);
    expect(contextEmpty).not.toHaveBeenCalled();
    expect(ev.defaultPrevented).toBe(false); // ExplorerPane doesn't act; App's window suppressor handles Home
  });
});

describe("ExplorerPane Home -> right-pane preview wiring (CPE-1132)", () => {
  it("forwards HomeView's `select` event as `homeSelect` (Home has no FileList/selectedEntries of its own)", async () => {
    const recents: RecentFile[] = [{ path: "/home/a.md", name: "a.md", opened: 1 }];
    const { component } = render(ExplorerPane, {
      selection: emptySelection(),
      inHome: true,
      recents,
    });
    const homeSelect = vi.fn();
    component.$on("homeSelect", (e) => homeSelect(e.detail));

    await fireEvent.click(screen.getByText("a.md"));

    expect(homeSelect).toHaveBeenCalledWith("/home/a.md");
  });
});
