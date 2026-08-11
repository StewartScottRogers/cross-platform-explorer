import { describe, it, expect } from "vitest";
import {
  parseSessionAnnouncement,
  applySessionAnnouncement,
  type AgentSession,
} from "./sidecar";
import { watchTargetFor, watchTargets } from "./agentSessions";
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

describe("watchTargets (CPE-1606/1625/1626 — arms exactly the session(s) at the CURRENT deepest project match)", () => {
  const sess = (id: string, cwd: string): AgentSession => ({ sessionId: id, agentId: "a", agentName: "A", provider: "p", model: "m", cwd });

  it("is empty when no agent is running (off means off ⇒ nothing armed)", () => {
    expect(watchTargets([], "/anywhere")).toEqual([]);
  });

  it("arms nothing when sessions are running but the explorer isn't inside any of them", () => {
    const sessions = [sess("s1", "/work/api"), sess("s2", "/other/web")];
    // A session the explorer never navigated into must not be armed, no matter how long it runs —
    // this is the exact CPE-1606 repro: launch an agent, never open its folder, watcher stays idle.
    expect(watchTargets(sessions, "/elsewhere")).toEqual([]);
  });

  it("arms only the session at the current path, leaving a sibling elsewhere untouched", () => {
    const sessions = [sess("s1", "/work/api"), sess("s2", "/other/web")];
    expect(watchTargets(sessions, "/work/api/routes")).toEqual([sessions[0]]);
  });

  it("picks the deepest project when nested agents overlap, arming only that one", () => {
    const sessions = [sess("s1", "/work"), sess("s2", "/work/api")];
    expect(watchTargets(sessions, "/work/api/routes")).toEqual([sessions[1]]);
    expect(watchTargets(sessions, "/work/web")).toEqual([sessions[0]]);
  });

  it("CPE-1626: disarms immediately on navigate-away — no sticky retention once you leave the folder", () => {
    // CPE-1606 originally retained a visited session's watch for its whole life, specifically because a
    // premature `flushSession` call would silently corrupt the metrics record (see agentSessionMetrics.ts
    // — CPE-1626 fixed that at the source: `flushSession` now only persists a row once a session has
    // genuinely ended, so unwatching a still-running session here is always safe. With that coupling
    // gone, `watchTargets` goes back to a pure "what's here right now" computation — no history needed.
    const sessions = [sess("s1", "/work/api")];
    expect(watchTargets(sessions, "/work/api")).toEqual(sessions); // armed while inside
    expect(watchTargets(sessions, "/elsewhere")).toEqual([]); // disarmed the moment you leave
  });

  it("re-arms a session when you navigate back into its folder — same session, resumable", () => {
    const sessions = [sess("s1", "/work/api")];
    expect(watchTargets(sessions, "/elsewhere")).toEqual([]);
    expect(watchTargets(sessions, "/work/api")).toEqual(sessions);
  });

  it("CPE-1625: arms BOTH sessions when two agents share the exact same cwd", () => {
    // Fleets/parallel agents plausibly point two sessions at the identical project folder. Before
    // CPE-1606 every running session was watched unconditionally, so both were visible; CPE-1606's
    // first-cut `.find()` narrowed that to the first co-located session only, silently orphaning the
    // second one from Radar/Cost/History (fixed by CPE-1625's `.filter()`) — must not regress here.
    const sessions = [sess("s1", "/work/api"), sess("s2", "/work/api")];
    expect(watchTargets(sessions, "/work/api/routes")).toEqual(sessions);
  });

  it("a session that actually ends (drops out of the running list) is simply absent — never armed again", () => {
    const running = [sess("s1", "/work/api")];
    expect(watchTargets(running, "/work/api")).toEqual(running);
    expect(watchTargets([], "/work/api")).toEqual([]); // ended: gone from `sessions` entirely
  });
});
