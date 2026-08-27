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

**2026-08-27 — swept, classified, derived the high-blast-radius set, filed the rest as CPE-1950.**
**Round 2 (Reviewer: three blockers) — re-swept case-insensitively, replaced the hand-rolled comment
stripper with the repo's existing one, and corrected a false justification.**

### How the enumeration was done (not from memory)

The ticket's seed grep returned **550** hits — mostly noise, because `same as ` and `mirrors ` match
ordinary design-kinship prose. The sweep ran in three passes, and then a fourth after review:

1. **Seed**, verbatim from the ticket: 550 hits across `*.rs|*.ts|*.mjs|*.svelte`.
2. **Broadened** the phrasing to 30 patterns once the real hits were visible — adding `identical to`,
   `copied from`, `verbatim`, `byte-identical`, `lifted from`, `parity with`, `must match`,
   `same list/set/order/values as`, `reproduces`, `duplicates`, `1:1 with`, `same argv/flags`,
   `char-for-char`, `keep in sync` — and widened the file set to `*.js|*.yml|*.yaml|*.toml`.
   Excluded `node_modules`, `target`, `dist`, `bindings.gen.ts`: **1,583** hits.
3. **Narrowed to the defect shape** — a comment claiming a concrete artifact *here* reproduces one in
   *another named file*. Stripping the `path:lineno:` prefix (so the grep's own filename does not
   match the filter) and keeping only hits whose **comment body** names a file, workflow or lockfile:
   **156** candidates across 118 files, each read and classified.
4. **Round 2: re-ran pass 2 with `-i`.** All 30 phrasings were lower-case, and this repo's comments
   start sentences: "Mirrors…", "Must match…", "Verbatim from…", "Exactly as…". Case-insensitively:
   1,807 raw hits, and pass 3 yields **215 candidates — a delta of 57 in 56 files** that the sweep
   as first run **could not reach**. Eight of the 57 are HARD, including a **fourth** copy of the
   exact claim this ticket exists to kill, in the same crate as the other three
   (`artifact_binding.rs:719`, "Exactly as `release-sidecar.yml` invokes it") — missed for one
   reason: a capital E.

**The bucket counts, reconciled (Reviewer, round 3).** Round 2's table read "20 HARD / 7 derived
/ 150 SOFT" printed under "215 candidates", and 20 + 7 + 150 = 177 does not partition 215. In a
ticket about claims that do not survive checking, that is the wrong place to be loose. Measured on
this branch:

| | count | how it was obtained |
|---|---:|---|
| candidate **lines** (pass 3, `-i`) | **216** | grep output, one row per matching line |
| distinct **sites** | **210** | lines in one file within 4 of each other collapsed — one claim usually spans several lines of a doc comment, and several of the 30 phrasings can match inside it |
| **HARD** | **20** | enumerated exhaustively, every one read individually |
| **already derived / self-annulled** | **7** | enumerated exhaustively |
| **SOFT (not defects)** | **183** | **residual**: 210 − 20 − 7 |

