import { describe, it, expect } from "vitest";
import { pushClosedTab } from "./tabs";

describe("pushClosedTab (CPE-356)", () => {
  it("appends the newest closed path last", () => {
    expect(pushClosedTab(["/a"], "/b")).toEqual(["/a", "/b"]);
  });

  it("does not mutate the input", () => {
    const s = ["/a"];
    pushClosedTab(s, "/b");
    expect(s).toEqual(["/a"]);
  });

  it("caps the stack, dropping the oldest", () => {
    let s: string[] = [];
    for (let i = 0; i < 15; i++) s = pushClosedTab(s, `/f${i}`, 10);
    expect(s).toHaveLength(10);
    expect(s[0]).toBe("/f5"); // oldest kept
    expect(s[s.length - 1]).toBe("/f14"); // newest
  });
});
