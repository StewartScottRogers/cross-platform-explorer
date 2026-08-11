/**
 * Component tests for the Batch-Media dialog (CPE-1093, epic CPE-723) — QA-Architect burndown
 * CPE-1105. The dialog shipped with only "human-verified looks-good" coverage (no component test);
 * this pins its op-building / debounced+gen-tokened plan preview / streamed-apply logic in CI.
 *
 * Mirrors the repo's established component-test mocking (see `DuplicatesDialog.test.ts` /
 * `ContentSearchDialog.test.ts`): mock `@tauri-apps/api/core`'s `invoke` + `Channel`, since both the
 * typed `commands.*` client (`../bindings.gen`) and the raw `rawInvoke`/`createChannel` streaming seam
 * (`../invoke`) ultimately flow through that module. `batch_media_plan` calls are queued as
 * manually-resolvable deferreds (rather than resolved immediately) so the generation-token test can
 * control resolution ORDER independently of call order — the whole point of that guard.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import { tick } from "svelte";
import BatchMediaDialog from "./BatchMediaDialog.svelte";

interface Deferred {
  args: any;
  resolve: (v: unknown) => void;
  reject: (e: unknown) => void;
}

let planCalls: Deferred[] = [];
let execCalls: Deferred[] = [];
// CPE-1590: `apply()` best-effort checkpoints every affected folder before an in-place-overwrite run —
// mirrors the `checkpoint_create` mock in `NearDuplicatesDialog.test.ts`/`SimilarImagesDialog.test.ts`.
let checkpointCalls: Array<[string, string]> = [];
// CPE-1599 adds "skipped": the checkpoint succeeds, but `CheckpointCreated.skipped` names file(s) it left
// out (oversize/budget) — recovery for those is incomplete even though the call itself didn't reject.
let checkpointBehavior: "ok" | "reject" | "skipped" = "ok";

const invoke = vi.fn((cmd: string, args?: any) => {
  if (cmd === "batch_media_plan") {
    return new Promise((resolve, reject) => planCalls.push({ args, resolve, reject }));
  }
  if (cmd === "batch_media_execute_stream") {
    return new Promise((resolve, reject) => execCalls.push({ args, resolve, reject }));
  }
  if (cmd === "checkpoint_create") {
    checkpointCalls.push([args.root, args.label]);
    if (checkpointBehavior === "reject") return Promise.reject(new Error("disk full"));
    const skipped =
      checkpointBehavior === "skipped" ? [{ path: `${args.root}/huge.jpg`, size: 999999999, reason: "oversize" }] : [];
    return Promise.resolve({
      checkpoint: { manifest_id: "m1", label: args.label, ts: 0 },
      new_blobs: 0,
      reused_blobs: 0,
      added_bytes: 0,
      skipped,
    });
  }
  return Promise.reject(new Error(`unexpected command: ${cmd}`));
});

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

/** Flush the microtask hops a manually-resolved deferred needs to continue past its `await` — through
 *  the typed client's own `await TAURI_INVOKE(...)`, then the dialog's `await commands.batchMediaPlan(...)`
 *  (or `rawInvoke`) — before flushing Svelte's own DOM-update cycle. Fake timers only fake timer
 *  callbacks, not promise microtasks, so this works regardless of whether `vi.useFakeTimers()` is active. */
async function settle() {
  for (let i = 0; i < 5; i++) await Promise.resolve();
  await tick();
}

/** The ordered op pills' labels, read straight from the DOM — scoped to `.pill-text` so it can't
 *  accidentally match the "Resize"/"Convert"/"Rotate" op-kind `<option>` text in the dropdown above. */
function pillLabels(): (string | null)[] {
  return Array.from(document.querySelectorAll(".pill-text")).map((el) => el.textContent);
}

beforeEach(() => {
  vi.useFakeTimers();
  invoke.mockClear();
  planCalls = [];
  execCalls = [];
  checkpointCalls = [];
  checkpointBehavior = "ok";
});
afterEach(() => {
  vi.useRealTimers();
});

const addButton = () => screen.getByText("+ Add") as HTMLButtonElement;
const applyButton = () => screen.getByText("Apply") as HTMLButtonElement;

