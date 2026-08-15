---
id: CPE-1712
title: A right-to-left override in a remote filename disguises its extension in Explorer
type: bug
priority: Medium
status: Doing
tags: ready
estimate: S
created: 2026-08-13
closed:
---

## Problem

Found by the PR #894 (CPE-1709) UAT, 2026-08-13, while enumerating character classes the download sink
mishandles. **Pre-existing — not introduced by that PR**, which is why it was filed rather than folded in.

`char::is_control()` matches only the **Cc** Unicode category. `U+202E RIGHT-TO-LEFT OVERRIDE` is category
**Cf** (format), so it passes every guard untouched.

Measured: a remote leaf `\u{202E}gnp.txt` downloads successfully and lands as a real file that **Windows
Explorer displays as `txt.png`**. The bytes are intact and the name is legal; only its *rendering* lies.

## Why it matters

This is the classic filename-spoofing trick, and a file explorer is precisely the application where it
lands. A user looking at a downloaded listing sees what appears to be a PNG and double-clicks it. What runs
is whatever the real extension says.

It is a **display** problem rather than a data-loss one, which is what separates it from CPE-1709 — the
bytes arrive correctly and the file opens. That is also why it is Medium rather than High: nothing is lost,
and the user must still act on the misrepresentation.

## Scope

The same sink CPE-1709 touched — `crates/server/src/transfer.rs`'s leaf handling — plus, importantly, **our
own rendering**. We control how the explorer draws a name; Explorer's behaviour is not ours to fix, but the
app showing the same lie is.

## The decision to make, and record

There are two distinct questions and they may get different answers:

1. **On disk.** Should a bidi control character be escaped in the local name the way CPE-1709 escapes
   Windows-unholdable characters? That keeps the file honest everywhere, at the cost of altering names that
   are legitimate in genuinely right-to-left languages. **Do not casually mangle real RTL filenames** —
   Arabic and Hebrew names contain legitimate bidi marks, and an over-eager rule would make the app
   unusable for those users while fixing a spoof they never encounter.
2. **In our UI.** Should the explorer render bidi controls visibly (an escape, a badge, an isolate) so a
   spoofed name cannot masquerade in *our* list even if it does in Explorer? This is likely the better
   half of the answer, because it is where we can act without touching anyone's data.

The full Cf set is worth considering, not only `U+202E`: `U+202A`–`U+202E`, `U+2066`–`U+2069`, and
`U+200E`/`U+200F`.

## Acceptance criteria

- [x] A remote name containing `U+202E` cannot present a misleading extension **in this app's own listing**.
      Record what it does show.
- [x] Decide and record whether the on-disk name is transformed. If it is, legitimate RTL filenames must
      survive — test with real Arabic and Hebrew names, not only with the spoof.
- [x] If the on-disk name is transformed, the mapping stays **injective** and round-trips, per CPE-1709's
      construction. Two distinct remote names must never collide onto one local file.
- [x] Enumerate the bidi/format set rather than fixing `U+202E` alone — the same "fix the reported
      character only" trap CPE-1709 explicitly avoided.
- [x] A test proves it, asserting on what the user sees, and breaking the guard turns a **distinct** test
      red, per the Evidence Rules in `Ticketing/wiki.md`.
- [x] Confirm no regression in CPE-1709's encoder: ordinary names, percent-bearing names, and the hostile
      set it covers must be unaffected.

## Notes

Filed by the Foreman from the PR #894 UAT, 2026-08-13. The UAT correctly scoped it as pre-existing and out
of that PR's scope.

Related: **CPE-1709** (the sink and its encoder), **CPE-1704** (the listing guard that stopped imposing
filesystem rules on every backend).

## Fold in while you are in this file (from the PR #894 UAT, 2026-08-13)

`cpe_1709_a_security_refusal_still_reports_ok` exercises only the **traversal** branch. It is not a
happy-path assertion -- it mixes refused and deliverable entries and pins `n == 1`, so it does
distinguish the two categories -- but its own doc comment lists **three** security refusals (traversal,
pre-existing symlink, uninspectable ancestor) and only one is covered.

