---
id: CPE-1845
title: a deliberate hold-back and a real failure are distinguishable only by string-matching the message
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-21
closed:
---

## Problem

`OpResult` (`crates/server/src/checkpoint_store.rs:151-170`) has **no structural flag** separating a
deliberate hold-back from a genuine failure. CPE-1823's stand-down produces both through the same field.

Measured by the independent Reviewer during CPE-1823's round-4 review, on a staged checkpoint with one
unrestorable key, 200 files added since, and one legitimate restorable entry:

```
applied=1  skipped=201  held_back_deletes=200  survivors=200
```

200 of those 201 results are deliberate hold-backs carrying an **identical paragraph**. A UI can only
tell them apart by string-matching `"not deleted:"`.

## Why it matters

This repo's standing rule — CPE-1804/CPE-1805, CPE-1806, CPE-1814 — is that **a silent skip must not
read as a pass**. The inverse now applies: a deliberate, correct, fail-safe hold-back must not read as
a failure, and 200 of them must not read as 200 separate problems.

There is a second, sharper wrinkle. The recorded UI wording is *"held back, re-run after fixing"*. That
is correct for the **plan-skipped** branch. It is **wrong** for the **checkpoint-keyed** branch, where
re-running on this platform can never help — a Linux capture containing one colon-named file will never
delete-clean on Windows. The user is told to retry something that cannot succeed, and no next step is
offered.

## Acceptance criteria

- [ ] `OpResult` carries a structural discriminant — a field or enum variant — for at least: applied,
      failed, skipped-by-plan (retryable), held-back-by-checkpoint (**not** retryable on this platform).
      Whoever does the UI work needs a field, not a prefix.
- [ ] The 200-identical-paragraphs case collapses to one statement plus a count, per the pill/summary
      conventions already used elsewhere. Do not ship 200 identical rows.
- [ ] The non-retryable branch offers a real next step or explicitly states there is none on this
      platform. It must not say "re-run".
- [ ] A test asserts a consumer can distinguish the four states **without** matching on message text.
      Red-proof it by collapsing two states onto one discriminant and observing red.
- [ ] Check every existing consumer of `OpResult` for message-text matching and convert it.

## Notes

Filed from CPE-1823's round-4 Reviewer findings, which explicitly recommended a separate ticket rather
than absorbing it into a PR already four rounds deep. The Reviewer designed the stand-down being
critiqued here and confirmed the trade is worth it — this is about reporting it honestly, not undoing it.

Related: CPE-1823 (the stand-down), CPE-1806 and CPE-1814 (the same "a skip is not a pass" family).

## Work Log

### 2026-08-22 — fixed, branch `cpe-1845-opresult-discriminant`

**The discriminant.** `OpResult` (`crates/server/src/model.rs`) gains `outcome: OpOutcome`, a four-variant
enum serialised `snake_case` so the TS side reads a discriminated union rather than a prose prefix:

| variant | wire | meaning | what the user can do |
|---|---|---|---|
| `Applied` | `"applied"` | performed | nothing |
| `Failed` | `"failed"` | attempted, failed | fix the named cause, try again |
| `SkippedByPlan` | `"skipped_by_plan"` | **held back, RETRYABLE** — something else in the run failed, so this one's premise is unproven | fix that, re-run |
| `HeldBackByCheckpoint` | `"held_back_by_checkpoint"` | **held back, NOT retryable here** — the checkpoint itself cannot be read correctly on this platform | see the next step; re-running cannot help |

`ok` is kept and is now *derived*: exactly `outcome == Applied`, pinned by
`model::tests::ok_is_derived_from_outcome_and_never_set_independently`. `OpOutcome::retryable()` and
`is_held_back()` are conveniences; the discriminant is the variant, and both helpers' docs say so.

