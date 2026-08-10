import { describe, it, expect, vi, beforeEach } from "vitest";
import { queryOsHighContrast } from "./highContrastSignal";

// `queryOsHighContrast` calls the `is_high_contrast_active` command through `./invoke`'s busy-tracking
// wrapper — mocking `./invoke` lets the test stand in for the whole backend read (CPE-1546).
const invokeMock = vi.fn(async (_cmd: string, _args?: unknown): Promise<unknown> => false);
vi.mock("./invoke", () => ({
  invoke: (...a: unknown[]) => (invokeMock as (...x: unknown[]) => unknown)(...a),
}));

describe("queryOsHighContrast (CPE-1546)", () => {
  beforeEach(() => invokeMock.mockReset());

  it("calls the is_high_contrast_active command", async () => {
    invokeMock.mockResolvedValueOnce(false);
    await queryOsHighContrast();
    expect(invokeMock).toHaveBeenCalledWith("is_high_contrast_active");
  });

  it("returns true when the OS reports high contrast active", async () => {
    invokeMock.mockResolvedValueOnce(true);
    await expect(queryOsHighContrast()).resolves.toBe(true);
  });

  it("returns false when the OS reports high contrast inactive", async () => {
    invokeMock.mockResolvedValueOnce(false);
    await expect(queryOsHighContrast()).resolves.toBe(false);
  });

  it("fails open to false when the command rejects (older host / non-Tauri / error)", async () => {
    invokeMock.mockRejectedValueOnce(new Error("command is_high_contrast_active not found"));
    await expect(queryOsHighContrast()).resolves.toBe(false);
  });

  it("never throws — a rejection is swallowed", async () => {
    invokeMock.mockRejectedValueOnce("boom");
    await expect(queryOsHighContrast()).resolves.toBe(false);
  });
});
