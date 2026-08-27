---
id: CPE-1881
title: hard-link write refusals are ungrouped in revert (8 MiB of prose) and invisible in transfer (stderr only)
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-23
closed: 2026-08-27
---

## Problem

CPE-1857 added a refusal when a write would land on a multiply-linked destination. The refusal is
correct. **How it is reported is not**, in two places, found and measured by the independent Security
Auditor on PR #1016.

### 1. Revert: one full 420-byte sentence per refused file

Each refusal carries the whole explanation — what a hard link is, why no path check can see it, and
how to break the link. Measured on a 200-file fixture:

```
CMD revert[200 hard-linked files]: applied=0 skipped=200 elapsed=1.26s
      total skipped-message bytes = 84180 (420 bytes/entry avg)
      extrapolated for 20,000 entries = 8220 KiB
```

**~8.2 MiB of identical prose across the IPC for a 20,000-entry revert.** This is not hypothetical:
a tree under `rsync --link-dest` or Time Machine-style dedup is hard-linked wholesale by design, and
legitimate in-root dedup refuses too (verified: two names both *inside* the root, both refused, as
designed).

**CPE-1847 already solved this exact shape** — it grouped hold-backs into one `HeldBackSummary` after
measuring 185 KiB as the problem. Write refusals bypass that grouping entirely, and are 44× worse.

### 2. Transfer: the user is told nothing at all

`download_tree`'s hard-link arm is an `eprintln!` and nothing else. The user sees the delivered count
silently one lower — no `undelivered` entry, no reason, no count of skips. Verified in the auditor's
run: the line went to stderr and `n == 0` was the only signal.

The PR argues this deliberately, and the reasoning is sound as far as it goes: an `undelivered` entry
would fail the *whole* transfer, which is worse. But that is a false choice between "fail everything"
and "say nothing" — **the third option was not considered.**

## What to do

1. **Revert:** fold the repeated sentence into the existing summary-plus-count shape CPE-1847 built.
   One explanation, once, plus the count and the paths. Reuse `HeldBackSummary`'s pattern rather than
   inventing a parallel one.
2. **Transfer:** carry per-entry skips out of `download_tree` as a **counted list**, the way
   `ArchiveReport.skipped` already does, so the user gets "N entries skipped, here is why" without
   failing the transfer.
3. While in `download_tree`, note the adjacent **symlink** arm has the same stderr-only shape. Fix
   both or state why not.

## Out of scope — do not fix here

- The one-shot registered `extract_archive` discards its `ArchiveReport` and returns `Ok(dest)`. That
  is pre-existing for *every* guard in that command and the GUI does not use it (the frontend uses
  `start_archive_extract`, which reports correctly). Recorded so it is not mistaken for coverage.

## Acceptance criteria

- [x] A 200-file all-refused revert produces one explanation, not 200 — measured, with the before and
      after byte counts.
- [x] A transfer that skips entries reports the count and reason to the user, without failing the
      transfer.
- [x] The existing per-entry path information is not lost in the grouping.

## Work Log

