---
id: CPE-1757
title: Make the bidi-escape an enforced invariant rather than a remembered one, and close the residual call sites
type: task
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-15
closed:
---

## Problem

From the PR #917 (CPE-1712) round-2 review, which approved that PR and named this as the follow-up that
turns "we fixed it this time" into "it stays fixed".

CPE-1712 stopped a right-to-left override in a filename from being drawn as a lie (`‮gnp.txt` rendering as
`txt.png`) by escaping bidi/format characters wherever a name reaches the DOM — now ~20 surfaces plus 10
confirmation strings. Round 1 of that PR missed most of them; round 2 added `displaySafePath` beside
`displaySafeName` as a discoverability signpost.

**The signpost is honest but it is not a barrier.** The reviewer's verdict on the question "does anything
now *prevent* a new call site from rendering raw?" was: **no — it still relies on the author remembering.**
Round 1's omission is the proof that remembering is not enough.

## The fix: a guard test, following two patterns this repo already owns

- `src/lib/sectionDocs.test.ts` fails CI when a `Section` lacks its doc page.
- `src/lib/invoke.ts`'s convention uses a guard test with an explicit **allowlist** to stop production code
  importing `invoke` from the wrong place.

Do the same here: a grep-based guard over `src/lib/components/*.svelte` (and the relevant `App.svelte`
strings) for raw `{…name}` / `{…path}` / `baseName(…)` / `basename(…)` renders, with the disclosed
not-yet-covered list as its allowlist. That converts the disclosure into an **enforced invariant** and makes
the uncovered list impossible to grow silently.

## Residual call sites, in priority order

**Fix first — undisclosed and it is a real decision surface:**

- `ConflictDialog.svelte:118,119` — `{f.path}` raw. This is the **overwrite-or-skip decision** on a
  copy/move collision, the closest thing on the remaining list to a "what am I about to do" moment, and it
  is not on the disclosed list at all.

**Then — an inconsistency inside the claimed-covered set:**

- `FileNameSearchDialog.svelte:97` — `<span class="root" title={root}>{baseName(root) || root}</span>` is
  still raw, while the byte-identical span in `ContentSearchDialog.svelte:110` and
  `DuplicatesDialog.svelte:107` **was** fixed in the same commit. Low risk (the user's own search root, not
  attacker-supplied) but exactly the kind of drift a guard test catches.

**Then — undisclosed sites that render names off the filesystem:**

- `RepoBrowser.svelte:360,362`; `AgentTimeline.svelte:903,983`; `ConsultedFiles.svelte:36`;
  `SessionHistoryDialog.svelte:124`; `IntegrityDialog.svelte:108`; `CheckpointDialog.svelte:284`;
  `DiffSideBySide.svelte:30`; `InspectCryptoDialog.svelte:60`; `BoardView.svelte:396`;
  `CopilotDialog.svelte:165`.

**Then — the already-disclosed list**, which the reviewer confirmed *can* show attacker-supplied names
(harvested by recursive scans of arbitrary directories, the same provenance as the archive-safety dialog
that was fixed) but judged genuinely lower-consequence, being diagnostic read-outs rather than decisions:
`ContentIndexSearchDialog:213`, `FileHealthDialog:524/585/643/683`, `NearDuplicatesDialog:206`,
`SimilarImagesDialog:206`, `DeclutterDialog:228`, `BatchMediaDialog:534/536`, `SplitFileDialog`,
`JoinPartsDialog`, `ExplorerPane`'s agent chip, `TerminalPanel`'s tab label, `Sidebar`'s agent-session chip.

Purely user-typed labels (macro/workspace/connection names) are **not** untrusted filesystem data and are
correctly out of scope.

## Also worth doing

`src/lib/filename.ts:112-118` — `displaySafePath`'s body is dead machinery now that the output is
established as byte-identical to `displaySafeName`. Make it `return displaySafeName(path);`, keep the doc
comment verbatim: same discoverability, zero risk of the two implementations drifting apart, six fewer lines.

## Acceptance criteria

- [ ] A guard test fails CI when a component renders a filesystem-derived name or path without the escape,
      with an explicit allowlist for anything deliberately excluded.
- [ ] Adding a new raw render in a new component reds that guard — demonstrate it by adding one, watching it
      fail, and removing it.
- [ ] `ConflictDialog` and `FileNameSearchDialog:97` are covered, and the allowlist matches the doc's
      not-yet-covered list exactly (no third, drifting version of the truth).
- [ ] `src/docs/03-explorer.md`'s covered/not-covered lists still match reality after the change.
- [ ] `displaySafePath` delegates rather than duplicating.
- [ ] The escape is still **display-only** — renaming edits the real name, thumbnails and type detection use
      the raw name, and no bracketed name reaches the backend as a path. `QuickLook`'s `<img src>` carrying
      the raw path is the correct behaviour and must stay.

## Notes

Related: CPE-1712 (PR #917), CPE-1709 (the on-disk encoder, deliberately untouched by both).
