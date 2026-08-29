/**
 * CheckpointDialog (CPE-1125, epic CPE-732). The palette-driven command surface over the CPE-1123
 * checkpoint_* commands. These assert: the dialog lists checkpoints for the root on open, creates a
 * checkpoint (create → refresh list), previews a revert, and — the guardrail this ticket calls out —
 * a revert (whole-tree or single-path) never fires on one click: it must be confirmed first. The typed
 * `commands.*` client routes through the mocked `../invoke`, so mocking `invoke` here drives it.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { styleRules, declaration, lengthToPx, contentIndependentHeightReason } from "../svelteCss";
import { stripRustComments } from "../rustSource";

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
      // CPE-1881 round 4: the single "outcome-failures" box split into two — "outcome-failed" (genuine
      // failures) and "outcome-refused" (grouped write refusals) — neither should render either.
      expect(screen.queryByTestId("outcome-failed")).toBeNull();
      expect(screen.queryByTestId("outcome-refused")).toBeNull();
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

/**
 * CPE-1983 — THE RED-PROOF: a click aimed at **Refresh** while the first list is still loading.
 *
 * THE DEFECT this replaces the eyeball for. `.list` used to be `max-height: 30vh; overflow: auto`
 * with no `height`, i.e. content-driven: a few tens of px while `onMount(loadList)`'s two round-trips
 * were in flight, then up to 30vh (210px at the harness window) once the checkpoints rendered.
 * `.backdrop` centres the dialog, so that growth is split evenly above and below and **everything
 * above `.list` slides UP by half of it** — the help button, the path input, **Refresh**, the label
 * input and **Create checkpoint**, five interactive controls.
 *
 * WHY THIS ONE RANKED ABOVE THE ONE CPE-1968 FIXED. In `OrganizeDialog` the mis-landed click landed
 * on the preview box and `.dialog`'s `on:click|stopPropagation` swallowed it in silence. Here the box
 * that arrives under the pointer is `.list`, and `.list` contains `Revert…` buttons. The consequence
 * is not a lost click; it is a destructive control armed by a click aimed at Refresh. It also re-runs
 * on every Refresh and after `doCreate`, so it is not only an on-open hazard.
 *
 * WHY A MODEL, and what this one is anchored to. jsdom has no layout engine, so the shift cannot be
 * measured by rendering. What CAN be modelled honestly is the ONE axis the defect lives on, from the
 * component's own declarations, read at run time (CPE-1933, never recalled). This dialog has a prose
 * `<p>` above the controls whose wrapped height jsdom cannot know — so the model is anchored at the
 * TOP OF `.paths` rather than at the dialog's top, and every band it needs is below that anchor and
 * has a declared height. The unknown prose block is above the anchor and cancels out of every term.
 *
 * WHAT THE MODEL ASSUMES, so a reader can check it rather than trust it:
 *   - `.paths`, `.create-row` and `.list` stack in that order with nothing between them. Asserted from
 *     the markup below, and the two conditional rows that CAN appear between them (`.err`, `.note`)
 *     are asserted absent in the run that does the hit-test.
 *   - each of `.paths` and `.create-row` is one row as tall as the 30px controls in it. `.path`,
 *     `.label-input` and `.btn` all declare `height: 30px`; that they agree is asserted, not assumed.
 *   - for the OLD content-driven shape, the loading height is taken as `.empty`'s declared VERTICAL
 *     PADDING alone. That is a deliberate LOWER bound — it ignores the "Loading…" text line, which
 *     jsdom cannot measure — and therefore makes the modelled shift an UPPER bound. The conclusion
 *     does not rest on the exact number: the test derives the loading height at which the aimed point
 *     would just re-enter `.paths` and asserts it is far above any one-line box.
 *
 * WHAT IT THEREFORE DOES NOT PROVE: which ROW of `.list` is under the point (that is a text-metrics
 * question), that wry's webview lays this out as modelled, or anything about the other dialogs — the
 * repo-wide leg is `src/lib/dialogBodyReflow.test.ts`.
 *
 * INDEPENDENTLY CROSS-CHECKED IN A REAL BROWSER, which is the leg a model most needs and least
 * deserves on its own. `scripts/dev-harness/checkpoint-dialog/` mounts this same component in
 * headless Chrome at the same 1000x700 viewport and reports Refresh's measured screen position. With
 * the pre-CPE-1983 CSS re-applied (`legacy=1`, `list=many`), it measures:
 *
 *     Refresh top @t=100ms : 320.6px     .list top @t=100ms : 396.6px
 *     Refresh top now      : 236.6px     .list top now      : 312.6px
 *     Refresh moved        :  84.0px     click aimed at Refresh now over: **.list (Revert…)**
 *
 * and with the shipped CSS, 0.0px with Refresh at 236.6px and `.list` at 312.6px in all four of
 * loading, empty, two checkpoints and twelve. **The ABSOLUTE band positions are what the hit-test
 * consumes** — the shift alone is weak evidence, because every term of it cancels.
 *
 * THE TWO METHODS DISAGREE BY 9px AND THAT IS THE MODEL BEHAVING AS DOCUMENTED, not a defect: the
 * model reports 93px because it takes the loading box as `.empty`'s padding alone (24px), a stated
 * LOWER bound that makes the shift an UPPER bound. The browser's real loading box is 42px — the same
 * 24px of padding plus one 18px text line, which is exactly the term jsdom cannot measure. Where the
 * two disagree the browser is right; the model is deliberately conservative in the direction that
 * over-states the hazard, and both agree on the only thing the hit-test asks: the point lands in
 * `.list`.
 */
