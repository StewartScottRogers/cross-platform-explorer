/**
 * Component tests for the in-app three-way conflict resolver (CPE-496 / CPE-1391). ConflictDialog wires
 * five typed `commands.forge*` calls (bindings.gen → ./invoke → @tauri-apps/api/core), none of which had
 * jsdom coverage before this spec. Mock the underlying `@tauri-apps/api/core` `invoke` (same recipe as
 * DuplicatesDialog.test.ts / AttributesDialog.test.ts) since the typed client is a thin dispatcher over it.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import ConflictDialog from "./ConflictDialog.svelte";

interface ConflictFile { path: string; code: string; label: string }
interface Versions { base: string | null; ours: string | null; theirs: string | null; merged: string | null; truncated: boolean }

const FILE_A: ConflictFile = { path: "a.txt", code: "both_modified", label: "both modified" };
const FILE_B: ConflictFile = { path: "sub/b.txt", code: "both_modified", label: "both modified" };

const VERSIONS: Record<string, Versions> = {
  "a.txt": { base: "base-a", ours: "ours-a", theirs: "theirs-a", merged: "<<<<<<< ours\nours-a\n=======\ntheirs-a\n>>>>>>> theirs", truncated: false },
  // No `base` (add/add-style conflict) and no `merged` — exercises the `merged ?? ours ?? theirs` fallback.
  "sub/b.txt": { base: null, ours: "ours-b", theirs: "theirs-b", merged: null, truncated: false },
};

// Mutable server-side state the mock reads/writes, so `forge_resolve_file` can realistically drop a file
// off the next `forge_conflict_state` response (mirrors the real backend: staging removes it from the
// unmerged list) and drive the component's "auto-select the next file" behavior.
let operation = "merge";
let files: ConflictFile[] = [];
let stateError: string | null = null;

const invoke = vi.fn(async (cmd: string, args?: any) => {
  switch (cmd) {
    case "forge_conflict_state":
      if (stateError) throw new Error(stateError);
      return { operation, files };
    case "forge_conflict_versions":
      return VERSIONS[args.file];
    case "forge_resolve_file":
      files = files.filter((f) => f.path !== args.file);
      return null;
    case "forge_conflict_continue":
      return "Rebase continued.";
    case "forge_conflict_abort":
      return "Aborted — working tree restored.";
    default:
      throw new Error(`unexpected command: ${cmd}`);
  }
});
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: unknown) => invoke(cmd, args),
}));

const resTextarea = () => screen.getByRole("textbox") as HTMLTextAreaElement;
const callsFor = (cmd: string) => invoke.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  operation = "merge";
  files = [FILE_A, FILE_B];
  stateError = null;
  invoke.mockClear();
});

describe("ConflictDialog (CPE-496 / CPE-1391)", () => {
  it("loads conflict state on mount, lists files, and auto-selects the first file's versions", async () => {
    render(ConflictDialog, { path: "/repo" });

    await waitFor(() => expect(screen.getByTitle("a.txt")).toBeTruthy());
    expect(invoke).toHaveBeenCalledWith("forge_conflict_state", { path: "/repo" });
    expect(screen.getByText("2 unresolved")).toBeTruthy();
    expect(screen.getByTitle("sub/b.txt")).toBeTruthy();

    // Auto-selected the first file and fetched its three versions.
    await waitFor(() => expect(callsFor("forge_conflict_versions")).toHaveLength(1));
    expect(invoke).toHaveBeenCalledWith("forge_conflict_versions", { path: "/repo", file: "a.txt" });
    // Resolution seeds from the working-tree `merged` content (markers included) when present.
    await waitFor(() => expect(resTextarea().value).toBe(VERSIONS["a.txt"].merged));
  });

  it("clicking another file in the list loads its versions and reseeds the resolution", async () => {
    render(ConflictDialog, { path: "/repo" });
    await waitFor(() => expect(resTextarea().value).toBe(VERSIONS["a.txt"].merged));

    await fireEvent.click(screen.getByTitle("sub/b.txt"));
    expect(invoke).toHaveBeenCalledWith("forge_conflict_versions", { path: "/repo", file: "sub/b.txt" });
    // b.txt has no `merged`, so resolution falls back to `ours`.
    await waitFor(() => expect(resTextarea().value).toBe(VERSIONS["sub/b.txt"].ours));
  });

  it("Ours / Theirs / Base chips overwrite the resolution textarea from the fetched versions", async () => {
    render(ConflictDialog, { path: "/repo" });
    await waitFor(() => expect(resTextarea().value).toBe(VERSIONS["a.txt"].merged));

    await fireEvent.click(screen.getByText("Theirs"));
    expect(resTextarea().value).toBe("theirs-a");

    await fireEvent.click(screen.getByText("Ours"));
    expect(resTextarea().value).toBe("ours-a");

    await fireEvent.click(screen.getByText("Base"));
    expect(resTextarea().value).toBe("base-a");
  });

  it("Base chip is disabled when the conflict has no base version (add/add-style conflict)", async () => {
    render(ConflictDialog, { path: "/repo" });
    await waitFor(() => expect(resTextarea().value).toBe(VERSIONS["a.txt"].merged));
    await fireEvent.click(screen.getByTitle("sub/b.txt"));
    await waitFor(() => expect(resTextarea().value).toBe(VERSIONS["sub/b.txt"].ours));

    expect((screen.getByText("Base") as HTMLButtonElement).disabled).toBe(true);
  });

  it("Mark resolved & stage stages the edited resolution, then reloads and auto-selects the next file", async () => {
    render(ConflictDialog, { path: "/repo" });
    await waitFor(() => expect(resTextarea().value).toBe(VERSIONS["a.txt"].merged));

    await fireEvent.input(resTextarea(), { target: { value: "resolved-a-content" } });
    await fireEvent.click(screen.getByText(/Mark resolved/));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("forge_resolve_file", { path: "/repo", file: "a.txt", content: "resolved-a-content" }));
    expect(screen.getByText("Staged a.txt")).toBeTruthy();

    // Reload dropped a.txt from the unmerged list; b.txt is auto-selected next.
    await waitFor(() => expect(screen.getByText("1 unresolved")).toBeTruthy());
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("forge_conflict_versions", { path: "/repo", file: "sub/b.txt" }));
    expect(screen.queryByTitle("a.txt")).toBeNull();
  });

  it("Continue is disabled while files remain unresolved, and enabled once the list is empty", async () => {
    render(ConflictDialog, { path: "/repo" });
    await waitFor(() => expect(resTextarea().value).toBe(VERSIONS["a.txt"].merged));
    const continueBtn = () => screen.getByText(/^Continue/).closest("button") as HTMLButtonElement;
    expect(continueBtn().disabled).toBe(true);

    await fireEvent.click(screen.getByText(/Mark resolved/)); // stage a.txt
    await waitFor(() => expect(resTextarea().value).toBe(VERSIONS["sub/b.txt"].ours));
    await fireEvent.click(screen.getByText(/Mark resolved/)); // stage b.txt

    await waitFor(() => expect(continueBtn().disabled).toBe(false));
    expect(invoke).toHaveBeenCalledWith("forge_resolve_file", { path: "/repo", file: "sub/b.txt", content: "ours-b" });
  });

  it("Continue calls forge_conflict_continue and dispatches done + close with the returned message", async () => {
    files = []; // simulate every conflict already staged
    const { component } = render(ConflictDialog, { path: "/repo" });
    await waitFor(() => expect(screen.getByText("0 unresolved")).toBeTruthy());

    const onDone = vi.fn();
    const onClose = vi.fn();
    component.$on("done", onDone);
    component.$on("close", onClose);

    await fireEvent.click(screen.getByText(/^Continue/));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("forge_conflict_continue", { path: "/repo" }));
    expect(onDone).toHaveBeenCalledTimes(1);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("Abort calls forge_conflict_abort and dispatches done + close, even with unresolved files remaining", async () => {
    const { component } = render(ConflictDialog, { path: "/repo" });
    await waitFor(() => expect(resTextarea().value).toBe(VERSIONS["a.txt"].merged));

    const onDone = vi.fn();
    const onClose = vi.fn();
    component.$on("done", onDone);
    component.$on("close", onClose);

    await fireEvent.click(screen.getByText("Abort"));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("forge_conflict_abort", { path: "/repo" }));
    expect(onDone).toHaveBeenCalledTimes(1);
    expect(onClose).toHaveBeenCalledTimes(1);
    // forge_resolve_file / forge_conflict_continue must never be called on the abort path.
    expect(callsFor("forge_resolve_file")).toHaveLength(0);
    expect(callsFor("forge_conflict_continue")).toHaveLength(0);
  });

  it("shows the empty state when there is no in-progress operation and nothing unresolved", async () => {
    operation = "none";
    files = [];
    render(ConflictDialog, { path: "/repo" });

    await waitFor(() => expect(screen.getByText("No conflicts — nothing to resolve.")).toBeTruthy());
    expect(screen.getByText("No operation in progress")).toBeTruthy();
    expect((screen.getByText("Abort") as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByText(/^Continue/).closest("button") as HTMLButtonElement).disabled).toBe(true);
  });

  it("surfaces a load error in the status line when forge_conflict_state rejects", async () => {
    stateError = "not a git repository";
    render(ConflictDialog, { path: "/repo" });

    await waitFor(() => expect(screen.getByText("not a git repository")).toBeTruthy());
    // NOTE (pinning shipped behavior, not asserting it's ideal): `operation`/`files` stay at their
    // initial defaults ("none" / []) on a load failure, so the empty-state panel renders concurrently
    // with the error in the status line — the user sees both "No conflicts — nothing to resolve." and
    // the real error text at the same time. This looks like a pre-existing UX inconsistency, not a
    // typed-call mis-wiring, so it's reported here rather than fixed (test-only ticket).
    expect(screen.getByText("No conflicts — nothing to resolve.")).toBeTruthy();
  });
});
