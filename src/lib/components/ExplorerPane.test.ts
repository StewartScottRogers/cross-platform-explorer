/**
 * CPE-1120: ExplorerPane must forward its live `sessions` prop straight into `<FileList>` so the
 * owner heat-map + legend (CPE-1116) resolve colours via the stable sorted-session index and show
 * agent names, instead of silently defaulting to `[]` and falling back to the per-id hash + a
 * shortened sessionId (the bug this ticket fixes — flagged by the CPE-1116 Reviewer + UAT).
 */
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import ExplorerPane from "./ExplorerPane.svelte";
import { emptySelection } from "../selection";
import { ingestActivity, clearActivity } from "../agentActivity";
import type { AgentSession } from "../sidecar";
import type { DirEntry } from "../types";

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