**A future change that moved `LeafProbe::PreExistingSymlink` into `undelivered` would pass this test.**
Add the missing cases when you next touch `crates/server/src/transfer.rs`.

Also worth recording there, deliberate as far as the UAT could tell but unstated: an **uninspectable
ancestor** ends `Ok` while an **uninspectable leaf** ends `Err`. That asymmetry is defensible -- the leaf
is the delivery target and the user genuinely did not get their file -- but it is not spelled out, and a
permission-denied leaf `lstat` now fails the whole transfer where it used to be silent. Judged an
improvement, not a defect; say so in the code rather than leaving it to be rediscovered.

One scoped limitation of CPE-1709 to note in passing: on an **astral-plane** name (emoji, where char
count and UTF-16 count diverge 1:2) the length explanation is **absent**, not wrong -- the message
degrades to the raw `os error 123`. Both properties that matter still hold: it ends `Err`, and it never
says "symlink".

## Work Log

**2026-08-15 — the two decisions, made and recorded.**

**1. On disk: NOT transformed.** `crates/server/src/transfer.rs`'s `windows_safe_segment` is left
untouched for the whole bidi/format set — this is the opposite answer from CPE-1709, and deliberately
so. CPE-1709's rewrite was *compelled*: the local filesystem genuinely could not hold the byte (`:`
diverted into an NTFS alternate data stream) or an ordinary Win32 application genuinely could not open
the result (a trailing dot/space, a reserved device name). None of that is true here — `U+202A`–`U+202E`,
`U+2066`–`U+2069`, and `U+200E`/`U+200F` are all legal on NTFS, ext4, and APFS; `download_tree` writes
them without incident and the file opens fine by its real name (measured directly:
`cpe_1712_the_reported_spoof_writes_byte_intact_through_the_real_sink` downloads the ticket's own
`\u{202E}gnp.txt` repro through the real sink and reads the bytes back byte-intact under the
**unrewritten** name). Nothing forces a rewrite, so rewriting anyway would trade a display bug for a
data-mangling one: a real Hebrew or Arabic filename can legitimately carry `U+200E`/`U+200F` — commonly
right before a Latin extension, to keep it drawing left-to-right inside otherwise right-to-left text — and
an eager on-disk rule would alter those users' filenames on every platform, forever, to fix a spoof they
were never exposed to (the spoof is Windows Explorer's rendering, which this app doesn't own).
`cpe_1712_real_arabic_and_hebrew_names_survive_the_windows_encoder_untouched` proves this with real
Arabic and Hebrew names, one carrying an explicit RLM before its extension, all byte-identical in and
out. The full reasoning is recorded next to the new `BIDI_FORMAT_CHARS` constant in `transfer.rs`, right
where a future reader would otherwise assume the omission was CPE-1709's own gap and "fix" it.

**2. In our UI: yes — this is the fix.** `src/lib/filename.ts` gains `displaySafeName(name)`, a pure
function that replaces each of the 11 enumerated bidi/format characters with its bracketed three-letter
abbreviation (`[RLO]`, `[LRM]`, …) wherever a name reaches the DOM. `\u{202E}gnp.txt` — the ticket's own
repro — now shows literally as `[RLO]gnp.txt`: the TRUE byte order, plus a visible flag that something
unusual is embedded, instead of the browser's own bidi algorithm drawing it as the deceptive `txt.png`.
Wired into every listing surface that renders a raw entry/path-segment name as text: the main file list
(`FileList.svelte`), the sidebar folder tree (`SidebarNode.svelte`), the address-bar breadcrumb
(`NavToolbar.svelte`), Find-by-name results (`FileNameSearchDialog.svelte`), Trash
(`TrashView.svelte`), Home's Recent/Favorites/Folders/Shared/Quick-access tiles (`HomeView.svelte`),
the preview-pane folder peek (`FolderBrowser.svelte`), and the details pane title
(`DetailsPane.svelte`). Deliberately **not** applied to a rename input's value — a user editing a
genuinely RTL name must see and edit their real characters, not the escaped stand-in — and not applied
to extension/thumbnail lookups, which still read the raw `entry.name`. `FileList.bidiSpoof.test.ts`
renders the real component (not just the pure function) and asserts on `container`/`screen` text — what
the user's eyes actually see — for both the spoof and a real Hebrew name.

