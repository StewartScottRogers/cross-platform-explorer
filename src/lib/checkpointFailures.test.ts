import { describe, it, expect, vi, beforeEach } from "vitest";

// vi.mock is hoisted; create the mock fn via vi.hoisted so the factory closes over an initialised
// binding (matches agentSessionMetrics.test.ts). `checkpointRecordFailure` is the only member this
// module calls.
const { checkpointRecordFailure } = vi.hoisted(() => ({ checkpointRecordFailure: vi.fn() }));
vi.mock("./bindings.gen", () => ({ commands: { checkpointRecordFailure } }));

import { recordCheckpointFailure } from "./checkpointFailures";

beforeEach(() => {
  checkpointRecordFailure.mockReset();
});

describe("recordCheckpointFailure (CPE-1600)", () => {
  it("calls checkpoint_record_failure with the root, operation, and an Error's message", async () => {
    checkpointRecordFailure.mockResolvedValue({ status: "ok", data: null });
    await recordCheckpointFailure("/repo/pics", "Before batch media overwrite", new Error("disk full"));
    expect(checkpointRecordFailure).toHaveBeenCalledWith("/repo/pics", "Before batch media overwrite", "disk full");
  });

  it("stringifies a non-Error reason (e.g. a plain-string Err(String) rejection)", async () => {
    checkpointRecordFailure.mockResolvedValue({ status: "ok", data: null });
    await recordCheckpointFailure("/repo", "Before removing clutter", "permission denied");
    expect(checkpointRecordFailure).toHaveBeenCalledWith("/repo", "Before removing clutter", "permission denied");
  });

  it("swallows a rejected checkpointRecordFailure call rather than throwing (best-effort, never a second failure)", async () => {
    checkpointRecordFailure.mockRejectedValue(new Error("store dir unwritable"));
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    await expect(recordCheckpointFailure("/repo", "Before metadata edit", new Error("boom"))).resolves.toBeUndefined();
    expect(errSpy).toHaveBeenCalled();
    errSpy.mockRestore();
  });

  it("swallows an error-envelope result ({status:'error'}) without throwing, same as a rejection", async () => {
    checkpointRecordFailure.mockResolvedValue({ status: "error", error: "disk full" });
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    await expect(
      recordCheckpointFailure("/repo", "Before removing similar images", new Error("boom")),
    ).resolves.toBeUndefined();
    expect(errSpy).toHaveBeenCalled();
    errSpy.mockRestore();
  });
});
