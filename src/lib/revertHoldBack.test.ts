/**
 * CPE-1845 — the frontend half of "a deliberate hold-back and a real failure are distinguishable only
 * by prose".
 *
 * These tests never read `error`, `reason` or `next_step` to decide *what a result is*. They read
 * `outcome`. Where a message is asserted at all it is to prove the shared explanation was carried
 * through once, not to classify anything — which is the whole point of the ticket.
 *
 * Red-proofs recorded in the ticket's Work Log; each one is a single-line change to `summarizeRevert`
 * or to `revert_engine.rs`, observed red, reverted.
 */
import { describe, it, expect } from "vitest";
import { summarizeRevert, MAX_LISTED } from "./revertHoldBack";
import type { OpOutcome, RevertOutcome } from "./bindings.gen";

const op = (path: string, outcome: OpOutcome, error = "") => ({
  path,
  ok: outcome === "applied",
  error,
  outcome,
});

/** The four states, exactly as the wire spells them. */
const ALL_STATES: OpOutcome[] = ["applied", "failed", "skipped_by_plan", "held_back_by_checkpoint"];

describe("summarizeRevert (CPE-1845)", () => {
  it("separates a genuine failure from a deliberate hold-back with every message blank", () => {
    const outcome: RevertOutcome = {
      applied: 1,
      skipped: [op("locked.txt", "failed"), op("added.txt", "skipped_by_plan")],
      held_back: {
        outcome: "skipped_by_plan",
        count: 1,
        reason: "",
        next_step: "",
        retryable: true,
        advises_manual_delete: false,
      },
      write_refusal: null,
    };
    const s = summarizeRevert(outcome);
    // Not "2 skipped". One failed, one held back — different things, counted separately.
    expect(s.failed).toBe(1);
    expect(s.heldBack).toBe(1);
    expect(s.failures.map((f) => f.path)).toEqual(["locked.txt"]);
    expect(s.listed).toEqual([{ path: "added.txt", detail: "" }]);
    expect(s.headline).toBe("Applied 1 change, 1 failed, 1 deletion held back.");
  });

  it("tells the retryable hold-back from the one no re-run can fix — structurally", () => {
    const retryable = summarizeRevert({
      applied: 0,
      skipped: [op("a.txt", "skipped_by_plan")],
      held_back: {
        outcome: "skipped_by_plan",
        count: 1,
        reason: "a blob is missing",
        next_step: "run the revert again",
        retryable: true,
        advises_manual_delete: false,
      },
      write_refusal: null,
    });
    const permanent = summarizeRevert({
      applied: 0,
      skipped: [op("a.txt", "held_back_by_checkpoint")],
      held_back: {
        outcome: "held_back_by_checkpoint",
        count: 1,
        reason: "this checkpoint records no files",
        next_step: "delete these files yourself",
        retryable: false,
        advises_manual_delete: true,
      },
      write_refusal: null,
    });
    expect(retryable.retryable).toBe(true);
    expect(permanent.retryable).toBe(false);
    // Both are hold-backs, so a UI branching on `heldBack` alone cannot tell them apart — which is
    // exactly why the wording must come from `nextStep` rather than being composed here.
    expect(retryable.heldBack).toBe(permanent.heldBack);
    expect(retryable.nextStep).not.toBe(permanent.nextStep);
    // CPE-1869: the copy-full-list affordance is gated on this field, read straight off the wire —
    // never inferred from `nextStep`'s wording.
    expect(retryable.advisesManualDelete).toBe(false);
    expect(permanent.advisesManualDelete).toBe(true);
  });

  it("collapses 200 identical hold-backs to one statement plus a count", () => {
    const skipped = Array.from({ length: 200 }, (_, i) => op(`added-${i}.txt`, "held_back_by_checkpoint"));
    const reason = "x".repeat(370); // the ~370-character paragraph CPE-1847 measured
    const s = summarizeRevert({
      applied: 1,
      skipped,
      held_back: {
        outcome: "held_back_by_checkpoint",
        count: 200,
        reason,
        next_step: "n",
        retryable: false,
        advises_manual_delete: true,
      },
      write_refusal: null,
    });
    expect(s.heldBack).toBe(200);
    // ONE copy of the paragraph reaches the screen, not 200 (which was ~185 KB).
    expect(s.reason).toBe(reason);
    const rendered = s.reason.length + s.listed.join("").length;
    expect(rendered).toBeLessThan(1000);
    expect(s.listed).toHaveLength(MAX_LISTED);
    expect(s.more).toBe(200 - MAX_LISTED);
    // CPE-1869: the on-screen preview is capped, but the full 200 are still retrievable — this is the
    // whole point of the ticket ("the held-back list tells you to delete files it will not show you").
    expect(s.allHeldBackPaths).toHaveLength(200);
    expect(s.allHeldBackPaths[0]).toBe("added-0.txt");
    expect(s.allHeldBackPaths[199]).toBe("added-199.txt");
  });

  it("says nothing about a hold-back when there was none", () => {
    const s = summarizeRevert({ applied: 3, skipped: [], held_back: null, write_refusal: null });
    expect(s.headline).toBe("Applied 3 changes.");
    expect(s.reason).toBe("");
    expect(s.nextStep).toBe("");
    expect(s.retryable).toBe(false);
    expect(s.listed).toEqual([]);
    // CPE-1869: no affordance and nothing to copy when nothing was held back.
    expect(s.advisesManualDelete).toBe(false);
    expect(s.allHeldBackPaths).toEqual([]);
    // CPE-1881: nothing to say about grouped write refusals either.
    expect(s.writeRefusalReason).toBe("");
    expect(s.writeRefusalCount).toBe(0);
  });

  it("CPE-1881: collapses 200 identical hard-link write refusals to one paragraph plus a count", () => {
    // Every refused write is STILL a normal `failed` entry in `skipped` (unlike a delete hold-back,
    // grouping a write refusal doesn't change its `outcome`) — this proves the shared paragraph is
    // ADDITIVE, not a replacement for per-path visibility: all 200 paths are still in `failures`, each
    // with its own (now short) per-path fact, and the paragraph reaches the screen exactly once.
    const reason =
      "200 checkpoint entries could not be written because the destination is hard-linked " + "x".repeat(300);
    const skipped = Array.from({ length: 200 }, (_, i) =>
      op(`f${i}.txt`, "failed", `this file has 2 names (it is hard-linked)`),
    );
    const s = summarizeRevert({
      applied: 0,
      skipped,
      held_back: null,
      write_refusal: { reason, count: 200 },
    });
    expect(s.failed).toBe(200);
    expect(s.failures).toHaveLength(200);
    expect(s.failures.every((f) => f.error === "this file has 2 names (it is hard-linked)")).toBe(true);
    // ONE copy of the shared paragraph reaches the screen, not 200.
    expect(s.writeRefusalReason).toBe(reason);
    expect(s.writeRefusalCount).toBe(200);
    const rendered = s.writeRefusalReason.length + s.failures.map((f) => f.error.length).reduce((a, b) => a + b, 0);
    expect(rendered).toBeLessThan(20_000);
  });

  it("carries the per-path detail the backend produces, and only when there is one", () => {
    // The alias/collision hold-back is the case with a real per-path fact: WHICH checkpoint entry this
    // path collides with. Dropping it threw away the most useful thing on the row.
    const s = summarizeRevert({
      applied: 0,
      skipped: [
        { path: "A.txt", ok: false, error: 'same file as checkpoint entry "a.txt"', outcome: "held_back_by_checkpoint" },
        { path: "B.txt", ok: false, error: "", outcome: "held_back_by_checkpoint" },
      ],
      held_back: {
        outcome: "held_back_by_checkpoint",
        count: 2,
        reason: "r",
        next_step: "n",
        retryable: false,
        // The alias/collision hold-back — CPE-1869's own "must not get the affordance" case.
        advises_manual_delete: false,
      },
      write_refusal: null,
    });
    expect(s.listed).toEqual([
      { path: "A.txt", detail: 'same file as checkpoint entry "a.txt"' },
      { path: "B.txt", detail: "" },
    ]);
    // CPE-1869: this is the case a delete affordance must never appear on.
    expect(s.advisesManualDelete).toBe(false);
  });

  it("never prints \"and 1 more\" — at one over the cap it lists the extra name instead", () => {
    const held = (n: number) =>
      summarizeRevert({
        applied: 0,
        skipped: Array.from({ length: n }, (_, i) => ({
          path: `p-${i}.txt`,
          ok: false,
          error: "",
          outcome: "held_back_by_checkpoint" as const,
        })),
        held_back: {
          outcome: "held_back_by_checkpoint",
          count: n,
          reason: "r",
          next_step: "n",
          retryable: false,
          advises_manual_delete: true,
        },
        write_refusal: null,
      });

    const exactly = held(MAX_LISTED);
    expect(exactly.listed).toHaveLength(MAX_LISTED);
    expect(exactly.more).toBe(0);

    // One over: "and 1 more" is a longer line than the name it replaces, so list it.
    const oneOver = held(MAX_LISTED + 1);
    expect(oneOver.listed).toHaveLength(MAX_LISTED + 1);
    expect(oneOver.more).toBe(0);

    // Two over: truncating now genuinely saves lines.
    const twoOver = held(MAX_LISTED + 2);
    expect(twoOver.listed).toHaveLength(MAX_LISTED);
    expect(twoOver.more).toBe(2);
  });

  it("buckets every one of the four wire states, and never by message text", () => {
    // Each state carries the SAME prose, so anything reading `error` cannot separate them; the
    // buckets below must still come out right.
    const same = "identical text on every entry";
    const s = summarizeRevert({
      applied: 1,
      skipped: [
        op("f.txt", "failed", same),
        op("p.txt", "skipped_by_plan", same),
        op("c.txt", "held_back_by_checkpoint", same),
      ],
      held_back: {
        outcome: "held_back_by_checkpoint",
        count: 2,
        reason: same,
        next_step: same,
        retryable: false,
        advises_manual_delete: true,
      },
      write_refusal: null,
    });
    expect(s.applied).toBe(1);
    expect(s.failed).toBe(1);
    expect(s.heldBack).toBe(2);
    expect(new Set(ALL_STATES).size).toBe(4);
  });
});
