import { describe, it, expect } from "vitest";
import {
  parseSessionAnnouncement,
  applySessionAnnouncement,
  type AgentSession,
} from "./sidecar";
import { watchTargetFor } from "./agentSessions";
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
