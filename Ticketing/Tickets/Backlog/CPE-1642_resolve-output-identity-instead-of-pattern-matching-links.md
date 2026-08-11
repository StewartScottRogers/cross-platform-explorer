---
id: CPE-1642
title: "Batch Media containment should resolve the output's true file identity, not pattern-match link shapes — symlink chains and a contended hard-link read still escape"
type: Bug
status: Backlog
priority: High
component: Backend
tags: [ready, big-design]
created: 2026-08-11
closed:
---

## Why
Three rounds of security audit on CPE-1623 (PR #828) each closed a real escape and each turned up a new
variant of the same underlying problem. The fixes shipped are genuine and verified — but the *approach* has
reached its limit, and this ticket exists to replace it rather than extend it a fourth time.

**What CPE-1623 did fix and verify (do not redo):** the original `..\..\folder\name` traversal from the
rename box; the IPC bypass where `execute_plan_walk` accepted hand-built `PlannedItem`s without re-deriving
containment; `C:foo` on bare-filename inputs; extensionless-input `..`; whole-segment `..` handling (with
`shot..final` correctly accepted); `Convert.to_ext` validation; and **single-hop** symlink/junction and
hard-link aliasing of the output's final component. Each was byte-verified with a reproduced negative
control. An intermediate junction in the path is also correctly resolved (independently confirmed).

## What still escapes

**A — same-directory symlink chain.** `link_alias_escapes` reads exactly **one hop** (`read_link` on the
output) and compares only that target's *directory* against the input's. If the immediate target is itself a
symlink sitting (textually) in the same folder, the directory comparison passes and nothing asks whether
*that* name is also a link pointing further out.

Demonstrated end-to-end with real bytes: `linkA → linkB` (relative, same dir), `linkB → outside\important.jpg`.
A `PlannedItem{input: selected\photo.jpg, output: selected\linkA.jpg}` with `confirmed_overwrite: true` gave
`Ok(BatchReport{written:1})`, and the outside victim's bytes changed.

**B — hard-link count read fails open under contention.** `hard_link_count`'s Windows path defaults to `1`
whenever `CreateFileW` fails — the same fallback used for the benign not-yet-existing case. So a genuinely
multiply-linked file whose open merely fails is treated as not-linked.

Demonstrated without elevation: with `selected\link.jpg` hard-linked to `outside\important.jpg` (correctly
refused when uncontended), holding an exclusive handle from an ordinary process
(`OpenOptions::share_mode(0)`) made `output_escapes_input_dir` return `false`. Any concurrent holder — another
process, an AV scanner, even a second thread in the same batch — flips the fail-closed rule to fail-open.

## Why this needs a different approach, not a fourth patch
Each round has fixed the shape that was demonstrated and left the next shape open: raw text → one-hop links →
chains → contended reads. The check is **pattern-matching link shapes**, and the space of shapes is not
bounded by our imagination.

**The durable fix is to resolve the output's true identity once, and compare identities** — not paths, and not
link-shape heuristics. Concretely worth designing around:
- Resolve the output (and its parent) to a real filesystem identity — on Windows, the volume serial + file
  index via an opened handle; on Unix, `(dev, ino)` — and ask whether that identity lives under the selected
  directory's identity. This collapses chains, hard links, junctions and future shapes into one question.
- Prefer **resolve-then-write on the same handle** where possible, which also narrows the TOCTOU window
  CPE-1624 tracks — the two tickets may share a design.
- Every failure to establish identity must **fail closed**, including a failed open under contention. That is
  precisely finding B: the fallback must be "refuse", never "assume unlinked".
- Keep it O(n)-amortized and memoized. `plan()` is ~209-219ms for 2000 files today (from 12 minutes before
  CPE-1613); there are canonicalize-count guards, and one was verified non-vacuous by injecting a real O(n²)
  regression and watching it fail. Do not regress either property.

## Also fix here (small, found in the same audit)
The refusal message for the accepted hard-link false positive reads *"...would land outside its own input's
folder"*, which is **factually untrue** in that case — nothing left the folder; the check merely couldn't
prove it hadn't. Say what is actually true ("couldn't verify this stays inside the folder"), since telling a
user something false about why their operation failed is its own defect.

## Acceptance criteria
- Findings A and B are refused, byte-verified, with negative controls that fail against today's code.
- Every identity-resolution failure path is enumerated in the work log with its direction; all fail closed.
- The single-hop cases CPE-1623 fixed still hold — re-run its regression tests, don't just assume.
- No new false positives on ordinary batches; the hard-link "all names inside the folder" case ideally stops
  being a false positive once identity is resolved properly.
- `plan()` stays linear and within ~10% of today's timing; the canonicalize guards stay green and non-vacuous.
- Refusal messages state what is actually true.

## Risk framing (why CPE-1623 merged with this open)
Both remaining escapes require the attacker to **already be able to create files inside the folder the user
selected**. On `main` before CPE-1623, the far easier `..\..\x` attack needed no such foothold and destroyed
an arbitrary file with no confirmation. So CPE-1623 is a strict, large reduction in attack surface, and
holding it back to chase a fourth variant would have left the easy attack shipping. This ticket carries the
remainder.

**Conflict surface:** `crates/server/src/batch_media.rs` (`output_escapes_input_dir`, `link_alias_escapes`,
`hard_link_count`, `path_key`), `crates/server/src/batch_execute.rs`. Overlaps **CPE-1624** (TOCTOU per-write
re-check + ADS) — these two should probably be designed together.
