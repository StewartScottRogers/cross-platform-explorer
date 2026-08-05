import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import DeclutterDialog from "./DeclutterDialog.svelte";

// The dialog wraps the modest collect-to-vec `organize_clutter` command (CPE-1329, epic CPE-979) — a
// plain awaited `commands.*` call through the typed client, not a streaming Channel. The mock resolves/
// rejects the raw backend return value; the generated bindings client wraps it into the `Result` shape.
// Cleanup mirrors NearDuplicatesDialog's (CPE-1324) mock for `delete_to_trash` / `checkpoint_create`,
// including the CPE-1328 non-Error ("Err(String)") rejection shape.
let calls: Array<{ cmd: string; args: unknown }> = [];
let responder: ((cmd: string, args: unknown) => unknown) | null = null;
let trashCalls: string[][] = [];
let checkpointCalls: Array<[string, string]> = [];
let checkpointBehavior: "ok" | "reject" | "reject-string" = "ok";

const invoke = vi.fn(async (cmd: string, args?: any) => {
  calls.push({ cmd, args });
  if (cmd === "delete_to_trash") {
    trashCalls.push(args.paths);
    return [];
  }
  if (cmd === "checkpoint_create") {
    checkpointCalls.push([args.root, args.label]);
    if (checkpointBehavior === "reject") throw new Error("disk full");
    // eslint-disable-next-line no-throw-literal -- deliberately a non-Error rejection (Err(String) shape)
    if (checkpointBehavior === "reject-string") throw "disk full (domain error)";
    return { id: "cp1", label: args.label };
  }
  if (!responder) throw new Error(`no responder set for ${cmd}`);
  return responder(cmd, args);
});

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

beforeEach(() => {
  invoke.mockClear();
  calls = [];
  responder = null;
  trashCalls = [];
  checkpointCalls = [];
  checkpointBehavior = "ok";
});

describe("DeclutterDialog — scan (CPE-1329)", () => {
  it("does not scan until the user clicks Scan, then calls organize_clutter with dir", async () => {
    responder = () => [];
    render(DeclutterDialog, { root: "/repo" });
    expect(invoke).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByTestId("dc-scan-btn"));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("organize_clutter", { dir: "/repo" }));
  });

  it("shows the empty state when nothing is flagged, without a stuck spinner", async () => {
    responder = () => [];
    render(DeclutterDialog, { root: "/repo" });
    await fireEvent.click(screen.getByTestId("dc-scan-btn"));

    await waitFor(() => expect(screen.getByTestId("dc-none")).toBeTruthy());
    expect(screen.getByText(/No clutter found/)).toBeTruthy();
  });

  it("surfaces a backend error instead of hanging in the loading state", async () => {
    responder = () => {
      throw "not a folder"; // a plain (non-Error) rejection — mirrors a Tauri command's Err(String)
    };
    render(DeclutterDialog, { root: "/repo" });
    await fireEvent.click(screen.getByTestId("dc-scan-btn"));

    await waitFor(() => expect(screen.getByText("not a folder")).toBeTruthy());
    expect(screen.getByTestId("dc-rescan-btn")).toBeTruthy();
  });

  it("rescanning replaces the previous results rather than appending to them", async () => {
    let callIndex = 0;
    responder = () => {
      callIndex += 1;
      if (callIndex === 1) return [{ name: "old.tmp", reason: "temp_or_partial" }];
      return [{ name: "new.tmp", reason: "temp_or_partial" }];
    };
    render(DeclutterDialog, { root: "/repo" });
    await fireEvent.click(screen.getByTestId("dc-scan-btn"));
    await waitFor(() => expect(screen.getByText("old.tmp")).toBeTruthy());

    await fireEvent.click(screen.getByTestId("dc-rescan-btn"));
    await waitFor(() => expect(screen.getByText("new.tmp")).toBeTruthy());
    expect(screen.queryByText("old.tmp")).toBeNull();
    expect(invoke).toHaveBeenCalledTimes(2);
  });
});

