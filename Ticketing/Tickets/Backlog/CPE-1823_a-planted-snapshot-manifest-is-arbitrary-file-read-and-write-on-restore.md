---
id: CPE-1823
title: "Security: a planted snapshot manifest is arbitrary file read and write on restore"
type: bug
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-20
closed:
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
