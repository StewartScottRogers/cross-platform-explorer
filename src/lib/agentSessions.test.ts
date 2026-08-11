import { describe, it, expect } from "vitest";
import {
  parseSessionAnnouncement,
  applySessionAnnouncement,
  type AgentSession,
} from "./sidecar";
import { watchTargetFor, watchTargets, markVisited } from "./agentSessions";
import { ingestSessionState, currentSessions } from "./agentSessions";

const started = (id: string, cwd = "Z:/repo") =>
  `session:${JSON.stringify({ event: "started", sessionId: id, agentId: "claude", agentName: "Claude Code", provider: "openrouter", model: "sonnet", cwd })}`;
const ended = (id: string) => `session:${JSON.stringify({ event: "ended", sessionId: id })}`;

describe("parseSessionAnnouncement (CPE-396 wire format)", () => {
  it("decodes a started announcement into a typed session", () => {
    const a = parseSessionAnnouncement(started("s1"));
    expect(a).toEqual({
      event: "started",
      session: {
        sessionId: "s1",
        agentId: "claude",
        agentName: "Claude Code",
        provider: "openrouter",
        model: "sonnet",
        cwd: "Z:/repo",
      },
    });
  });

  it("decodes an ended announcement (identity fields may be absent)", () => {
    const a = parseSessionAnnouncement(ended("s1"));
    expect(a?.event).toBe("ended");
    expect(a?.session.sessionId).toBe("s1");
  });

  it("returns null for non-session, malformed, or identity-less frames", () => {
    expect(parseSessionAnnouncement("ui:http://127.0.0.1:9/")).toBeNull(); // not a session frame
    expect(parseSessionAnnouncement("session:{not json")).toBeNull();
    expect(parseSessionAnnouncement(`session:${JSON.stringify({ event: "x", sessionId: "s" })}`)).toBeNull();
    expect(parseSessionAnnouncement(`session:${JSON.stringify({ event: "started" })}`)).toBeNull(); // no id
  });
});

describe("applySessionAnnouncement (CPE-396 reducer)", () => {
  const s1: AgentSession = { sessionId: "s1", agentId: "claude", agentName: "Claude Code", provider: "p", model: "m", cwd: "/a" };
  it("adds a started session; replaces one with the same id", () => {
    const one = applySessionAnnouncement([], { event: "started", session: s1 });
    expect(one).toEqual([s1]);
    const moved = applySessionAnnouncement(one, { event: "started", session: { ...s1, cwd: "/b" } });
    expect(moved).toEqual([{ ...s1, cwd: "/b" }]); // still one entry, updated
  });

  it("drops an ended session and is a no-op for an unknown id", () => {
    expect(applySessionAnnouncement([s1], { event: "ended", session: s1 })).toEqual([]);
    expect(applySessionAnnouncement([s1], { event: "ended", session: { ...s1, sessionId: "other" } })).toEqual([s1]);
  });
});

describe("session store ingest (CPE-396)", () => {
  it("reflects start then end in the reactive store", () => {
    ingestSessionState(started("store-test", "Z:/proj"));
    expect(currentSessions().find((s) => s.sessionId === "store-test")?.cwd).toBe("Z:/proj");
    ingestSessionState(ended("store-test"));
    expect(currentSessions().some((s) => s.sessionId === "store-test")).toBe(false);
  });

  it("ignores a malformed payload without throwing", () => {
    expect(() => ingestSessionState("session:{broken")).not.toThrow();
  });
});

describe("watchTargetFor (CPE-399 — which project am I in)", () => {
  const sess = (cwd: string): AgentSession => ({ sessionId: cwd, agentId: "a", agentName: "A", provider: "p", model: "m", cwd });

  it("matches the folder itself and any descendant, cross-platform", () => {
    const sessions = [sess("Z:\\repos\\app")];
    expect(watchTargetFor(sessions, "Z:\\repos\\app")).toBe("Z:\\repos\\app");
    expect(watchTargetFor(sessions, "Z:/repos/app/src/lib")).toBe("Z:\\repos\\app"); // descendant, mixed seps
    expect(watchTargetFor(sessions, "Z:/repos/app-other")).toBe(""); // sibling with shared prefix ≠ inside
    expect(watchTargetFor(sessions, "Z:/elsewhere")).toBe("");
  });

  it("picks the deepest project when nested agents overlap", () => {
    const sessions = [sess("/work"), sess("/work/api")];
    expect(watchTargetFor(sessions, "/work/api/routes")).toBe("/work/api");
    expect(watchTargetFor(sessions, "/work/web")).toBe("/work");
  });

  it("returns empty when no agent is running", () => {
    expect(watchTargetFor([], "/anywhere")).toBe("");
  });
});

