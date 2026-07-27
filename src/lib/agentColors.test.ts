import { describe, it, expect } from "vitest";
import { colorForActor, AGENT_COLOR_COUNT } from "./agentColors";
import type { AgentSession } from "./sidecar";

const session = (id: string): AgentSession => ({
  sessionId: id,
  agentId: id,
  agentName: "",
  provider: "",
  model: "",
  cwd: "",
});

describe("colorForActor (owner-coloured heat-map palette, CPE-1116)", () => {
  it('"user" always maps to the dedicated --agent-user var, regardless of the session list', () => {
    expect(colorForActor("user", [])).toBe("var(--agent-user)");
    expect(colorForActor("user", [session("sess-1"), session("sess-2")])).toBe("var(--agent-user)");
  });

  it('"unknown" (and empty) always maps to the neutral --agent-unknown var', () => {
    expect(colorForActor("unknown", [])).toBe("var(--agent-unknown)");
    expect(colorForActor("unknown", [session("sess-1")])).toBe("var(--agent-unknown)");
  });

  it("assigns a cycling --agent-N var by the actor's position in the SORTED session list", () => {
    const sessions = [session("sess-b"), session("sess-a"), session("sess-c")];
    // Sorted by sessionId: sess-a(0), sess-b(1), sess-c(2) -> --agent-1, --agent-2, --agent-3.
    expect(colorForActor("sess-a", sessions)).toBe("var(--agent-1)");
    expect(colorForActor("sess-b", sessions)).toBe("var(--agent-2)");
    expect(colorForActor("sess-c", sessions)).toBe("var(--agent-3)");
  });

  it("is stable across re-renders for an unchanged session set, independent of arrival order", () => {
    const callA = [session("sess-x"), session("sess-y")];
    const callB = [session("sess-y"), session("sess-x")]; // same set, different order (e.g. re-render)
    expect(colorForActor("sess-x", callA)).toBe(colorForActor("sess-x", callB));
    expect(colorForActor("sess-y", callA)).toBe(colorForActor("sess-y", callB));
  });

  it("wraps around the fixed palette size for more sessions than colours", () => {
    const many = Array.from({ length: AGENT_COLOR_COUNT + 2 }, (_, i) => session(`s${i}`));
    // Sorted lexically: s0, s1, s10, s11, s2, s3, s4, s5 (string sort) — just assert wraparound:
    // the (N)th and (N + AGENT_COLOR_COUNT)th sorted sessions land on the same colour slot.
    const sortedIds = many.map((s) => s.sessionId).sort((a, b) => a.localeCompare(b));
    const first = colorForActor(sortedIds[0], many);
    const wrapped = colorForActor(sortedIds[AGENT_COLOR_COUNT], many);
    expect(wrapped).toBe(first);
  });

  it("falls back to a deterministic id-hash colour when the actor isn't in the session list", () => {
    // The session ended (or the actor's annotation is fading out) — colour must still be one of
    // the fixed vars and must be the SAME every time for the same id (never random).
    const c1 = colorForActor("ghost-session", [session("sess-a")]);
    const c2 = colorForActor("ghost-session", [session("sess-a")]);
    expect(c1).toBe(c2);
    expect(c1).toMatch(/^var\(--agent-[1-6]\)$/);
  });
});
