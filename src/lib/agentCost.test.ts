import { describe, it, expect } from "vitest";
import {
  foldCost,
  normalizeAgentCost,
  totalTokens,
  formatTokens,
  formatUsd,
  ingestCost,
  currentCost,
  clearAgentCost,
  type AgentCost,
} from "./agentCost";

const c = (sessionId: string, inputTokens: number, outputTokens: number, costUsd: number): AgentCost => ({
  sessionId,
  inputTokens,
  outputTokens,
  costUsd,
});

describe("normalizeAgentCost", () => {
  it("accepts a well-formed payload", () => {
    expect(normalizeAgentCost({ sessionId: "s1", inputTokens: 10, outputTokens: 20, costUsd: 0.05 })).toEqual(
      c("s1", 10, 20, 0.05),
    );
  });

  it("rejects malformed / missing / wrong-typed fields", () => {
    expect(normalizeAgentCost(null)).toBeNull();
    expect(normalizeAgentCost(undefined)).toBeNull();
    expect(normalizeAgentCost("garbage")).toBeNull();
    expect(normalizeAgentCost({})).toBeNull();
    expect(normalizeAgentCost({ sessionId: "", inputTokens: 1, outputTokens: 2, costUsd: 0 })).toBeNull(); // empty id
    expect(normalizeAgentCost({ sessionId: "s1", inputTokens: "10", outputTokens: 20, costUsd: 0.05 })).toBeNull();
    expect(normalizeAgentCost({ sessionId: "s1", inputTokens: 10, outputTokens: 20 })).toBeNull(); // missing costUsd
    expect(
      normalizeAgentCost({ sessionId: "s1", inputTokens: NaN, outputTokens: 20, costUsd: 0.05 }),
    ).toBeNull(); // non-finite
  });
});

describe("foldCost", () => {
  it("stores latest-wins per sessionId, leaving other sessions untouched", () => {
    let s = foldCost({}, c("a", 1, 2, 0.01));
    s = foldCost(s, c("b", 3, 4, 0.02));
    expect(s).toEqual({ a: c("a", 1, 2, 0.01), b: c("b", 3, 4, 0.02) });
    // A second event for "a" replaces its record; "b" is untouched.
    s = foldCost(s, c("a", 100, 200, 1.5));
    expect(s.a).toEqual(c("a", 100, 200, 1.5));
    expect(s.b).toEqual(c("b", 3, 4, 0.02));
  });
});

describe("totalTokens", () => {
  it("sums input + output", () => {
    expect(totalTokens({ inputTokens: 10, outputTokens: 25 })).toBe(35);
  });

  it("is NaN/negative-safe: a bad field contributes 0, never NaN", () => {
    expect(totalTokens({ inputTokens: NaN, outputTokens: 25 })).toBe(25);
    expect(totalTokens({ inputTokens: -5, outputTokens: 25 })).toBe(25);
    expect(totalTokens({ inputTokens: NaN, outputTokens: NaN })).toBe(0);
  });
});

describe("formatTokens", () => {
  it("adds thousands separators", () => {
    expect(formatTokens(1234567)).toBe("1,234,567");
    expect(formatTokens(0)).toBe("0");
    expect(formatTokens(42)).toBe("42");
  });

  it("is NaN/negative-safe, rendering 0 rather than NaN or a negative number", () => {
    expect(formatTokens(NaN)).toBe("0");
    expect(formatTokens(-10)).toBe("0");
    expect(formatTokens(Infinity)).toBe("0");
  });

  it("truncates a fractional count", () => {
    expect(formatTokens(10.9)).toBe("10");
  });
});

describe("formatUsd", () => {
  it("formats to 4 decimal places with a leading $", () => {
    expect(formatUsd(0.05)).toBe("$0.0500");
    expect(formatUsd(12.3)).toBe("$12.3000");
    expect(formatUsd(0)).toBe("$0.0000");
  });

  it("is NaN/negative-safe, rendering $0.0000 rather than $NaN", () => {
    expect(formatUsd(NaN)).toBe("$0.0000");
    expect(formatUsd(-1)).toBe("$0.0000");
    expect(formatUsd(Infinity)).toBe("$0.0000");
  });
});

describe("store lifecycle", () => {
  it("ingests a payload and clears back to empty", () => {
    clearAgentCost();
    ingestCost({ sessionId: "s1", inputTokens: 5, outputTokens: 5, costUsd: 0.01 });
    expect(currentCost()).toEqual({ s1: c("s1", 5, 5, 0.01) });
    clearAgentCost();
    expect(currentCost()).toEqual({});
  });

  it("ignores an empty/malformed payload without touching the store", () => {
    clearAgentCost();
    ingestCost("garbage");
    ingestCost(null);
    ingestCost({ sessionId: "", inputTokens: 1, outputTokens: 2, costUsd: 0.1 });
    expect(currentCost()).toEqual({});
  });

  it("folds multiple sessions independently through ingestCost", () => {
    clearAgentCost();
    ingestCost({ sessionId: "a", inputTokens: 1, outputTokens: 1, costUsd: 0.001 });
    ingestCost({ sessionId: "b", inputTokens: 2, outputTokens: 2, costUsd: 0.002 });
    expect(currentCost()).toEqual({
      a: c("a", 1, 1, 0.001),
      b: c("b", 2, 2, 0.002),
    });
    clearAgentCost();
  });
});