describe("CPE-1983 — a click aimed at Refresh mid-load lands where it was aimed", () => {
  const SRC = readFileSync(join(process.cwd(), "src", "lib", "components", "CheckpointDialog.svelte"), "utf8");

  /**
   * The harness window's height, DERIVED from the app's own `.inner_size(w, h)` rather than pasted —
   * the same derivation `OrganizeDialog.test.ts` uses, and for the same reason. Rust comments are
   * stripped first so a commented-out or quoted copy cannot answer (CPE-1933 rule 2).
   */
  const VIEWPORT_H = (() => {
    const rust = stripRustComments(readFileSync(join(process.cwd(), "src-tauri", "src", "lib.rs"), "utf8"));
    const hits = [...rust.matchAll(/\.inner_size\(\s*([\d.]+)\s*,\s*([\d.]+)\s*\)/g)];
    expect(hits.length, "expected exactly one `.inner_size(w, h)` call in src-tauri/src/lib.rs").toBe(1);
    return parseFloat(hits[0][2]);
  })();

  /**
   * The one top-level rule whose SELECTOR LIST contains `.cls`.
   *
   * `styleBlock` cannot answer here: this component groups three rows into
   * `.paths, .create-row, .revert-one { … }`, and a `.paths {`-anchored regex does not match a
   * grouped selector — it would report "found 0" and the model would be written around whatever the
   * author reached for instead. Reading the enumerated rules is the fix, not a looser regex.
   */
  function ruleFor(cls: string): string {
    const hits = styleRules(SRC).filter(
      (r) => !r.atRule && r.selector.split(",").some((s) => s.trim() === `.${cls}`),
    );
    if (hits.length !== 1) {
      throw new Error(`expected exactly one top-level rule naming \`.${cls}\`, found ${hits.length}`);
    }
    return hits[0].block;
  }

  /** A declaration of `.cls` resolved to px, or a throw naming what was missing. */
  function px(cls: string, prop: string): number {
    const raw = declaration(ruleFor(cls), prop);
    if (raw === undefined) throw new Error(`.${cls} declares no \`${prop}\``);
    return lengthToPx(raw, VIEWPORT_H);
  }

  /** The height of the 30px control rows, asserted to be one number rather than assumed. */
  function controlRowHeight(): number {
    const heights = ["path", "label-input", "btn"].map((c) => px(c, "height"));
    expect(new Set(heights).size, `.path/.label-input/.btn no longer agree on a height: ${heights}`).toBe(1);
    return heights[0];
  }

  /**
   * `.list`'s height in a phase. With a definite `height` both phases agree — that IS the fix. With
   * the old `max-height`-only shape they do not, which is the defect.
   */
  function listHeight(phase: "loading" | "settled"): number {
    const block = ruleFor("list");
    const definite = declaration(block, "height");
    if (definite) return lengthToPx(definite, VIEWPORT_H);
    if (phase === "settled") {
      const cap = declaration(block, "max-height");
      if (!cap) throw new Error(".list declares neither a height nor a max-height");
      return lengthToPx(cap, VIEWPORT_H);
    }
    // The stated LOWER bound on the loading box: `.empty`'s vertical padding, with no text line.
    return 2 * px("empty", "padding");
  }

  interface Band {
    name: string;
    top: number;
    bottom: number;
  }

  /** The three bands below the `.paths` anchor, for a given phase. Offsets are from `.paths`' top. */
  function bands(phase: "loading" | "settled"): Band[] {
    const row = controlRowHeight();
    const gap = px("paths", "margin-bottom"); // the grouped `.paths, .create-row, .revert-one` rule
    let y = 0;
    return (
      [
        ["paths", row, gap],
        ["create-row", row, gap],
        ["list", listHeight(phase), 0],
      ] as [string, number, number][]
    ).map(([name, h, m]) => {
      const band = { name, top: y, bottom: y + h };
      y += h + m;
      return band;
    });
  }

  const bandNamed = (list: Band[], name: string) => list.find((b) => b.name === name)!;
  const bandAt = (list: Band[], y: number) =>
    list.find((b) => y >= b.top && y < b.bottom)?.name ?? "(between the modelled bands)";

  it("stacks paths, create-row, list in that order in the markup", () => {
    const markup = SRC.slice(0, SRC.indexOf("<style>"));
    const order = ["paths", "create-row", "list"].map((c) => markup.indexOf(`class="${c}"`));
    expect(order, "expected all three stacked rows in the markup").not.toContain(-1);
    expect(order, "expected paths, create-row, list in markup order").toEqual([...order].sort((a, b) => a - b));
  });

  it("gives .list the SAME height while loading as once the checkpoints render", () => {
    expect(
      declaration(ruleFor("list"), "max-height"),
      "`.list` declares a max-height again. Paired with no `height` that makes the box a function of " +
        "its CONTENT, so it grows when onMount(loadList) lands and the centred dialog slides Refresh " +
        "(and four other controls) up out from under the pointer — CPE-1983.",
    ).toBeUndefined();

    const reason = contentIndependentHeightReason(ruleFor("list"), VIEWPORT_H);
    expect(reason, `\`.list\` must have a height that cannot depend on the checkpoint list. It ${reason}.`).toBeNull();

    expect(bandNamed(bands("settled"), "paths")).toEqual(bandNamed(bands("loading"), "paths"));
  });

  it("swallows a click that lands on the dialog body, so a mis-landed click is silent", () => {
    expect(SRC).toMatch(/<div class="dialog"[^>]*on:click\|stopPropagation/);
  });

  it("centres the dialog vertically, which is why a height change moves the rows above it", () => {
    expect(ruleFor("backdrop")).toMatch(/place-items:\s*center/);
  });

  it("red-proof: the click aimed at Refresh at t=0 still hits Refresh once the list lands", async () => {
    // Hold `checkpoint_list` open so the dialog is genuinely in its loading state when the point is
    // taken, then release it — the same shape as a slow disk.
    let release!: () => void;
    const held = new Promise<void>((r) => (release = r));
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "checkpoint_list") {
        await held;
        return CHECKPOINTS;
      }
      if (cmd === "checkpoint_failures_list") return [];
      return null;
    });

    render(CheckpointDialog, { initialPath: "/work/proj" });

    // t=0: the list has not landed. This is the instant the pointer is over Refresh.
    expect(screen.queryByTestId("cp-m-2"), "no checkpoint may have rendered yet").toBeNull();
    expect(screen.queryByTestId("error"), "the model has no `.err` row; this run must not have one").toBeNull();
    expect(screen.queryByTestId("note"), "the model has no `.note` row; this run must not have one").toBeNull();
    const aimed = bandNamed(bands("loading"), "paths");
    const aimY = (aimed.top + aimed.bottom) / 2;

    release();
    await screen.findByTestId("cp-m-2");

    // The dialog is centred, so growing `.list` by Δ moves `.paths` UP by Δ/2 — which means the point
    // the pointer is still resting on is now Δ/2 FURTHER DOWN the dialog than it was.
    const shift = (listHeight("settled") - listHeight("loading")) / 2;
    const landedIn = bandAt(bands("settled"), aimY + shift);

    // RED-PROOF, run and recorded here rather than only in the PR body (CPE-1933 rule 3). With
    // `.list` reverted to `max-height: 30vh` (no `height`): `shift` = **93px**, `landedIn` resolves
    // to **"list"**, this test dispatches its click at `Revert…` instead of Refresh, and
    // `confirm-revert` is on screen when it asserts — a click aimed at Refresh put a destructive
    // action one Enter away. **2 of 32 red in this file** (this test and the inverted height
    // assertion above), plus the repo-wide `dialogBodyReflow.test.ts` leg naming
    // `CheckpointDialog.svelte#list`: 3 of 47 across the two files.
    // Note which leg did NOT red, because it says what each is for: "loading and settled do not move
    // the rows above the list" stayed green, correctly — it compares the DOM outside `.list`, and the
    // defect was geometric, not structural. Neither leg subsumes the other.
    // Restored; 32/32 green with `height: clamp(160px, 30vh, 260px)`, where `landedIn` is "paths" and
    // `shift` is 0.
    const target =
      landedIn === "paths"
        ? screen.getByTestId("refresh-btn")
        : landedIn === "create-row"
          ? screen.getByTestId("create-btn")
          : // WHICH row of `.list` is under the point is a text-metrics question jsdom cannot answer.
            // The first row is taken deliberately: it is the worst case, and the worst case is the
            // whole reason this component was ranked above the one CPE-1968 fixed.
            screen.getByTestId("revert-btn-m-2");

    invokeMock.mockClear();
    await fireEvent.click(target);

    expect(
      screen.queryByTestId("confirm-revert"),
      `a click aimed at the centre of the Refresh row while the list was loading landed in ` +
        `"${landedIn}" once it rendered — the dialog re-centred and the row moved ${shift}px. In this ` +
        "dialog that is not a swallowed click: `.list` carries `Revert…`, so the stray click ARMS A " +
        "DESTRUCTIVE ACTION. `.list` must have a height that does not depend on the checkpoint list.",
    ).toBeNull();

    expect(
      invokeMock,
      `the click landed in "${landedIn}" instead of on Refresh, so the list was never re-read`,
    ).toHaveBeenCalledWith("checkpoint_list", { root: "/work/proj" });
  });

  it("the hazard does not depend on the modelled loading height being exact", () => {
    // The model's one unmeasurable input is how tall the "Loading…" box is. So rather than defend a
    // number, derive the number at which the conclusion would flip: the loading height above which
    // the aimed point stays inside `.paths`. Anything below it lands in `.list`.
    const row = controlRowHeight();
    const gap = px("paths", "margin-bottom");
    const listTop = 2 * row + 2 * gap;
    const settled = lengthToPx(
      declaration(ruleFor("list"), "max-height") ?? declaration(ruleFor("list"), "height")!,
      VIEWPORT_H,
    );
    // aim (row/2) + (settled - loading)/2 >= listTop  ⇔  loading <= settled + row - 2*listTop
    const flipsAt = settled + row - 2 * listTop;

    expect(
      flipsAt,
      `a "Loading…" box shorter than ${flipsAt}px puts the click in \`.list\`. That is only a real ` +
        "hazard if it is comfortably above a one-line box (padding plus one 12.5px line), which is " +
        "what makes the red-proof's conclusion independent of the text metrics jsdom cannot measure.",
    ).toBeGreaterThan(2 * px("empty", "padding") + 20);
  });

  it("loading and settled do not move the rows above the list", async () => {
    // LEG 2, the DOM one, which is what makes the geometric leg sufficient: the geometry speaks only
    // for `.list`; it would miss a node appearing ABOVE the controls. So render two list sizes and
    // assert everything outside `.list` is byte-identical.
    const outsideList = (): string => {
      const clone = (document.querySelector(".dialog") as HTMLElement).cloneNode(true) as HTMLElement;
      clone.querySelector('[data-testid="checkpoint-list"]')!.innerHTML = "";
      return clone.innerHTML;
    };

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "checkpoint_list") return CHECKPOINTS;
      if (cmd === "checkpoint_failures_list") return [];
      return null;
    });
    const { unmount } = render(CheckpointDialog, { initialPath: "/work/proj" });
    await screen.findByTestId("cp-m-2");
    const withTwo = outsideList();
    unmount();

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "checkpoint_list") return [CHECKPOINTS[0]];
      if (cmd === "checkpoint_failures_list") return [];
      return null;
    });
    render(CheckpointDialog, { initialPath: "/work/proj" });
    await screen.findByTestId("cp-m-2");

    expect(
      outsideList(),
      "the number of checkpoints changed something OUTSIDE `.list`, so the centred dialog re-centres " +
        "and the controls above the list move under the pointer (CPE-1983). Only `.list`'s CONTENTS " +
        "may differ between list sizes.",
    ).toEqual(withTwo);
  });
});