describe("BatchMediaDialog op building (CPE-1093 / CPE-1105)", () => {
  it("adds ops in order with correct pill labels, and removing one preserves the remaining order", async () => {
    render(BatchMediaDialog, { paths: ["/repo/a.png"] });

    // Resize is the default op kind (max_px 1024) — already a complete op.
    await fireEvent.click(addButton());
    expect(screen.getByText("Resize 1024px")).toBeTruthy();

    const opSelect = screen.getByLabelText("Operation") as HTMLSelectElement;
    await fireEvent.change(opSelect, { target: { value: "convert" } });
    const extInput = screen.getByLabelText("Target extension") as HTMLInputElement;
    await fireEvent.input(extInput, { target: { value: "avif" } });
    await fireEvent.click(addButton());
    expect(screen.getByText("Convert → avif")).toBeTruthy();

    await fireEvent.change(opSelect, { target: { value: "rotate" } });
    await fireEvent.click(addButton()); // default rotate 90°
    expect(screen.getByText("Rotate 90°")).toBeTruthy();

    // Order preserved: pills read in the order they were added.
    expect(pillLabels()).toEqual(["Resize 1024px", "Convert → avif", "Rotate 90°"]);

    // Remove the middle pill (Convert) — the rest keep their relative order.
    const removeButtons = screen.getAllByLabelText("Remove operation");
    await fireEvent.click(removeButtons[1]);
    expect(screen.queryByText("Convert → avif")).toBeNull();
    expect(pillLabels()).toEqual(["Resize 1024px", "Rotate 90°"]);
  });

  it("disables Add for an incomplete Convert (empty extension) or out-of-range Compress quality", async () => {
    render(BatchMediaDialog, { paths: ["/repo/a.png"] });
    const opSelect = screen.getByLabelText("Operation") as HTMLSelectElement;

    await fireEvent.change(opSelect, { target: { value: "convert" } });
    const extInput = screen.getByLabelText("Target extension") as HTMLInputElement;
    await fireEvent.input(extInput, { target: { value: "" } });
    expect(addButton().disabled).toBe(true);
    await fireEvent.input(extInput, { target: { value: "webp" } });
    expect(addButton().disabled).toBe(false);

    await fireEvent.change(opSelect, { target: { value: "compress" } });
    const qualityInput = screen.getByLabelText("Compress quality") as HTMLInputElement;
    await fireEvent.input(qualityInput, { target: { value: "0" } });
    expect(addButton().disabled).toBe(true);
    await fireEvent.input(qualityInput, { target: { value: "101" } });
    expect(addButton().disabled).toBe(true);
    await fireEvent.input(qualityInput, { target: { value: "50" } });
    expect(addButton().disabled).toBe(false);
  });
});

describe("BatchMediaDialog plan preview (debounced + generation-tokened)", () => {
  it("debounces the plan call and renders the returned PlannedItem rows", async () => {
    render(BatchMediaDialog, { paths: ["/repo/a.png", "/repo/b.png"] });
    await fireEvent.click(addButton()); // resize 1024 (default)

    // Not called yet — still inside the 200ms debounce window.
    expect(invoke).not.toHaveBeenCalledWith("batch_media_plan", expect.anything());
    await vi.advanceTimersByTimeAsync(200);

    expect(invoke).toHaveBeenCalledWith("batch_media_plan", {
      job: { ops: [{ op: "resize", max_px: 1024 }], non_destructive: true, confirmed_overwrite: false },
      inputs: ["/repo/a.png", "/repo/b.png"],
    });
    expect(planCalls).toHaveLength(1);

    planCalls[0].resolve([
      { input: "/repo/a.png", output: "/repo/a-1024.png", summary: "resized to 1024px" },
      { input: "/repo/b.png", output: "/repo/b-1024.png", summary: "resized to 1024px" },
    ]);
    await settle();

    expect(screen.getAllByText("resized to 1024px")).toHaveLength(2);
    expect(screen.getByText("a.png")).toBeTruthy(); // input basename
    expect(screen.getByText("a-1024.png")).toBeTruthy(); // output basename
    expect(screen.getByText("2 files will be written.")).toBeTruthy();
  });

  it("drops a stale plan response that resolves after a newer one has already landed", async () => {
    render(BatchMediaDialog, { paths: ["/repo/a.png"] });
    await fireEvent.click(addButton()); // ops change #1 -> schedules plan call #1
    await vi.advanceTimersByTimeAsync(200);
    expect(planCalls).toHaveLength(1);
    const stale = planCalls[0];

    // A second, superseding change (toggle non-destructive) before the first response arrives.
    await fireEvent.click(screen.getByLabelText(/Write to new files/i));
    await vi.advanceTimersByTimeAsync(200);
    expect(planCalls).toHaveLength(2);
    const fresh = planCalls[1];

    // Resolve the NEWER call first.
    fresh.resolve([{ input: "/repo/a.png", output: "/repo/a-new.png", summary: "fresh-result" }]);
    await settle();
    expect(screen.getByText("fresh-result")).toBeTruthy();

    // The STALE call resolves after — it must be dropped, not clobber the fresher plan.
    stale.resolve([{ input: "/repo/a.png", output: "/repo/a-old.png", summary: "stale-result" }]);
    await settle();
    expect(screen.queryByText("stale-result")).toBeNull();
    expect(screen.getByText("fresh-result")).toBeTruthy(); // unchanged
  });

  it("flows the non-destructive toggle into the built BatchJob", async () => {
    render(BatchMediaDialog, { paths: ["/repo/a.png"] });
    await fireEvent.click(addButton());
    await fireEvent.click(screen.getByLabelText(/Write to new files/i)); // uncheck: default true -> false
    await vi.advanceTimersByTimeAsync(200);

    expect(planCalls).toHaveLength(1); // both changes coalesce into a single debounced call
    expect(planCalls[0].args.job.non_destructive).toBe(false);
  });
});

