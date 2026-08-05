import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import MetadataStudioDialog from "./MetadataStudioDialog.svelte";
import type { DirEntry } from "../types";

// CPE-1325: metadata write-back mutates a file in place with no undo, so `save()` takes a best-effort
// `checkpointCreate` of the target folder BEFORE `metadataWrite` runs — mirroring the pre-move checkpoint
// already shipped in SimilarImagesDialog. This must be attempted exactly ONCE per save (single file or
// applyToAll batch), and a rejected checkpoint must never block the write.
let checkpointCalls: Array<[string, string]> = [];
let writeCalls: string[] = [];
let writeEditsCalls: unknown[] = [];
let checkpointBehavior: "ok" | "reject" = "ok";

const field = { group: "id3", key: "Title", value: "Old Title", editable: true };
// CPE-1326: a richer field set (two editable, one read-only) for the batch-op tests below.
const fields2 = [
  { group: "id3", key: "Title", value: "Old Title", editable: true },
  { group: "id3", key: "Artist", value: "Old Artist", editable: true },
  { group: "id3", key: "Duration", value: "3:45", editable: false },
];
let metaFields: Array<{ group: string; key: string; value: string; editable: boolean }> = [field];

// `commands.*` (bindings.gen.ts) wraps whatever this raw `invoke` resolves to into `{status:"ok",
// data: ...}` itself (or catches a throw into `{status:"error", ...}`) — so the mock below returns/throws
// RAW values, mirroring the real Tauri IPC boundary, not the already-wrapped `Result`.
const invoke = vi.fn(async (cmd: string, args?: any) => {
  if (cmd === "metadata_read") {
    return metaFields;
  }
  if (cmd === "metadata_writable") {
    return true;
  }
  if (cmd === "checkpoint_create") {
    checkpointCalls.push([args.root, args.label]);
    if (checkpointBehavior === "reject") throw new Error("checkpoint boom");
    return { checkpoint: { manifest_id: "cp1", label: args.label }, new_blobs: 1, reused_blobs: 0, skipped: [] };
  }
  if (cmd === "metadata_write") {
    writeCalls.push(args.path);
    writeEditsCalls.push(args.edits);
    return [{ ...field, value: "New Title" }];
  }
  return null;
});

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

function makeEntry(path: string): DirEntry {
  const name = path.split("/").pop() ?? path;
  return { name, path, is_dir: false, size: 10, modified: null, extension: "mp3", hidden: false, is_symlink: false };
}

beforeEach(() => {
  invoke.mockClear();
  checkpointCalls = [];
  writeCalls = [];
  writeEditsCalls = [];
  checkpointBehavior = "ok";
  metaFields = [field];
});

describe("MetadataStudioDialog checkpoint-before-save (CPE-1325)", () => {
  it("takes a best-effort checkpoint of the file's folder BEFORE metadataWrite, once, for a single-file save", async () => {
    render(MetadataStudioDialog, { entries: [makeEntry("/repo/music/song.mp3")] });

    const input = await screen.findByDisplayValue("Old Title");
    await fireEvent.input(input, { target: { value: "New Title" } });

    await fireEvent.click(screen.getByText("Save"));

    await waitFor(() => expect(writeCalls).toEqual(["/repo/music/song.mp3"]));
    expect(checkpointCalls).toEqual([["/repo/music", "Before metadata edit"]]);

    // Ordering: the checkpoint call must land before the write call in the mock's invocation order.
    const cpIdx = invoke.mock.calls.findIndex((c) => c[0] === "checkpoint_create");
    const writeIdx = invoke.mock.calls.findIndex((c) => c[0] === "metadata_write");
    expect(cpIdx).toBeGreaterThanOrEqual(0);
    expect(cpIdx).toBeLessThan(writeIdx);
  });

  it("does NOT block the write when the checkpoint is rejected (best-effort, non-blocking)", async () => {
    checkpointBehavior = "reject";
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    render(MetadataStudioDialog, { entries: [makeEntry("/repo/music/song.mp3")] });

    const input = await screen.findByDisplayValue("Old Title");
    await fireEvent.input(input, { target: { value: "New Title" } });
    await fireEvent.click(screen.getByText("Save"));

    // The write still happens even though the checkpoint failed.
    await waitFor(() => expect(writeCalls).toEqual(["/repo/music/song.mp3"]));
    expect(checkpointCalls).toEqual([["/repo/music", "Before metadata edit"]]);
    // No blocking error is surfaced from the failed checkpoint — the save itself still reports success.
    await waitFor(() => expect(screen.getByText(/Saved/)).toBeTruthy());
    errSpy.mockRestore();
  });

  it("checkpoints ONCE before a whole applyToAll batch, not per-file", async () => {
    render(MetadataStudioDialog, {
      entries: [makeEntry("/repo/music/a.mp3"), makeEntry("/repo/music/b.mp3")],
    });

    const input = await screen.findByDisplayValue("Old Title");
    await fireEvent.input(input, { target: { value: "New Title" } });

    const applyAll = screen.getByLabelText(/Apply to all/);
    await fireEvent.click(applyAll);

    await fireEvent.click(screen.getByText("Save"));

    await waitFor(() => expect(writeCalls).toEqual(["/repo/music/a.mp3", "/repo/music/b.mp3"]));
    // Exactly one checkpoint call for the whole batch.
    expect(checkpointCalls).toEqual([["/repo/music", "Before metadata edit"]]);
  });
});

