---
id: CPE-1708
title: When a remote listing hides keys, the count only reaches a log line the user never sees
type: task
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-13
closed:
---

## Problem

CPE-1704 fixed the bug where a legal S3 key silently vanished from a listing. Its acceptance criteria also
said that a key the guard **genuinely must** refuse should not vanish silently either — *"surface it under a
visibly-escaped display name, or report that N entries were filtered. Either is acceptable; dropping it
invisibly is not."*

That half is **deferred, not delivered**, and the ticket should say so plainly rather than read as closed.

Where it got to (PR #890, round 3):

- The count is real and trustworthy — a `usize` on `crates/vfs`'s `RemoteListing { entries, filtered }`,
  computed in-process from the provider's own filtering, **not** reconstructable or spoofable from wire data.
- It stops at the Tauri boundary. `list_dir`'s response is a `specta`-typed contract that reaches the
  frontend, and the CPE-1704 worker judged threading a new field through it out of scope for a fix already
  spanning three crates. **That was the right call** — it flagged rather than rushed.
- So today the count reaches an `eprintln!` and nothing else. **A user with a hidden key sees a listing that
  looks complete.**

### Why the earlier attempt is not the answer

Round 2 tried a synthetic `⚠ N keys hidden` entry appended to the listing. Do not revive it. The reviewer
found it **worse than the silent drop**:

- **Spoofable.** A real object can be named exactly like the marker (measured: accepted, emitted as a normal
  7-byte file). Since the genuine marker contained a `/` and was itself refused by the shared name guard,
  **the only such row a user could ever have seen was an attacker-planted one.**
- **Dishonest fields.** `is_dir: false, size: 0` — it claimed to be a zero-byte file.
- **Delete reported success.** S3 `DELETE` of a missing key returns **204**, so deleting the marker would
  have said it worked, and it would still be there on refresh.
- Off-by-one in item counts, included in select-all, and it slipped past `MAX_LIST_ENTRIES`.

The count must travel as **data**, not as a fake row in the data.

## Scope

The `list_dir` command's response type and its `specta` binding, `crates/vfs`'s `RemoteListing`, and
whatever surfaces it in the UI.

## Acceptance criteria

- [x] The filtered count reaches the frontend as a typed field, not a synthetic entry.
- [x] **Regenerate `bindings.gen.ts`.** Editing a `specta::Type` struct — even its doc comment — without
      regenerating fails CI's typed-bindings drift guard. Local crate-only checks miss it.
- [x] The UI says something honest and useful when the count is non-zero. Decide the surface — a status-bar
      note, an inline banner, a toast — and record why. It must not imply the *listing* failed; the listing
      succeeded and some entries were not representable.
- [x] The wording is for a person, not a maintainer. The round-2 attempt cited an internal ticket ID in
      user-facing text; keep ticket references in code comments where they help.
- [x] Zero filtered entries produces **no** UI noise at all. This is the common case by an enormous margin.
- [x] A test proves the count survives the whole path — provider → `RemoteListing` → command → frontend —
      and that breaking any link turns a **distinct** test red, per the Evidence Rules in
      `Ticketing/wiki.md`.
- [x] Confirm every other provider (SFTP, WebDAV, FTP, local) reports zero and is unaffected. The
      `list_with_filtered_count` default delegates to `list` and reports 0; that default must stay correct.

## Notes

Filed by the Foreman from PR #890 (CPE-1704), 2026-08-13, on the worker's own recommendation.

Nothing can hit this today — `crates/s3` is not wired into the app until **CPE-1685**. Ideally this lands
before that one, so S3 support does not ship with a listing that can quietly omit an object. It is a
smaller and less urgent gap than CPE-1704's was, though: the keys being hidden now are only those a
correctly-scoped S3 guard genuinely refuses, not ordinary files with a colon in the name.

Related: **CPE-1704** (which produced the count), **CPE-1685** (which makes it user-visible),
**CPE-1683** (the listing).

## Work Log (2026-08-18)

### What changed

- `crates/server/src/model.rs`: added `ListDirResult { entries: Vec<DirEntry>, filtered: usize }` —
  the `list_dir` command's response envelope. Doc comment restates why `filtered` is data, never a
  synthetic row (PR #890 round 2's rejected approach), for anyone reading the type in isolation.
- `src-tauri/src/lib.rs`:
  - `list_dir` (`Result<Vec<DirEntry>, String>` → `Result<ListDirResult, String>`). Local arm calls a
    new `local_list_dir_result(path)` (hardcodes `filtered: 0`); remote arm calls `remote_list_dir_impl`
    then the new `listing_to_result(listing)`.
  - `list_dir_stream` (`Result<usize, String>` → `Result<StreamDirResult, String>`, new local struct
    `StreamDirResult { total: usize, filtered: usize }`). This is **beyond** the ticket's literal scope
    note (which named `list_dir` specifically) — see "Surface chosen" below for why I extended it.
  - `remote_list_dir_impl` now returns `cpe_vfs::connect::RemoteListing` (entries **and** filtered
    together) instead of discarding `filtered` into an `eprintln!` and returning a bare `Vec<DirEntry>`.
  - `remote_list_dir_stream_impl` computes `StreamDirResult` via a new `stream_result_for(&listing)`
    **before** entries are drained into channel batches, so a cancelled walk can't produce a
    partially-correct count.
  - The old "not yet surfaced in the UI" `eprintln!` is gone — both paths now carry the count as typed
    data, so the diagnostic log was redundant.
- `src/lib/bindings.gen.ts`: regenerated via
  `cargo run --bin export_bindings --features "specta-bindings sidecar-platform"` (from `src-tauri`).
  Confirmed clean (`git diff --numstat`) after every subsequent Rust edit in this ticket.
- `src/lib/components/ExplorerPane.svelte`: new `export let filteredHidden = 0`, bound back to App.
  `revalidateDir` (the `list_dir` background-revalidation path, CPE-756) sets it from `ListDirResult`.
  `loadListing`'s stream branch (the pane's actual first-paint path for a fresh navigation,
  STREAMING.md) now captures `list_dir_stream`'s `StreamDirResult` and sets `filteredHidden` from it;
  the cache-served branch resets it to `0` (never a stale remembered value) until the background
  revalidation 300ms later corrects it.
- `src/App.svelte`: new `let filteredHidden = 0`, `bind:filteredHidden` on `<ExplorerPane>`, passed to
  `<StatusBar>`. `createNewItem`'s dedupe-list fetch updated for the new `{entries, filtered}` shape.
- `src/lib/components/Sidebar.svelte`: `loadChildren`'s `commands.listDir` call updated for the new
  shape (destructures `.entries`).
- `src/lib/components/StatusBar.svelte`: new `filteredHidden` prop + the rendered note (see wording
  below) + `.filtered-hidden` CSS (`--accent` — the app's INFO tone, never `--danger`).
- ~40 `App.*.test.ts` files: `"list_dir"` invoke mocks updated from a bare `DirEntry[]` to
  `{ entries, filtered: 0 }` (mechanical, `perl -pi` + two hand-edited multi-line cases). No behavior
  change — every one of these still resolves `filtered: 0`.
- `src/lib/bidiEscape.guard.test.ts`: the line-number REGISTRY entries for `ExplorerPane.svelte` and
  `App.svelte` (an unrelated pre-existing guard test that pins exact line numbers) shifted because my
  edits landed above the recorded offender lines — updated to match, no content change.

### The surface I chose, and why

**A status-bar note, rendered only when `filteredHidden > 0`** — `src/lib/components/StatusBar.svelte`,
next to the existing `hiddenShown` ("Hidden files shown") note, which is the closest existing precedent:
a folder-scoped fact about what is/isn't currently shown, not a per-event toast. I did **not** reuse the
existing `notice`/`showNotice()` toast mechanism (5s auto-dismiss) — that's for one-off events (an
operation finished, a path wasn't found); this is a standing fact about the CURRENT listing that should
stay visible for as long as the user is looking at that folder, not blip and vanish while they're still
reading it.

Colour: `--accent` (app.css: "ERROR/INFO reuse `--danger`/`--accent`") — the INFO tone, never `--danger`,
so it cannot read as "this folder failed to load." No chip/pill/badge element is involved, so the
tick-tack reflow rule (CLAUDE.md) doesn't apply here.

**Scope decision beyond the ticket's literal text** — the ticket's Scope line named `list_dir`'s response
specifically, and CPE-1704's own note said the same. But `list_dir` is the **collect-to-vec** command
(STREAMING.md); the pane's real navigation path is `list_dir_stream`, and `list_dir`'s only production
caller besides App's two minor helpers is `revalidateDir`, which only runs 300ms after a **cache-served**
navigation (CPE-756) — never on the very first view of a filtered folder. Shipping the fix only on
`list_dir` would have meant a user's FIRST visit to a filtered remote folder still showed nothing (the
exact bug this ticket exists to close), and only a second visit (cache hit + background revalidate) would
show the note. I judged that outcome would not actually satisfy "a user with a hidden key sees a listing
that looks complete" — the ticket's own problem statement — so I threaded `filtered` through
`list_dir_stream` too (`StreamDirResult`, new local struct in `src-tauri/src/lib.rs`). Verified this was
safe before doing it: `commands.listDirStream` (the generated typed wrapper) has zero call sites outside
`bindings.gen.ts` itself; every real caller (`ExplorerPane.svelte`, `FolderBrowser.svelte`) calls the raw
`list_dir_stream` command directly and either discards the return value entirely or (after this change)
reads `.filtered` with `?? 0`, so no test mock shape needed to change for the ~50 files that stub
`list_dir_stream` at the raw-invoke level.

### Exact user-facing wording

- Plural: `"N entries were hidden because their names could not be shown safely"`
- Singular (`filteredHidden === 1`): `"1 entry was hidden because its name could not be shown safely"`
- `title` tooltip on the note: `"The folder itself loaded successfully — these entries specifically
  could not be shown."` — reinforces that the listing succeeded.
- Zero: nothing rendered at all.

No ticket IDs in any of the above (CPE-1704/1708/PR #890 stay in code comments only, per the AC).

### CPE-579 (in-app docs) judgment

A status-bar note is **not** a new user-facing *section* — it's a small addition to the existing listing
view (mirrors `hiddenShown`, which also never got its own doc entry). No `src/docs/*.md` page or
`sectionDocs.ts` entry added; `src/lib/sectionDocs.test.ts` still passes unchanged.

### Tests + red-proof

All four new Rust tests and all new frontend assertions were sabotaged individually, confirmed red, then
reverted and confirmed green again — sabotage happened only AFTER the commit `548896ad` (rust+frontend
feature code) and `0352277` (local-arm extraction) were in place, so nothing was ever at risk from
`git checkout --`.

**`src-tauri/src/lib.rs` (`cargo test --lib`):**

1. `filtered_count_survives_remote_dir_entries_into_list_dir_result` — sabotaged `listing_to_result` to
   hardcode `filtered: 0`:
   ```
   thread 'tests::filtered_count_survives_remote_dir_entries_into_list_dir_result' panicked:
   assertion `left == right` failed: list_dir's ListDirResult must carry the real count through, not 0
     left: 0
    right: 2
   test result: FAILED. 0 passed; 1 failed
   ```
   Confirmed `filtered_count_survives_remote_dir_entries_into_stream_dir_result` stayed GREEN under this
   same sabotage (distinct test, distinct link).

2. `filtered_count_survives_remote_dir_entries_into_stream_dir_result` — sabotaged `stream_result_for`
   to hardcode `filtered: 0`:
   ```
   thread 'tests::filtered_count_survives_remote_dir_entries_into_stream_dir_result' panicked:
   assertion `left == right` failed: list_dir_stream's StreamDirResult must carry the real count through
     left: 0
    right: 2
   test result: FAILED. 0 passed; 1 failed
   ```

3. `list_dir_result_and_stream_dir_result_serialize_the_filtered_field_by_name` — sabotaged
   `StreamDirResult` with `#[serde(rename = "count")]` on `filtered`:
   ```
   thread 'tests::list_dir_result_and_stream_dir_result_serialize_the_filtered_field_by_name' panicked:
   assertion `left == right` failed: StreamDirResult must serialize `filtered` under that exact key
     left: Null
    right: 2
   test result: FAILED. 0 passed; 1 failed
   ```

4. `list_dir_local_arm_always_reports_zero_filtered` — sabotaged `local_list_dir_result` to hardcode
   `filtered: 1` (this test was refactored mid-ticket to call the SAME function `list_dir`'s Local route
   calls, so it pins real production code, not a hand-duplicated copy):
   ```
   thread 'tests::list_dir_local_arm_always_reports_zero_filtered' panicked:
   assertion `left == right` failed: a local listing has no name-guard filtering to report
     left: 1
    right: 0
   test result: FAILED. 0 passed; 1 failed
   ```

After each: reverted, `cargo test --lib` → `192 passed; 0 failed`.

**Frontend (`npx vitest run`):**

5. `StatusBar.test.ts`'s "says N entries were hidden…" + "uses the singular for exactly one" —
   sabotaged `StatusBar.svelte`'s `{#if filteredHidden > 0}` to `{#if false}`:
   ```
   FAIL src/lib/components/StatusBar.test.ts > StatusBar filtered-hidden note (CPE-1708) >
     says N entries were hidden, for a person, when the count is non-zero
   FAIL src/lib/components/StatusBar.test.ts > StatusBar filtered-hidden note (CPE-1708) >
     uses the singular for exactly one
   Tests  2 failed | 10 passed (12)
   ```

6. `App.filteredHiddenNote.test.ts`'s "shows the honest, person-facing note on the VERY FIRST paint…" —
   sabotaged `ExplorerPane.svelte`'s stream-result capture to `filteredHidden = 0` unconditionally:
   ```
   FAIL src/App.filteredHiddenNote.test.ts > filtered-hidden status-bar note (CPE-1708) >
     shows the honest, person-facing note on the VERY FIRST paint of a filtered remote-style listing
     — not only after a later cache revalidation
   Tests  1 failed | 1 passed (2)
   ```
   ("shows nothing when filtered nothing" stayed green — distinct test, distinct assertion.)

After each: reverted, `npx vitest run` (full suite) → `313 files / 4081 tests passed`.

### Verification (final state, this branch)

- `cargo test --lib` (src-tauri): `192 passed; 0 failed`
- `cargo clippy --all-targets -- -D warnings` (src-tauri): clean
- `cargo clippy --all-targets --features sidecar-platform -- -D warnings` (src-tauri): clean
- `cargo run --bin export_bindings --features "specta-bindings sidecar-platform"` then
  `git diff --numstat ../src/lib/bindings.gen.ts`: empty (committed bindings match)
- `crates/server`: `cargo clippy --all-targets -- -D warnings`, `cargo test` (32 passed),
  `cargo clippy --all-targets --features index -- -D warnings`: all clean
- `crates/vfs`: untouched by this ticket (not in the diff) — relied on its existing, unmodified
  `remote_dir_entries`/`RemoteListing` test coverage from CPE-1704
- `npm run check` (svelte-check + tsc): 0 errors, 0 warnings
- `npx vitest run`: 313 files / 4081 tests passed

Not run locally: `crates/vfs`'s live-server conformance suites (SFTP/WebDAV/FTP containers) and the
Linux/macOS legs of the backend/crates CI matrices — Windows-only box, per the sprint's standing note
that local Windows runs can't stand in for the 3-OS matrix.


## Work Log addendum (2026-08-18, reviewer findings F1-F3)

Foreman review (via a dedicated reviewer pass) confirmed the bidi-registry line shifts, the byte-identical
`bindings.gen.ts` regen, and the ~45 test-mock re-shapes were all mechanically correct, and confirmed the
`list_dir_stream` scope extension was necessary (not scope creep) — but found one blocking issue and two
non-blocking ones.

**F1 (BLOCKING, fixed)** — `filteredHidden` was surviving into views with NO real folder listing behind
them: `loadPath` short-circuits Home before `loadListing` (and therefore `filteredHidden`) is ever
touched, and `enterArchive`/smart-folder/structured-search never call `loadPath` at all. A note from the
last real filtered folder kept asserting itself over Home/an archive/a smart folder — a false statement
in the status bar, exactly what this ticket exists to remove. Fixed by gating at the single point of
consumption (`src/App.svelte`'s `<StatusBar filteredHidden={...}>` prop) rather than resetting in each
early-return, so a future early-return can't forget it:
`filteredHidden={isHome || archive || smartFolder || structuredSearch ? 0 : filteredHidden}`.
Regression test added to `App.filteredHiddenNote.test.ts`: navigate into a filtered drive, confirm the
note shows, press Ctrl+T (new tab → Home), confirm the note is gone. Red-proofed: reverting the gate
(`{filteredHidden}` alone) reproduces exactly the reviewer's repro —
`expected <span …(2)></span> to be null` — then reverted, confirmed green.

**F2 (fixed)** — the `.filtered-hidden` note's `title` tooltip claimed (in a comment) to carry "the full
sentence" the same way `.notice` does, but it was actually a different, shorter, fixed string —
truncation at a narrow window (`max-width: 45%`, ellipsis) left no way to recover the real count. Fixed
by hoisting the sentence into `$: filteredHiddenText` and reusing it in both the visible body and the
`title` (`$: filteredHiddenTitle`, which appends the "loaded successfully" reassurance); the CSS comment
now describes what the code actually does. Regression test added to `StatusBar.test.ts`: renders with
`filteredHidden: 3`, asserts the `title` attribute contains BOTH the exact sentence and "loaded
successfully". Red-proofed against the old fixed-string title — failed with `expected 'The folder itself
loaded successfully…' to contain '3 entries were hidden…'` — then reverted, confirmed green.

**F3 (non-blocking, fixed)** — `src/docs/31-network.md` documented S3's `.`/`..` key-shape refusal but
never said such keys are dropped from LISTINGS too, nor that the count is now shown. Added a bullet
(reviewer's exact wording) right after the `.`/`..` bullet.

Not touched (reviewer filed separately, explicitly out of this ticket's scope): a `revalidateDir` that
can fire while Home is showing (CPE-756 class), pane B's `filteredHidden` never being surfaced, and the
local arm silently dropping unreadable entries without counting them.

Re-verified after F1-F3: `npm run check` (0 errors), `npx vitest run` (313 files / 4083 tests passed —
2 new tests added: the F1 Ctrl+T regression and the F2 tooltip regression), `cargo test --lib`
(src-tauri, 192 passed, unaffected — F1-F3 are frontend + docs only).