**The 200 identical paragraphs.** `RestoreReport` no longer pushes the shared paragraph onto every
held-back delete. It carries `held_back: Option<HeldBack>` — **one** `reason`, **one** `next_step`, one
`outcome`, and a `paths: Vec<(String, String)>` where the per-path string carries only what genuinely
differs (empty for the blanket branches; for the round-5 resolution branch it names the colliding
checkpoint entry, which really is per path). `RevertOutcome` mirrors that to the wire as
`held_back: Option<HeldBackSummary> { outcome, count, reason, next_step, retryable }`, with the held-back
paths still listed in `skipped` (so `skipped.len()` keeps meaning "actions that did not happen" for every
existing caller) but carrying an empty `error`. Measured on the ticket's own 200-delete shape:
**60,400 bytes of per-path prose → 0**, one 302-character statement plus `count: 200`.

**The second half — the wording.** ~~The `report.skipped` branch (a locked file, a missing blob) is the
only transient one and is the only branch that says "run the revert again".~~ **That sentence was false
when written, and the code it described was wrong the same way — corrected in round 2, see below.** What
stands: the two checkpoint-keyed branches — an emptied `files` map (CPE-1847) and a key this platform
cannot restore (CPE-1823) — are `HeldBackByCheckpoint` and say plainly that *re-running will not change
this on this computer*, then offer what actually works. The round-5 resolution branch is also
`HeldBackByCheckpoint` and says the honest thing for it — nothing needs doing, those files **are** the
checkpoint's content under another spelling on this volume, so deleting them would destroy checkpoint
content.

**Consumer enumeration (AC 5), done before any conversion.** Two searches: `"not deleted"` across
`crates/` + `src-tauri/`, and every `OpResult` / `RevertOutcome` / `.skipped` reference across Rust, TS
and Svelte.

*Classified by message text — all converted:*

1. `checkpoint_store.rs:1340` — `op.error.starts_with("not deleted:")` plus
   `op.error.contains("{n} file")`. Now asserts `op.outcome == HeldBackByCheckpoint`, `!op.ok`, and reads
   the count off `held_back.count` as a **number**. It also now asserts the next step does not say
   "run the revert again".
2. `revert_engine.rs:980` / `:1052` / `:1165` / `:1275` — four `why.contains("not deleted")` assertions.
   All four go through one new helper, `held_back_as(&report, path) -> Option<OpOutcome>`, which asks the
   structure and deliberately does **not** look at `report.skipped` (a hold-back landing there would now
   be the bug, not a pass). Two of them assert `HeldBackByCheckpoint`, two `SkippedByPlan` — which is
   itself new information the prose match could not express.

*Not message-text classification, left alone (recorded so the next reader does not re-audit them):* the
`.error.contains(...)` assertions in `backup.rs`, `copilot.rs`, `organize_apply.rs` and `src-tauri/lib.rs`
assert that a **genuine failure's** reason is helpful. They do not decide *what state* a result is in, so
they are not the coupling this ticket removes.

*Frontend:* **zero** message-text matchers existed — the three screens could not distinguish the states at
all. **Three** files declare their own structural subsets of `OpResult` (`path`/`ok`/`error`) and read
`ok` only, so all three are unaffected: `folderWatch.ts:19`, `BackupDashboard.svelte:15`, and
`App.svelte:251` (`reportResults` at `:3110` filters on `!r.ok` and branches no further) — the third was
missing from this list until review round 2 pointed it out. One type-only fix: a `folderWatch.test.ts`
fixture literal needed `outcome: "failed"`.

**AC 6 — surfacing the reasons: done, and the docs corrected to match.** The docs claim was real:
CPE-1847's reviewer found `src/docs/16-checkpoints.md` promising "exactly which cleanups did not happen
**and why**" while `CheckpointDialog.svelte` (`:311`, `:172`), `AgentTimeline.svelte:813` and
`CopilotDialog.svelte:299` each rendered only `skipped {n}` and dropped every `error`. All three now
render one shared `RevertOutcomePanel.svelte` (over a pure `src/lib/revertHoldBack.ts` summariser), which
shows **applied / failed / held back as three separate counts**, then the single reason, the next step,
and up to 8 held-back path names followed by "and N more". Genuine failures keep their own per-path
reasons, which are distinct and worth showing. The doc's hedge ("recorded per file but is not shown in
the dialog yet — a future update will list it") is deleted, and the section now describes what the screen
actually does, including the temporary-vs-permanent split. `sectionDocs.ts` is unchanged — no new
`Section`.