describe("DeclutterDialog — findings grouped by reason (CPE-1329)", () => {
  it("renders findings grouped under their human-labelled reason, most-definitive first", async () => {
    responder = () => [
      { name: "notes.txt.bak", reason: "backup" },
      { name: "empty.log", reason: "zero_byte" },
      { name: "setup.exe", reason: "installer" },
      { name: "movie.mp4.part", reason: "temp_or_partial" },
    ];
    render(DeclutterDialog, { root: "/repo" });
    await fireEvent.click(screen.getByTestId("dc-scan-btn"));

    const groups = await screen.findAllByTestId("dc-group");
    expect(groups).toHaveLength(4);
    // Zero-byte is the most-definitive reason and renders first (mirrors the backend's own check order).
    expect(groups[0].textContent).toContain("Empty file");
    expect(groups[0].textContent).toContain("empty.log");
    expect(groups[1].textContent).toContain("Installer");
    expect(groups[1].textContent).toContain("setup.exe");
    expect(groups[2].textContent).toContain("Temporary / partial download");
    expect(groups[2].textContent).toContain("movie.mp4.part");
    expect(groups[3].textContent).toContain("Backup / leftover");
    expect(groups[3].textContent).toContain("notes.txt.bak");
    expect(screen.getByText(/4 items found/)).toBeTruthy();
  });

  it("joins the bare filename the backend returns with root to build each row's path", async () => {
    responder = () => [{ name: "empty.log", reason: "zero_byte" }];
    const { component } = render(DeclutterDialog, { root: "/repo" });
    const navigated: string[] = [];
    component.$on("navigate", (e: CustomEvent<string>) => navigated.push(e.detail));
    await fireEvent.click(screen.getByTestId("dc-scan-btn"));
    await waitFor(() => expect(screen.getByTestId("dc-row")).toBeTruthy());

    await fireEvent.click(screen.getByText("empty.log"));
    expect(navigated).toEqual(["/repo/empty.log"]);
  });
});

describe("DeclutterDialog — selection gating, no keeper guard (CPE-1329)", () => {
  it("SAFETY: nothing is pre-selected and Move to Bin is disabled at zero selection", async () => {
    responder = () => [{ name: "empty.log", reason: "zero_byte" }];
    render(DeclutterDialog, { root: "/repo" });
    await fireEvent.click(screen.getByTestId("dc-scan-btn"));
    await waitFor(() => expect(screen.getByTestId("dc-row")).toBeTruthy());

    const moveBtn = screen.getByTestId("dc-move-btn") as HTMLButtonElement;
    expect(moveBtn.disabled).toBe(true);
    const box = screen.getByRole("checkbox") as HTMLInputElement;
    expect(box.checked).toBe(false);
  });

  it("unlike near-dup groups, selecting EVERY finding is allowed (no per-group keeper guard)", async () => {
    responder = () => [
      { name: "empty.log", reason: "zero_byte" },
      { name: "setup.exe", reason: "installer" },
    ];
    render(DeclutterDialog, { root: "/repo" });
    await fireEvent.click(screen.getByTestId("dc-scan-btn"));
    await waitFor(() => expect(screen.getAllByTestId("dc-row")).toHaveLength(2));

    const boxes = screen.getAllByRole("checkbox") as HTMLInputElement[];
    await fireEvent.click(boxes[0]);
    await fireEvent.click(boxes[1]);

    const moveBtn = screen.getByTestId("dc-move-btn") as HTMLButtonElement;
    await waitFor(() => expect(moveBtn.disabled).toBe(false));
    expect(screen.getByText("Move 2 to Recycle Bin")).toBeTruthy();
  });
});