- **2026-08-23 17:00 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`,
  from the Security Auditor's findings F3 and F4 on PR #1016. Both were measured rather than asserted.
  Neither blocks that PR: the refusal itself is correct and the alternative is the write going through.
- **2026-08-27 USMST** — Worked end to end on branch `cpe-1881-refusal-reporting`.
  - **Revert (item 1):** `revert_engine.rs`'s `execute_restore` now groups every hard-link write refusal
    into one `WriteRefusalGroup` (mirroring `HeldBack`'s "one explanation, not N copies" pattern) —
    `RestoreReport.write_refusal: Option<WriteRefusalGroup> { reason, count }`. Each refused path still
    gets its own (now short) entry in `skipped` — `Refused::hard_linked` carries only
    `"this file has {N} names (it is hard-linked)"`, not the ~420-byte essay — so no per-path visibility
    is lost, only the repeated boilerplate. The shared paragraph is built from the SAME
    `LinkGuardWording::hard_link_reason()` fields `fsutil::copy_file_onto_no_follow_with_wording` uses
    for its own single-entry message, so the two forms cannot drift apart. Wired through
    `checkpoint_store::RevertOutcome.write_refusal: Option<WriteRefusalSummary>` (new specta type,
    `bindings.gen.ts` regenerated) and rendered in `RevertOutcomePanel.svelte` as a new block styled
    like the existing held-back paragraph. Measured on a 200-file all-hard-linked fixture
    (`revert_engine.rs::cpe_1881_two_hundred_hard_linked_refusals_cost_one_paragraph_not_two_hundred`):
    total report bytes fall from the ticket's measured 84,180 to under 20,000 (one ~400-byte paragraph
    + 200 short per-path facts), asserted directly in the test. Proved red (grouping disabled via a
    temporary `if false &&` toggle, restored after) before green.
  - **Transfer (items 2+3):** `transfer::download_tree` now returns `DownloadReport { files, skipped:
    Vec<String> }` instead of a bare `usize` — the `ArchiveReport.skipped` shape the ticket asked for,
    uncapped (never truncated). Both the hard-link leaf arm AND its adjacent pre-existing-symlink arm
    (item 3) now push a named reason into `skipped` in addition to their existing `eprintln!`, and the
    transfer still ends `Ok` — the "third option" the ticket named. Updated the one downstream caller
    (`cpe-sftp`'s `download_tree` wrapper) and every test call site across `transfer.rs`/`sftp`/`webdav`
    that read the old `usize`. `download_tree` has no live Tauri-command caller yet (traced — confirmed
    dead outside tests), so this is a backend-correctness fix with no GUI surface to screenshot.
  - **Docs:** `src/docs/16-checkpoints.md` (the hard-link bullet now says "refused" not "held back",
    since it lands in `failures`/`write_refusal`, not `held_back`, and notes the group-once behavior)
    and `src/docs/31-network.md` (new "A pre-existing link at the destination name is skipped, and now
    reported" subsection).
  - Full `cargo test` for `cpe-server`/`cpe-sftp`/`cpe-webdav` green (2397 + 36 + 32 tests). `cargo
    clippy --all-targets -- -D warnings` clean in both feature modes (default, `specta`) for all three
    crates. `npm run check` and the full `vitest` suite green (only 2 pre-existing, unrelated
    `msrvSync.test.ts` failures from this worktree's CRLF checkout of `ci.yml` — not touched by this
    ticket). `bidiEscape.guard.test.ts` caught the new raw `{summary.writeRefusalReason}` render;
    wrapped in `displaySafeName` defensively (the text is backend-composed from a count + static
    wording today, never a raw path, but the wrap costs nothing).
  - Opened PR #1046 against `main`, branch `cpe-1881-refusal-reporting`. CI still pending as of this
    entry. Moving to Done per the sprint's ticket-flow convention (folder location tracks work state,
    not merge state); the PR is the durable record if CI turns something up.
- **2026-08-27 (round 2, post-UAT) USMST** — Independent UAT PASSed, re-measured the byte counts itself
  (8,976 bytes on the 200-file fixture vs. the ticket's 84,180 pre-fix — 89% smaller, matching this
  ticket's own test), confirmed nothing is capped three separate ways (source, `revertHoldBack.test.ts`,
  a 200-row screenshot), and shot the six screenshots (both themes, `CheckpointDialog`-chrome mimic,
  long-path wrap) this worker didn't take. Two follow-ups came back from that pass, both addressed:
  1. **`src/docs/16-checkpoints.md` understated what ships.** It said the write-refusal list showed "the
     first few, then a count" — true of the held-back list next to it, false of this one (`.ro-failures`
     is `{#each summary.failures as f}`, no slice, genuinely uncapped). Corrected, and the paragraph now
     explicitly contrasts the two: held-back is a capped preview (8, "and N more", with the copy-all
     button for the rest); write-refusal is the full uncapped list, every entry always on screen.
  2. **The transfer messages explained why and stopped.** Both the hard-link and pre-existing-symlink
     skip messages in `download_tree` ended at "nothing was written for this entry" with no remedy —
     unlike the revert paragraph's "Break the link first… and run this again." Added an actionable
     sentence to each, in the same voice, composed fresh rather than forced through
     `fsutil::LinkGuardWording` (that type's wording is written for the restore/backup domain
     specifically; reusing it here would be the wrong kind of sharing for a third, unrelated caller).
  Also did the UAT-flagged **optional** fix, since it turned out to be a genuine one-line change: the
  held-back and write-refusal boxes now each carry a small "Held back" / "Refused" label
  (`.ro-held-label`) so the two grey paragraphs next to each other in the panel don't read as
  interchangeable. Re-ran the full `cpe-server` suite (2397 tests), `clippy --all-targets -D warnings`
  (both feature modes), `npm run check`, and the `bidiEscape`/`revertHoldBack` vitest suites — all green.
  Disclosure preserved for the next person: `download_tree` still has no live Tauri-command/UI caller
  (only `cpe-sftp`/`cpe-webdav`'s thin wrappers and tests reach it) — wiring the existing remote
  commands through it is CPE-685, a separate attended step; this ticket's transfer half is a
  backend-correctness fix with no user-reachable surface yet.
- **2026-08-27 (round 3, Visual Critic, attempt 3 of 3 — landed complete) USMST** — The Critic credited
  the grouping structure (bold headline → bordered paragraph → bulleted detail) and confirmed the round-2
  "Held back"/"Refused" labels independently, but returned four real findings on parts round 2 didn't
  touch. All four addressed:
  - **D1 (must fix) — the 200-row list read as guillotined, flush against the host dialog's own border
    with no scroll cue, and `writeRefusalCount` was computed but never rendered.** The coordinator's
    decision, taken as a deliberate hybrid rather than either of the Critic's two options: `.ro-failures`
    now sits in its own bordered box (`.ro-failures-box`, matching `.ro-held`'s visual language) with a
    **bounded ~200px/~10-row scroll region** — a real scrollbar, not a truncation — while the DATA stays
    completely uncapped (every row is still in the DOM, `{#each summary.failures as f}` unchanged). Added
    a `failuresHeading` ("Refused (200)" / "Failed (N)" when nothing was grouped) so the count is stated
    explicitly rather than only implied by the paragraph's first three words, and a **"Copy all N refused
    paths" button** mirroring the held-back block's CPE-1869 affordance.
  - **Structural fix underneath D1/D3.** Distinguishing a grouped write refusal from a genuine per-file
    failure needed a non-textual signal (`revertHoldBack.ts`'s standing rule: never infer from `error`'s
    wording). Added `paths: Vec<String>` to `WriteRefusalGroup`/`WriteRefusalSummary` (Rust + wire type,
    `bindings.gen.ts` regenerated) — the refused paths, in plan order — so the frontend can key `f.grouped`
    off path membership in `write_refusal.paths`, never off text matching. This also backs the copy-all
    button (same data, same reason to want it as CPE-1869's held-back one).
  - **D3 (fix) — colour weight was inverted.** 200 identical, low-information grouped rows painted
    `--warn` amber — the SAME weight a genuine locked-file failure earns — so the amber mass was the
    visual impression at arm's length, burying the one paragraph worth reading. Grouped rows (`f.grouped`)
    now render at `--text-dim`, matching the held-back list's weight for the same class of secondary
    detail; an ungrouped `<li>` (a real failure) keeps `--warn`. Verified in a screenshot with both kinds
    in the same list (see below) — "locked.docx — permission denied" reads amber, "photo1.jpg — this file
    has 2 names…" reads dim, side by side.
  - **D2 (fix) — the headline called a deliberate refusal a failure.** "applied 0 changes, 200 failed"
    directly contradicted the paragraph one line below it explaining these were refused on purpose.
    `summarizeRevert`'s headline now splits exactly like the held-back clause already does: genuine
    failures (`failures.length - writeRefusalCount`) keep the word "failed"; the grouped count gets its
    own "refused" clause. When everything is grouped (the common case), "failed" disappears entirely and
    only "refused" shows; a mix of both (a locked file alongside grouped hard-link refusals) shows both
    clauses. Pinned by a new test with a genuine failure mixed alongside two grouped refusals.
  - **D4 — evidence fix, not a product fix.** The original long-path-wrap screenshots reused the 200-file
    string against a 4-row fixture (mismatched count) and no path in that fixture actually exceeded the
    row width, so `overflow-wrap: anywhere` went untested. Re-captured with a real unbreakable long
    hash-like filename (no separators) and a matching count (2 grouped + 1 genuine = "1 failed, 2
    refused", agreeing with the "2 checkpoint entries…" paragraph).
  - **Measurements recorded, not acted on (per the Critic/coordinator):** contrast is fine in both themes;
    `--surface-alt` measured 1–2% off the page background in both themes, so the `.ro-held`/
    `.ro-failures-box` boxes' separation is carried almost entirely by their 1px `--border-strong` border
    — recorded as a CSS comment on `.ro-failures-box` so a future edit doesn't soften/remove that border
    without adding a second cue.
  - **Screenshots re-captured**, both themes, both named scenarios: `cpe-1881-{light,dark}-host-dialog.png`
    (200-row grouped case in real `CheckpointDialog`-width chrome) and
    `cpe-1881-{light,dark}-long-path-wrap.png` (mixed grouped+genuine, real unbreakable long name),
    landed in `.claude/sprint-metrics/visual-evidence/`. Captured via a temporary, uncommitted dev-harness
    page (`scripts/dev-harness/cpe1881-panel/`, deleted after use — not a permanent addition) mounting the
    real `RevertOutcomePanel.svelte`, driven through `claude-in-chrome` rather than a raw headless-Chrome
    CLI invocation: a bare `chrome.exe --headless=new --screenshot=…` on this machine returned a screenshot
    of an unrelated already-open window instead of rendering the target page (recorded here as a real
    environment hazard, not a product concern) — switched to the sanctioned in-session browser tool, which
    rendered correctly. Also discovered mid-capture: `vite.harness.revert-heldback.config.ts`'s hardcoded
    port 4329 was already occupied by another agent's dev server on this shared machine (its SPA fallback
    served the real app shell for any path, which is what the first mis-capture actually was — not a
    hijacked screen, just an unrelated already-running server on the same port); the temporary harness
    was run on a different port instead of touching that process.
  - Re-ran the full suite after every change: `cpe-server` (2397 tests), `cargo clippy --all-targets -D
    warnings` (both feature modes), `npm run check`, and the full `vitest` suite (4605/4607 — the same 2
    pre-existing unrelated `msrvSync.test.ts` failures as round 2). All green.
- **2026-08-27 (round 4, Visual Critic, explicit 3-attempt-limit override) USMST** — **The coordinator
  explicitly overrode their own stated 3-attempt review limit for this round and recorded why, here, so
  the decision is checkable by someone who wasn't in the conversation (the gap CPE-1835 is open about):
  each review round's findings have been strictly finer-grained than the last (round 2: docs + missing
  remedies; round 3: layout/colour/headline; round 4: a count/row mismatch plus contrast measurements),
  nothing has been RE-found across rounds, and two of round 4's five findings are correctness/
  accessibility defects rather than taste — not a case of an open-ended review loop, a converging one.**
  Findings and fixes:
  1. **`Refused (N)` undercounted its own list whenever a genuine failure was mixed in with grouped
     refusals** — the heading counted only `write_refusal.count` while the `<ul>` below it rendered ALL
     of `summary.failures` (round 3's own doc comment said as much: "genuine failures AND grouped write
     refusals together"). Root cause of round 3's finding 5 too (colour-only split in one shared list).
     Fixed by splitting into two separately-headed boxes — `Failed (N)` (genuine, `--warn`) and
     `Refused (N)` (grouped, `--text-dim`) — computed in `RevertOutcomePanel.svelte` as
     `genuineFailures`/`groupedFailures`, filtered on `f.grouped` (still read off `write_refusal.paths`
     membership, never `error`'s wording). Each box's own count is now exactly what it contains, and the
     distinction between the two kinds is now structural (which box) rather than colour-dependent —
     closing finding 5 as a side effect, as intended rather than accidentally: colour-only separation
     measured hue-only at matched lightness in light theme (a colour-vision check away from invisible)
     and INVERTED in dark theme (the quiet colour was the one row worth reading).
  2. **The scroll region's default browser scrollbar thumb measured 2.24:1 on `--surface-alt` in light
     theme** — below the WCAG 3:1 UI-component minimum, and the only remaining scroll cue there once the
     half-clipped last row is discounted (dark theme measured 9.62:1, no problem). Styled explicitly:
     `scrollbar-color`/`scrollbar-width` (Firefox) plus `::-webkit-scrollbar-*` (Chromium/WebView2, what
     this app ships on) off `--border-strong`, which already clears 3:1 in both themes.
  3. **Both the explanation paragraph and the list below it were labelled "Refused"** — the round-2 fix
     for the held-back/write-refusal ambiguity reintroduced the identical ambiguity one box over. The
     paragraph is now labelled `WHY`; the list keeps `Refused (N)`. Also: none of round 3's four
     screenshots exercised the "Held back" box at all, so the original distinction was unverified by the
     evidence — a new scenario (`held-back-and-refused`, both a held-back delete group AND a grouped
     write refusal in the same result) was captured this round specifically to prove `Held back` and
     `WHY`/`Refused (N)` read as distinct boxes side by side.
  4. **The long-path-wrap fixture's 89-char hash fit the row width to the pixel and wrapped at a space
     before an em dash — a separator, not inside the token** — so it proved the token doesn't overflow,
     not that `overflow-wrap: anywhere` breaks mid-token. Lengthened to 124 characters (no separators);
     re-captured, the wrap now lands inside the hash itself.
  5. Closed by finding 1's split (see above), not by a colour swap — the coordinator's own call, made
     explicitly so the reasoning survives without a follow-up question: "structural, not chromatic" beats
     patching the colours again, since a THIRD colour pairing could just as easily fail a THIRD contrast
     check in one theme or the other. The split makes the distinction visible in greyscale and to a
     colour-vision-deficient reader without depending on hue at all.
  - **Decided without a round-trip (per the coordinator, recorded here rather than re-litigated):** the
    scroll region stays at ~10 rows / `max-height: 200px` as shipped — the Critic's own offered range was
    6/10/16, and 10 was already built and keeps the explanation primary while the list still reads as
    obviously substantial.
  - **Housekeeping the coordinator did on their side, recorded so it isn't rediscovered as a mystery:**
    the six `cpe-1881-*.png` working-tree files were stale pre-round-3 images sitting at the same paths
    this PR adds — removed on the coordinator's end; this branch's committed versions are authoritative.
  - Re-captured all six screenshots (the four from round 3, plus two new for finding 3's held-back
    coexistence proof) via the same temporary, uncommitted harness pattern as round 3 (built fresh each
    round, deleted after use). `npm run check` and the full `vitest` suite (4605/4607 — same 2
    pre-existing unrelated `msrvSync.test.ts` failures) green; no Rust files touched this round.
- **2026-08-27 (round 5, final — merges after this) USMST** — Picked up on a fresh worktree after the
  prior worker was gone; branch already Reviewer-APPROVED and UAT-PASSED. Five items, all landed:
  1. **MUST FIX — a latent recurrence of the exact bug round 4 removed.** The `Refused` box gated on and
     rendered `groupedFailures` (derived from `write_refusal.paths` `Set` membership) but headed itself
     with `summary.writeRefusalCount`, a separate backend scalar (`write_refusal.count`) that can diverge
     from `paths` on a duplicate path, a count/paths mismatch, or a refused path with no matching `failed`
     entry — silently reopening "this box's own heading undercounts its own list" through a second field.
     Same shape on the copy button: labelled with `writeRefusalCount`, copied `allWriteRefusalPaths`.
     Fixed by deriving BOTH the heading and the copy button (label and clipboard payload) from
     `groupedFailures.length`/`groupedFailures.map(f => f.path)` — the exact set the `<ul>` renders —
     never from the two backend scalars again. Proved with a new
     `src/lib/components/RevertOutcomePanel.test.ts`: a fixture with `write_refusal.count: 3` against only
     2 matching `skipped` entries. Confirmed RED first (temporarily reverted the heading/button back to
     `summary.writeRefusalCount`, ran the test, got the exact predicted failure —
     `expected 'Why checkpoint entries…Refused (3)…Copy all 3 refused paths' to contain 'Refused (2)'` /
     `'Copy all 2 refused paths'` — then restored the fix and reran GREEN). Also added tests for items 2,
     4, and 5 in the same file (5 tests total).
  2. **Nested the WHY paragraph inside the REFUSED box, above its list.** `.ro-held` (WHY) and
     `.ro-failures-box` (REFUSED) were styled identically (same border/radius/background), so WHY read as
     a peer section with nothing binding it to what it explains — worse in the combined held-back+refused
     screenshot, where HELD BACK carries its own explanation inline and WHY sat detached one box over.
     Moved the WHY markup inside `.ro-failures-box`, added an unbordered `.ro-refusal-why` (spacing only —
     the border/background are already the parent's) instead of the old `.ro-held` peer styling. Three
     stacked grey surfaces collapse to two; every box in the panel is now structurally uniform. The word
     "WHY" is unchanged.
  3. **One clause, both counts.** The unrestorable-entry hold-back's reason paragraph
     (`revert_engine.rs`, the `!unrestorable.is_empty()` branch) named only the unrestorable-entry count
     ("1 of this checkpoint's entries cannot be restored…"); the deletion count it withholds only ever
     appeared in the headline ("2 deletions held back") and the copy button ("Copy all 2 held-back
     paths"), one box over — a reader scanning numbers saw 1, 2, 2 with nothing connecting them. Reworded
     the `format!` to end "...under a name spelled differently here — so {N} deletion{s} {is/are} held
     back:", naming the causal chain in one clause and colon-introducing the list right below it. Pinned
     with a new Rust test, `cpe_1881_round5_unrestorable_reason_names_both_counts` (1 unrestorable name, 2
     deletes — the exact fixture shape in the held-back-and-refused screenshot), asserting the reason
     contains both `"1 of this checkpoint's entries cannot be restored"` and `"so 2 deletions are held
     back"`. Updated the one test whose assertion depended on this string's OLD trailing clause structure;
     no other test referenced this branch's exact wording.
  4. **Gave the FAILED box a copy button.** HELD BACK and REFUSED both had "Copy all N … paths"
     (CPE-1869/round 3); FAILED — capped at the same ~10-row scroll region against the ticket's own "batch
     of 200 locked files" hypothetical — never did. Added `copyFailedPaths`/`showCopyFailedAffordance`/
     `absoluteFailedPaths`, mirroring `copyWriteRefusalPaths` exactly, gated on the rendered
     `genuineFailures` list (same reasoning as item 1, not a derived count).
  5. **Shortened the repeated per-row refusal suffix to "— N hard links".** Every refused row repeated
     "this file has N names (it is hard-linked)" — up to 200 times — restating both the box heading and
     the WHY paragraph now sitting directly above it. Decided call, not a question, per the brief:
     `Refused::hard_linked` in `revert_engine.rs` now builds `"{N} hard link{s}"` alone; the "why"
     explanation lives once, above. Updated the one test asserting the old `"hard-linked"` substring on
     this specific per-path reason (`cpe_1857_an_overwrite_through_a_hard_link_never_reaches_the_outside_file`)
     to assert `"hard link"` instead; the GROUP paragraph's own `"hard-linked"` wording (a different string,
     asserted by a different test) is untouched.
  - **Comment added, not acted on, per the brief:** recorded in `.ro-failures`'s CSS comment that the
    scroll-region scrollbar thumb now measures 3.33:1 in dark theme (down from round 4's 9.62:1 —
    `--border-strong` has moved since) and 3.71:1 in light theme; both clear the 3:1 floor but neither is
    comfortable margin, and a future `--border-strong` edit should re-check this thumb before landing.
  - `src/docs/16-checkpoints.md`: noted the FAILED list is now bounded/scrollable with the same Copy-all
    button as HELD BACK/REFUSED (item 4).
  - Re-ran the full `cpe-server` suite (2398 tests, including the new
    `cpe_1881_round5_unrestorable_reason_names_both_counts`), `cargo clippy --all-targets -D warnings`
    (both feature modes), `npm run check`, and the full `vitest` suite (4610/4612 — same 2 pre-existing
    unrelated `msrvSync.test.ts` CRLF-checkout failures every prior round also hit). No specta-typed
    struct changed, so no `bindings.gen.ts` regen was needed.
  - Re-captured all six `cpe-1881-{light,dark}-{host-dialog,long-path-wrap,held-back-and-refused}.png`
    screenshots via a fresh temporary, uncommitted harness (`scripts/dev-harness/cpe1881-r5/` +
    `vite.harness.cpe1881-r5.config.ts`, port 48812 — deliberately unusual since this machine runs several
    concurrent agents; the target page's own title was confirmed with a `curl` grep before screenshotting,
    per the round-3 hazard where a bare headless-Chrome invocation once returned an unrelated already-open
    window). Captured via `claude-in-chrome`, not a raw headless-Chrome CLI call. Harness and vite config
    deleted after use; killed only the exact PID bound to port 48812, nothing else on the shared machine.
  - Pushed to the existing branch `cpe-1881-refusal-reporting` / PR #1046. This closes the ticket's last
    round; the PR is expected to merge from here.
