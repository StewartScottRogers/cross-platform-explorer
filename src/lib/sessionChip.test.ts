// CPE-490: the chip must be deterministic + identical for a given session id, so the console tab and
// the left-pane leaf correlate. (The launcher.html JS duplicates this logic; these tests pin the rules
// both copies must follow.)
import { describe, it, expect } from "vitest";
import { SESSION_CHIP_COLORS, sessionColor, sessionNum, shortModel } from "./sessionChip";

describe("sessionChip", () => {
  it("gives the same colour for the same id every time", () => {
    expect(sessionColor("s2")).toBe(sessionColor("s2"));
    expect(SESSION_CHIP_COLORS).toContain(sessionColor("s2"));
  });

  it("spreads different ids across the palette", () => {
    const seen = new Set(["s1", "s2", "s3", "s4", "s5", "s6", "s7", "s8"].map(sessionColor));
    expect(seen.size).toBeGreaterThan(1); // not all the same colour
  });

  it("derives the chip number from the id's digits", () => {
    expect(sessionNum("s1")).toBe("1");
    expect(sessionNum("s12")).toBe("12");
    expect(sessionNum("nodigits")).toBe("•");
  });

  it("shortens a model to its last, tag-trimmed segment", () => {
    expect(shortModel("anthropic/claude-sonnet-5")).toBe("claude-sonnet-5");
    expect(shortModel("claude-sonnet-4-5")).toBe("claude-sonnet-4-5");
    expect(shortModel("openai/gpt-4o:free")).toBe("gpt-4o");
    expect(shortModel("")).toBe("");
  });
});