describe("watchTargets (CPE-1606 — only visited sessions are armed, replacing CPE-1099's watch-everything)", () => {
  const sess = (id: string, cwd: string): AgentSession => ({ sessionId: id, agentId: "a", agentName: "A", provider: "p", model: "m", cwd });

  it("is empty when no agent is running (off means off ⇒ nothing armed)", () => {
    expect(watchTargets([], new Set())).toEqual([]);
  });

  it("arms nothing when sessions are running but none has ever been visited", () => {
    const sessions = [sess("s1", "/work/api"), sess("s2", "/other/web")];
    // A session the explorer never navigated into must not be armed, no matter how long it runs —
    // this is the exact CPE-1606 repro: launch an agent, never open its folder, watcher stays idle.
    expect(watchTargets(sessions, new Set())).toEqual([]);
  });

  it("arms only the visited session, leaving an unvisited sibling untouched", () => {
    const sessions = [sess("s1", "/work/api"), sess("s2", "/other/web")];
    expect(watchTargets(sessions, new Set(["s1"]))).toEqual([sessions[0]]);
  });
});

describe("markVisited (CPE-1606 — grows the visited-this-run set that gates watchTargets)", () => {
  const sess = (id: string, cwd: string): AgentSession => ({ sessionId: id, agentId: "a", agentName: "A", provider: "p", model: "m", cwd });

  it("adds the session whose project the explorer navigates into", () => {
    const sessions = [sess("s1", "/work/api")];
    const visited = markVisited(sessions, "/work/api/routes", new Set());
    expect(visited).toEqual(new Set(["s1"]));
  });

  it("leaves the visited set untouched (same contents) when navigating somewhere unrelated to any agent", () => {
    const sessions = [sess("s1", "/work/api")];
    const visited = markVisited(sessions, "/elsewhere", new Set());
    expect(visited).toEqual(new Set());
  });

  it("does NOT visit a sibling agent's project just because a different one is on screen", () => {
    const sessions = [sess("s1", "/work/api"), sess("s2", "/work/web")];
    const visited = markVisited(sessions, "/work/api", new Set());
    expect(visited).toEqual(new Set(["s1"])); // s2 stays unarmed until its own folder is opened
  });

  it("retains a visited session after navigating away from it — no watcher thrash on every nav", () => {
    // Deliberate design choice (CPE-1606 Work Log): tearing the watch down the instant you step out of
    // the folder would (a) re-arm/disarm a `notify` watcher on every single navigation when hopping
    // between two sibling agent projects, and (b) prematurely flush that session's Cost/History row as
    // if it had ended (reconcileAgentWatch flushes on removal), fragmenting one live session's metrics
    // into two rows. So once a project is visited, it stays in the set — and stays watched — for the
    // rest of that session's life, even after the explorer navigates elsewhere.
    const sessions = [sess("s1", "/work/api")];
    const afterVisit = markVisited(sessions, "/work/api", new Set());
    const afterLeaving = markVisited(sessions, "/elsewhere", afterVisit);
    expect(afterLeaving).toEqual(new Set(["s1"]));
  });

  it("accumulates both sibling projects when you visit each in turn — neither evicts the other", () => {
    const sessions = [sess("s1", "/work/api"), sess("s2", "/work/web")];
    const afterApi = markVisited(sessions, "/work/api", new Set());
    const afterWeb = markVisited(sessions, "/work/web", afterApi);
    expect(afterWeb).toEqual(new Set(["s1", "s2"]));
  });

  it("disarms once the session actually ends — leaving (the session's lifetime) genuinely disarms it", () => {
    const running = [sess("s1", "/work/api")];
    const visited = markVisited(running, "/work/api", new Set());
    expect(visited).toEqual(new Set(["s1"]));
    // The session ends: it drops out of the running-sessions list entirely (App.svelte's reconcile
    // loop mirrors this via `$agentSessions`). The next markVisited call must prune it — a session
    // that no longer exists can't stay "armed" forever.
    const afterEnded = markVisited([], "/elsewhere", visited);
    expect(afterEnded).toEqual(new Set());
    expect(watchTargets([], afterEnded)).toEqual([]);
  });

  it("is a no-op (same Set instance) when nothing changed, so callers can skip redundant reconciles", () => {
    const sessions = [sess("s1", "/work/api")];
    const visited = new Set(["s1"]);
    expect(markVisited(sessions, "/elsewhere", visited)).toBe(visited);
  });
});
