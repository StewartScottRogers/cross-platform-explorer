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

**Recorded, not fixed:** `safe_segments` refuses `:` and `\` on *every* platform, so such a Unix filename
captures fine and then cannot be restored on the machine it came from. Pre-existing (revert has always
refused it) and inherited on purpose — forking a second, more lenient predicate is how the next hole gets
found. The fix belongs in `safe_segments`, `cfg!(windows)`-gating the refusal so restore and revert move
together. Written up in `restore`'s doc.

**Gates.** `cargo clippy --all-targets -- -D warnings` → exit 0. `cargo test --lib` → **2291 passed, 0
failed**. src-tauri untouched (nothing outside `crates/server` references `snapshot_capture`), no
`specta::Type` touched, so no bindings regen. One **pre-existing, unrelated** failure in the separate
`--test archive_panic_safety` binary (`sevenz_signature_header_offset_overflow_is_caught_and_returns_err`)
— reproduced identically with this change stashed, and it passes when run alone, so it is a flaky 7z leg,
not this work.

**Red-proof, per guard** (each observed red, then reverted):

| Test | Line broken | Observed |
|---|---|---|
| `..` traversal / absolute component / drive-relative | `safe_target(…)` → `scan_source_path(dest_path, rel)` **and** `if !confined_to(…)` → `if false` | all three red, naming the escaped file. One line alone does **not** red them — the two guards overlap by design, so the proof is against the genuine pre-fix pair |
| interior link | `if !crate::fsutil::confined_to(&target, dest_path) {` → `if false {` | red **alone**: wrote through the junction into the sibling directory, and nothing else red — it uniquely covers guard 2 |
| escaping `hash` (read) | `blob_source(…)?` → `blobs_dir_path.join(&file.hash)` | red alone: "pulled 33 bytes from outside the blob store into the restored tree" |
| escaping `hash` (delete, `prune`) | `validate_blob_name(hash)…?` → `let _ = hash;` | red alone: deleted the victim file outside the store |
| `..evil` still restores | inserted `if !rel.split('/').all(transfer::is_safe_name) { return Err(…) }` | red: the over-tightening this test exists to catch |
