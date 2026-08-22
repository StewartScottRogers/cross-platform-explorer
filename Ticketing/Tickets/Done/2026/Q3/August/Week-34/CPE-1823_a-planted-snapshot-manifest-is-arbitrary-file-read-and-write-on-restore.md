---
id: CPE-1823
title: "Security: a planted snapshot manifest is arbitrary file read and write on restore"
type: bug
priority: High
status: Done
tags: ready
estimate: M
created: 2026-08-20
closed: 2026-08-22
---

## Problem

`crates/server/src/snapshot_capture.rs:102-108` — `root_relative_to_abs` builds the restore target by
`p.push(part)` for each `/`-split segment of the manifest's stored path, with **no rejection of `..`
and no rejection of an absolute component**. `Path::push` with an absolute component *replaces* the
whole path, so a single crafted segment relocates the write anywhere on the volume; `..` walks up
from the restore root.

`restore` (`:218-224`) uses that function for the **write target**, and `blobs_dir.join(&file.hash)`
for the **read source** — `hash` being another unsanitised manifest field.

So a hand-edited or planted manifest JSON yields **arbitrary file write** (restore writes attacker-chosen
content to an attacker-chosen path) and **arbitrary file read** (the blob source is pulled from an
attacker-chosen path) at the privilege of the app.

## Why it matters

The manifest is *trusted downstream* while being an ordinary on-disk JSON file the user — or anything
running as the user, or anything that can write into the snapshot directory — can edit. A snapshot
directory copied from elsewhere, restored from a shared drive, or synced by a cloud client is enough.
There is no signature, no canonicalisation, and no containment check between reading the manifest and
writing the files it names.

Every other write path in this crate is being hardened right now (CPE-1765 claims the picked name so
a copy cannot land outside the chosen folder). This one bypasses the question entirely by letting the
*input* choose the path.

## Acceptance criteria

- [ ] `root_relative_to_abs` rejects any segment that is `..`, is absolute, contains a drive
      prefix/root component, or is otherwise not a plain single component — returning an error, not a
      silently-sanitised path.
- [ ] After building the target, the result is canonicalised and asserted to be **inside** the restore
      root, so a link planted mid-path cannot redirect the write either. Reuse the containment helper
      the crate already has rather than writing a second one — check `is_self_or_descendant` and the
      `transfer::is_safe_name` family first.
- [ ] `file.hash` is validated as a plain hex blob name before it is joined onto `blobs_dir`, so the
      read source cannot escape either.
- [ ] A restore that hits a rejected entry fails **loudly and per-entry**, naming the offending path —
      it must not silently skip, because a silently-skipped restore entry is a file the user believes
      was restored.
- [ ] Tests stage a genuinely malicious manifest for each shape: `..` traversal, an absolute component,
      a drive-relative component on Windows, a link planted at an interior component, and an escaping
      `hash`. Each asserts **the harm did not happen** (nothing written or read outside the root)
      before asserting the `Result`.
- [ ] Red-proof each test: remove the guard it covers, observe red, revert, record the line.

## Notes

Found 2026-08-20 by the independent Security Auditor while auditing PR #968 (CPE-1765) — it audits
`snapshot_capture::save_manifest`, which CPE-1765 fixed correctly, and answered the "is the manifest
trusted downstream?" question with "yes, and here is why that is a problem". **Pre-existing, not
introduced by CPE-1765.** Filed separately so it is not absorbed into that ticket's scope.

## Work Log

### 2026-08-21 — fixed, branch `cpe-1823-manifest-traversal`

**What was wrong, confirmed by staging it.** Both halves of the report reproduce, and the red-proof runs
below print the escaped path each time. The write side reached a sibling temp directory (`..`), an
arbitrary absolute path (Windows `push` replaces), and the **app's own working directory**
(`Z:cpe1823-…txt`, the drive-relative shape). The read side pulled 33 bytes of a file outside the store
into the restored tree. A third site the ticket did not name carries the same defect: `prune` joins the
same unvalidated `hash` onto `blobs/` and `remove_file`s it — an arbitrary **delete**, staged and
reproduced too, and fixed here.

**The fix — no third containment helper.** `restore` now runs three guards per entry, all before that
entry creates any directory:

1. `revert_engine::safe_target` for the write target. This is the crate's existing "resolve a
   caller-supplied relative path safely under a root" helper, whose own doc invites exactly this reuse,
   and which already guards `revert_engine`'s writes against manifests from *this same store* — so a
   restore and a revert can no longer disagree about which entries are legal. (`transfer::is_safe_name`
   was the other candidate the ticket named; it is stricter than the round trip can afford — it refuses
   any leaf beginning with `..`, which would make a legally-named captured file unrestorable, pinned by
   `cpe_1823_a_legal_dotdot_prefixed_filename_still_round_trips`. `is_self_or_descendant` lives in
   `src-tauri`, not in this crate, and answers a different question — src-dir vs. dest-dir on a copy.)
