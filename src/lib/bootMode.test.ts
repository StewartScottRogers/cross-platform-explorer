import { describe, it, expect } from "vitest";
import { bootMode } from "./bootMode";

describe("bootMode", () => {
  it("defaults to the explorer with no markers", () => {
    expect(bootMode("")).toBe("explorer");
    expect(bootMode("?foo=1&bar=2")).toBe("explorer");
  });

  it("selects the float preview on ?float", () => {
    expect(bootMode("?float=1")).toBe("float");
    expect(bootMode("?float")).toBe("float");
  });

  it("selects the standalone board on ?board", () => {
    expect(bootMode("?board=1")).toBe("board");
    expect(bootMode("?board")).toBe("board");
  });

  it("prefers float when both markers are present", () => {
    expect(bootMode("?float=1&board=1")).toBe("float");
  });
});
