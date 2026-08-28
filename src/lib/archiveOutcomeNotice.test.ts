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
import { archiveOutcomeNotice, transferReasonsLabel, type TransferReport } from "./transfers";
import { translate } from "./i18n";

const t = (key: string, params?: Record<string, string | number>) => translate("en", key, params);

/** A `transfer://done` payload, defaulting to a clean extract of two entries. */
function report(over: Partial<TransferReport> = {}): TransferReport {
  return { id: 1, op: "extract", transferred: 2, skipped: 0, failed: 0, cancelled: false, errors: [], ...over };
}

describe("CPE-1775 / CPE-1935 archiveOutcomeNotice", () => {
  it("says how many entries were skipped, in the headline, without hovering anything", () => {
    const msg = archiveOutcomeNotice(report({ transferred: 3, skipped: 2, errors: ["a: x", "b: y"] }), t);
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
    const msg = archiveOutcomeNotice(report({ transferred: 1, skipped: 1 }), t);
    expect(msg).toContain("1 item extracted");
    expect(msg).toContain("1 entry was skipped");
    expect(msg).not.toContain("entries");
  });

  it("says COMPRESSED for a compress op, not extracted", () => {
    const msg = archiveOutcomeNotice(report({ op: "compress", transferred: 4, skipped: 1 }), t);
    expect(msg).toContain("4 items compressed");
    expect(msg).not.toContain("extracted");
  });

  it("adds NOTHING when nothing was skipped or failed — the normal path gains no new noise", () => {
    expect(archiveOutcomeNotice(report(), t)).toBeNull();
    expect(archiveOutcomeNotice(report({ transferred: 1 }), t)).toBeNull();
  });

  it("keeps a genuine FAILURE distinguishable from a skip", () => {
    // Reporting a failure as "N skipped" would be the mirror of the defect CPE-1775 fixed — a guard
    // choosing not to write is not the same event as the filesystem refusing to. `failed` is why
    // `skipped` had to be a NEW field rather than a reuse, and the two nouns must stay apart.
    const msg = archiveOutcomeNotice(report({ transferred: 3, skipped: 1, failed: 1 }), t)!;
    expect(msg).toContain("1 entry couldn't be written");
    expect(msg).toContain("1 entry was skipped");
    // The more actionable half first: a file the user asked for and did not get.
    expect(msg.indexOf("couldn't be written")).toBeLessThan(msg.indexOf("was skipped"));
  });

  it("CPE-1935: a partly-failed extraction still says what LANDED", () => {
    // The ticket's own shape, at the UI. `failed > 0` used to make this function return null, and
    // `App.svelte` then showed `errors[0]` — one sentence naming the single entry that did NOT land,
    // about a run that had written 23 of 27 files. The headline must carry both numbers.
    const msg = archiveOutcomeNotice(report({ transferred: 23, failed: 4 }), t)!;
    expect(msg).toContain("23 items extracted");
    expect(msg).toContain("4 entries couldn't be written");
    // ...and say the rest of the archive is there, so "re-run" is an informed choice, not a guess.
    expect(msg.toLowerCase()).toContain("the rest of the archive was extracted");
    expect(msg.toLowerCase()).toContain("operations panel");
  });

  it("CPE-1935: uses the singular for exactly one failed entry", () => {
    const msg = archiveOutcomeNotice(report({ transferred: 1, failed: 1 }), t)!;
    expect(msg).toContain("1 entry couldn't be written");
    expect(msg).not.toContain("entries");
  });

  it("defers to the cancellation notice for a cancelled run", () => {
    expect(archiveOutcomeNotice(report({ skipped: 2, cancelled: true }), t)).toBeNull();
  });

  it("carries no attacker-controlled text — only counts", () => {
    // The reason strings embed the ARCHIVE's entry name. They belong in the panel, where they are
    // escaped through `displaySafePath` and can be read at leisure, not spliced into a 5-second toast.
    const hostile = "‮gnp.txt: unsafe entry name, skipped";
    for (const over of [{ skipped: 1 }, { failed: 1 }] as const) {
      // CPE-1935 added the `failed` branch, so it gets the same check rather than inheriting the
      // property by assumption.
      const msg = archiveOutcomeNotice(report({ ...over, errors: [hostile] }), t);
      expect(msg).not.toContain("gnp.txt");
      expect(msg).not.toContain("‮");
    }
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
      const msg = archiveOutcomeNotice(
        report({ transferred: 3, skipped: 2 }),
        (key, params) => translate(loc, key, params),
      );
      expect(msg, `${loc} must not fall back to English`).not.toBe(
        archiveOutcomeNotice(report({ transferred: 3, skipped: 2 }), t),
      );
      expect(msg, `${loc} left a placeholder unsubstituted: ${msg}`).not.toMatch(/\{\w+\}/);
      expect(msg, `${loc} lost the skipped count`).toContain("2");
    }
  });
});
