/**
 * CheckpointDialog (CPE-1125, epic CPE-732). The palette-driven command surface over the CPE-1123
 * checkpoint_* commands. These assert: the dialog lists checkpoints for the root on open, creates a
 * checkpoint (create → refresh list), previews a revert, and — the guardrail this ticket calls out —
 * a revert (whole-tree or single-path) never fires on one click: it must be confirmed first. The typed
 * `commands.*` client routes through the mocked `../invoke`, so mocking `invoke` here drives it.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";

const invokeMock = vi.fn(async (_cmd: string, _args?: unknown): Promise<unknown> => null);
vi.mock("../invoke", () => ({
  invoke: (...a: unknown[]) => (invokeMock as (...x: unknown[]) => unknown)(...a),
  unwrap: <T>(r: { status: string; data?: T; error?: unknown }): T => {
    if (r.status === "ok") return r.data as T;
    throw r.error instanceof Error ? r.error : new Error(String(r.error));
  },
}));

import CheckpointDialog from "./CheckpointDialog.svelte";

const CHECKPOINTS = [
  { manifest_id: "m-2", label: "before refactor", ts: 2000 },
  { manifest_id: "m-1", label: "", ts: 1000 },
];

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "checkpoint_list") return CHECKPOINTS;
    // CPE-1600: `loadList` always reads both lists — an unhandled-by-a-test-case default of `[]`
    // (not `null`) matches what the real backend returns for a root with no failed attempts.
    if (cmd === "checkpoint_failures_list") return [];
    return null;
  });
});

describe("CheckpointDialog (CPE-1125)", () => {
  it("lists checkpoints for the root on open", async () => {
    render(CheckpointDialog, { initialPath: "/work/proj" });
    expect(await screen.findByTestId("cp-m-2")).toBeTruthy();
    expect(screen.getByTestId("cp-m-1")).toBeTruthy();
    expect(invokeMock).toHaveBeenCalledWith("checkpoint_list", { root: "/work/proj" });
  });

  it("creates a checkpoint via checkpoint_create, then refreshes the list", async () => {
    render(CheckpointDialog, { initialPath: "/work/proj" });
    await screen.findByTestId("cp-m-2");

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "checkpoint_create") {
        return {
          checkpoint: { manifest_id: "m-3", label: "clean", ts: 3000 },
          new_blobs: 4,
          reused_blobs: 1,
          added_bytes: 128,
          skipped: [],
        };
      }
      if (cmd === "checkpoint_list") return [{ manifest_id: "m-3", label: "clean", ts: 3000 }, ...CHECKPOINTS];
      return null;
    });

    await fireEvent.input(screen.getByTestId("checkpoint-label"), { target: { value: "clean" } });
    await fireEvent.click(screen.getByTestId("create-btn"));

    expect(invokeMock).toHaveBeenCalledWith("checkpoint_create", { root: "/work/proj", label: "clean" });
    expect(await screen.findByTestId("cp-m-3")).toBeTruthy();
    expect(await screen.findByTestId("note")).toBeTruthy();
  });

  it("previews a revert via checkpoint_preview_revert and surfaces the drift count", async () => {
    render(CheckpointDialog, { initialPath: "/work/proj" });
    await screen.findByTestId("cp-m-2");

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "checkpoint_preview_revert") {
        return { creates: 1, overwrites: 2, deletes: 0, bytes_written: 4096, total: 3, drift_count: 1, drift_paths: ["/work/proj/a.txt"] };
      }
      if (cmd === "checkpoint_list") return CHECKPOINTS;
      return null;
    });

    await fireEvent.click(screen.getByTestId("preview-btn-m-2"));

    expect(invokeMock).toHaveBeenCalledWith("checkpoint_preview_revert", { root: "/work/proj", manifestId: "m-2", session: null });
    const panel = await screen.findByTestId("preview-panel");
    // CPE-1165: the preview is framed as the revert PLAN ("what reverting will do"), in plain
    // user-outcome language — not a summary of the user's edits. The bare "drift N" is gone.
    expect(panel.textContent).toContain("Reverting will:");
    expect(panel.textContent).toContain("restore 1"); // creates → files you deleted come back
    expect(panel.textContent).toContain("overwrite 2"); // overwrites → changed files reset
    expect(panel.textContent).toContain("1 changed since this checkpoint"); // drift_count reworded
    expect(panel.textContent).not.toContain("drift 1");
    expect(await screen.findByTestId("drift-list")).toBeTruthy();
  });

  it("does NOT call checkpoint_revert on the first click — it arms a confirmation panel instead", async () => {
    render(CheckpointDialog, { initialPath: "/work/proj" });
    await screen.findByTestId("cp-m-2");

    await fireEvent.click(screen.getByTestId("revert-btn-m-2"));

    expect(invokeMock).not.toHaveBeenCalledWith("checkpoint_revert", expect.anything());
    expect(await screen.findByTestId("confirm-revert")).toBeTruthy();
  });

  it("cancelling the confirmation panel never reverts", async () => {
    render(CheckpointDialog, { initialPath: "/work/proj" });
    await screen.findByTestId("cp-m-2");

    await fireEvent.click(screen.getByTestId("revert-btn-m-2"));
    await screen.findByTestId("confirm-revert");
    await fireEvent.click(screen.getByTestId("confirm-cancel-btn"));

    expect(screen.queryByTestId("confirm-revert")).toBeNull();
    expect(invokeMock).not.toHaveBeenCalledWith("checkpoint_revert", expect.anything());
  });

  it("reverts (whole tree) only after the confirm panel is accepted, then emits reverted", async () => {
    const { component } = render(CheckpointDialog, { initialPath: "/work/proj" });
    await screen.findByTestId("cp-m-2");

    const reverted = vi.fn();
    component.$on("reverted", reverted);

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "checkpoint_revert") return { applied: 3, skipped: [] };
      if (cmd === "checkpoint_list") return CHECKPOINTS;
      return null;
    });

    await fireEvent.click(screen.getByTestId("revert-btn-m-2"));
    await screen.findByTestId("confirm-revert");
    await fireEvent.click(screen.getByTestId("confirm-yes-btn"));

    expect(invokeMock).toHaveBeenCalledWith("checkpoint_revert", { root: "/work/proj", manifestId: "m-2" });
    expect(await screen.findByTestId("outcome-panel")).toBeTruthy();
    expect(reverted).toHaveBeenCalled();
  });

  it("reverts a single path via checkpoint_revert_one only after confirming, scoped to the typed path", async () => {
    render(CheckpointDialog, { initialPath: "/work/proj" });
    await screen.findByTestId("cp-m-2");

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "checkpoint_preview_revert") return { creates: 0, overwrites: 1, deletes: 0, bytes_written: 10, total: 1, drift_count: 0, drift_paths: [] };
      if (cmd === "checkpoint_revert_one") return { applied: 1, skipped: [] };
      if (cmd === "checkpoint_list") return CHECKPOINTS;
      return null;
    });

    // Select the checkpoint (Preview also selects it) so the revert-one row appears.
    await fireEvent.click(screen.getByTestId("preview-btn-m-2"));
    await screen.findByTestId("revert-one-path");

    await fireEvent.input(screen.getByTestId("revert-one-path"), { target: { value: "/work/proj/a.txt" } });
    await fireEvent.click(screen.getByTestId("revert-one-btn"));

    // Still armed, not yet applied.
    expect(invokeMock).not.toHaveBeenCalledWith("checkpoint_revert_one", expect.anything());
    await fireEvent.click(await screen.findByTestId("confirm-yes-btn"));

    expect(invokeMock).toHaveBeenCalledWith("checkpoint_revert_one", { root: "/work/proj", manifestId: "m-2", path: "/work/proj/a.txt" });
  });

  describe("per-file diff (CPE-1197 frontend half)", () => {
    async function previewWithDrift() {
      render(CheckpointDialog, { initialPath: "/work/proj" });
      await screen.findByTestId("cp-m-2");

      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === "checkpoint_preview_revert") {
          return { creates: 0, overwrites: 1, deletes: 0, bytes_written: 10, total: 1, drift_count: 1, drift_paths: ["a.txt"] };
        }
        if (cmd === "checkpoint_list") return CHECKPOINTS;
        return null;
      });

      await fireEvent.click(screen.getByTestId("preview-btn-m-2"));
      await screen.findByTestId("drift-list");
    }

    it("calls checkpointDiffFile with the right args and renders DiffPeek's before/after on Open diff", async () => {
      await previewWithDrift();

      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === "checkpoint_diff_file") return { before: "old line\n", after: "new line\n" };
        return null;
      });

      await fireEvent.click(screen.getByTestId("diff-btn-a.txt"));

      expect(invokeMock).toHaveBeenCalledWith("checkpoint_diff_file", { root: "/work/proj", manifestId: "m-2", relPath: "a.txt" });
      const panel = await screen.findByTestId("diff-panel-a.txt");
      expect(panel.textContent).toContain("old line");
      expect(panel.textContent).toContain("new line");

      // Clicking again collapses the panel.
      await fireEvent.click(screen.getByTestId("diff-btn-a.txt"));
      expect(screen.queryByTestId("diff-panel-a.txt")).toBeNull();
    });

    it("shows a small notice — not a crash — when checkpointDiffFile errors (binary/oversize/unknown-path)", async () => {
      await previewWithDrift();

      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === "checkpoint_diff_file") throw new Error("a.txt: live file is not valid UTF-8 text (binary diff isn't supported).");
        return null;
      });

      await fireEvent.click(screen.getByTestId("diff-btn-a.txt"));

      const notice = await screen.findByTestId("diff-error");
      expect(notice.textContent).toContain("binary diff isn't supported");
    });

    it("ignores a stale diff response when the user switches rows before it resolves (race guard)", async () => {
      render(CheckpointDialog, { initialPath: "/work/proj" });
      await screen.findByTestId("cp-m-2");
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === "checkpoint_preview_revert") {
          return { creates: 0, overwrites: 2, deletes: 0, bytes_written: 20, total: 2, drift_count: 2, drift_paths: ["a.txt", "b.txt"] };
        }
        if (cmd === "checkpoint_list") return CHECKPOINTS;
        return null;
      });
      await fireEvent.click(screen.getByTestId("preview-btn-m-2"));
      await screen.findByTestId("drift-list");

      // a.txt's diff resolves LATE (gated); b.txt's resolves immediately.
      let resolveA!: () => void;
      const aGate = new Promise<void>((r) => { resolveA = r; });
      invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
        if (cmd === "checkpoint_diff_file") {
          if ((args as { relPath?: string } | undefined)?.relPath === "a.txt") {
            await aGate;
            return { before: "A-before", after: "A-after" };
          }
          return { before: "B-before", after: "B-after" };
        }
        return null;
      });

      // Open A (its call is still pending), then switch to B (resolves now).
      await fireEvent.click(screen.getByTestId("diff-btn-a.txt"));
      await fireEvent.click(screen.getByTestId("diff-btn-b.txt"));
      const bPanel = await screen.findByTestId("diff-panel-b.txt");
      expect(bPanel.textContent).toContain("B-before");

      // Let A's now-stale response land: it must NOT reopen A or pollute B's panel.
      resolveA();
      await new Promise((r) => setTimeout(r, 0));
      expect(screen.queryByTestId("diff-panel-a.txt")).toBeNull();
      const bAfter = screen.getByTestId("diff-panel-b.txt");
      expect(bAfter.textContent).toContain("B-before");
      expect(bAfter.textContent).not.toContain("A-before");
    });
  });

  describe("failed checkpoint attempts (CPE-1600)", () => {
    const FAILURES = [
      { operation: "Before batch media overwrite", reason: "disk is read-only", ts: 2500 },
      { operation: "Before removing clutter", reason: "permission denied", ts: 500 },
    ];

    it("reads checkpoint_failures_list for the root and renders failed attempts distinctly, with no restore actions", async () => {
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === "checkpoint_list") return CHECKPOINTS;
        if (cmd === "checkpoint_failures_list") return FAILURES;
        return null;
      });

      render(CheckpointDialog, { initialPath: "/work/proj" });
      await screen.findByTestId("cp-m-2");
      expect(invokeMock).toHaveBeenCalledWith("checkpoint_failures_list", { root: "/work/proj" });

      // Both failed attempts render, each with a distinct row testid namespace (never `cp-<manifest_id>`,
      // since a failure has no manifest id) and no Preview/Revert buttons at all.
      const rows = screen.getAllByTestId(/^cpf-/);
      expect(rows.length).toBe(2);
      for (const row of rows) {
        expect(row.querySelector('[data-testid^="preview-btn-"]')).toBeNull();
        expect(row.querySelector('[data-testid^="revert-btn-"]')).toBeNull();
      }
      // The reason and operation both surface in the row text.
      expect(screen.getByText("disk is read-only")).toBeTruthy();
      expect(screen.getByText("permission denied")).toBeTruthy();
    });

    it("interleaves failed attempts with real checkpoints newest-first by timestamp", async () => {
      // FAILURES[0].ts=2500 sits between CHECKPOINTS' m-2 (ts=2000) and nothing newer; FAILURES[1].ts=500
      // sits older than both real checkpoints (m-1 ts=1000). Expected order: failure(2500), cp m-2(2000),
      // cp m-1(1000), failure(500).
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === "checkpoint_list") return CHECKPOINTS;
        if (cmd === "checkpoint_failures_list") return FAILURES;
        return null;
      });

      render(CheckpointDialog, { initialPath: "/work/proj" });
      await screen.findByTestId("cp-m-2");

      const list = screen.getByTestId("checkpoint-list");
      const ids = Array.from(list.querySelectorAll("[data-testid]")).map((el) => el.getAttribute("data-testid"));
      const failureIdx = ids.findIndex((id) => id?.startsWith("cpf-"));
      const m2Idx = ids.indexOf("cp-m-2");
      const m1Idx = ids.indexOf("cp-m-1");
      const lastFailureIdx = ids.map((id, i) => (id?.startsWith("cpf-") ? i : -1)).filter((i) => i >= 0).pop();
      expect(failureIdx).toBeLessThan(m2Idx);
      expect(m2Idx).toBeLessThan(m1Idx);
      expect(m1Idx).toBeLessThan(lastFailureIdx as number);
    });

    it("a checkpoint-list failure doesn't blank an already-loaded failures list, and vice versa", async () => {
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === "checkpoint_list") throw new Error("root not readable");
        if (cmd === "checkpoint_failures_list") return FAILURES;
        return null;
      });

      render(CheckpointDialog, { initialPath: "/work/proj" });
      // The failure rows still render even though `checkpoint_list` rejected.
      expect(await screen.findByText("disk is read-only")).toBeTruthy();
      expect(screen.queryByTestId("cp-m-2")).toBeNull();
    });

    it("renders no failure rows and no crash when checkpoint_failures_list returns nothing (defensive null guard)", async () => {
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === "checkpoint_list") return CHECKPOINTS;
        // Simulate a lenient/older backend response shape that resolves to `null` rather than `[]`.
        if (cmd === "checkpoint_failures_list") return null;
        return null;
      });

      render(CheckpointDialog, { initialPath: "/work/proj" });
      await screen.findByTestId("cp-m-2");
      expect(screen.queryAllByTestId(/^cpf-/).length).toBe(0);
    });
  });

  /**
   * CPE-1845 — the docs promised the user is told which cleanups did not happen AND WHY; no screen
   * rendered a single reason. These assert the reason and the next step now reach the DOM, and that the
   * two hold-back kinds do not get the same advice.
   *
   * **Limit, stated rather than papered over:** jsdom applies no component CSS under this project's
   * vitest config, so nothing here can check layout, ordering on screen, colour, or whether the panel is
   * visible. Text presence/absence only.
   */
  describe("held-back reasons on screen (CPE-1845)", () => {
    const HELD = {
      applied: 1,
      skipped: [
        { path: "added-1.txt", ok: false, error: "", outcome: "held_back_by_checkpoint" },
        { path: "added-2.txt", ok: false, error: "", outcome: "held_back_by_checkpoint" },
      ],
      held_back: {
        outcome: "held_back_by_checkpoint",
        count: 2,
        reason: "THE-ONE-REASON this checkpoint records no files at all",
        next_step: "THE-NEXT-STEP delete these files yourself",
        retryable: false,
        advises_manual_delete: true,
      },
    };

    async function revertWith(outcome: unknown) {
      render(CheckpointDialog, { initialPath: "/work/proj" });
      await screen.findByTestId("cp-m-2");
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === "checkpoint_revert") return outcome;
        if (cmd === "checkpoint_list") return CHECKPOINTS;
        return null;
      });
      await fireEvent.click(screen.getByTestId("revert-btn-m-2"));
      await screen.findByTestId("confirm-revert");
      await fireEvent.click(screen.getByTestId("confirm-yes-btn"));
      return await screen.findByTestId("outcome-panel");
    }

    it("renders the one shared reason and the next step, plus the held-back path names", async () => {
      const panel = await revertWith(HELD);
      expect(panel.textContent).toContain("THE-ONE-REASON");
      expect(panel.textContent).toContain("THE-NEXT-STEP");
      expect(panel.textContent).toContain("added-1.txt");
      expect(panel.textContent).toContain("added-2.txt");
      // Held back is counted as held back, not lumped in with failures.
      expect(panel.textContent).toContain("2 deletions held back");
      expect(panel.textContent).not.toContain("failed");
    });

    it("prints the reason ONCE for the whole group, not once per path", async () => {
      const many = {
        ...HELD,
        skipped: Array.from({ length: 50 }, (_, i) => ({
          path: `added-${i}.txt`,
          ok: false,
          error: "",
          outcome: "held_back_by_checkpoint",
        })),
        held_back: { ...HELD.held_back, count: 50 },
      };
      const panel = await revertWith(many);
      const copies = (panel.textContent ?? "").split("THE-ONE-REASON").length - 1;
      expect(copies).toBe(1);
      expect(panel.textContent).toContain("and 42 more"); // 50 - MAX_LISTED(8)
    });

    it("does not tell the user to re-run when the backend says re-running cannot help", async () => {
      const panel = await revertWith(HELD);
      // The advice is the backend's, whatever it is — the screen must not compose its own "re-run".
      expect(panel.textContent).toContain("THE-NEXT-STEP");
      expect(panel.textContent?.toLowerCase()).not.toContain("re-run after fixing");
    });

    it("escapes the bidi/format characters a failure reason carries from a filename", async () => {
      // `apply_delete`/`apply_write` format their reason as `"{target}: {os error}"`, so a
      // user-controlled FILENAME rides inside `f.error`. It was rendered raw while `f.path` beside it
      // went through displaySafePath (CPE-1845 UAT). U+202E flips the text that follows it.
      const panel = await revertWith({
        applied: 0,
        skipped: [
          {
            path: "C:/w/invoice‮gnp.exe",
            ok: false,
            error: "C:/w/invoice‮gnp.exe: Access is denied. (os error 5)",
            outcome: "failed",
          },
        ],
        held_back: null,
      });
      const text = panel.textContent ?? "";
      expect(text).not.toContain("‮");
      // Escaped to its bracketed tag, the same treatment the adjacent path already got.
      expect(text).toContain("[RLO]");
    });

    it("names the checkpoint entry a collided path belongs to, per path", async () => {
      const panel = await revertWith({
        applied: 0,
        skipped: [
          {
            path: "A.txt",
            ok: false,
            error: 'same file as checkpoint entry "a.txt"',
            outcome: "held_back_by_checkpoint",
          },
        ],
        held_back: {
          outcome: "held_back_by_checkpoint",
          count: 1,
          reason: "THE-ONE-REASON these paths resolve to the same files",
          next_step: "THE-NEXT-STEP nothing needs doing",
          retryable: false,
        },
      });
      expect(panel.textContent).toContain('same file as checkpoint entry "a.txt"');
    });

    it("keeps the leading verb on the note at the top of the dialog, which is detached from the panel", async () => {
      await revertWith(HELD);
      // `note` renders in the shared slot with "Checkpoint … captured", so a bare "Applied 1 change"
      // does not say what was applied.
      expect(await screen.findByText(/^Revert — applied 1 change/)).toBeTruthy();
    });

    it("REGRESSION: a healthy checkpoint renders one line and no hold-back box at all", async () => {
      const panel = await revertWith({ applied: 2, skipped: [], held_back: null });
      expect(panel.textContent?.trim()).toBe("Reverted — applied 2 changes.");
      expect(screen.queryByTestId("outcome-held-back")).toBeNull();
      expect(screen.queryByTestId("outcome-held-paths")).toBeNull();
      expect(screen.queryByTestId("outcome-failures")).toBeNull();
    });

    it("shows a genuine failure separately from a hold-back, in the same result", async () => {
      const panel = await revertWith({
        applied: 0,
        skipped: [
          { path: "locked.txt", ok: false, error: "FAILURE-DETAIL the file is locked", outcome: "failed" },
          { path: "added-1.txt", ok: false, error: "", outcome: "skipped_by_plan" },
        ],
        held_back: {
          outcome: "skipped_by_plan",
          count: 1,
          reason: "RETRYABLE-REASON one entry could not be restored this time",
          next_step: "RETRYABLE-STEP run the revert again",
          retryable: true,
        },
      });
      expect(panel.textContent).toContain("1 failed");
      expect(panel.textContent).toContain("1 deletion held back");
      expect(panel.textContent).toContain("FAILURE-DETAIL");
      expect(panel.textContent).toContain("RETRYABLE-REASON");
      expect(panel.textContent).toContain("RETRYABLE-STEP");
    });

    /**
     * CPE-1869 — "the held-back list tells you to delete files it will not show you". The revert panel
     * named up to 8 held-back paths, then told the permanent cases to "delete these files yourself",
     * with no way to get the rest of the list without re-running the revert. This adds a copy-to-
     * clipboard affordance for the untruncated set, gated on the backend's `advises_manual_delete` —
     * never on the `held_back_by_checkpoint` discriminant alone, which the alias/collision hold-back
     * also carries.
     */
    describe("copy-full-list affordance (CPE-1869)", () => {
      function mockClipboard() {
        const writeText = vi.fn().mockResolvedValue(undefined);
        Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
        return writeText;
      }

      it("offers to copy every held-back path, as absolute paths under the reverted root, when the backend advises manual deletion", async () => {
        const writeText = mockClipboard();
        const panel = await revertWith(HELD);
        const btn = await screen.findByTestId("outcome-copy-held-paths");
        expect(btn.textContent).toContain("Copy all 2 held-back paths");

        await fireEvent.click(btn);

        // `/work/proj` is `initialPath` — CheckpointDialog's revert root — joined onto each
        // `/`-relative held-back path, quoted one-per-line the way Explorer's "Copy as path" does.
        expect(writeText).toHaveBeenCalledWith('"/work/proj/added-1.txt"\n"/work/proj/added-2.txt"');
        expect(await screen.findByText("Copied")).toBeTruthy();
      });

      it("does NOT offer the copy affordance on the alias/collision hold-back", async () => {
        mockClipboard();
        await revertWith({
          applied: 0,
          skipped: [
            {
              path: "A.txt",
              ok: false,
              error: 'same file as checkpoint entry "a.txt"',
              outcome: "held_back_by_checkpoint",
            },
          ],
          held_back: {
            outcome: "held_back_by_checkpoint",
            count: 1,
            reason: "these paths resolve to the same files",
            next_step: "nothing needs doing",
            retryable: false,
            // This IS the acceptance criterion: these paths are the checkpoint's own content under
            // another spelling, so a "go delete them" affordance would be the bug, not the fix.
            advises_manual_delete: false,
          },
        });
        expect(screen.queryByTestId("outcome-copy-held-paths")).toBeNull();
      });

      it("does NOT offer the copy affordance on the retryable hold-back", async () => {
        mockClipboard();
        await revertWith({
          applied: 0,
          skipped: [{ path: "added-1.txt", ok: false, error: "", outcome: "skipped_by_plan" }],
          held_back: {
            outcome: "skipped_by_plan",
            count: 1,
            reason: "one entry could not be restored this time",
            next_step: "run the revert again",
            retryable: true,
            advises_manual_delete: false,
          },
        });
        expect(screen.queryByTestId("outcome-copy-held-paths")).toBeNull();
      });
    });
  });
});