describe("BatchMediaDialog validation + Apply gating", () => {
  it("Apply is disabled with zero ops", () => {
    render(BatchMediaDialog, { paths: ["/repo/a.png"] });
    expect(applyButton().disabled).toBe(true);
  });

  it("a batchMediaPlan error surfaces and disables Apply (also disabled at 0 planned items)", async () => {
    render(BatchMediaDialog, { paths: ["/repo/a.png"] });
    await fireEvent.click(addButton());
    await vi.advanceTimersByTimeAsync(200);
    expect(planCalls).toHaveLength(1);

    // Reject with a plain (non-Error) value — the typed client's catch treats that as a domain
    // `Result::Err`, exactly like a real validation failure from `cpe_server::batch_media::plan`.
    planCalls[0].reject("rotate degrees must be 90, 180, or 270");
    await settle();

    expect(screen.getByText("rotate degrees must be 90, 180, or 270")).toBeTruthy();
    expect(applyButton().disabled).toBe(true);
  });

  it("Apply stays disabled when the plan resolves with zero planned items", async () => {
    render(BatchMediaDialog, { paths: ["/repo/a.png"] });
    await fireEvent.click(addButton());
    await vi.advanceTimersByTimeAsync(200);
    planCalls[0].resolve([]);
    await settle();
    expect(applyButton().disabled).toBe(true);
  });
});