**Bindings.** `OpResult`, `OpOutcome`, `RevertOutcome` and `HeldBackSummary` are `specta::Type`, so
`bindings.gen.ts` was regenerated with
`cargo run --bin export_bindings --features "specta-bindings sidecar-platform"` (+102/−6) and re-run at
the end to confirm it is not stale — the trap CPE-1844 hit.

### Evidence — red-proofs, one line each, observed red then reverted

| Guard | Line broken | Observed |
|---|---|---|
| the discriminant itself (AC 4's own red-proof: collapse two states onto one) | `revert_engine.rs` retryable branch `Some(HeldBack::new(OpOutcome::SkippedByPlan,` → `OpOutcome::HeldBackByCheckpoint` | `cpe_1845_a_consumer_tells_the_four_states_apart_with_every_message_erased` red: `left: {"Failed", "HeldBackByCheckpoint"} right: {"Failed", "SkippedByPlan"}` — with every message string already blanked, so nothing but the discriminant was left to fail on |
| the one-statement collapse | `revert_engine.rs` `group.paths.push((action.path.clone(), String::new()))` → `…, group.reason.clone()` | same test red: `the shared explanation must NOT be copied onto each path — that is the ~185 KB CPE-1847 measured; 60400 bytes of per-path prose found` (200 paths × the 302-char paragraph) |
| the non-retryable wording | `revert_engine.rs` unrestorable-key `next_step`: `"There is no fix for this on this computer:"` → `"Run the revert again once that is fixed:"` | `cpe_1845_only_the_retryable_hold_back_may_tell_the_user_to_run_it_again` red on the `unrestorable-key` leg: `must not tell the user to "run the revert again"` |
| the reasons actually reaching the DOM | `RevertOutcomePanel.svelte` `{#if summary.reason}` → `{#if false && summary.reason}` | 4 of the new `CheckpointDialog` tests red: `expected 'Applied 1 change, 2 deletions held ba…' to contain 'THE-ONE-REASON'` / `'THE-NEXT-STEP'` / `'RETRYABLE-REASON'`, and the once-only count `expected +0 to be 1` |
| the frontend reading the field, not `ok` | `revertHoldBack.ts` `.filter((r) => r.outcome === "failed")` → `.filter((r) => !r.ok)` | 4 red across both files: `expected 2 to be 1` (a hold-back counted as a failure), and on screen `expected 'Applied 1 change, 2 failed, 2 deletio…' not to contain 'failed'` |

**Fixture liveness is folded into helpers, not repeated per test** (the CPE-1844 lesson). `live_hold_back`
refuses a report that armed no hold-back, or that carries a hold-back with no paths / no reason / no next
step, so a fixture that quietly stopped arming cannot pass by omission. The four-states test additionally
asserts each of its three runs is live *before* classifying: run 1 applied something, runs 2 and 3 left
`added.txt` on disk (i.e. the delete really was held back). The missing-blob hash is asserted absent
before it is relied on to fail.

**A test of mine failed for the right reason and was rewritten.** The first version of the wording test
banned the substring `"re-run"` outright, and went red on the *correct* message ("Re-running will not
change this — …"). Banning the word would have forced a worse message; the assertion now bans the
**instruction forms** ("run the revert again", "try again", …) and separately *requires* the text to say
re-running cannot help. Both permanent branches are covered, and the unrestorable-key leg is armed
portably with a `..` segment (refused by `safe_segments` on every OS) so it runs on all three CI legs
rather than Windows only.

### A limit, stated rather than papered over

jsdom applies no component CSS under this project's vitest config, so the component tests here check
**text presence/absence only** — not layout, not ordering on screen, not colour, not whether the panel is
actually visible. That is recorded in the test file's own doc comment as well as here. The panel's styling
uses theme tokens (`--text`, `--text-dim`, `--warn`, `--surface-alt`, `--border-strong`, `--radius`) in
both palettes, but nothing automated verifies how it looks.

### Gates

`crates/server`: `cargo clippy --all-targets -- -D warnings` → **exit 0**. `cargo test` (every target) →
**2348 lib** (4 ignored) + `archive_panic_safety` 21 + `binary_data_preview_panic_safety` 22 +
`checkpoint_roundtrip` 2 + `finder_tags_os_interop` 1 + `native_meta_os_interop` 1 +
`parser_panic_safety` 45 + `sample_fixtures` 16 + `thumb_svg_panic_safety` 32 + `ticket_mcp` 0 —
**0 failed**. Baseline on this head was **2343** lib, so **+5**: three in `model` (constructor/derivation,
the retryable split, four distinct wire tokens), `cpe_1845_a_consumer_tells_the_four_states_apart_with_
every_message_erased`, and `cpe_1845_only_the_retryable_hold_back_may_tell_the_user_to_run_it_again`.

`src-tauri`, both feature modes: clippy default → **0**, `--features sidecar-platform` → **0**;
`cargo test` → **214** / **269** — unchanged from baseline (no test added there; the change reaches it
only through `cpe_server`'s types).

Frontend: `npx vitest run` → **4384 passed / 328 files, 0 failed** (baseline **4375 / 327**, so **+9**:
5 in the new `revertHoldBack.test.ts`, 4 in `CheckpointDialog.test.ts`). `npm run check` → **0 errors,
0 warnings** (one fixture literal in `folderWatch.test.ts` needed the new field).

`bidiEscape.guard.test.ts`'s REGISTRY is exhaustive-by-equality, so all three edited components' recorded
offender sets were recomputed live and replaced, and `RevertOutcomePanel.svelte` registered — both its
path renders go through `displaySafePath`; what remains recorded is a count and the backend's own
reason / next-step / per-failure strings, the same class as most other entries in that table.

**Not verifiable on this machine, and it is the merge gate:** `Server crates` on **ubuntu and macOS**.
The converted CPE-1823 assertions sit next to `#[cfg(unix)]` legs, and the new unrestorable-key wording
leg runs on all three, so both must be green on this head by SHA before merge.

### 2026-08-22 — round 2: the UAT's false sentence, and the Reviewer's blocker (the branch that is not one branch)

The independent UAT PASSED the three single-case panels and the mixed case (it built a real
`execute_restore` harness and rendered `RevertOutcomePanel` rather than paraphrasing), and the
independent Reviewer verified every red-proof, the 60,400 figure (302 UTF-8 bytes × 200, derived
independently), the empty-`error` compatibility question by enumeration, and the bidi guard's
line-exactness. Both then found real defects. Everything below is in this branch.

**THE BLOCKER, and it was this ticket's own acceptance criterion.** Round 1 labelled *every* hold-back
derived from a non-empty `report.skipped` as `SkippedByPlan`, `retryable: true`, "…and run the revert
again". But `report.skipped` is fed by **every** write refusal, and only some are transient. The Reviewer
measured it through the production `execute_restore` rather than arguing it:

```text
PROBE skipped=[("a/../evil.txt", "escapes dest_root: `.`/`..` segment")]
PROBE outcome=SkippedByPlan retryable=true
      next_step=This one is temporary: … and run the revert again …
```

`escapes dest_root` (four forms), the Win32-unstable name rule, the resolved-containment failure and the
Create-premise refusal are verdicts on the checkpoint's **stored spelling** — this platform reaches them
identically every run. Only an attempt that was made and failed (a locked file, a pruned blob, a
permission error) is transient.

**Resolved by splitting the branch, not by softening the words.** The answer is now *carried from the
point of refusal*, where it is known, instead of inferred later from prose or from which branch fired:
`apply_write`/`apply_delete` return `Result<(), Refused>` where `Refused { reason, permanent }`, built by
`Refused::transient(..)` / `Refused::permanent(..)` at each site; `execute_restore` accumulates
`any_permanent_refusal`; the branch reads that flag. Every reason string is byte-identical to before —
only the classification is new. Site by site: `safe_target` (all four `escapes dest_root` forms, the
Win32 rule, the containment failure) → permanent; `no checkpoint entry for this path` → permanent;
`blob_source` (non-hex hash, blob outside the store) → permanent; the Create-premise refusal → permanent;
`create_dir_all`, `fs::copy`, `remove_file` → transient.

**A test now pins the blocker** (`cpe_1845_a_permanent_write_refusal_is_never_reported_as_retryable`),
running BOTH legs from the same fixture shape so neither passes by the other's accident, with a liveness
assertion that the checkpoint-key stand-down above it did **not** arm — the first draft of this test
passed with the fix disabled precisely because that branch had armed instead.

**And a test that locked the wrong answer in is corrected in place.**
`cpe_1823_a_case_aliased_entry_never_destroys_the_file_it_resolves_onto` asserted `SkippedByPlan` for an
`A.txt`/`a.txt` case fold. A case-insensitive volume does not stop folding between runs, so that refusal
is permanently non-retryable; the assertion now reads `HeldBackByCheckpoint` and carries the reason, so a
later correct fix cannot read as a regression here. One fixture was wrong for a related reason: the
retryable leg of the wording test used the blob name `"1845-no-such-blob"`, which is refused by the
**hex-name rule** (permanent) rather than by the missing file (transient) — now `"1845bbbb"`.

**THE UAT'S MUST-FIX — a false sentence in the mixed case.** With an unrestorable-name checkpoint AND a
locked file or missing blob in the same run (an ordinary cross-platform backup restored on a machine with
a file open), the panel printed *"Everything restorable has already been restored"* three lines above two
entries that had just failed to restore. Not a harmless inaccuracy: it is the **premise** of the next
clause, so a user who trusts it deletes the leftovers believing the restore half finished. The clause is
now conditional on `report.skipped.is_empty()`; otherwise it reads *"The restorable half is restored only
where it could be: check the failures listed above first."* Pinned by
`cpe_1845_the_completeness_claim_is_absent_when_something_actually_failed`, and the pure case is pinned
too (the claim must still be present when nothing failed) so the fix cannot degrade into deleting the
sentence.

**Docs enumerated three hold-back cases and there are four.** The alias/collision hold-back is real,
permanent, and its advice is the **opposite** — those files *are* the checkpoint's own content, so
deleting them destroys it. The screen already got this right; only `src/docs/16-checkpoints.md` misled,
because its blanket permanent paragraph prescribed deletion. Fourth bullet added, and the paragraph no
longer prescribes: it says the advice differs per cause and to read the screen. Also corrected: "three
separate counts" → "up to three", since a zero count is omitted.

**Per-failure error strings were rendered raw, in a place they had never been rendered at all.** Two
problems, both fixed:

- `apply_delete`/`apply_write` format their reason as `"{target}: {os error}"`, so a **user-controlled
  filename** rides inside `f.error` — the exact bidi/spoof class `displaySafeName` exists for — and the
  panel rendered it unescaped while the adjacent `f.path` was escaped. Now `displaySafeName(f.error)`,
  pinned by a component test that plants U+202E and asserts the bracketed `[RLO]` tag appears instead.
  The `bidiEscape.guard.test.ts` registry comment claiming these were "the same class as most other
  entries in this table" was wrong and is corrected in place — most entries are counts and labels; this
  one provably carried a filename.
- The internal checkpoint-store path was reaching user-facing text. `fs::copy`'s reason now names the
  blob by its (already hex-validated) **hash**, and `snapshot_capture::blob_source`'s two messages no
  longer print `blobs_dir`. Asserted in the mixed-case test: no reason may contain `"blobs"`, and it must
  still say *which* stored copy is missing.

**`ok`/`outcome` consistency is now unrepresentable-if-wrong, not debug-asserted.** `OpResult::held_back`
took an `OpOutcome` guarded by `debug_assert!` — compiled out of release, so
`held_back(p, OpOutcome::Applied, …)` would have *shipped* `ok: false` with `outcome: Applied` while CI
stayed green. It now takes a `HeldBackOutcome` (a two-variant Rust-only enum, deliberately not on the
wire, with `as_outcome()`), and `HeldBack::outcome` is that narrow type too. **Still open, recorded not
fixed:** `OpResult`'s fields are `pub` on a `pub` struct, so any crate could build an inconsistent
literal; the Reviewer grepped `crates/` and `src-tauri/` and found none, and the constructors are the only
construction path today.

**Smaller items, all done.**

- The per-path detail the engine deliberately produces (`same file as checkpoint entry "a.txt"`) was
  discarded by `summarizeRevert`, which mapped to `r.path` only — the Reviewer's framing was "either
  render it or stop claiming it as a design point". It is rendered, after the path, when non-empty; it is
  empty for every high-volume case so it costs nothing at 200 paths.
- `"and 1 more"` is a longer line than the name it replaces, so at exactly one over the cap the list
  stretches by one instead of truncating. Pinned at `MAX_LISTED`, `+1` and `+2`.
- `CheckpointDialog`'s `note` renders at the TOP of the dialog in the slot shared with "Checkpoint …
  captured", detached from the panel, so a bare "Applied 2 changes" no longer said what was applied.
  `RevertOutcomePanel` gained a `verb` prop ("Reverted" / "Undo"); the note reads `Revert — applied 2
  changes.`

**Recorded, NOT fixed — the cap/advice contradiction at scale.** With 200 held back, the panel says
"delete these files yourself" and then lists 8 names plus "and 192 more", so the user is told to act on a
list they cannot see. Survivable for the empty-checkpoint case (the held-back set is the whole folder);
**not** survivable for the unrestorable-key case, where the set is "everything added since the
checkpoint" and appears nowhere else. The fix is a copy-the-full-list / reveal-in-the-file-pane
affordance — a new UI surface rather than a wording change, so out of scope here. Written into
`MAX_LISTED`'s own doc comment and flagged for the coordinator to file.

**Round-2 red-proofs** (in addition to round 1's five, all of which the Reviewer reproduced verbatim):

| Guard | Line broken | Observed |
|---|---|---|
| the permanent/transient split | `revert_engine.rs` `if any_permanent_refusal {` → `if false && any_permanent_refusal {` | `cpe_1845_a_permanent_write_refusal_is_never_reported_as_retryable` red, reproducing the Reviewer's probe: `outcome: SkippedByPlan`, `"could not be restored this time (a/../evil.txt)"`, `"…and run the revert again"` |
| the conditional completeness claim | `if report.skipped.is_empty() {` → `if true \|\| report.skipped.is_empty() {` | `cpe_1845_the_completeness_claim_is_absent_when_something_actually_failed` red: the UAT's exact false sentence, printed beside two failures |
| the blob-path suppression | `fs::copy`'s reason reverted to `format!("{}: {e}", blob.display())` | same test red: `a reason shown to the user must not expose the private blob-store layout: gone1.txt: C:\…\blobs\11111111: The system cannot find the file specified. (os error 2)` |
| the `f.error` escape | `RevertOutcomePanel.svelte` `{displaySafeName(f.error)}` → `{f.error}` | `expected 'Reverted — applied 0 changes, 1 faile…' not to contain '\u202e'` |

**Round-2 gates.** `crates/server`: clippy `--all-targets -- -D warnings` → **0**; `cargo test` →
**2350 lib** (4 ignored) + `archive_panic_safety` 21 + `binary_data_preview_panic_safety` 22 +
`checkpoint_roundtrip` 2 + `finder_tags_os_interop` 1 + `native_meta_os_interop` 1 +
`parser_panic_safety` 45 + `sample_fixtures` 16 + `thumb_svg_panic_safety` 32 + `ticket_mcp` 0 —
**0 failed**. Round 1 was 2348 lib, so **+2**: the permanent-refusal test and the mixed-case test.
`src-tauri` both feature modes: clippy **0** / **0**; `cargo test` **214** / **269**, unchanged.
`npx vitest run` → **4390 passed / 328 files, 0 failed** (round 1: 4384, so **+6** — 2 in
`revertHoldBack.test.ts`, 4 in `CheckpointDialog.test.ts`). `npm run check` → **0 errors, 0 warnings**.
`bindings.gen.ts` regenerated with CI's own command: **no diff** — `HeldBackOutcome` is deliberately not a
`specta::Type`, so the wire is unchanged from round 1.
