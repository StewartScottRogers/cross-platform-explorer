/**
 * CPE-1881 round 5, item 1 — the REFUSED box's heading and copy button must derive from
 * `groupedFailures`, the SAME set the `<ul>` right below them actually renders, never from the
 * backend's `write_refusal.count`/`write_refusal.paths` alone.
 *
 * Those normally agree (same source, filtered two ways — `groupedFailures` is `summary.failures`
 * filtered by `write_refusal.paths` Set membership), but a duplicate path, a count/paths mismatch, or
 * a refused path with no matching `outcome: "failed"` entry makes them diverge — reopening the exact
 * "this box's own heading undercounts its own list" defect round 4 already fixed once (Critic
 * finding 1), through a second field this time.
 *
 * The fixture below deliberately diverges: `write_refusal.count: 3` against only two `skipped` entries
 * whose path is actually a member of `write_refusal.paths`. A heading/button reading off
 * `write_refusal.count` renders "3"; one reading off what is actually in the DOM renders "2". This is
 * the red-then-green fixture: with the round-4 field restored (`summary.writeRefusalCount` /
 * `summary.allWriteRefusalPaths`) both assertions below fail; with `groupedFailures.length` /
 * `absoluteWriteRefusalPaths` (built from `groupedFailures`) they pass.
 */
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/svelte";
import type { RevertOutcome } from "../bindings.gen";
import RevertOutcomePanel from "./RevertOutcomePanel.svelte";

const DIVERGENT_OUTCOME: RevertOutcome = {
  applied: 0,
  skipped: [
    { path: "a.txt", ok: false, error: "2 hard links", outcome: "failed" },
    { path: "b.txt", ok: false, error: "2 hard links", outcome: "failed" },
  ],
  held_back: null,
  write_refusal: {
    reason:
      "checkpoint entries could not be written because the destination is hard-linked (each has " +
      "more than one name)",
    // Deliberately mismatched with `paths.length` (2) — a duplicate path in the backend's own list, a
    // stale count, or a refused path with no `failed` entry can all produce this shape. The heading
    // and button must key off what is actually rendered (2 rows), not this scalar.
    count: 3,
    paths: ["a.txt", "b.txt"],
  },
};

describe("RevertOutcomePanel (CPE-1881 round 5, item 1 — count/paths divergence)", () => {
  it("heads the Refused box with the rendered row count, not the backend's write_refusal.count", () => {
    render(RevertOutcomePanel, { outcome: DIVERGENT_OUTCOME });
    const box = screen.getByTestId("revert-outcome-refused");
    // Fixture liveness: the list must actually hold 2 rows, or this proves nothing about a mismatch.
    expect(box.querySelectorAll("li")).toHaveLength(2);
    expect(box.textContent).toContain("Refused (2)");
    expect(box.textContent).not.toContain("Refused (3)");
  });

  it("labels the copy-refused-paths button with the same count it renders, not write_refusal.count", () => {
    render(RevertOutcomePanel, { outcome: DIVERGENT_OUTCOME });
    const button = screen.getByTestId("revert-outcome-copy-refused-paths");
    expect(button.textContent).toContain("Copy all 2 refused paths");
    expect(button.textContent).not.toContain("Copy all 3 refused paths");
  });
});

describe("RevertOutcomePanel (CPE-1881 round 5, item 2 — WHY nested inside REFUSED)", () => {
  it("renders the WHY paragraph as a descendant of the Refused box, not a sibling", () => {
    render(RevertOutcomePanel, { outcome: DIVERGENT_OUTCOME });
    const refusedBox = screen.getByTestId("revert-outcome-refused");
    const why = screen.getByTestId("revert-outcome-write-refusal");
    expect(refusedBox.contains(why)).toBe(true);
    // WHY must read above the list it explains, not below it.
    const list = refusedBox.querySelector("ul");
    expect(list).toBeTruthy();
    const followsWhy = Boolean(
      why.compareDocumentPosition(list as Node) & Node.DOCUMENT_POSITION_FOLLOWING,
    );
    expect(followsWhy).toBe(true);
  });
});

describe("RevertOutcomePanel (CPE-1881 round 5, item 4 — Failed box gets a copy button)", () => {
  const MANY_FAILURES: RevertOutcome = {
    applied: 0,
    skipped: [
      { path: "locked1.docx", ok: false, error: "permission denied", outcome: "failed" },
      { path: "locked2.docx", ok: false, error: "permission denied", outcome: "failed" },
    ],
    held_back: null,
    write_refusal: null,
  };

  it("offers a copy-failed-paths button mirroring the Held back / Refused affordance", () => {
    render(RevertOutcomePanel, { outcome: MANY_FAILURES });
    const button = screen.getByTestId("revert-outcome-copy-failed-paths");
    expect(button.textContent).toContain("Copy all 2 failed paths");
  });
});

describe("RevertOutcomePanel (CPE-1881 round 5, item 5 — shortened per-row refusal suffix)", () => {
  it("renders the short per-file link-count fact, not the repeated hard-link essay", () => {
    const outcome: RevertOutcome = {
      applied: 0,
      skipped: [{ path: "lib1.dll", ok: false, error: "2 hard links", outcome: "failed" }],
      held_back: null,
      write_refusal: {
        reason: "checkpoint entries could not be written because the destination is hard-linked",
        count: 1,
        paths: ["lib1.dll"],
      },
    };
    render(RevertOutcomePanel, { outcome });
    const box = screen.getByTestId("revert-outcome-refused");
    expect(box.textContent).toContain("2 hard links");
    expect(box.textContent).not.toContain("this file has 2 names");
  });
});