describe("DeclutterDialog — safe move-to-bin (CPE-1329)", () => {
  it("checkpoints first, then trashes ONLY the selected paths, then prunes them from the list", async () => {
    responder = () => [
      { name: "empty.log", reason: "zero_byte" },
      { name: "setup.exe", reason: "installer" },
    ];
    render(DeclutterDialog, { root: "/repo" });
    await fireEvent.click(screen.getByTestId("dc-scan-btn"));
    await waitFor(() => expect(screen.getAllByTestId("dc-row")).toHaveLength(2));

    const boxes = screen.getAllByRole("checkbox") as HTMLInputElement[];
    await fireEvent.click(boxes[0]); // select only empty.log
    await fireEvent.click(await screen.findByText("Move 1 to Recycle Bin"));

    await waitFor(() => expect(trashCalls).toEqual([["/repo/empty.log"]]));
    // Recoverable Bin only — never a permanent delete.
    expect(invoke).not.toHaveBeenCalledWith("delete_permanently", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("delete", expect.anything());
    // A checkpoint was taken first, scoped to the folder.
    expect(checkpointCalls).toEqual([["/repo", "Before removing clutter"]]);
    const checkpointIdx = calls.findIndex((c) => c.cmd === "checkpoint_create");
    const trashIdx = calls.findIndex((c) => c.cmd === "delete_to_trash");
    expect(checkpointIdx).toBeGreaterThanOrEqual(0);
    expect(trashIdx).toBeGreaterThan(checkpointIdx);
    // The removed finding is pruned; the untouched one (setup.exe) remains.
    await waitFor(() => expect(screen.queryByText("empty.log")).toBeNull());
    expect(screen.getByText("setup.exe")).toBeTruthy();
  });

  it("a checkpoint failure (thrown Error) does not block the (already recoverable) trash move", async () => {
    checkpointBehavior = "reject";
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    responder = () => [{ name: "empty.log", reason: "zero_byte" }];
    render(DeclutterDialog, { root: "/repo" });
    await fireEvent.click(screen.getByTestId("dc-scan-btn"));
    await waitFor(() => expect(screen.getByTestId("dc-row")).toBeTruthy());

    await fireEvent.click(screen.getByRole("checkbox"));
    await fireEvent.click(await screen.findByText("Move 1 to Recycle Bin"));

    // Checkpoint failed but the trash move still went through — a checkpoint is a bonus, not a gate.
    await waitFor(() => expect(trashCalls).toEqual([["/repo/empty.log"]]));
    await waitFor(() => expect(screen.getByTestId("dc-none")).toBeTruthy());
    expect(errSpy).toHaveBeenCalled();
    errSpy.mockRestore();
  });

  it(
    "CPE-1328: a checkpoint that resolves to a non-throwing {status:'error'} envelope (Err(String)) " +
      "still lets the trash move proceed — non-blocking preserved for the case a bare `await` used to miss",
    async () => {
      checkpointBehavior = "reject-string";
      const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
      responder = () => [{ name: "empty.log", reason: "zero_byte" }];
      render(DeclutterDialog, { root: "/repo" });
      await fireEvent.click(screen.getByTestId("dc-scan-btn"));
      await waitFor(() => expect(screen.getByTestId("dc-row")).toBeTruthy());

      await fireEvent.click(screen.getByRole("checkbox"));
      await fireEvent.click(await screen.findByText("Move 1 to Recycle Bin"));

      // The trash move still happens even though the checkpoint's envelope was an error.
      await waitFor(() => expect(trashCalls).toEqual([["/repo/empty.log"]]));
      await waitFor(() => expect(screen.getByTestId("dc-none")).toBeTruthy());
      // The failure is surfaced to the console for diagnostics rather than silently vanishing.
      expect(errSpy).toHaveBeenCalled();
      errSpy.mockRestore();
    },
  );

  it("selection resets on rescan", async () => {
    responder = () => [{ name: "empty.log", reason: "zero_byte" }];
    render(DeclutterDialog, { root: "/repo" });
    await fireEvent.click(screen.getByTestId("dc-scan-btn"));
    await waitFor(() => expect(screen.getByTestId("dc-row")).toBeTruthy());

    await fireEvent.click(screen.getByRole("checkbox"));
    await waitFor(() => expect(screen.getByText("Move 1 to Recycle Bin")).toBeTruthy());

    await fireEvent.click(screen.getByTestId("dc-rescan-btn"));
    await waitFor(() => expect(screen.getByTestId("dc-row")).toBeTruthy());
    expect(screen.getByText("Move 0 to Recycle Bin")).toBeTruthy();
    const moveBtn = screen.getByTestId("dc-move-btn") as HTMLButtonElement;
    expect(moveBtn.disabled).toBe(true);
  });
});