So the three buckets partition **by construction**: HARD and already-derived are exhaustive lists,
and SOFT is defined as everything else. The earlier "150" (and round 1's "108") were *estimates* of
the SOFT bucket rather than a computed residual, which is why they did not add up. Collapsing
multi-line comments accounts for only 6 of the gap (216 → 210); the rest was the estimate. Nothing
was dropped — the arithmetic was wrong, not the classification.

**Correcting a claim this Work Log itself made.** Round 1 justified widening the file set by saying
the seed "omits `*.yml`, which is exactly where CPE-1872's defect lived". That is **false**, and it
was itself a provenance claim of the kind this ticket exists to eliminate. CPE-1872's stale claims
lived in `crates/updater-verify/tests/release_guard.rs` — a `.rs` file the seed already covered; the
workflow was only the *referenced* file. Measured: the seed missed them because of **phrasing and
line-wrapping** (`"same as "` is present at `:815` but wrapped as `same\n/// as`, and grep is
line-based; `"exactly as"` was never in the seed at all). The file-set widening contributed **0 of
the 20 HARD hits** — every one is `.rs`, `.ts` or `.svelte`. Broadening the phrasings was the right
move; the reason given for it was not.

### What was derived

**1. Release plumbing — four claims about `release-sidecar.yml` (highest blast radius).**
`release_guard.rs:815`, `release_guard.rs:854`, `hostile_manifests.rs:687` and (round 2)
`artifact_binding.rs:719` each claimed a hard-coded argv or argument pair reproduced the sidecar
job's. CPE-1917 corrected one comment in this same crate and left these four.

Three were **already inaccurate**, in two different ways:
- `release_guard.rs`'s `run_with_expect_channel` passes neither `--manifest` nor
  `--expect-url-prefix`, both of which the real job passes.
- `hostile_manifests.rs`'s `run_expecting_channel` *does* pass `--manifest` (to a tempdir); its real
  divergence is **`--skip-pin-check`**, which it passes and the workflow must never pass — that flag
  would disarm the CPE-1873 pin on the one invocation guarding a real release. (Round 1's replacement
  comment named the wrong divergence here; corrected.)

`crates/updater-verify/tests/release_workflow_wiring.rs` (CPE-1917's proven pattern) now reads
**`release-sidecar.yml`** as well as `release.yml`: 6 new tests derive the download dir and the verify
argv from the workflow and **execute the real binary** with them. `artifact_binding.rs` derives its
`(channel, --conf productName)` pair by reading `--expect-channel` out of the workflow and
`productName` out of the config that workflow's `--conf` actually names. The load-bearing invariant is
the *pairing* — base plain-`productName` `--conf` **with** `--expect-channel sidecar` — which a
well-meaning "fix" would break while every hard-coded unit test stayed green.

**2. `keymap.ts` ↔ `shortcuts.ts` — 34 chords, with a documented prior failure.**
Nothing compared them: `keymap.test.ts` never imported `shortcuts.ts`, or vice versa — yet the file's
own inline note records a CPE-1547 review catching **4 of the 34 transcribed wrong**. `keymap.test.ts`
now joins by `description` and asserts each chord against the sheet (glyphs translated to
`KeyboardEvent.key` form), **and** that it is the sheet's FIRST key — the registry's own stated rule
that only the primary chord is modeled. 34/34 clean.

### Anchor hardening — round 1 got this wrong, twice

Round 1 added `code_lines()`, which blanked only comment-**only** lines. The Reviewer showed a
**trailing** comment walking straight through it:

```
--expect-url-prefix "https://…/${TAG}/"  # was: --expect-channel sidecar
```

`the_sidecar_job_checks_the_sidecar_channel_using_the_base_plain_conf` — the assertion written
specifically to replace the prose claims — **passed**, reading the flag out of the comment. That is
PR #1056's hole reproduced inside the fix for it.

The repo already shipped the right stripper: **`src/lib/shellScriptLines.ts`**, extracted at CPE-1849
and hardened through CPE-1908 rounds 2 and 3 *precisely so a second hand-rolled stripper could not
disagree with the first on an edge case*. `code_lines` was a fifth one implementing the weakest of its
three rules. Replaced with **`crates/updater-verify/src/workflow_scan.rs`**, a faithful Rust port
(quote-, escape-, word-boundary- and heredoc-aware; here-strings excluded, which matters —
`release-sidecar.yml:760` has `done <<< "$names"`), used by **both** Rust consumers so there is one
Rust implementation, not two.

And the port does not merely *claim* fidelity: both languages run against
**`src/lib/shellScriptLines.cases.json`**, a shared 14-case file the Rust test reads at run time. Add
a case on either side and both are held to it.

The same lesson was applied to `MacroRunConfirm.test.ts`, which CLAUDE.md cites as a worked example:
its walker anchored on "the first `format!(` after the fn", and PR #1056's Reviewer had found the one
adversarial source that beat it **silently** — a comment quoting the old message. It now strips Rust
comments (quote-aware) before scanning, killing the class rather than that one shape.

### The shared oracle immediately earned itself: a latent bug in `shellScriptLines.ts`

The cross-language case file was added to stop the Rust port drifting from the TypeScript reference.
Its first real act was to catch a bug **in the reference**, and to show the port was the safer side.

Input `done <<< "names"` — a here-string whose word is a quoted **literal** rather than `$names`:

```
Rust: ["done <<< \"names\"", "echo after"]    correct
TS:   ["done <<< \"names\""]                  swallows the rest of the script
```

`HEREDOC_START`'s `(?!<)` only refuses a match beginning at the **first** `<` of `<<<`. The engine
retries from the **second**, where `<<` consumes chars 2-3, the lookahead sees a space and passes,
and the `\1` backreference closes on the quoted word — opening a phantom heredoc named `names` that
skips every following line.

That is a **false negative**, and this module's own header calls that direction the unsafe one for a
presence-implies-coverage ratchet: a real unhardened `apt-get`, or a real `--expect-channel`, sitting
after such a line drops out of the scan and the guard reports clean. `shellScriptLines.ts` backs
`channelPurityCoverage.test.ts` and `releaseHangHardening.test.ts`, so both were exposed. **Latent,
not live** — no such shape exists in the repo today, and the real `$names` form at
`release-sidecar.yml:760` never matched (`$` is not `[A-Za-z_]`).

Fixed by requiring **both** guards, `(?<!<)<<(?!<)`. Two cases added to the shared file (the bare
shape, and a here-string co-occurring with a real heredoc on one line, which the fix must still
open). Red-proofed by reverting the lookbehind: both new cases fail with `echo after` swallowed.

This belongs to **CPE-1936**'s family (heredoc gaps in `shellScriptLines`) — that ticket's owner can
treat this shape as **already closed**. Noted at the site too.

Shipping a shared oracle whose two halves are known to disagree would have been worse than not
having one, so this was fixed here rather than deferred.

### Red-proofs (each derivation made to go red by changing the referenced source)

| # | Change to the referenced source | Result |
|---|---|---|
| 1 | `release-sidecar.yml`: `--conf` → `tauri.sidecar.conf.json` | pairing test **FAILED** |
| 2 | `release-sidecar.yml`: `--search release-assets` → `src-tauri/target` | **2 FAILED**, incl. the *executable* test |
| 3 | `release-sidecar.yml`: delete `--expect-channel sidecar` | **3 FAILED**; verifier infers "plain" and rejects a genuine sidecar release |
| 4 | `shortcuts.ts`: `Ctrl+L` → `Ctrl+E` | **FAILED**, naming the exact drift |
| 5 | `shortcuts.ts`: rename a description | **2 FAILED** (orphan + chord) |
| 6 | **R2** `release-sidecar.yml`: `--expect-channel` moved into a **trailing comment** | `artifact_binding` **FAILED** ("no longer passes --expect-channel"); wiring test **FAILED** with `left: None` — the flag is no longer read out of the comment. Pre-fix this passed. |
| 7 | **R2** `MacroRunConfirm.test.ts`: comment stripping disabled | **FAILED**: extracted `'the OLD wording {}'` from a comment |
| 8 | **R2** delete the shared case file / truncate it | Rust port test fails loudly rather than agreeing vacuously |

All sources restored; `git diff --numstat` clean, no whole-file rewrites.

### Left undone, deliberately

The lower-blast-radius HARD hits are **classified, not derived**, and filed as **CPE-1950** with
blast radius and a suggested fix each. Two of them (`connect.rs:236`, `paths.ts:21`) are **already
factually wrong today** — the drift has happened and nobody noticed — which makes them the cheapest
wins in that ticket. The 183 SOFT sites are not defects and need no action.

### A disputed finding, resolved: the file changed under us

Round 2 recorded this as "a Reviewer claim that did not hold". That framing was itself an unchecked
claim about a tree I had not looked at — the exact failure mode this ticket exists to kill — so here
is the measurement.

**Both readings were correct, for different trees.** The review asked me to drop
`MacroRunConfirm.test.ts` from CLAUDE.md's worked examples because it "contains no runtime derivation
(`grep readFileSync` → nothing)". At `ed4d9f61`, the base it was given, that file was **357 lines with
no `readFileSync`** — the grep was right. At `1ec1f22c`, my round-2 base after **#1056 landed**, it is
**548 lines with `readFileSync` at `:15`** and the derivation at `:507+`. The claim was true when made
and stopped being true when #1056 merged; I checked the post-rebase tree and reported a contradiction
that was really a rebase.

The citation therefore **stands**, and the review earned its keep anyway: it red-proofed the new
hardening by making `stripRustComments` the identity and getting
`expected 'the OLD wording {}' to deeply equal 'the CURRENT wording {}'` — load-bearing, not
decorative.

### Pattern recorded

`CLAUDE.md` → **Guards and ratchets** gains *"Derive provenance, don't claim it (CPE-1933)"* with
**three** rules, not two: enumerate case-insensitively; anchor on code via the shared stripper (never
hand-roll, and never a whole-line filter); red-proof it.
