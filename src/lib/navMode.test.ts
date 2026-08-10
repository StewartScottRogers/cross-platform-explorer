import { describe, it, expect } from "vitest";
import { initialNavState, reduceNavKey, type NavState } from "./navMode";

describe("navMode initialNavState (CPE-1552)", () => {
  it("starts in normal mode with empty buffers", () => {
    expect(initialNavState()).toEqual({ mode: "normal", pendingChord: "", pendingCount: "" });
  });
});

describe("navMode reduceNavKey — motion (CPE-1552)", () => {
  it("h emits a left motion with default count 1", () => {
    const { state, intent } = reduceNavKey(initialNavState(), "h");
    expect(intent).toEqual({ kind: "motion", dir: "left", count: 1 });
    expect(state).toEqual({ mode: "normal", pendingChord: "", pendingCount: "" });
  });

  it("j emits a down motion", () => {
    const { intent } = reduceNavKey(initialNavState(), "j");
    expect(intent).toEqual({ kind: "motion", dir: "down", count: 1 });
  });

  it("k emits an up motion", () => {
    const { intent } = reduceNavKey(initialNavState(), "k");
    expect(intent).toEqual({ kind: "motion", dir: "up", count: 1 });
  });

  it("l emits a right motion", () => {
    const { intent } = reduceNavKey(initialNavState(), "l");
    expect(intent).toEqual({ kind: "motion", dir: "right", count: 1 });
  });

  it("a typed count prefix (3 then j) produces a count-3 down motion and resets the count", () => {
    const afterThree = reduceNavKey(initialNavState(), "3");
    expect(afterThree.intent).toEqual({ kind: "none" });
    expect(afterThree.state.pendingCount).toBe("3");

    const afterJ = reduceNavKey(afterThree.state, "j");
    expect(afterJ.intent).toEqual({ kind: "motion", dir: "down", count: 3 });
    expect(afterJ.state.pendingCount).toBe("");
  });

  it("a multi-digit count accumulates (1 then 2 then j -> count 12)", () => {
    let s: NavState = initialNavState();
    s = reduceNavKey(s, "1").state;
    expect(s.pendingCount).toBe("1");
    s = reduceNavKey(s, "2").state;
    expect(s.pendingCount).toBe("12");
    const { intent } = reduceNavKey(s, "j");
    expect(intent).toEqual({ kind: "motion", dir: "down", count: 12 });
  });

  it("a bare 0 (no count in progress) is not a count prefix and clears buffers as unrecognized", () => {
    const { state, intent } = reduceNavKey(initialNavState(), "0");
    expect(intent).toEqual({ kind: "none" });
    expect(state).toEqual({ mode: "normal", pendingChord: "", pendingCount: "" });
  });

  it("0 mid-count accumulates normally (1 then 0 then j -> count 10)", () => {
    let s: NavState = initialNavState();
    s = reduceNavKey(s, "1").state;
    s = reduceNavKey(s, "0").state;
    expect(s.pendingCount).toBe("10");
    const { intent } = reduceNavKey(s, "j");
    expect(intent).toEqual({ kind: "motion", dir: "down", count: 10 });
  });
});

describe("navMode reduceNavKey — gg / G (CPE-1552)", () => {
  it("a single g buffers pendingChord and emits none", () => {
    const { state, intent } = reduceNavKey(initialNavState(), "g");
    expect(intent).toEqual({ kind: "none" });
    expect(state.pendingChord).toBe("g");
  });

  it("gg emits a top motion and clears the buffer", () => {
    const afterFirstG = reduceNavKey(initialNavState(), "g").state;
    const { state, intent } = reduceNavKey(afterFirstG, "g");
    expect(intent).toEqual({ kind: "motion", dir: "top", count: 1 });
    expect(state).toEqual({ mode: "normal", pendingChord: "", pendingCount: "" });
  });

  it("g followed by an unrelated key clears the buffer and re-dispatches that key correctly", () => {
    const afterFirstG = reduceNavKey(initialNavState(), "g").state;
    const { state, intent } = reduceNavKey(afterFirstG, "j");
    // The "j" is not swallowed — it resolves to its own real motion intent, not "none".
    expect(intent).toEqual({ kind: "motion", dir: "down", count: 1 });
    expect(state.pendingChord).toBe("");
  });

  it("g followed by an unrecognized key clears the buffer without emitting a stray intent", () => {
    const afterFirstG = reduceNavKey(initialNavState(), "g").state;
    const { state, intent } = reduceNavKey(afterFirstG, "q");
    expect(intent).toEqual({ kind: "none" });
    expect(state.pendingChord).toBe("");
  });

  it("G emits a bottom motion", () => {
    const { intent } = reduceNavKey(initialNavState(), "G");
    expect(intent).toEqual({ kind: "motion", dir: "bottom", count: 1 });
  });

  it("a typed count before G is consumed (3 then G -> count 3)", () => {
    const afterThree = reduceNavKey(initialNavState(), "3").state;
    const { intent } = reduceNavKey(afterThree, "G");
    expect(intent).toEqual({ kind: "motion", dir: "bottom", count: 3 });
  });
});

