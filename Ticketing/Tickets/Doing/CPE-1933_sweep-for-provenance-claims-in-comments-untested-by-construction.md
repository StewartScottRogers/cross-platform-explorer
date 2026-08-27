---
id: CPE-1933
title: sweep for **provenance claims in comments** — "mirrors `release.yml`", "same as X" — which are untested by construction and decay silently
type: task
priority: Medium
status: In Progress
tags: ready
estimate: M
created: 2026-08-27
---

## The pattern

From CPE-1917's worker, generalising a defect it found *inside the fix for the bug it was sent to
investigate*:

> The defect was not a wrong assertion — the tests pass and prove something real. It was a **doc
> comment asserting provenance**: *"exactly the way `release.yml` invokes it"*. Provenance claims in
> comments are **untested by construction**, and they are the ones that decay silently, because the
> test staying green reads as the claim staying true.

That last clause is the whole problem. A stale provenance comment is worse than no comment, because
the surrounding green test actively vouches for it.

## How it presented

CPE-1872's `release_guard.rs` carried three claims that it mirrored `release.yml`'s invocation. They
were **true for exactly one commit**. Round 2 of that same PR moved the check out of the build matrix
into a post-matrix job and changed the arguments — and left the hard-coded argv, and its claim,
behind. Every test kept passing. Nothing could have caught it, because nothing derived the claim from
the thing it claimed to mirror.

The fix shape is the one CPE-1917 used and it is the general answer: **either derive it from the
source at runtime, or do not claim it.** Its new `release_workflow_wiring.rs` reads the invariant's
two halves out of `release.yml` from two different places and executes them against each other, so it
cannot rot the same way.

## Acceptance criteria

- [ ] Sweep for live instances. Suggested starting point, from the worker who found it:
      `grep -rn "exactly the way\|same as \|mirrors \|matches the workflow\|kept in sync with" --include=*.rs --include=*.ts --include=*.mjs --include=*.svelte`
      Expect a handful. Broaden the phrasing once you see what the real ones look like — this list is
      a seed, not the enumeration.
- [ ] For each hit, classify: **derive it** (read the referenced source at runtime and assert against
      it), **delete the claim** (keep the code, drop the assertion of provenance), or **genuinely
      unavoidable** — and if the last, say at the site that the claim is unverified and why, so a
      future reader treats it as folklore rather than fact.
- [ ] Prioritise by blast radius. A stale provenance claim in **release plumbing** or a **security
      guard** is worth deriving properly; one in a UI helper may be fine to simply delete.
- [ ] Red-proof each derivation: change the referenced source and confirm the deriving test goes red.
      A derivation that does not actually re-read its source is the same defect with extra steps.
- [ ] Record the pattern wherever this repo keeps its testing guidance, so the next person writing
      "same as X" reaches for a derivation instead.

## Notes

Filed 2026-08-27 by the sprint Foreman from CPE-1917's worker. Sibling of **CPE-1929** (shadowed
guards — a guard that cannot go red because an earlier one answers first) and **CPE-1932** (rules
followed from memory rather than enumerated). All three are the same family: **a claim that looks
checked and is not.**

Related: **CPE-1872** (where the stale claims lived), **CPE-1917** (which corrected three of them and
built the deriving guard).

## Work Log

**2026-08-27 — swept, classified, derived the two highest-blast-radius, filed the rest as CPE-1950.**

### How the enumeration was done (not from memory)

The ticket's seed grep returned **550** hits — far too many, and mostly noise, because `same as ` and
`mirrors ` match ordinary design-kinship prose. So the sweep ran in three widening/narrowing passes:

1. **Seed**, verbatim from the ticket: 550 hits across `*.rs|*.ts|*.mjs|*.svelte`.
2. **Broadened** the phrasing to 30 patterns once the real hits were visible — adding `identical to`,
   `copied from`, `verbatim`, `byte-identical`, `lifted from`, `parity with`, `must match`,
   `same list/set/order/values as`, `reproduces`, `duplicates`, `1:1 with`, `same argv/flags`,
   `char-for-char`, `keep in sync` — and widened the file set to `*.js|*.yml|*.yaml|*.toml`
   (the seed's file list omits **workflows**, which is exactly where CPE-1872's defect lived).
   Excluded `node_modules`, `target`, `dist`, and the generated `bindings.gen.ts`: **1,583** hits.