2. `fsutil::confined_to` on the resolved target, because 1 is textual and blind to a link planted at an
   **interior** component — a path of entirely innocent-looking segments that still leaves the folder.
3. `blob_source` on the read side: a plain hex content address (which alone forbids `.`, `/`, `\`, `:`
   and `..`), then `confined_to` against `blobs/` so a link planted at a blob's name cannot substitute
   another file's bytes. `prune` reuses its name half before its documented point of no return, so a
   planted manifest costs nothing rather than costing the manifest file and then failing.

Rejection is loud and per-entry, and every message names the offending manifest path (`refusal()`, one
shared formatter). A silent skip was never on the table: a restore is *believed*, so a silently absent
file is the CPE-1803/1804/1805/1816 defect again.

`root_relative_to_abs` is now the private `scan_source_path`, documented as the deliberately unvalidated
join with exactly one caller — `capture`'s blob loop, whose `rel` came from `strip_prefix`ing a real
`DirEntry` seconds earlier and is used to *read*. Routing that through `safe_target` would abort whole
captures on Linux/macOS over a legal filename like `2026-08-21 10:30 notes.txt`.

**Recorded, not fixed** *(superseded — see round 2, where this was fixed and the "pre-existing" framing
was wrong)*: `safe_segments` refuses `:` and `\` on *every* platform, so such a Unix filename captures
fine and then cannot be restored on the machine it came from.

**Gates.** `cargo clippy --all-targets -- -D warnings` → exit 0. `cargo test --lib` → **2291 passed, 0
failed**. (Round 1 also reported a failure in the separate `--test archive_panic_safety` binary. **That
claim is withdrawn**: the independent Reviewer got 21 passed / 0 failed, and a re-run here now gives the
same. It was environmental on that run — an unreproducible failure has no business sitting in the record.)

**Red-proof, per guard** (each observed red, then reverted):

| Test | Line broken | Observed |
|---|---|---|
| `..` traversal / absolute component / drive-relative | `safe_target(…)` → `scan_source_path(dest_path, rel)` **and** `if !confined_to(…)` → `if false` | all three red, naming the escaped file. Breaking guard 1 alone reds nothing **on Windows** (round 1 said "does not red them", unqualified — wrong: on Unix the absolute shape does red via `files_under`). Round 2 adds a test only guard 1 can satisfy, on every platform |
| interior link | `if !crate::fsutil::confined_to(&target, dest_path) {` → `if false {` | red **alone**: wrote through the junction into the sibling directory, and nothing else red — it uniquely covers guard 2 |
| escaping `hash` (read) | `blob_source(…)?` → `blobs_dir_path.join(&file.hash)` | red alone: "pulled 33 bytes from outside the blob store into the restored tree" |
| escaping `hash` (delete, `prune`) | `validate_blob_name(hash)…?` → `let _ = hash;` | red alone: deleted the victim file outside the store |
| `..evil` still restores | inserted `if !rel.split('/').all(transfer::is_safe_name) { return Err(…) }` | red: the over-tightening this test exists to catch |

### 2026-08-21 — round 2: the fix was in the wrong function

**The headline, and it is the important part.** `snapshot_capture::restore` has **no production caller** —
nothing in `src-tauri/` or `sidecar/` references `snapshot_capture` at all. Round 1 hardened the path only
the tests exercise, and then claimed in the PR body and in `restore`'s own doc that arbitrary read was
closed. With the two live sinks below still open, that claim was **false**. Both the independent Security
Auditor and the independent Reviewer found the same two, separately.

**The fourth sink — `revert_engine::apply_write`** (`revert_engine.rs:149`). `blobs_dir.join(&state.hash)`
where `state` came from `manifest_snapshot` — the same planted JSON. Observed:
`RestoreReport { applied: 1, skipped: [] }`, 45 bytes from outside the store landed in the user's tree,
and the report **counted it applied**. Shipping: `checkpoint_revert` / `checkpoint_revert_one`, registered
at `src-tauri/src/lib.rs:5357`/`5373`.

**The fifth sink, higher impact — `checkpoint_store::checkpoint_diff_file`** (`:554`). Same join, then
`fs::read` into `FileDiff.before`, which is **displayed**. The command is registered at
`src-tauri/src/lib.rs:5433` and called from the frontend via `bindings.gen.ts:2067`. Folded in
follow-up 5 while there: the `DIFF_MAX_BYTES` gate measured `state.size` — the manifest's *claim* — and
the `fs::read` under it was unbounded, so `size: 1` unlocked an unlimited read of an attacker-chosen file.
The cap is now measured on the blob itself, matching what the live half has always done.

Both call the same `blob_source` / `validate_blob_name`, now `pub(crate)` — no third validator. The
Auditor confirmed the hex check is fully anchored (it defeated `abc/../../../etc/passwd`, `abcd\0`,
`beef:stream`, `deadbeef.`, and a 4096-character name).

**Blocker 3 — an entry that was neither refused nor written, returning `Ok(())`.** `restore("sub/NUL")`
→ `Ok`, nothing on disk (the copy "succeeds" into the null device). `restore("evil.txt ")` → `Ok`, landed
as `evil.txt`. Three entries `a.txt`, `a.txt `, `a.txt.` → **one** file holding the second one's content.
That is precisely the class this module's own doc invokes as its reason never to skip silently, sitting
inside the function that says it. Fixed with the crate's existing predicates —
`fsutil::win32_name_is_unstable` and `transfer::is_windows_device_name` (made `pub(crate)`) — `cfg!(windows)`-gated,
because `NUL` and `notes. ` are ordinary distinct filenames on Linux and macOS.

**New blocker — round 1 broke a legally-named Unix file, and called it pre-existing.** `safe_segments`
refused `:` and `\` on every platform. For *revert* that is a per-file skip that continues; for `restore`
it **aborts the whole manifest**, so `2026-08-21 10:30 notes.txt` — or any macOS Finder name containing
`/`, which the volume stores as `:` — went from restoring fine to a half-restored tree. Different outcome,
and new. Fixed at the source: the rule is `cfg!(windows)`-gated in `safe_segments` itself, so restore and
revert move together. Nothing pinned the cross-platform refusal (the two path-safety tests there catch
their fixtures via `is_absolute`), and a `#[cfg(unix)]` round-trip test now covers it. This also resolves a
contradiction the Reviewer found between two paragraphs of my own doc.

**Two pins the Reviewer proved were missing by breaking the code and watching all 7 tests stay green:**
- Moving the three guards *below* `create_dir_all` — a plausible tidy-up refactor — kept every test green
  while creating an attacker-named directory outside the restore folder. `files_under` enumerates files
  only, so it structurally could not see it. The `..` test now carries a directory component and asserts
  the directory itself never appears.
- Breaking guard 1 alone reds nothing on Windows. There is now an input only the textual guard can refuse
  on any platform (`""` and `a//b`): `confined_to(dest, dest)` answers *true* for the empty path by
  design, and `a//b` resolves to a perfectly contained `dest/a/b`.

**Doc corrections, not left standing.** "A link planted at a blob's name cannot substitute another file's
bytes" was true of symlinks and junctions (both verified refused) and **false for hard links**, which need
no privilege on Windows and which `canonicalize` resolves to themselves. Also recorded: replacing `blobs/`
*itself* with a directory link relocates the whole store, because `confined_to` canonicalises the root too.
Both are now written down as limits rather than implied not to exist. The headless `": refusing …"`
message for an empty path now names its subject, and an absent `blobs/` is reported as an unopenable store
rather than as tampering.

**A test of mine passed for the wrong reason and was rewritten.** The follow-up-5 size test left the
oversize file on disk, so with the guard sabotaged the function still returned `Err` — from the *live*
half's cap, which has always measured the real file — and the byte-count assertion matched that message
just as happily. It passed under sabotage. With the live side shrunk to nine bytes, only the checkpoint
half can cap, and the sabotage now reds with `HARM: 5242881 bytes were read and returned past the 5242880
cap`. This is the third instance of the copied-sibling-assertion trap in this ticket's own tests; the two
in round 1 were caught by reasoning, this one only by running the sabotage.

**Round-2 gates.** `crates/server`: clippy `--all-targets -- -D warnings` → **exit 0** (two real
`err().expect()` findings fixed); `cargo test --lib` → **2296 passed, 0 failed, 4 ignored**;
`--test archive_panic_safety` → **21 passed, 0 failed**. `src-tauri`, **both** feature modes, which now
applies because the fix is in shipping code: clippy default → 0, clippy `--features sidecar-platform` → 0,
`cargo test` → **210 passed**, `cargo test --features sidecar-platform` → **265 passed**. No `specta::Type`
struct or command signature changed, so `bindings.gen.ts` is unaffected.

**Round-2 red-proofs** (each observed red, then reverted):

| Guard | Line broken | Observed |
|---|---|---|
| revert sink | `blob_source(blobs_dir, &state.hash)?` → `blobs_dir.join(&state.hash)` | `HARM: the revert pulled 45 bytes from outside the blob store into the user's tree` |
| diff sink | `blob_source(&store_dir.join("blobs"), …)?` → `store_dir.join("blobs").join(&state.hash)` | **this row was inaccurate as written** — the victim's file did *not* appear, because the fixture aimed two levels up when the store is five deep, so the raw join was `NotFound`. It red only on the error-message assertion. Corrected in round 3; it now reds with `HARM: … put a file from outside the store on screen: "THE VICTIM PRIVATE KEY FROM OUTSIDE THE STORE"` |
| diff size cap | `fs::metadata(&blob_path)…len()` → `state.size` | `HARM: 5242881 bytes were read and returned past the 5242880 cap, on the strength of a manifest claiming size: 1` |
| Win32 aliasing / device names | deleted the `win32_addresses_a_different_path` block | red: `sub/NUL` restored `Ok` with nothing on disk |
| guards-before-mkdir (PIN 1) | moved all three guards below `create_dir_all(parent)` | `HARM: a ".." manifest path created the directory …/planted-dir` |
| textual guard alone (PIN 2) | `safe_target(…)` → `scan_source_path(…)` | red **on Windows** on the `""` / `a//b` entry — the gap the Reviewer demonstrated |

**Not verified locally:** the `#[cfg(unix)]` colon/backslash round-trip test cannot run on this Windows
machine (`Q1\Q2 report.txt` is not a creatable name here) — it is covered only by CI's ubuntu and macOS
legs.

### 2026-08-21 — round 3: the guard was on the wrong function again, and this time it cost a file

**Same asymmetry, third instance.** Round 2 put `win32_addresses_a_different_path` in
`snapshot_capture::restore` — which still has no production caller — and not in `revert_engine`'s
`apply_write`/`apply_delete`, which `checkpoint_revert`/`checkpoint_revert_one` reach from registered
commands. Measured on the shipping path:

```text
device name:  report = RestoreReport { applied: 1, skipped: [] }; tree = []
alias:        plan   = [("a.txt", "Delete"), ("a.txt ", "Create")]
              report = RestoreReport { applied: 2, skipped: [] }; tree = []; a.txt = Err(NotFound)
```

The second is **destructive**. `plan_restore` reads `a.txt ` and `a.txt` as two keys; writes run first so
the Create lands *on* `a.txt`, then the Delete removes it. The user's only copy is gone and the command
reports complete success.

**Fixed by hoisting, not by patching two call sites.** The predicate now lives in `safe_segments`, so all
four `safe_target` callers inherit it and a fifth cannot forget it.

**Refusing the write was not enough — the paired Delete still destroyed the file.** With the guard in
place the report became `applied: 1, skipped: 1` and `a.txt` was *still gone*: `plan_restore` had emitted
`Delete a.txt` on its own reading. So `execute_restore` now **stands the destructive half down whenever
any write was skipped**: a delete's whole justification is "this path is not in the checkpoint", which
requires having read and applied the checkpoint correctly. Held-back deletes are reported per path with
the reason, never silent. Deliberately about the class, not that one alias — it also covers the
case-insensitive `A.txt`/`a.txt` collapse now recorded as open.

**A fourth instance, found by the inventory walk rather than by review.** Walking every manifest-derived
value to its sink turned up resolved containment (`confined_to`) on `restore` and **nowhere else**. A
junction at an interior component of the user's tree redirected the shipping revert both ways —
`applied: 2, skipped: []`, a file written outside the tree and a file outside the tree *deleted*.
`confined_to` moved into `safe_target` alongside the textual rules, so restore, both revert sinks and the
live-file diff all inherit it.

**The inventory, walked and recorded** — every manifest-derived value that reaches the filesystem:

| # | Value | Sink | Guards |
|---|---|---|---|
| 1 | `files` key → write target | `restore` pass 2 | `safe_target` (textual + Win32 + `confined_to`) |
| 2 | `files` key → write target | `revert_engine::apply_write` **(shipping)** | same, inherited |
| 3 | `files` key → delete target | `revert_engine::apply_delete` **(shipping)** | same, inherited, **+ delete stand-down** |
| 4 | `files` key → live-file read | `checkpoint_diff_file` **(shipping)** | same, inherited |
| 5 | `hash` → blob read | `restore`, `apply_write`, `checkpoint_diff_file` | `blob_source` (hex + `confined_to`) |
| 6 | `hash` → blob delete | `prune` **(shipping, via retention)** | `validate_blob_name` before the point of no return; containment deliberately omitted — a `remove_file` on a link removes the link |
| 7 | `size` → diff cap | `checkpoint_diff_file` | **not trusted at all** — the cap is enforced on the read |

Also checked and clear: `manifest_id` (caller-supplied) is guarded by `validate_manifest_id` in the single
`load_manifest` chokepoint; the manifest's own `id` field is never used to build a read path; `skipped[]`
is surfaced to the UI and never joined; `capture`'s blob-name join uses a hash it computed from disk, not
one it was handed.

**Folded in — the size cap was a TOCTOU pair.** `metadata()` then `read()` is two opens; measured, a
concurrent appender got `15728645` bytes past a `5242880` cap. Replaced with one `File::open` +
`.take(cap + 1)`, bounded by construction rather than narrowed. Applied to **both** halves of the diff —
fixing only the checkpoint half would have left the same read available through the same command, which
is this ticket's own recurring mistake.

**The abort premise was still false, so the structure changed instead of the wording.** `restore` now
validates every entry in a pre-pass and only then writes: the abort is total (nothing written, the
property `prune` already had) and one message names every offending entry. The old refuse-as-you-go loop
was justified by "nothing legitimate can produce a refused entry", which the round-2 gates falsified
across platforms — `NUL`, `notes. `, `a\b` are all names a Linux capture stores and a Windows restore must
refuse, and stores moved between machines is this ticket's own threat premise.

**Test-quality fixes, all four found by review rather than by me:**
- The B2 diff test **could not fail on its harm axis**: it aimed `../../` at a store that is five levels
  deep, so the raw join was `NotFound` and the secret never appeared either way. The idiom was copied
  from `snapshot_capture`'s read-side test, whose store is shallower — the copied-fixture trap, fourth
  instance in this ticket. Now the victim is planted as a sibling of the real `store_dir`, both the climb
  and an absolute path are staged, and **the fixture is asserted live** before the guard is asked about
  it. The error assertion moved off the path (which a `rel_path`-prefixed io error would satisfy) onto the
  rule that must refuse it.
- The textual test's **empty-path leg passed under its own sabotage** — all the power was in `a//b`. It
  now asserts the refusal is the textual guard's, and pins `refusal()`'s empty-name handling, which
  nothing covered.
- The aliasing leg asserted only the `Err`, never the collapse. It now asserts `files_under` first, and
  reds under sabotage with `three distinct manifest entries collapsed onto ["a.txt"]`.
- The colon regression was reported in `restore` but only covered through `execute_restore`. The names
  are now in the `#[cfg(unix)]` capture→restore round trip too, `NUL` and `notes. ` alongside them.

**Round-3 gates.** `crates/server`: clippy → **0**; `cargo test` (all targets) → **2301 lib + 21 + 22 + 45
+ 32 + 16 + 2 + 1 + 1, 0 failed**. `src-tauri` both modes: clippy default → **0**, sidecar → **0**;
`cargo test` → **210**, `--features sidecar-platform` → **265**. No `specta::Type` or command signature
touched.

**Round-3 red-proofs** (each observed red, then reverted):

| Guard | Line broken | Observed |
|---|---|---|
| Win32 rule hoisted into `safe_segments` | deleted the `win32_addresses_a_different_path` call | `applied: 1, skipped: []` (device) and `applied: 2, skipped: []` (alias) — the reported figures exactly, plus the `restore` test, from one line |
| delete stand-down | `if report.skipped.is_empty()` → `if true` | the alias destroyed the file with the refusal sitting visibly beside it; the cross-platform stand-down test red too |
| `confined_to` in `safe_target` | deleted the containment block | `HARM: the revert wrote through the planted link` **and** deleted the victim outside the tree — `applied: 2, skipped: []` |
| pre-pass totality | replaced with the refuse-as-you-go loop | `HARM: entries sorting before the refused one were already written: ["aaa-good.txt"]` |
| diff sink, corrected fixture | `blob_source(…)?` → raw join | now reds on harm: `put a file from outside the store on screen: "THE VICTIM PRIVATE KEY FROM OUTSIDE THE STORE"` |

**Recorded, not fixed:** a case-sensitive capture holding both `A.txt` and `a.txt` still collapses onto
one file on a case-insensitive volume — same class, not a per-segment rule (either name alone is legal;
only the pair is a problem), so it needs a whole-manifest collision check. Pre-existing, documented on
`win32_addresses_a_different_path`. The delete stand-down limits the damage.

**Still not verified locally:** the `#[cfg(unix)]` legs cannot run on this Windows machine — CI's ubuntu
and macOS Server-crates legs are their only verification, and must be green before merge.
**Update:** round 3's CI settled at **18 passing**, the only non-pass being `GUI smoke (windows-latest)`
reporting `skipping` (conditional, not a failure). Server crates went green on ubuntu **and** macOS, so
the `#[cfg(unix)]` legs and the cross-platform stand-down have run on both Unix platforms.

### 2026-08-21 — round 4: a delete that needed no attacker, and the window I opened myself

Ordered as instructed: the shipping data-loss bug first, then the one in the function nothing calls.

**B2 (done first) — `checkpoint_revert_one` destroyed the user's file, and not only under attack.** A
macOS or Linux capture holding `a.txt ` — a name those platforms store happily — cherry-reverted on
Windows deleted the user's `a.txt`. `revert_one` asks `checkpoint.get("a.txt")`, gets `None` because the
checkpoint spells it with a trailing space, and plans a lone `Delete`; on Windows the two are the same
file, so the checkpoint *does* hold it. Round 3's stand-down could not fire, because a one-action plan
contains no write to skip:

```text
plan   = [("a.txt", "Delete")]
report = RestoreReport { applied: 1, skipped: [] }; a.txt = Err(NotFound)
```

Fixed the Reviewer's way, not the Auditor's: the stand-down is now a property of the **checkpoint** —
every `checkpoint.keys()` through `safe_segments` — rather than of the plan's outcome. Whole-tree and
cherry-revert are covered by one rule, and a legitimate Linux checkpoint containing `a.txt ` stays
*partially* usable on Windows (preview, diff, and every restorable entry still work). Refusing the
checkpoint upstream at `manifest_snapshot` would have closed the same class by converting a data-loss bug
into a total-refusal bug for a real user with a real capture. It also makes my own round-3 comment
literally true: a delete's justification presupposes having read the checkpoint correctly, so key it on
the checkpoint.

**B1 — the pre-pass TOCTOU, which is mine.** Splitting `restore` into validate-all-then-write-all made
entry #1's `confined_to` verdict stale by the whole of pass 1 plus every preceding copy. The attacker
does not race blindly: the first byte on disk *is* the signal the verdicts are stale. Deterministic
wait-then-swap escaped **5/5** with `Ok(())` returned. Pass 2 now re-resolves each entry immediately
before its own copy; pass 1 is kept for the abort decision only, so both properties hold — nothing
written if any entry is refused, and the check a write relies on is the last thing before it.
Contained to the function with no production caller (the shipped `execute_restore` was never pre-pass and
the Auditor could not break it: 0/5 deterministic, 20,000+ blind swaps, zero escapes), but fixed anyway.

**`copy_file_into_claimed_slot` — asked, answered no.** CPE-1765's claim-the-name primitive is right
where the name is *picked*; this name is **chosen by the caller**, and `create_new` refuses a name that
already exists. Restoring a snapshot over a tree that still holds files, and `revert_engine`'s
first-class `Overwrite` op, both depend on writing onto an existing file — claiming the slot would turn
those into refusals. The residual final-component TOCTOU is recorded in the code instead.

**UX fix folded in:** held-back-delete reasons now name the blocking entries (up to three, then a count)
instead of only saying how many. A user looking at one held-back file no longer has to scan the rest of
the list to find the cause.

**A test of mine failed for the wrong reason and was caught by running it.** The cherry-revert fixture
used `serde_json`'s `take()` to move the manifest key, which leaves the old key behind holding `null`;
that fails to deserialize, so the command errored before planning anything and the test panicked at its
`unwrap` having proved nothing about deletes. Switched to `remove`. Fifth instance in this ticket of a
test that looked like it was testing something and was not.

**Recorded, not fixed** (all three confirmed acceptable in review): the stand-down is attacker-triggerable
as denial-of-revert; it is blunt — 500 deletes plus one locked file holds back all 500, so the UI copy
should read "held back, re-run after fixing" rather than 501 failures; and `safe_target` now
canonicalises on every call, unmeasured against a network share. Also recorded: an emptied `"files": {}`
turns a whole-tree revert into "delete every file" with nothing to stand down — pre-existing, semantically
defensible, and surfaced by `checkpoint_preview_revert` before the user confirms. All four are written
into the code next to the mechanism.

**Round-4 gates.** `crates/server`: clippy → **0**; `cargo test` → **2303 lib** + 21 + 22 + 45 + 32 + 16 +
2 + 1 + 1, **0 failed**. `src-tauri` both modes: clippy **0** / **0**; tests **210** / **265**.

**Round-4 red-proofs:**

| Guard | Line broken | Observed |
|---|---|---|
| checkpoint-keyed stand-down | `checkpoint.keys().filter(…)` → `Vec::new()` (the round-3 shape) | `HARM: cherry-revert deleted the user's only copy … RevertOutcome { applied: 1, skipped: [] }` — the UAT's figures exactly |
| pass-2 re-validation | pass 2 iterates pass 1's `(target, blob)` pairs | `HARM: a component swapped after pass 1 blessed it took the write outside the restore folder — restore returned Ok(())` |

### 2026-08-21 — round 5: every rule so far asked the SPELLING; the hazard is the RESOLVED PATH

Round 4's two blockers stay shut (audited independently: pre-pass TOCTOU 0/5, cherry-revert covered for
both trailing spellings through both commands). This round is about the axis all four previous rounds
were on the wrong side of.

**The blocker.** The round-4 stand-down arms via `checkpoint.keys().filter(|k| safe_segments(k).is_err())`.
`A.txt` and `a.txt` both *pass* `safe_segments` — neither is a device name, neither ends in a dot or
space — so the filter is empty and nothing arms. On a case-folding volume they are one file. Reproduced
here through the registered commands before touching anything, matching the auditor's figures exactly:

```text
CMD revert[case-alias]     -> RestoreReport { applied: 2, skipped: [] }; a.txt = Err(NotFound)
CMD revert_one[case-alias] -> RestoreReport { applied: 1, skipped: [] }; a.txt = Err(NotFound)
R4-OVERWRITE (lone Create) -> RestoreReport { applied: 1, skipped: [] }; payroll.csv = "ATTACKER CHOSEN BYTES"
```

Byte-for-byte the round-3 harm with `A.txt` substituted for `a.txt `. The third shape is worse and
structurally outside the stand-down's reach: `RestoreOp` has three variants and the stand-down guarded
one, so a **destructive non-delete** — a single `Create` — rewrote a live file with nothing skipped.

**A fourth shape, found here rather than reported: this is not a Windows bug.** A directory link inside
the reverted tree gives one file two perfectly legal spellings on Linux and macOS as well —
`sub/f.txt` and `alias/f.txt` — and `confined_to` admits both, *correctly*, because both resolve inside
the tree. Cherry-reverting the aliased spelling deleted the checkpoint's own file: `applied: 1,
skipped: []`. That settles the design question on its own — a Windows-gated name rule could never have
closed this — and it is the leg that runs on the ubuntu and macOS CI legs.

**The design, and where I departed from the recommendation.** Adopted the principle in full —
`fsutil::confined_to`'s own "assert on the resolved path, never on the spelling that produced it" — as a
new `revert_engine::landing(root, rel)`: canonicalise and answer *which file does this address*, `None`
if nothing answers yet. Deliberately not a safety check and deliberately not `safe_target`: it resolves
even the spellings `safe_target` refuses (it has to — `a.txt ` must resolve to `a.txt` for the collision
to be visible) and declines only the shapes that are escape questions rather than identity questions
(`..`, `.`, empty, absolute), which stay `safe_target`'s. `None` is always the safe direction: no
collision, and the action then faces `safe_target` exactly as before.

Three rules, one helper:

1. **`apply_write`: a `Create` whose target already exists is refused** — checked immediately before the
   copy. This is the premise, not the name: `plan_restore` emits `Create` only for a path in the
   checkpoint and absent from the scan, so if something already answers to it, the plan's reading of the
   tree and the filesystem's resolution disagree — which *is* the aliasing signal. `Overwrite` is
   untouched, so no legitimate overwrite is refused.
2. **`execute_restore`: a delete whose resolved target is also addressed by a checkpoint entry stands
   down** — per delete, naming the colliding key. A delete's whole justification is "this path is not in
   the checkpoint"; asked of the spelling that can be true while it is false of the file.
3. **`snapshot_capture::restore`: two entries that resolve onto one file are refused** — in pass 1 where
   the target already exists (total abort, nothing written) and in pass 2 by observed identity where the
   destination is fresh and the collision is invisible until the first entry lands.

This subsumes trailing space and dot, case folding, 8.3 short names, Unicode-folding volumes and
in-tree links without enumerating any of them; it fixes cherry-revert for free, because the collision is
visible in `checkpoint` versus `current` with no write in the plan; and it refuses **nothing** by name,
so the objection that (correctly) killed the round-4 upstream name-refusal — a legitimate Linux capture
becoming an unusable checkpoint on Windows — does not bite.

**Where I did not take the recommendation.** It proposed resolving *every* write target in a plan-level
pre-pass and refusing write/write collisions there. I did not, for two reasons. First, round 4's own
lesson: a pre-pass verdict is stale by the time the write happens, and this ticket has already paid for
that once (5/5 escapes). Rule 1 checked immediately before each copy covers the same ground — the first
`Create` writes, the second then finds its target occupied — with the verdict fresh. Second, in
`execute_restore` a write/write collision is close to unreachable independently of rule 1: two
`Overwrite`s onto one file would require the *same* `scan_dir` to have returned both spellings. The
whole-manifest collision check does exist where it is genuinely needed — `restore`, where every entry is
a write and there is no `Create`/`Overwrite` distinction to lean on.

**The blanket checkpoint-keyed stand-down is kept, not replaced.** The two are complementary: resolution
cannot answer for a device name (`sub/NUL` resolves to `\\?\NUL`, which no checkpoint entry collides
with), and the blanket rule cannot see an alias that is spelled legally. Removing either would reopen
something.

**Which destructive shape is widest — settled, then re-settled when my own settlement turned out to
contain a false claim.** The Reviewer said the emptied `"files": {}` manifest; the Auditor said the case
alias. I ruled for the Auditor on *reach*, arguing the empty-manifest shape is narrower because it needs
the user to confirm a whole-tree revert whose `checkpoint_preview_revert` reads "delete 5, restore 0".
**That argument is measured false**, and it sat in the one section of this log whose stated purpose is
correcting false claims:

```text
C1 CMD revert[empty manifest]:     applied=5 skipped=0   survivors = []
C2 CMD revert_one[empty manifest]: applied=1 skipped=0   survivors = [f1, f2, f4, f5]
```

C2 destroys files **one at a time** through `checkpoint_revert_one`, behind a per-file confirm that says
nothing about a mass delete and never consults `checkpoint_preview_revert` at all. So the empty-manifest
shape is not merely wider than I ranked it — it is wider than *either* checker first said, and the
Auditor has withdrawn its own round-4 position on the strength of C2.

**The ranking has also gone moot in the direction that matters.** The alias was the widest shape while it
was open; round 5 closes it. What that leaves standing is the emptied manifest, so both ranking sites —
`revert_engine.rs:128-151` and this paragraph, the only two in the tree — now read: **the emptied
`"files": {}` manifest (CPE-1847) is the widest REMAINING shape**, with C2 recorded next to it and the
whole-tree-confirm argument deleted rather than softened. It is still not closed by refusing it: an empty
checkpoint is a legal capture of an empty folder, so a rule that refuses it refuses a real one.

**Recorded-as-fact errors corrected** (a false verified-fact is worse than an honest assumption):

1. `revert_engine.rs:112` — "the widest destructive shape … is **not** this one". Rewritten as above.
2. The final-component residual read as "the final component is unprotected". It is not: `confined_to`
   canonicalises the final component too, and an independent audit planted **17,488** symlinks at it for
   **zero** writes through. The true residual is "checked, but not atomically". And it is **reducible,
   not irreducible** — this crate already ships the pattern (`batch_media`'s never-follow-a-link-at-the-
   final-component open, `O_NOFOLLOW` / `FILE_FLAG_OPEN_REPARSE_POINT`, no libc, already used by
   `batch_execute`); adopting it changes `fs::copy`'s attribute-preserving behaviour on Windows, so it is
   **CPE-1846**, and the comment now cites that ID rather than saying "its own ticket" and leaving the
   reader to find it.
3. The `safe_target` canonicalise-cost note said "a 10k-file revert is 10k+ canonicalise walks". Wrong
   since round 4: `restore` resolves every entry **twice**, so it is 20k+, plus round 5's `landing`
   resolutions (one per checkpoint key and one per delete, only when the plan contains a delete). Also
   recorded: `safe_segments` over 20,000 keys measures ~32 ms in a debug build and is dwarfed by the
   walks, so there is nothing to cache on the textual side — no caching added.
4. `snapshot_capture.rs:359` — the pass-2 error was a literal multi-line format string carrying ~18 stray
   spaces (`"…changed during the                  restore…"`), reaching the user verbatim. Fixed.
5. The denial-of-revert follow-up got **cheaper** in round 4, and is now recorded at its real cost: one
   checkpoint key with a trailing space — no blob, no write attempted, no I/O at all — holds back every
   delete of that checkpoint. Also corrected: on that branch the hold is **permanent**, not transient
   (measured by the Reviewer at scale: one unrestorable key, 200 files added since, one restorable entry
   → `applied: 1, skipped: 201`, 200 survivors, restorable half correct), which is why that branch's
   message deliberately omits "re-run once resolved" while the `report.skipped` branch keeps it.

**Also fixed, a real docs defect:** `const NAMED_CAUSES` had been dropped between
`win32_addresses_a_different_path`'s 38-line doc block and its signature, so rustdoc rendered four rounds
of argument on a `usize` and left the function undocumented. Moved below the function, with a note saying
what it did. In this ticket the docs are the artifact, so this is not cosmetic.

**Round-5 gates.** `crates/server`: clippy `--all-targets -- -D warnings` → **0**; `cargo test` →
**2308 lib** (4 ignored) + `archive_panic_safety` 21 + `binary_data_preview_panic_safety` 22 +
`checkpoint_roundtrip` 2 + `finder_tags_os_interop` 1 + `native_meta_os_interop` 1 +
`parser_panic_safety` 45 + `sample_fixtures` 16 + `thumb_svg_panic_safety` 32 + `ticket_mcp` 0,
**0 failed**. `src-tauri` both feature modes: clippy default → **0**, `--features sidecar-platform` →
**0**; `cargo test` → **210**, `--features sidecar-platform` → **265**. No `specta::Type` struct or
command signature changed, so `bindings.gen.ts` is unaffected.

**Round-5 red-proofs.** Every new test asserts the harm did not happen *before* it looks at the `Result`,
and carries the round-4 fixture-liveness assertion ("fixture is inert: … or this test certifies
nothing") — six inert tests have been caught in this ticket and in all six the fixture never reached the
harm. Each proof is one line, observed red, then reverted:

| Guard | Line broken | Observed |
|---|---|---|
| `Create` premise (`apply_write`) | `if action.op == RestoreOp::Create && …` → `if false && action.op == …` | three tests red on their harm axis: `HARM: the revert destroyed the user's file via a case alias` (`applied: 1`, with the delete correctly held back — so the two guards are independently load-bearing), `HARM: a lone Create rewrote the user's file under an aliased spelling — RestoreReport { applied: 1, skipped: [] }`, and `HARM: checkpoint_revert destroyed the user's file through a case alias` |
| delete resolution stand-down | `.and_then(\|at\| checkpoint_lands_on.get(&at).copied())` → `.and_then(\|_at\| None::<&String>)` | `HARM: the revert deleted a file the checkpoint holds, reached under its other spelling — RestoreReport { applied: 1, skipped: [] }` (the cross-platform link leg) and `HARM: checkpoint_revert_one destroyed the user's file through a case alias — RevertOutcome { applied: 1, skipped: [] }` |
| pass-1 collision (`restore`) | `if let Some(first) = lands_on.insert(at, rel) {` → `… .filter(\|_\| false) {` | `HARM: a manifest refused for a collision still overwrote the destination — the abort must be total when pass 1 can see the collision` (pass 2 still caught it, but only after the write — which is exactly the totality property pass 1 exists for) |
| pass-2 collision (`restore`) | `if written.contains(&at) {` → `if false && written.contains(&at) {` | `HARM: a manifest with two entries restored as ["A.txt"] and returned Ok(()) — one captured file silently never arrived` |

**Still not verified locally, and it is the merge gate:** every `#[cfg(unix)]` leg, plus the new
cross-platform link-alias delete test, can only run on ubuntu and macOS. Server crates must be green on
both on this head before merge.