**Why the two answers differ, and why that's the right shape for this bug.** The problem statement
splits cleanly along "compelled vs. not" the same axis CPE-1709 used, just landing on the opposite side:
nothing compels an on-disk rewrite, so don't do one; rendering is a presentation choice we fully control,
re-evaluated on every redraw and touching no one's data, so that's where the fix belongs. Explorer's own
rendering of the on-disk name stays spoofed after this ticket — that's Explorer's bug, not this app's
sink's, exactly as scoped ("Explorer's behaviour is not ours to fix, but the app showing the same lie
is").

**Enumeration, not the single reported character.** Both the Rust `BIDI_FORMAT_CHARS` constant and the
TS `BIDI_FORMAT_CODES` map cover all 11 code points — the five embeddings/overrides `U+202A`–`U+202E`,
the four isolates `U+2066`–`U+2069`, and the two marks `U+200E`/`U+200F` — not just the reported
`U+202E`. Both are built from `String.fromCharCode`/`\u{...}` numeric escapes rather than literal
characters in the source, deliberately: a literal RLO/LRO sitting in a source file would itself reorder
the surrounding text for anyone reading or diffing it — the exact hazard this ticket exists to defuse.

**Mutation evidence (Evidence Rule 2).** Removing `U+202E` from the TS map reds exactly the two
`displaySafeName` tests that assert on it, with every other test (including `validateFileName`'s) still
green. Making `windows_safe_segment` treat `U+202E` as unsafe (the wrong answer to decision 1) reds
exactly the two new `cpe_1712_*` Rust tests, while all 27 `cpe_1709_*`/`cpe_1696_*` tests stay green —
proving decision 1 and CPE-1709's own regression coverage are independently guarded. Reverted after each
mutation; full output pasted in the PR body.

**CPE-1709 regression: unaffected.** Full `cpe-server` suite (2185 lib tests + integration binaries),
`cargo clippy --all-targets -- -D warnings` in both default and `--all-features` modes, and `src-tauri`
(default + `sidecar-platform`) all green. Frontend: `npm run check` clean, full vitest suite 305 files /
4005 tests green.

**Fold-in items closed while in this file (PR #894 UAT):**
- `cpe_1709_a_security_refusal_still_reports_ok` only ever exercised the traversal branch of the three
  security refusals its own doc comment named. Added
  `cpe_1712_a_preexisting_symlink_refusal_still_reports_ok` (unix-gated, same reason the existing
  symlink tests are) to close the pre-existing-symlink gap, and updated the original test's doc comment
  to say plainly what it does and does not cover, plus where the third category (uninspectable ancestor)
  is actually asserted (`classify_ancestor_probe`'s own unit tests).
- The ancestor-vs-leaf `Ok`/`Err` asymmetry is now spelled out in a comment at the `existing_ancestor`
  error arm in `download_tree`, including the "a permission-denied leaf `lstat` now fails the whole
  transfer where it used to be silent" behaviour change, called an improvement rather than a defect.
- The astral-plane scoped limitation (length explanation absent, not wrong, on an emoji-bearing name) is
  now recorded on `describe_undeliverable`'s own doc comment instead of living only in a UAT note.

**Docs:** `src/docs/03-explorer.md`'s existing "Files" section gains a bullet on the spoof-flagging
behaviour, written for a non-technical reader. No new `Section`, so `src/lib/sectionDocs.ts` is
unchanged — confirmed by `sectionDocs.test.ts` staying green.
