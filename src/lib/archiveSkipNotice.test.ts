/**
 * CPE-1775 — a refused archive entry must be visible in the PRIMARY notice, not only in a tooltip.
 *
 * The bug: `extract_zip_archive_stream` has always recorded a refusal in `TransferReport.errors`, and
 * `App.svelte` read that array only when `report.failed > 0`. A refused entry increments neither
 * `failed` nor (before this ticket) `skipped`, so the user got a plain "1 item extracted" success toast
 * for an archive whose hostile entry had just been refused — the same experience as the bug CPE-1758 was
 * written to fix, with the mechanism changed from "written somewhere invisible" to "refused and not
 * mentioned".
 *
 * These assert on the notice a real `TransferReport` payload produces — the shape that crosses
 * `transfer://done` — rather than on any helper's internals, so a refactor that stops populating
 * `skipped` fails here.
 */
import { describe, it, expect } from "vitest";
import { archiveSkipNotice, transferReasonsLabel, type TransferReport } from "./transfers";
import { translate } from "./i18n";

const t = (key: string, params?: Record<string, string | number>) => translate("en", key, params);

/** A `transfer://done` payload, defaulting to a clean extract of two entries. */
function report(over: Partial<TransferReport> = {}): TransferReport {
  return { id: 1, op: "extract", transferred: 2, skipped: 0, failed: 0, cancelled: false, errors: [], ...over };
}

describe("CPE-1775 archiveSkipNotice", () => {
  it("says how many entries were skipped, in the headline, without hovering anything", () => {
    const msg = archiveSkipNotice(report({ transferred: 3, skipped: 2, errors: ["a: x", "b: y"] }), t);
    expect(msg).not.toBeNull();
    // Both halves: what landed, and what did not.
    expect(msg).toContain("3 items extracted");
    expect(msg).toContain("2 entries were skipped");
    // In the user's terms. "unsafe entry name" is developer language and must not be the headline.
    expect(msg).not.toContain("unsafe entry name");
    expect(msg!.toLowerCase()).toContain("safely");
    // And it must point at where the reasons are, in one obvious step.
    expect(msg!.toLowerCase()).toContain("operations panel");
  });

  it("uses the singular for exactly one skipped entry", () => {
    const msg = archiveSkipNotice(report({ transferred: 1, skipped: 1 }), t);
    expect(msg).toContain("1 item extracted");
    expect(msg).toContain("1 entry was skipped");
    expect(msg).not.toContain("entries");
  });

  it("says COMPRESSED for a compress op, not extracted", () => {
    const msg = archiveSkipNotice(report({ op: "compress", transferred: 4, skipped: 1 }), t);
    expect(msg).toContain("4 items compressed");
    expect(msg).not.toContain("extracted");
  });

  it("adds NOTHING when nothing was skipped — the normal path gains no new noise", () => {
    expect(archiveSkipNotice(report(), t)).toBeNull();
    expect(archiveSkipNotice(report({ transferred: 1 }), t)).toBeNull();
  });

  it("keeps a genuine FAILURE distinguishable from a skip", () => {
    // A failure has its own headline (the first error), and reporting it as "N skipped" would be the
    // mirror of the defect this ticket fixes. `failed` is also why `skipped` had to be a NEW field
    // rather than a reuse.
    expect(archiveSkipNotice(report({ skipped: 1, failed: 1 }), t)).toBeNull();
  });

  it("defers to the cancellation notice for a cancelled run", () => {
    expect(archiveSkipNotice(report({ skipped: 2, cancelled: true }), t)).toBeNull();
  });

  it("carries no attacker-controlled text — only counts", () => {
    // The reason strings embed the ARCHIVE's entry name. They belong in the panel, where they are
    // escaped through `displaySafePath` and can be read at leisure, not spliced into a 5-second toast.
    const hostile = "‮gnp.txt: unsafe entry name, skipped";
    const msg = archiveSkipNotice(report({ skipped: 1, errors: [hostile] }), t);
    expect(msg).not.toContain("gnp.txt");
    expect(msg).not.toContain("‮");
  });

  it("labels the panel's disclosure button with a count and noun that agree", () => {
    // Review nit: the first version took the NUMBER from `errors.length` and the NOUN from `skipped`.
    // The archive paths keep those 1:1, so it read correctly there and the bug was invisible — but a
    // copy/move row carries per-item conflict skips AND can carry a separate error line.
    expect(transferReasonsLabel({ skipped: 2, failed: 0, errors: ["a", "b", "c"] })).toBe(
      "· 2 skipped — why?",
    );
    // A genuine failure keeps the neutral wording and the error count; a skip must not be called a
    // failure, nor a failure a skip.
    expect(transferReasonsLabel({ skipped: 2, failed: 1, errors: ["a", "b", "c"] })).toBe(
      "· 3 problems — why?",
    );
    expect(transferReasonsLabel({ skipped: 0, failed: 1, errors: ["a"] })).toBe("· 1 problem — why?");
    expect(transferReasonsLabel({ skipped: 1, failed: 0, errors: ["a"] })).toBe("· 1 skipped — why?");
    // Nothing to disclose ⇒ no button at all, so a clean run is visually unchanged.
    expect(transferReasonsLabel({ skipped: 0, failed: 0, errors: [] })).toBeNull();
    expect(transferReasonsLabel(undefined)).toBeNull();
  });

  it("is translated, not English-only, in every locale the app ships as complete", () => {
    for (const loc of ["de", "es", "ja", "ru"] as const) {
      const msg = archiveSkipNotice(
        report({ transferred: 3, skipped: 2 }),
        (key, params) => translate(loc, key, params),
      );
      expect(msg, `${loc} must not fall back to English`).not.toBe(
        archiveSkipNotice(report({ transferred: 3, skipped: 2 }), t),
      );
      expect(msg, `${loc} left a placeholder unsubstituted: ${msg}`).not.toMatch(/\{\w+\}/);
      expect(msg, `${loc} lost the skipped count`).toContain("2");
    }
  });
});