describe("BatchMediaDialog apply + streamed progress", () => {
  async function planAndReachApplyReady() {
    render(BatchMediaDialog, { paths: ["/repo/a.png", "/repo/b.png"] });
    await fireEvent.click(addButton());
    await vi.advanceTimersByTimeAsync(200);
    planCalls[0].resolve([
      { input: "/repo/a.png", output: "/repo/a-1024.png", summary: "s1" },
      { input: "/repo/b.png", output: "/repo/b-1024.png", summary: "s2" },
    ]);
    await settle();
  }

  it("streams OpResult batches into a done/failed progress readout and dispatches apply on completion", async () => {
    await planAndReachApplyReady();
    expect(applyButton().disabled).toBe(false);

    await fireEvent.click(applyButton());
    expect(invoke).toHaveBeenCalledWith(
      "batch_media_execute_stream",
      expect.objectContaining({
        items: [
          { input: "/repo/a.png", output: "/repo/a-1024.png", summary: "s1" },
          { input: "/repo/b.png", output: "/repo/b-1024.png", summary: "s2" },
        ],
        job: { ops: [{ op: "resize", max_px: 1024 }], non_destructive: true, confirmed_overwrite: false },
      }),
    );
    expect(execCalls).toHaveLength(1);
    const channel = execCalls[0].args.onResult as { onmessage: ((v: unknown) => void) | null };
    expect(typeof channel.onmessage).toBe("function");

    channel.onmessage!([{ path: "/repo/a.png", ok: true, error: "" }]);
    await tick();
    expect(screen.getByText("1/2 done")).toBeTruthy();
    // Progress bar reflects real (non-NaN) percent, not the "before a plan resolves" 0/0 case.
    const fill = document.querySelector(".progress .fill") as HTMLElement;
    expect(fill.style.width).toBe("50%");

    channel.onmessage!([{ path: "/repo/b.png", ok: false, error: "boom" }]);
    await tick();
    expect(screen.getByText("2/2 done, 1 failed")).toBeTruthy();

    execCalls[0].resolve({ written: 1, skipped: [["/repo/b.png", "boom"]] });
    await settle();

    // Channel torn down on completion — no dangling subscription (leak guard). The `apply` dispatch
    // itself is asserted by the dedicated test below.
    expect(channel.onmessage).toBeNull();
  });

  it("dispatches the apply event with the final BatchReport on completion", async () => {
    const { component } = render(BatchMediaDialog, { paths: ["/repo/a.png"] });
    await fireEvent.click(addButton());
    await vi.advanceTimersByTimeAsync(200);
    planCalls[0].resolve([{ input: "/repo/a.png", output: "/repo/a-1024.png", summary: "s1" }]);
    await settle();

    const onApply = vi.fn();
    component.$on("apply", (e: CustomEvent) => onApply(e.detail));

    await fireEvent.click(applyButton());
    expect(execCalls).toHaveLength(1);
    execCalls[0].resolve({ written: 1, skipped: [] });
    await settle();

    expect(onApply).toHaveBeenCalledWith({
      report: { written: 1, skipped: [] },
      checkpointFailures: [],
      checkpointPartial: [],
    });
  });

  it("holds the dialog open on skips, lists each skipped file + reason, and dispatches only on Done (CPE-1115)", async () => {
    const { component } = render(BatchMediaDialog, { paths: ["/repo/photo.jpg", "/repo/pixel.png"] });
    await fireEvent.click(addButton());
    await vi.advanceTimersByTimeAsync(200);
    planCalls[0].resolve([
      { input: "/repo/photo.jpg", output: "/repo/photo-out.jpg", summary: "s1" },
      { input: "/repo/pixel.png", output: "/repo/pixel-out.png", summary: "s2" },
    ]);
    await settle();

    const onApply = vi.fn();
    component.$on("apply", (e: CustomEvent) => onApply(e.detail));

    await fireEvent.click(applyButton());
    expect(execCalls).toHaveLength(1);
    // One file written, one skipped (an undecodable input) — the CPE-1115 scenario.
    execCalls[0].resolve({ written: 1, skipped: [["/repo/photo.jpg", "not a valid image"]] });
    await settle();

    // The dialog stays open on a results panel that names the skipped file + reason and the counts —
    // NOT a silent close/dispatch.
    const skips = screen.getByTestId("batch-media-skips");
    expect(skips.textContent).toContain("photo.jpg");
    expect(skips.textContent).toContain("not a valid image");
    expect(skips.textContent).toContain("1 written");
    expect(skips.textContent).toContain("1 skipped");
    expect(onApply).not.toHaveBeenCalled();

    // "Done" acknowledges → hands the report to the parent (which refreshes the listing + closes).
    await fireEvent.click(screen.getByTestId("batch-media-done"));
    expect(onApply).toHaveBeenCalledWith({
      report: { written: 1, skipped: [["/repo/photo.jpg", "not a valid image"]] },
      checkpointFailures: [], // non-destructive path here — no checkpoint was ever attempted
      checkpointPartial: [],
    });
  });

  it("tears down the channel subscription on unmount even mid-apply (no leak)", async () => {
    const { unmount } = render(BatchMediaDialog, { paths: ["/repo/a.png"] });
    await fireEvent.click(addButton());
    await vi.advanceTimersByTimeAsync(200);
    planCalls[0].resolve([{ input: "/repo/a.png", output: "/repo/a-1024.png", summary: "s1" }]);
    await settle();

    await fireEvent.click(applyButton());
    expect(execCalls).toHaveLength(1);
    const channel = execCalls[0].args.onResult as { onmessage: ((v: unknown) => void) | null };
    expect(typeof channel.onmessage).toBe("function");

    // Destroy the dialog before the stream ever resolves (e.g. the parent unmounts it).
    unmount();
    expect(channel.onmessage).toBeNull();
  });
});