describe("navMode reduceNavKey — visual mode toggling via v (CPE-1552)", () => {
  it("v from normal mode enters visual mode", () => {
    const { state, intent } = reduceNavKey(initialNavState(), "v");
    expect(intent).toEqual({ kind: "enterVisual" });
    expect(state.mode).toBe("visual");
  });

  it("v from visual mode exits back to normal", () => {
    const visual = reduceNavKey(initialNavState(), "v").state;
    const { state, intent } = reduceNavKey(visual, "v");
    expect(intent).toEqual({ kind: "exitVisual" });
    expect(state.mode).toBe("normal");
  });

  it("motions in visual mode keep the mode as visual", () => {
    const visual = reduceNavKey(initialNavState(), "v").state;
    const { state, intent } = reduceNavKey(visual, "j");
    expect(intent).toEqual({ kind: "motion", dir: "down", count: 1 });
    expect(state.mode).toBe("visual");
  });
});

describe("navMode reduceNavKey — Escape (CPE-1552)", () => {
  it("Escape from visual mode forces normal and emits exitVisual", () => {
    const visual = reduceNavKey(initialNavState(), "v").state;
    const { state, intent } = reduceNavKey(visual, "Escape");
    expect(intent).toEqual({ kind: "exitVisual" });
    expect(state).toEqual({ mode: "normal", pendingChord: "", pendingCount: "" });
  });

  it("Escape from normal mode (already normal) emits none", () => {
    const { state, intent } = reduceNavKey(initialNavState(), "Escape");
    expect(intent).toEqual({ kind: "none" });
    expect(state).toEqual({ mode: "normal", pendingChord: "", pendingCount: "" });
  });

  it("Escape clears any pending chord/count buffers", () => {
    let s: NavState = initialNavState();
    s = reduceNavKey(s, "3").state;
    s = reduceNavKey(s, "g").state;
    expect(s.pendingChord).toBe("g");
    const { state } = reduceNavKey(s, "Escape");
    expect(state).toEqual({ mode: "normal", pendingChord: "", pendingCount: "" });
  });
});

describe("navMode reduceNavKey — d/y/p ops (CPE-1552)", () => {
  it("d emits a delete op without changing mode", () => {
    const { state, intent } = reduceNavKey(initialNavState(), "d");
    expect(intent).toEqual({ kind: "op", op: "delete" });
    expect(state.mode).toBe("normal");
  });

  it("y emits a yank op without changing mode", () => {
    const { state, intent } = reduceNavKey(initialNavState(), "y");
    expect(intent).toEqual({ kind: "op", op: "yank" });
    expect(state.mode).toBe("normal");
  });

  it("p emits a paste op without changing mode", () => {
    const { state, intent } = reduceNavKey(initialNavState(), "p");
    expect(intent).toEqual({ kind: "op", op: "paste" });
    expect(state.mode).toBe("normal");
  });

  it("d/y/p in visual mode do not exit visual mode", () => {
    const visual = reduceNavKey(initialNavState(), "v").state;
    const { state, intent } = reduceNavKey(visual, "d");
    expect(intent).toEqual({ kind: "op", op: "delete" });
    expect(state.mode).toBe("visual");
  });
});

describe("navMode reduceNavKey — filter / command start (CPE-1552)", () => {
  it("/ emits startFilter without changing mode", () => {
    const { state, intent } = reduceNavKey(initialNavState(), "/");
    expect(intent).toEqual({ kind: "startFilter" });
    expect(state.mode).toBe("normal");
  });

  it(": emits startCommand without changing mode", () => {
    const { state, intent } = reduceNavKey(initialNavState(), ":");
    expect(intent).toEqual({ kind: "startCommand" });
    expect(state.mode).toBe("normal");
  });
});

describe("navMode reduceNavKey — unrecognized keys (CPE-1552)", () => {
  it("an unrecognized key (e.g. 'q') emits none and clears pending buffers", () => {
    const { state, intent } = reduceNavKey(initialNavState(), "q");
    expect(intent).toEqual({ kind: "none" });
    expect(state).toEqual({ mode: "normal", pendingChord: "", pendingCount: "" });
  });

  it("an unrecognized key clears a pending count buffer too", () => {
    const afterThree = reduceNavKey(initialNavState(), "3").state;
    const { state, intent } = reduceNavKey(afterThree, "q");
    expect(intent).toEqual({ kind: "none" });
    expect(state.pendingCount).toBe("");
  });

  it("never throws on an arbitrary unmapped key", () => {
    expect(() => reduceNavKey(initialNavState(), "Tab")).not.toThrow();
    expect(() => reduceNavKey(initialNavState(), "F5")).not.toThrow();
  });
});
