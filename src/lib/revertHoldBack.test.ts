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
      held_back: { outcome: "skipped_by_plan", count: 1, reason: "", next_step: "", retryable: true },
    };
    const s = summarizeRevert(outcome);
    // Not "2 skipped". One failed, one held back — different things, counted separately.
    expect(s.failed).toBe(1);
    expect(s.heldBack).toBe(1);
    expect(s.failures.map((f) => f.path)).toEqual(["locked.txt"]);
    expect(s.listed).toEqual(["added.txt"]);
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
      },
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
      },
    });
    expect(retryable.retryable).toBe(true);
    expect(permanent.retryable).toBe(false);
    // Both are hold-backs, so a UI branching on `heldBack` alone cannot tell them apart — which is
    // exactly why the wording must come from `nextStep` rather than being composed here.
    expect(retryable.heldBack).toBe(permanent.heldBack);
    expect(retryable.nextStep).not.toBe(permanent.nextStep);
  });

  it("collapses 200 identical hold-backs to one statement plus a count", () => {
    const skipped = Array.from({ length: 200 }, (_, i) => op(`added-${i}.txt`, "held_back_by_checkpoint"));
    const reason = "x".repeat(370); // the ~370-character paragraph CPE-1847 measured
    const s = summarizeRevert({
      applied: 1,
      skipped,
      held_back: { outcome: "held_back_by_checkpoint", count: 200, reason, next_step: "n", retryable: false },
    });
    expect(s.heldBack).toBe(200);
    // ONE copy of the paragraph reaches the screen, not 200 (which was ~185 KB).
    expect(s.reason).toBe(reason);
    const rendered = s.reason.length + s.listed.join("").length;
    expect(rendered).toBeLessThan(1000);
    expect(s.listed).toHaveLength(MAX_LISTED);
    expect(s.more).toBe(200 - MAX_LISTED);
  });

  it("says nothing about a hold-back when there was none", () => {
    const s = summarizeRevert({ applied: 3, skipped: [], held_back: null });
    expect(s.headline).toBe("Applied 3 changes.");
    expect(s.reason).toBe("");
    expect(s.nextStep).toBe("");
    expect(s.retryable).toBe(false);
    expect(s.listed).toEqual([]);
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
      held_back: { outcome: "held_back_by_checkpoint", count: 2, reason: same, next_step: same, retryable: false },
    });
    expect(s.applied).toBe(1);
    expect(s.failed).toBe(1);
    expect(s.heldBack).toBe(2);
    expect(new Set(ALL_STATES).size).toBe(4);
  });
});
