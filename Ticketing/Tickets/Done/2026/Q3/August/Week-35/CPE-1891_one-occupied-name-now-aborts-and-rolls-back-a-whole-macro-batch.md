---
id: CPE-1891
title: one occupied name now aborts and rolls back a whole macro batch, with no way to say "yes, overwrite"
type: bug
priority: Medium
status: Done
tags: ready
estimate: M
created: 2026-08-25
closed: 2026-08-27
---

## Problem

CPE-1734 made macro Convert **refuse** a destination that is a link *or* a plain occupied name,
rather than silently writing through / clobbering it. That refusal is correct and it closed a real
data-loss path.

But two pre-existing properties combine badly with it:

1. **`macro_apply_run` is strictly all-or-nothing.** Any `Err` from `macro_apply_op` aborts the whole
   run immediately and replays every already-applied op's inverse in reverse.
2. **There is no confirm-and-retry path.** `MacroRunConfirm.svelte` renders the raw error string with
   no affordance to say "yes, overwrite" and continue.

So converting 200 files where file #150 happens to collide now **fails wholesale and rolls back the
149 conversions that already succeeded**, leaving the user with an error string and no route forward
short of finding and renaming the colliding file by hand.

Before CPE-1734 that batch completed, silently clobbering one file. That was a worse default — this
ticket is not an argument to restore it — but the user has gone from "one file quietly overwritten"
to "nothing done, and no way to proceed."

## The parity that was claimed, and the half that is missing

CPE-1734's reasoning was that refusing matches the Batch-Media engine, which already refuses an
unconfirmed in-place overwrite. True as far as it goes — but Batch-Media's refusal comes **with an
escape hatch**: `overwritesInPlace()` → a confirm panel → `confirmOverwriteJob()` →
`confirmed_overwrite: true`, plus a pre-overwrite checkpoint. The macro engine has the refusal and not
the hatch.

Found by PR #1025's reviewer, which checked the parity claim rather than accepting it.

## What to do

Decide between these deliberately, and record why — they are genuinely different products:

1. **Give the macro engine Batch-Media's confirm path.** Most consistent, most work: surface the
   collision, let the user confirm, take the same pre-overwrite checkpoint, continue.
2. **Pre-flight the run.** Check every destination *before* applying anything, and present the whole
   collision set up front — "these 3 of 200 will be skipped / need confirmation" — rather than
   discovering it at file #150. This fits the all-or-nothing contract rather than fighting it.
3. **Make Convert's refusal skippable per entry** while keeping the abort for genuine errors. This
   breaks the documented all-or-nothing macro contract, so it needs the strongest justification of
   the three.

**Whichever you choose, the user must be able to see which names collided.** Note **CPE-1869** just
landed a copy-the-full-list affordance on the revert panel for exactly this shape of problem — a list
the user is told to act on but cannot see. Reuse the approach rather than inventing a second one.

## Not in scope

The all-or-nothing rollback itself is the documented, deliberate macro contract and predates this. Do
not change it as a side effect; if option 3 is chosen, that is a deliberate contract change and must
be argued in the work log.

## Acceptance criteria

- [x] A 200-file convert with one colliding name no longer loses the other 199 conversions with no
      recourse — demonstrated end to end.
- [x] The user can see *which* names collided.
- [x] Whatever path is chosen, the link/write-through refusal from CPE-1734 stays absolute — a confirm
      may allow overwriting a **plain file**, never writing through a link.
- [x] The chosen approach is recorded with its reasoning against the other two.

## Work Log

