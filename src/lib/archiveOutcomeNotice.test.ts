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
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, it, expect } from "vitest";
import {
  archiveOutcomeNotice,
  archiveRunLandedNothing,
  transferReasonsLabel,
  type TransferReport,
} from "./transfers";
import { COMPLETE_LOCALES, translate } from "./i18n";

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
    // Round 2 fixed two holes here. It named four locales by hand — CPE-1932 says enumerate — and it
    // exercised only the SKIPPED branch, so `notice.archiveFailed{One,Many}`, the two keys this ticket
    // added, were never asked for in any language. Both branches, every complete locale.
    const locales = COMPLETE_LOCALES.filter((l) => l !== "en");
    expect(locales.length, "COMPLETE_LOCALES came back near-empty — this leg would pass vacuously")
      .toBeGreaterThanOrEqual(8);
    const branches = [
      { name: "skipped", over: { transferred: 3, skipped: 2 }, count: "2" },
      { name: "failed", over: { transferred: 3, failed: 2 }, count: "2" },
      { name: "one failed", over: { transferred: 1, failed: 1 }, count: "1" },
      { name: "both", over: { transferred: 3, skipped: 2, failed: 1 }, count: "2" },
    ] as const;
    for (const loc of locales) {
      for (const b of branches) {
        const en = archiveOutcomeNotice(report(b.over), t);
        const msg = archiveOutcomeNotice(report(b.over), (key, params) => translate(loc, key, params));
        expect(msg, `${loc}/${b.name} produced nothing`).not.toBeNull();
        expect(msg, `${loc}/${b.name} must not fall back to English`).not.toBe(en);
        expect(msg, `${loc}/${b.name} left a placeholder unsubstituted: ${msg}`).not.toMatch(/\{\w+\}/);
        expect(msg, `${loc}/${b.name} lost its count`).toContain(b.count);
      }
    }
  });

  /**
   * **CPE-1935 round 2 — a clean run that wrote zero FILES is a success, not a failure.**
   *
   * `done` counts files only, so an archive holding nothing but directories finishes
   * `{ done: 0, failed: 0, skipped: 0, errors: [] }` — measured at the engine on both this branch and
   * its merge base. Round 1 gated `App.svelte`'s failure toast on `transferred === 0 && skipped === 0`,
   * which is true of that success, and so re-created this ticket's own defect (returning *before*
   * `onSuccess` and the pane refresh) on a different input.
   */
  describe("archiveRunLandedNothing", () => {
    it("is FALSE for a clean run that wrote no files — the empty-folder archive", () => {
      // Extract an archive of empty folders: the folders are created, `done` stays 0.
      expect(archiveRunLandedNothing({ transferred: 0, skipped: 0, failed: 0 })).toBe(false);
      // Right-click an empty folder -> Compress: the .zip is written, `done` stays 0.
      expect(archiveRunLandedNothing({ transferred: 0, skipped: 0, failed: 0 })).toBe(false);
    });

    it("is TRUE only when something failed and nothing at all landed", () => {
      expect(archiveRunLandedNothing({ transferred: 0, skipped: 0, failed: 1 })).toBe(true);
      expect(archiveRunLandedNothing({ transferred: 0, skipped: 0, failed: 9 })).toBe(true);
    });

    it("is FALSE whenever anything landed or was refused, however many also failed", () => {
      // The whole point of the ticket: 23 of 27 files are on disk and the user must be shown them.
      expect(archiveRunLandedNothing({ transferred: 23, skipped: 0, failed: 4 })).toBe(false);
      // A refusal is a thing that happened and has its own headline; it is not "nothing landed".
      expect(archiveRunLandedNothing({ transferred: 0, skipped: 1, failed: 1 })).toBe(false);
      expect(archiveRunLandedNothing({ transferred: 1, skipped: 0, failed: 0 })).toBe(false);
    });

    it("is what App.svelte actually branches on, at BOTH archive sites", () => {
      // CPE-1933: a predicate with a green unit test that no caller uses is worth nothing, and the
      // defect this replaces was in the caller, not in any helper. So read the caller.
      //
      // Red-proofed (CPE-1933 rule 3): putting the round-1 predicate back at the failure-toast site —
      // `if (r.transferred === 0 && r.skipped === 0) {` — fails this leg with
      // *"App.svelte's failure-toast branch no longer routes through archiveRunLandedNothing:
      // expected +0 to be 1"*, 1 failed / 14 passed. It re-reads the file on every run, so it cannot
      // go stale the way the claim it replaces did.
      //
      // Whole-line comments are dropped first because the two sites are documented with prose that
      // quotes the old expression verbatim. This filter cannot hide a violation from the negative
      // assertion below — a trailing comment quoting the expression would make that assertion RED, the
      // conservative direction — and the positive assertions require the match to begin the line, so a
      // comment cannot satisfy one.
      const appSvelte = readFileSync(join(process.cwd(), "src", "App.svelte"), "utf8");
      expect(appSvelte.length, "App.svelte came back empty — this leg would pass vacuously")
        .toBeGreaterThan(10_000);
      const code = appSvelte
        .split(/\r?\n/)
        .filter((l) => {
          const s = l.trim();
          return !s.startsWith("//") && !s.startsWith("*") && !s.startsWith("/*");
        });
      const startsWith = (re: RegExp) => code.filter((l) => re.test(l.trim())).length;

      // The no-`pending` fallback: refresh unless the run is cancelled or delivered nothing.
      expect(
        startsWith(/^if \(!r\.cancelled && !archiveRunLandedNothing\(r\)\) \{/),
        "App.svelte's fallback branch no longer routes through archiveRunLandedNothing",
      ).toBe(1);
      // The registered-`pending` path: the failure toast that returns before `onSuccess`.
      expect(
        startsWith(/^if \(archiveRunLandedNothing\(r\)\) \{/),
        "App.svelte's failure-toast branch no longer routes through archiveRunLandedNothing",
      ).toBe(1);
      // And the round-1 predicates must not come back in either shape.
      for (const gone of ["r.transferred === 0 && r.skipped === 0", "r.transferred > 0 || r.skipped > 0"]) {
        expect(
          code.filter((l) => l.includes(gone)),
          `App.svelte has gone back to \`${gone}\`, which treats a clean zero-file run as a failure`,
        ).toEqual([]);
      }
    });
  });
});