describe("BatchMediaDialog overwrite-in-place confirm (CPE-1590)", () => {
  /** Uncheck "Write to new files", add a Compress op (no dedicated output-renaming suffix), and resolve
   *  the plan so every planned output equals its input — the exact "unchecking the box silently arms an
   *  overwrite" scenario the ticket reports. */
  async function planOverwriteReady(paths: string[] = ["/repo/a.jpg"]) {
    const rendered = render(BatchMediaDialog, { paths });
    const opSelect = screen.getByLabelText("Operation") as HTMLSelectElement;
    await fireEvent.change(opSelect, { target: { value: "compress" } });
    await fireEvent.click(addButton());
    await fireEvent.click(screen.getByLabelText(/Write to new files/i)); // uncheck -> non_destructive: false
    await vi.advanceTimersByTimeAsync(200);
    expect(planCalls).toHaveLength(1);
    expect(planCalls[0].args.job.non_destructive).toBe(false);
    planCalls[0].resolve(paths.map((p) => ({ input: p, output: p, summary: "compress q80" })));
    await settle();
    return rendered;
  }

  it("does not execute on the first Apply click when the plan would overwrite originals in place", async () => {
    await planOverwriteReady();
    expect(screen.queryByTestId("overwrite-confirm")).toBeNull();

    await fireEvent.click(applyButton());
    expect(execCalls).toHaveLength(0); // NOT executed — a confirm is required first

    const confirm = screen.getByTestId("overwrite-confirm");
    expect(confirm.textContent).toContain("1");
    expect(confirm.textContent).toContain("Undo");
    expect(screen.getByTestId("overwrite-confirm-go").textContent).toContain("Overwrite 1 file");
  });

  it("names the correct count for a multi-file overwrite plan", async () => {
    await planOverwriteReady(["/repo/a.jpg", "/repo/b.jpg", "/repo/c.jpg"]);
    await fireEvent.click(applyButton());
    expect(screen.getByTestId("overwrite-confirm-go").textContent).toContain("Overwrite 3 files");
  });

  it("Cancel on the confirm panel backs out without executing or checkpointing, and Apply returns", async () => {
    await planOverwriteReady();
    await fireEvent.click(applyButton());
    expect(screen.getByTestId("overwrite-confirm")).toBeTruthy();

    await fireEvent.click(screen.getByTestId("overwrite-confirm-cancel"));
    expect(screen.queryByTestId("overwrite-confirm")).toBeNull();
    expect(execCalls).toHaveLength(0);
    expect(checkpointCalls).toHaveLength(0);
    expect(applyButton().disabled).toBe(false); // the normal action row is back, not stuck
  });

  it("Escape backs out of the confirm panel first, without closing the whole dialog", async () => {
    await planOverwriteReady();
    await fireEvent.click(applyButton());
    expect(screen.getByTestId("overwrite-confirm")).toBeTruthy();

    await fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByTestId("overwrite-confirm")).toBeNull();
    expect(screen.queryByRole("dialog")).toBeTruthy(); // dialog itself is still open
  });

  it("confirming takes a best-effort pre-write checkpoint of the affected folder, then executes", async () => {
    await planOverwriteReady(["/repo/pics/a.jpg", "/repo/pics/b.jpg"]);
    await fireEvent.click(applyButton());
    await fireEvent.click(screen.getByTestId("overwrite-confirm-go"));
    await settle();

    // One checkpoint per distinct affected folder — both files share "/repo/pics", so exactly one call.
    expect(checkpointCalls).toEqual([["/repo/pics", "Before batch media overwrite"]]);
    expect(execCalls).toHaveLength(1);
  });

  it("a pre-write checkpoint failure never blocks the already-confirmed write", async () => {
    checkpointBehavior = "reject";
    await planOverwriteReady();
    await fireEvent.click(applyButton());
    await fireEvent.click(screen.getByTestId("overwrite-confirm-go"));
    await settle();

    expect(checkpointCalls).toHaveLength(1);
    expect(execCalls).toHaveLength(1); // still proceeds — checkpoint is a bonus net, not a gate
  });

  it("shows a visible warning naming the folder when the pre-write checkpoint fails, and holds the " +
    "dialog open until acknowledged — even on an otherwise-clean run with nothing skipped", async () => {
    checkpointBehavior = "reject";
    const { component } = await planOverwriteReady(["/repo/pics/a.jpg"]);
    const onApply = vi.fn();
    component.$on("apply", (e: CustomEvent) => onApply(e.detail));

    await fireEvent.click(applyButton());
    await fireEvent.click(screen.getByTestId("overwrite-confirm-go"));
    expect(checkpointCalls).toEqual([["/repo/pics", "Before batch media overwrite"]]);
    expect(execCalls).toHaveLength(1);

    // A clean execute — nothing skipped — would normally close the dialog immediately. The checkpoint
    // failure alone must still hold it open so the warning is actually seen.
    execCalls[0].resolve({ written: 1, skipped: [] });
    await settle();

    expect(onApply).not.toHaveBeenCalled(); // held open, not auto-dispatched/closed
    // CPE-1599 UAT follow-up: an outright checkpoint failure keeps the BLUNT pre-CPE-1599 wording — this
    // folder has NO recovery net at all, and must never read like a minor/partial gap.
    const warning = screen.getByTestId("checkpoint-warning");
    expect(warning.textContent).toContain("No checkpoint was taken");
    expect(warning.textContent).toContain("pics"); // names the affected folder, not just a generic notice
    expect(screen.queryByTestId("checkpoint-warning-partial")).toBeNull(); // NOT the softer partial panel
    expect(screen.getByTestId("batch-media-done")).toBeTruthy();

    // "Done" still hands the (unmodified) report to the parent once the user has acknowledged the warning —
    // and carries the failed folder out with it, so the app-level notice can repeat the warning even if the
    // user dismisses this panel on reflex (CPE-1590 review round 2).
    await fireEvent.click(screen.getByTestId("batch-media-done"));
    expect(onApply).toHaveBeenCalledWith({
      report: { written: 1, skipped: [] },
      checkpointFailures: ["/repo/pics"],
      checkpointPartial: [],
    });
  });

  it("carries the checkpoint failure out on the cancel path too — Escape must not discard it unread", async () => {
    checkpointBehavior = "reject";
    const { component } = await planOverwriteReady(["/repo/pics/a.jpg"]);
    const onCancel = vi.fn();
    component.$on("cancel", (e: CustomEvent) => onCancel(e.detail));

    await fireEvent.click(applyButton());
    await fireEvent.click(screen.getByTestId("overwrite-confirm-go"));
    execCalls[0].resolve({ written: 1, skipped: [] });
    await settle();

    // The warning is showing; the user hits Escape out of habit rather than reading it.
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledWith({ checkpointFailures: ["/repo/pics"], checkpointPartial: [] });
  });

  it("does NOT show either checkpoint warning, and closes normally, when the pre-write checkpoint succeeds", async () => {
    const { component } = await planOverwriteReady(["/repo/a.jpg"]);
    const onApply = vi.fn();
    component.$on("apply", (e: CustomEvent) => onApply(e.detail));

    await fireEvent.click(applyButton());
    await fireEvent.click(screen.getByTestId("overwrite-confirm-go"));
    expect(checkpointCalls).toHaveLength(1); // took the checkpoint, and it succeeded (default behavior)
    execCalls[0].resolve({ written: 1, skipped: [] });
    await settle();

    expect(screen.queryByTestId("checkpoint-warning")).toBeNull();
    expect(screen.queryByTestId("checkpoint-warning-partial")).toBeNull();
    expect(screen.queryByTestId("batch-media-done")).toBeNull(); // not held open
    expect(onApply).toHaveBeenCalledWith({
      report: { written: 1, skipped: [] },
      checkpointFailures: [],
      checkpointPartial: [],
    }); // closed immediately
  });

  it("editing the op list after opening the confirm panel dismisses it (no stale confirm)", async () => {
    await planOverwriteReady();
    await fireEvent.click(applyButton());
    expect(screen.getByTestId("overwrite-confirm")).toBeTruthy();

    await fireEvent.click(screen.getAllByLabelText("Remove operation")[0]);
    expect(screen.queryByTestId("overwrite-confirm")).toBeNull();
    expect(execCalls).toHaveLength(0);
  });

  it("shows a persistent hint under the checkbox once unchecked, independent of the current op combo", async () => {
    render(BatchMediaDialog, { paths: ["/repo/a.jpg"] });
    expect(screen.queryByTestId("overwrite-hint")).toBeNull();
    await fireEvent.click(screen.getByLabelText(/Write to new files/i));
    expect(screen.getByTestId("overwrite-hint")).toBeTruthy();
    await fireEvent.click(screen.getByLabelText(/Write to new files/i));
    expect(screen.queryByTestId("overwrite-hint")).toBeNull();
  });

  it("the non-destructive / distinct-output path never shows the confirm panel (no new friction)", async () => {
    render(BatchMediaDialog, { paths: ["/repo/a.jpg"] });
    await fireEvent.click(addButton()); // resize, default non_destructive: true
    await vi.advanceTimersByTimeAsync(200);
    expect(planCalls[0].args.job.non_destructive).toBe(true);
    planCalls[0].resolve([{ input: "/repo/a.jpg", output: "/repo/a-1024.jpg", summary: "resize" }]);
    await settle();

    await fireEvent.click(applyButton());
    expect(screen.queryByTestId("overwrite-confirm")).toBeNull();
    expect(execCalls).toHaveLength(1); // runs immediately, exactly as before this ticket
    expect(checkpointCalls).toHaveLength(0); // no overwrite ⇒ no pre-write checkpoint either
  });

  // ---- CPE-1599: confirmed_overwrite threading -------------------------------------------------------

  it("sends confirmed_overwrite: true ONLY after the confirm panel's own button is clicked", async () => {
    await planOverwriteReady(["/repo/a.jpg"]);
    await fireEvent.click(applyButton());
    expect(execCalls).toHaveLength(0); // first Apply click never reaches the backend

    await fireEvent.click(screen.getByTestId("overwrite-confirm-go"));
    await settle();

    expect(execCalls).toHaveLength(1);
    expect(execCalls[0].args.job.confirmed_overwrite).toBe(true);
    expect(execCalls[0].args.job.non_destructive).toBe(false);
  });

  it("a non-overwrite plan always sends confirmed_overwrite: false — the flag is never set outside the confirm path", async () => {
    render(BatchMediaDialog, { paths: ["/repo/a.jpg"] });
    await fireEvent.click(addButton()); // resize, default non_destructive: true — no in-place items
    await vi.advanceTimersByTimeAsync(200);
    planCalls[0].resolve([{ input: "/repo/a.jpg", output: "/repo/a-1024.jpg", summary: "resize" }]);
    await settle();

    await fireEvent.click(applyButton());
    expect(execCalls).toHaveLength(1);
    expect(execCalls[0].args.job.confirmed_overwrite).toBe(false);
  });

  // ---- CPE-1599: the pre-overwrite checkpoint's `skipped` field is inspected, not discarded -----------

  it("surfaces a checkpoint that SUCCEEDED but left file(s) uncaptured on its OWN, less-alarming panel — never the outright-failure one", async () => {
    checkpointBehavior = "skipped";
    const { component } = await planOverwriteReady(["/repo/pics/a.jpg"]);
    const onApply = vi.fn();
    component.$on("apply", (e: CustomEvent) => onApply(e.detail));

    await fireEvent.click(applyButton());
    await fireEvent.click(screen.getByTestId("overwrite-confirm-go"));
    expect(checkpointCalls).toEqual([["/repo/pics", "Before batch media overwrite"]]);
    expect(execCalls).toHaveLength(1); // the checkpoint call itself resolved — proceeds like a success

    execCalls[0].resolve({ written: 1, skipped: [] });
    await settle();

    // Held open exactly like a hard checkpoint failure, even though `checkpoint_create` never rejected.
    expect(onApply).not.toHaveBeenCalled();
    expect(screen.queryByTestId("checkpoint-warning")).toBeNull(); // NOT the outright-failure panel
    const warning = screen.getByTestId("checkpoint-warning-partial");
    expect(warning.textContent).toContain("didn't fully cover");
    expect(warning.textContent).toContain("pics");
    expect(warning.textContent).toContain("1 file"); // names the actual uncaptured count

    await fireEvent.click(screen.getByTestId("batch-media-done"));
    expect(onApply).toHaveBeenCalledWith({
      report: { written: 1, skipped: [] },
      checkpointFailures: [],
      checkpointPartial: [{ dir: "/repo/pics", skippedCount: 1 }],
    });
  });

  it("carries a partial checkpoint out on the cancel path too — Escape must not discard it unread", async () => {
    checkpointBehavior = "skipped";
    const { component } = await planOverwriteReady(["/repo/pics/a.jpg"]);
    const onCancel = vi.fn();
    component.$on("cancel", (e: CustomEvent) => onCancel(e.detail));

    await fireEvent.click(applyButton());
    await fireEvent.click(screen.getByTestId("overwrite-confirm-go"));
    execCalls[0].resolve({ written: 1, skipped: [] });
    await settle();

    await fireEvent.keyDown(window, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledWith({
      checkpointFailures: [],
      checkpointPartial: [{ dir: "/repo/pics", skippedCount: 1 }],
    });
  });
});