- **2026-08-25 17:15 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`, from
  PR #1025's reviewer. It approved that PR and flagged this anyway, having traced the parity claim into
  `batchMedia.ts` and found the confirm path the macro engine lacks. The bar it applied — "the PR body
  does not mention this interaction, and it will surprise a user converting 200 files where file #150
  collides" — is the right one.

- **2026-08-27 — Design decision: Option 2 (pre-flight), with the confirm mechanism borrowed from
  Batch-Media's own `confirmed_overwrite` gate rather than reinvented.**

  **Against the three options as posed:**

  - **Option 1 (give the macro engine Batch-Media's full confirm path, including a mid-run pause +
    checkpoint)** was rejected as more machinery than the failure mode needs. Batch-Media's confirm
    panel exists because its engine discovers collisions file-by-file *during* a streamed apply and has
    to pause execution in place. The macro engine's `resolve()` step is already pure and filesystem-free
    — every planned destination is known **before** a single byte moves — so there is nothing to pause:
    every collision can be discovered up front, cheaply (stats only), with zero ops applied. Building a
    pause-resume state machine to solve a problem that a one-shot scan already solves is exactly the
    "clever" behaviour PURPOSE.md's tiebreaker asks to avoid in favour of predictable.
  - **Option 3 (make Convert's refusal skippable per-entry, quietly relaxing the all-or-nothing
    contract)** was rejected outright per the ticket's own "Not in scope": it changes a documented,
    deliberate contract (`macro_apply_run`'s doc comment) as a side effect, and does so silently — a
    partially-applied run with some entries skipped is a *third* product behaviour nobody asked for and
    breaks the "hang onto `ResolvedRun`, `macro_undo` reverses the WHOLE run" undo contract the rest of
    the engine (and its tests) depend on.
  - **Option 2 (pre-flight)** fits the existing all-or-nothing contract instead of fighting it: nothing
    is skipped, nothing is silently partial — either the whole run proceeds (every destination is free,
    or every collision is confirmed) or nothing runs at all. This is the "give the user the whole
    picture before committing" shape PURPOSE.md's predictable tiebreaker favours, and it was the
    smallest change: no new pause/resume state, no change to the rollback code path at all.

  **The escape hatch itself reuses Batch-Media's `confirmed_overwrite: bool` pattern (`BatchJob` /
  `batch_execute::execute_plan_walk`, CPE-1599), not a new one.** `macro_run` gained the identical
  parameter name and the identical posture: the *engine* refuses an unconfirmed collision before
  applying anything (mirroring `execute_plan_walk`'s own up-front all-or-nothing scan), not just the
  UI — a devtools call or a future automation surface gets the same protection MacroRunConfirm does.
  This is what "give the macro engine Batch-Media's confirm path" from option 1 actually meant, kept
  without inheriting the mid-run pause machinery that came bundled with it in Batch-Media's version.

  **New backend surface:** `macro_preflight` (read-only, `Vec<MacroCollision>`) lets the frontend show
  the *whole* collision set before Run is clickable — "N destinations need confirmation" up front,
  never discovered one at a time via repeated run/rollback/retry cycles. `MacroCollision.confirmable`
  is `false` for a link (live or dangling) unconditionally: CPE-1734's absolute rule survives the new
  bypass exactly — `rename_into_confirmed_slot` and `overwrite_confirmed_no_follow` (the two new
  confirmed-write primitives) each re-check the link condition themselves rather than trusting the
  preflight verdict, so even a TOCTOU race between preflight and apply can't turn into a write-through.

  **Frontend:** `MacroRunConfirm.svelte` follows the inline-instant-control convention — an inline
  checkbox ("Overwrite these files"), never a second modal — that flips Run's label to "Overwrite N and
  Run" once checked. A blocked (link) collision is listed the same way (reusing CPE-1869's
  copy-the-full-list affordance and panel styling) but carries no checkbox at all — there is nothing to
  toggle that would unblock it, matching the backend's absolute refusal.

- **2026-08-27 — PR #1044 review round 2: independent Reviewer + Visual Critic passes found three
  blockers and a should-fix in the escape hatch itself, folded into the same PR.**

  **Blocker 1 (security):** `overwrite_confirmed_no_follow`'s post-open re-check was by PATH
  (`symlink_metadata`) only — a hard link is not a reparse point, so it read as an ordinary file, and a
  confirmed Convert through a hard link from inside the macro root wrote the new bytes at BOTH names,
  including one outside `resolve()`'s `within_root` guard. Fixed by calling
  `batch_media::handle_facts` on the open handle and refusing `is_reparse_point`/`is_dir`/`links > 1`,
  mirroring `copy_file_onto_no_follow_with_wording` exactly (the function this ticket's own doc
  comment already claimed to mirror, but hadn't, fully).

  **Blocker 2 (trust boundary):** `confirmed_overwrite` was a blanket `bool` handed to every op in the
  run — confirming the one collision a 200-file batch actually had also switched off the occupancy
  guard on the other 199. Re-scoped to `Vec<String>` (confirmed destination paths, matching exactly
  what `MacroCollision.to` already carries): the backend only bypasses the occupancy guard at a `to`
  the frontend actually named, re-derived fresh at run time on every call, never trusted from an
  earlier preflight.

  **Blocker 3 (irreversibility):** undo/rollback of a confirmed overwrite cannot recover the victim's
  bytes — nothing preserves them anywhere. Chose the reviewer-sanctioned MINIMUM over building a
  pre-overwrite checkpoint: qualified the rollback error message, documented it on `macro_undo`'s doc
  comment, and added a plain-language warning next to the confirm checkbox
  ("This can't be undone…"). Not the checkpoint the ticket's option 1 named, because that would mean
  re-deriving Batch-Media's own backup/checkpoint subsystem for a second engine — a separate initiative,
  not a cheap addition on top of blockers 1+2's rework.

  **Should-fix:** the confirmed rename/move path (`macro_rename_bridge`) reconstructed its destination
  differently from the unconfirmed path for a Rename template containing a path separator (silently
  dropping the embedded subdirectory vs. using the full path) — a real destination DIVERGENCE, not just
  missing guards. Fixed by unifying: rename and move, confirmed and unconfirmed, all route through one
  bridge now, so there is exactly one destination, always.

  **Visual Critic follow-up:** hoisted `MacroCollision.reason` OUT of the per-row list (added earlier
  this same round after UAT flagged it as fetched-but-never-rendered) into one sentence per DISTINCT
  hazard kind under the heading — the per-row placement was N copies of one paragraph and clipped
  mid-sentence past a handful of blocked names. Added the missing "Copy all N names" button to the
  blocked panel (it needed one at least as much as the confirmable panel, being the list the user must
  act on by hand) and gated the "Overwrite N and Run" label on `blocked.length === 0` so a still-blocked
  mixed run never reads as armed.

  16 new/rewritten Rust tests, 13 vitest cases. Full suites green (`crates/server` 2401, `src-tauri
  --lib` 230); `clippy --all-targets -D warnings` clean, both feature modes, both crates; `npm run
  check` clean. New `scripts/dev-harness/macro-collision` dev harness (mirrors CPE-1869's
  revert-heldback-copy shape) used to re-capture the blocked-collision screenshots plus new
  mixed-collision ones proving the should-fix.

- **2026-08-27 — PR #1044 review round 3 (Visual Critic): VISUAL PASS, three small items.** The
  hoisted-reason redesign, mixed-state clarity, copy-button placement, and the (unrequested) undo
  warning line all confirmed working as designed. Three follow-ups, all folded in:
  1. **Real defect invisible with only one blocked item:** the hoisted reason sentence embedded the
     FIRST matching collision's own path verbatim — with several blocked items it would name only one
     path while the list below showed several, looking mismatched. Fixed with `genericizeReason()`
     (strips the leading quoted-path clause; the sentence now reads "This destination is a link, and
     …"). New harness case (`?case=many`, three blocked items across two hazard kinds) + a new
     evidence pair prove it.
  2. Added a dim "Run is blocked by N links above" status line next to the Run button — the friction
     moment named (tick the confirm box, Run still won't light, nothing says why right there) is now
     answered at the point of the click.
  3. Dark screenshots were stuck showing "booting…" in the diagnostic overlay (pixels were already
     correct — only the harness's own JS diagnostic readout hadn't settled). Replaced a fixed 100ms
     timeout with a DOM-readiness poll; all six evidence files re-captured.

- **2026-08-27 — Round 4: PR #1044 fully APPROVED but red on CI; picked up from a prior session that
  had gone stale. Three concrete failures fixed, nothing else touched.**

  **1. Hard-coded hex ratchet (`src/app.css.test.ts`), 401 vs baseline 399.** Not a real colour — the
  ratchet's `HEX_LITERAL` regex (`#[0-9a-fA-F]{3,8}\b`) also matches a hex-*looking* PR number in prose:
  `MacroRunConfirm.svelte` gained two `"PR #1044 review round 2"` comment mentions across earlier rounds,
  and `#1044` (digits 1/0/4/4, all valid hex) matched the same pattern a real `#rrggbb` would. Confirmed
  by grepping the PR's full diff for `^\+.*#[0-9a-fA-F]{3,8}\b` in every `.svelte` file — the only two
  hits were those comments, no CSS anywhere touched a literal colour. Fixed by dropping the `#` (→ "PR
  1044"), restoring the count to exactly 399/86 without moving the baseline.

  **2. Bidi/format-character escape guard (`src/lib/bidiEscape.guard.test.ts`).** The round-3 preflight
  UI added real new render sites. Judged individually rather than blanket-wrapped:
  - **Wrapped in `displaySafePath`:** `c.to` (`MacroCollision.to`, a real destination path) in both the
    blocked and confirmable collision `<li>` lists — both the text content AND the `title=` attribute,
    matching `RevertOutcomePanel.svelte`'s existing convention for the same CPE-1869 list shape.
  - **REGISTRY updates (not wrapped) — none of these carry a filename/path:** `blocked.length` /
    `confirmable.length` / `blockedPreview.more` / `confirmablePreview.more` (plain counts);
    `=== 1 ? "" : "s"` and the "s"/"this file"/"these files" pluralisers (literal strings, not user
    data); the two "Copy all N names" button template literals (count + literal strings only);
    `preflightError` (an error string — kept raw, matching this same file's pre-existing `planError`/
    `runError`/`undoError` entries, all already-accepted raw error text); and `reason` — verified safe by
    reading the backend wording (`fsutil::classify_symlink_slot` / `classify_create_slot`): the ONLY
    embedded path in either message is the leading `"<path>" ` clause, which `genericizeReason()` always
    strips before render, so no path can reach the DOM through it. Also re-registered `runLabel` (the
    ternary that used to render raw directly now lives in the `$: runLabel = …` reactive assignment, one
    level removed from the render position) and dropped the now-stale
    `text:running ? "Running…" : "Run"` entry.

  **3. CPE-1817 staging-guard drift (`.github/workflows/ci.yml`, all 3 OSes).** `lib.rs` now has 10
  `require_staged_reason("trash_roundtrip", ..)` call sites (grep-confirmed), but the CI step's two name
  lists + doc-comment counts still said 9. Traced the 10th to
  `cpe_1891_macro_convert_confirmed_overwrite_replaces_the_plain_target_and_still_trashes_the_original`
  (this ticket's own end-to-end trash-undo test, added in round 2). Its
  `#[cfg(any(target_os = "windows", target_os = "linux"))]` gate and `cfg!(target_os = "linux")`
  `supported_here` argument are byte-for-byte the shared-shape pattern the other 7 round-trip tests use
  (not the `true`-literal linux-only shape the 2 panic-boundary tests use), so it was added to the
  `shared_sites` list, not `linux_only_sites` — verified against the two linux-only tests' actual `true`
  argument and `#[cfg(target_os = "linux")]`-only gate to make sure the classification wasn't reversed.
  Updated the doc-comment counts (9→10 sites, 7→8 shared, "ten tests" on macOS) alongside, leaving the
  one deliberately-historical "2 of 9" sentence (describing CPE-1817's own original state) untouched.

  **Verification before push:** full `npx vitest run` — 335 test files, 4615 tests, only
  `src/lib/msrvSync.test.ts` failed (2 tests), and that failure is a pre-existing local-checkout-only
  artifact matching the exact pattern `.gitattributes` already documents for CPE-1880 (`core.autocrlf=true`
  on this Windows box checks `.github/workflows/ci.yml` out with CRLF; the test's literal `"\n  msrv:\n"`
  substring search only matches LF; the Linux CI runner checks the same file out with LF and stays green)
  — confirmed by reading raw bytes at offset 0 of the untouched file header, CRLF from the very first byte,
  before any of this round's edits. Not one of the three assigned failures and not touched. `npm run check`
  clean. `cargo clippy --all-targets -- -D warnings` clean in both feature modes for both `crates/server`
  (default, and `--features pdf-thumb,video-thumb,waveform,dicom-thumb`) and `src-tauri` (default, and
  `--features sidecar-platform`).

- **2026-08-27 — Round 5: `3205cb9c` went green on all three of round 4's fixes, but surfaced a fourth,
  macOS-only failure in this PR's own round-2 security work** —
  `fsutil::tests::overwrite_confirmed_no_follow_never_writes_through_a_dangling_link` panicked on
  `macos-latest` at its `assert!(e.contains("is a link"), ...)` line; Ubuntu and Windows stayed green.

  **Diagnosed from the code path, not guessed.** `overwrite_confirmed_no_follow`'s OWN doc comment
  (this same file, above it) already names and measures this exact class of bug from CPE-1846: "on
  Unix `O_NOFOLLOW` makes the `open` itself fail with `ELOOP`, so neither [post-open] check ever
  runs" — and records that an earlier revert test which pinned the Windows-only "is a link" wording
  unconditionally **reddened `Server crates` on ubuntu and macOS both**, with
  `could not open the destination for writing: Too many levels of symbolic links (os error 40)`. The
  new test's `overwrite_confirmed_no_follow(&link, ..)` call opens `link` through the exact same
  `batch_media::open_no_follow` primitive, the exact same way, against the exact same
  `make_dangling_link` fixture — so it is the identical bug class, not a new one: on Windows the
  no-follow open succeeds on the reparse point and the post-open `symlink_metadata` check is what
  refuses (wording it "is a link"); on Unix the open itself refuses first (`ELOOP`), before that
  check is ever reached, so the wording is `open_no_follow`'s own "could not open for writing: ..."
  wrapper instead. **The refusal is real on every platform** — the hard-link sibling test in this
  same block (`overwrite_confirmed_no_follow_refuses_a_hard_linked_destination`) is unaffected, since
  a hard link is not a symlink and isn't blocked by `O_NOFOLLOW`, so its open succeeds and
  `handle_facts`'s `links > 1` check catches it identically on every platform (its assertion was
  already written portably: `e.contains("hard-linked") || e.contains("names")`) — only the dangling-
  link leg's SENTENCE is platform-specific, which is precisely what CPE-1846's own doc comment
  already prescribes handling: "assert the class (refused, permanent, victim untouched) freely; gate
  any assertion on the sentence itself behind `cfg!(windows)`."

  **Fix, following that prescription to the letter:** the "is a link" wording assertion is now gated
  behind `cfg!(windows)`; the two platform-agnostic assertions already present and unchanged (the
  link's target was never created, and the link itself survives) still run on every platform and are
  what actually prove the write-through never happened. Added a doc comment on the test itself
  pointing at the precedent so a future reader does not have to re-derive this. Verified locally on
  Windows (the only platform reachable here): `cargo test --lib fsutil::` — 98 passed, 0 failed, 1
  ignored, including this test with `--nocapture` showing no skip notice (the link genuinely staged
  and the refusal genuinely fired) — and `cargo clippy --all-targets -- -D warnings` clean, both
  feature modes, both `crates/server` and `src-tauri` (re-run after this change; a `#[cfg(test)]`-only
  edit, so `src-tauri`'s clean run was a formality but run anyway rather than assumed).