// CPE-1326: two batch conveniences, both staging pending edits (built by `buildMetaEdits`/`stageFieldEdits`
// in metaEdits.ts) into the SAME existing pending-edits + Save flow exercised above — so the CPE-1325
// checkpoint-before-save still fires and nothing bypasses `metadataWrite`.
describe("MetadataStudioDialog batch ops (CPE-1326)", () => {
  it("only offers the batch ops once more than one file is selected", async () => {
    render(MetadataStudioDialog, { entries: [makeEntry("/repo/music/song.mp3")] });
    await screen.findByDisplayValue("Old Title");
    expect(screen.queryByText("Strip editable metadata")).toBeNull();
    expect(screen.queryByText("Copy from first")).toBeNull();
  });

  it("'Strip editable metadata' stages a clear for every editable field (skipping read-only ones) and applies it per the current Apply-to-all scope", async () => {
    metaFields = fields2;
    render(MetadataStudioDialog, {
      entries: [makeEntry("/repo/music/a.mp3"), makeEntry("/repo/music/b.mp3")],
    });
    await screen.findByDisplayValue("Old Title");

    // Broaden scope to the whole selection before stripping.
    await fireEvent.click(screen.getByLabelText(/Apply to all/));
    await fireEvent.click(screen.getByText("Strip editable metadata"));

    // Staged edits show up immediately in the editable inputs (Duration has no input — it's read-only).
    expect((screen.getByLabelText("Title") as HTMLInputElement).value).toBe("");
    expect((screen.getByLabelText("Artist") as HTMLInputElement).value).toBe("");

    await fireEvent.click(screen.getByText("Save"));

    await waitFor(() => expect(writeCalls).toEqual(["/repo/music/a.mp3", "/repo/music/b.mp3"]));
    const expectedEdits = [
      { edit: "clear", group: "id3", key: "Title" },
      { edit: "clear", group: "id3", key: "Artist" },
    ];
    expect(writeEditsCalls).toEqual([expectedEdits, expectedEdits]);
  });

  it("'Copy from first' stages the primary's own editable values, forces Apply-to-all on, and writes them to every selected file", async () => {
    metaFields = fields2;
    render(MetadataStudioDialog, {
      entries: [makeEntry("/repo/music/a.mp3"), makeEntry("/repo/music/b.mp3")],
    });
    await screen.findByDisplayValue("Old Title");

    const applyAll = screen.getByLabelText(/Apply to all/) as HTMLInputElement;
    expect(applyAll.checked).toBe(false);

    await fireEvent.click(screen.getByText("Copy from first"));

    // "Copy from first" only makes sense across the whole selection, so it turns Apply-to-all on itself.
    expect(applyAll.checked).toBe(true);
    // The primary's own inputs are unchanged (copying it onto itself is a no-op value-wise).
    expect(screen.getByDisplayValue("Old Title")).toBeTruthy();
    expect(screen.getByDisplayValue("Old Artist")).toBeTruthy();

    await fireEvent.click(screen.getByText("Save"));

    await waitFor(() => expect(writeCalls).toEqual(["/repo/music/a.mp3", "/repo/music/b.mp3"]));
    const expectedEdits = [
      { edit: "set", group: "id3", key: "Title", value: "Old Title" },
      { edit: "set", group: "id3", key: "Artist", value: "Old Artist" },
    ];
    expect(writeEditsCalls).toEqual([expectedEdits, expectedEdits]);
  });
});