3. **Narrowed to the defect shape.** The dangerous class is not "mirrors X" in general — it is a
   comment claiming a concrete artifact *here* reproduces one in *another named file*. Stripping the
   `path:lineno:` prefix (so the grep's own filename does not match the filter) and keeping only hits
   whose **comment body** names a file, workflow, or lockfile: **156** candidates across 118 files.

Those 156 were then classified individually: **12 HARD**, **4 already derived / self-annulled**,
**108 SOFT (not defects)**. The full breakdown is in the PR body.

### What was derived

**1. Release plumbing — three live claims about `release-sidecar.yml` (highest blast radius).**
`release_guard.rs:815`, `release_guard.rs:854` and `hostile_manifests.rs:687` each claimed a
hard-coded argv reproduced the sidecar job's. CPE-1917 corrected one comment in this very file and
left these three. Two were **already false**: the real job also passes `--manifest
release-assets/latest.json` and `--expect-url-prefix`, neither of which
`run_with_expect_channel` passes at all — CPE-1872's decay, again, in the same crate.

Extended `crates/updater-verify/tests/release_workflow_wiring.rs` (CPE-1917's proven pattern) to read
**`release-sidecar.yml`** as well as `release.yml`: 6 new tests that derive the download dir and the
verify argv from the workflow and **execute the real binary** with them. The load-bearing invariant
is the *pairing* — `--conf` at the BASE plain-productName `tauri.conf.json` **together with**
`--expect-channel sidecar` — which is what a well-meaning "fix" would break while every hard-coded
unit test stayed green. The three prose claims now point at the derivation and say what they still
prove on their own terms.

**2. `keymap.ts` ↔ `shortcuts.ts` — 34 chords, with a documented prior failure.**
`keymap.ts` claimed its `defaultChord` values "are transcribed from that group's `keys` column" in
`shortcuts.ts`. Nothing checked it: `keymap.test.ts` never imported `shortcuts.ts` and vice versa —
yet the file's own inline note records that a CPE-1547 review caught **4 of the 34 transcribed
wrong**. Drift is quiet and user-facing: the Shortcuts dialog advertises a key the app does not
honour. `keymap.test.ts` now joins the two by `description` and asserts every chord against the sheet
(display glyphs translated to `KeyboardEvent.key` form). The join is currently **34/34 clean**.

### Anchor hardening (the CPE-1928 trap, hit for real)

The scanners anchor on substrings (`gh release download`, `--bin verify-release-artifacts`).
`release-sidecar.yml` contains **two prose comments** that mention `gh release download` while
discussing it — so extending the guard to that workflow without a comment filter parses them as
calls. Added `code_lines()`, which blanks comment-only lines (keeping indices so continuations still
align); one rule covers both YAML comments and shell comments inside `run: |`. **Proved
load-bearing**: with the filter removed, 4 tests fail with
`a gh release download call passes no --dir: # Same contents: write inheritance note …`.

### Red-proofs (each derivation was made to go red)

| # | Change to the referenced source | Result |
|---|---|---|
| 1 | `release-sidecar.yml`: `--conf` → `tauri.sidecar.conf.json` | `the_sidecar_job_checks_the_sidecar_channel_using_the_base_plain_conf` **FAILED** |
| 2 | `release-sidecar.yml`: `--search release-assets` → `src-tauri/target` | 2 FAILED, incl. the **executable** test — proves it re-reads the source |
| 3 | `release-sidecar.yml`: delete `--expect-channel sidecar` | 3 FAILED; the verifier then infers "plain" from the conf and **rejects a genuine sidecar release** |
| 4 | Remove the comment filter from `code_lines` | 4 FAILED on a comment parsed as a call |
| 5 | `shortcuts.ts`: `Ctrl+L` → `Ctrl+E` for "Edit address" | `editAddress: keymap says "Ctrl+L", shortcuts.ts documents ["Ctrl+E","Alt+D"]` **FAILED** |
| 6 | `shortcuts.ts`: rename description "New tab" → "Open a new tab" | 2 FAILED (orphan + chord) |

All sources restored; `git diff --numstat` confirms zero residue in `release-sidecar.yml` and
`shortcuts.ts`.

### Left undone, deliberately

Seven remaining HARD hits (RepoBrowser↔`clone_host`, `replayFold.test.ts`'s hand-copied Rust oracle,
`batchMedia.ts`↔`batch_media.rs`, s3↔webdav XML-depth guard, `entrySearch.ts`↔`date_filter.rs`, two
gui-smoke fixture-name pairs, the revert dev-harness wire text) are filed as **CPE-1950** with the
classification, blast radius and suggested fix for each. This follows the ticket's own scope-control
clause: the sweep sprawls, so the high-blast-radius ones were done properly rather than all twelve
shallowly. The 108 SOFT hits are not defects and need no action.

### Pattern recorded

`CLAUDE.md` → **Guards and ratchets** gains *"Derive provenance, don't claim it (CPE-1933)"*, next to
CPE-1932's *"Enumerate, don't recall"* — including the two rules that make a derivation real (anchor
on code not prose; red-proof it) and the worked examples to copy from.