// CPE-1327: per-field revert + "Reset all edits" — pure client-side discards of staged edits. Neither
// must ever call metadataWrite or checkpointCreate (they never save or checkpoint), and per-field revert
// must leave every OTHER field's pending edit untouched (reuses the CPE-1326 explicit-`edited`
// reactivity pattern, so the DOM input re-renders from the fresh `edited` reference).
describe("MetadataStudioDialog per-field revert + reset-all edits (CPE-1327)", () => {
  it("shows a revert control only on a dirty field, and clicking it restores just that field", async () => {
    metaFields = fields2;
    render(MetadataStudioDialog, { entries: [makeEntry("/repo/music/song.mp3")] });
    await screen.findByDisplayValue("Old Title");

    // No revert control before any edit is made.
    expect(screen.queryByLabelText(/Revert Title/)).toBeNull();
    expect(screen.queryByLabelText(/Revert Artist/)).toBeNull();

    const titleInput = screen.getByLabelText("Title") as HTMLInputElement;
    const artistInput = screen.getByLabelText("Artist") as HTMLInputElement;
    await fireEvent.input(titleInput, { target: { value: "New Title" } });
    await fireEvent.input(artistInput, { target: { value: "New Artist" } });

    // Both fields are now dirty and each shows its own revert control.
    expect(titleInput.value).toBe("New Title");
    expect(artistInput.value).toBe("New Artist");
    const revertTitle = screen.getByLabelText(/Revert Title/);
    expect(screen.getByLabelText(/Revert Artist/)).toBeTruthy();

    await fireEvent.click(revertTitle);

    // Title is back to its loaded value and its revert control disappears; Artist's edit is untouched.
    expect(titleInput.value).toBe("Old Title");
    expect(screen.queryByLabelText(/Revert Title/)).toBeNull();
    expect(artistInput.value).toBe("New Artist");
    expect(screen.getByLabelText(/Revert Artist/)).toBeTruthy();

    // No write or checkpoint was triggered by the revert.
    expect(writeCalls).toEqual([]);
    expect(checkpointCalls).toEqual([]);
  });

  it("'Reset all edits' clears every pending edit back to loaded values, across the whole applyToAll selection, without writing to disk", async () => {
    metaFields = fields2;
    render(MetadataStudioDialog, {
      entries: [makeEntry("/repo/music/a.mp3"), makeEntry("/repo/music/b.mp3")],
    });
    await screen.findByDisplayValue("Old Title");

    await fireEvent.click(screen.getByLabelText(/Apply to all/));
    await fireEvent.click(screen.getByText("Strip editable metadata"));

    const titleInput = screen.getByLabelText("Title") as HTMLInputElement;
    const artistInput = screen.getByLabelText("Artist") as HTMLInputElement;
    expect(titleInput.value).toBe("");
    expect(artistInput.value).toBe("");

    await fireEvent.click(screen.getByText("Reset all edits"));

    // Every field is restored to its loaded value.
    expect(titleInput.value).toBe("Old Title");
    expect(artistInput.value).toBe("Old Artist");
    expect(screen.queryByLabelText(/Revert Title/)).toBeNull();
    expect(screen.queryByLabelText(/Revert Artist/)).toBeNull();

    // Save is now disabled (nothing pending) and neither metadataWrite nor checkpointCreate ever fired.
    expect((screen.getByText("Save") as HTMLButtonElement).disabled).toBe(true);
    expect(writeCalls).toEqual([]);
    expect(checkpointCalls).toEqual([]);
  });

  it("does not offer 'Reset all edits' when there are no pending edits", async () => {
    render(MetadataStudioDialog, { entries: [makeEntry("/repo/music/song.mp3")] });
    await screen.findByDisplayValue("Old Title");
    expect(screen.queryByText("Reset all edits")).toBeNull();
  });
});
