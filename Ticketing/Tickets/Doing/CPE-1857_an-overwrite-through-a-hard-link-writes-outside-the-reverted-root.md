---
id: CPE-1857
title: an Overwrite through a pre-existing hard link writes outside the reverted root
type: bug
priority: Medium
status: Doing
tags: ready
estimate: M
created: 2026-08-22
closed:
---

## Problem

`fs::copy` writes **through an existing inode**. If an in-tree name is a hard link to a file outside the
reverted root, an `Overwrite` rewrites the outside file.

Measured by the independent Security Auditor during CPE-1823's round-5 audit, through the registered
command:

```
CMD revert[hardlink out-of-tree]: applied=1 skipped=0
      h.txt   = Ok("CHECKPOINT-H")
      outside = Ok("CHECKPOINT-H")     <- outside the reverted root
```

`canonicalize` cannot see hard links — a hard link is not a reparse point and has no "target" to resolve —
so neither `confined_to` nor CPE-1823's new `landing` can detect it. Both correctly report the path as
inside the root, because it **is**.

## The precondition, stated precisely

A planted manifest **cannot create** the hard link. It can only **aim at one that already exists**.

But the aiming half is fully attacker-chosen: the manifest controls both the path and the blob hash, so
given any pre-existing in-tree hard link, an attacker picks which of the store's blobs lands on the far
end of it.

## Why Medium

It needs a pre-existing hard link, which is not something the threat model can manufacture. Against that:
hard links occur naturally (deduplicating backup tools, package managers, some sync clients), the threat
premise CPE-1823 established already covers a store copied between machines or synced by a cloud client,
and the write lands somewhere the user never named.

This is a property of **every file writer in the crate**, not of CPE-1823's changes — it is already
recorded on `confined_to`'s own doc as a known limit. CPE-1823 correctly did not treat it as a regression.

## Acceptance criteria

- [x] Decide and record whether writes should refuse a target with a link count above 1, or open with a
      flag that refuses to follow, or accept the limit explicitly. Each has a real cost — refusing on link
      count would break legitimate deduplicated trees — so the decision matters more than the mechanism.
- [x] Whatever is chosen applies to **every** writer in the crate, not just the revert path. Enumerate them
      first: `revert_engine::apply_write`, `snapshot_capture::restore`, the transfer and archive writers,
      and anything else `fs::copy`/`fs::write` reaches. A partial sweep presented as complete is this
      repo's most-repeated defect.
- [x] A test stages the exact shape above — in-tree name hard-linked to an out-of-tree file — and asserts
      the outside file's bytes are unchanged **before** asserting the `Result`.
- [x] Red-proof with the minimal realistic change, observe red, revert, record the line. Assert the fixture
      is live first: confirm the hard link really was created (link count, or writing through one name and
      reading the other), or the test certifies nothing.
- [x] Update `confined_to`'s doc if the limit changes, and CPE-1823's residual notes if this closes one of
      them.

## Notes

Found by the independent Security Auditor during CPE-1823's round-5 audit — the round where 24 attack
shapes were tried and none got through. It classified this as pre-existing and not a merge blocker, and
asked that it be filed rather than folded into a ticket already five rounds deep.

Related: CPE-1846 (the final-component link swap, reducible with the crate's existing NOFOLLOW pattern),
CPE-1847 (the emptied manifest), CPE-1823 (the guards this sits beside).

## Work Log

### 2026-08-23 — fixed at the write; the rule is a link count read off the handle

**The decision (AC1).** Of the ticket's three options, **refuse a target whose open handle reports more
than one name**. The second option does not exist for this: `O_NOFOLLOW` /
`FILE_FLAG_OPEN_REPARSE_POINT` refuse a name that *stands in* for another object, and a hard link does
not stand in for anything — it **is** the object, on equal footing with every other name for it. There
is no following to refuse. The third (break the link: sibling + place) was already built and measured by
CPE-1870 and loses on correctness before it loses on speed — a plain `rename` silently reduces the
destination's explicit ACL to the parent's inherited set, and `ReplaceFileW`, which preserves it, costs
7.5 ms/file against `copy_file_onto_no_follow`'s 0.39 ms.

**Mechanism.** `HandleFacts::links` was *already being read* at every site that mattered —
`nNumberOfLinks` off the `GetFileInformationByHandle` call the reparse-point check already makes on
Windows, `st_nlink` off the `fstat` the Unix arm already makes. So the rule costs **no extra syscall** in
the restore writer, and it cannot be defeated by a path swap, because it is read from the very handle the
bytes are about to go through. `revert_engine::apply_write` classifies the refusal **permanent** (a file
does not shed its other names between runs, so "run the revert again" would be exactly the CPE-1845 loop).

**The sweep (AC2).** 68 raw `fs::copy`/`fs::write`/`File::create`/`OpenOptions` hits under
`crates/server/src` → 54 production sites → every one that can land on a **pre-existing** name given a
verdict. The full table is committed on `batch_media::name_is_multiply_linked`. Scoping rule recorded
there: a name claimed with `create_new`/`O_EXCL`/`CREATE_NEW` has exactly one link, so every
`create_exclusive` / `claim_*_slot` / `copy_file_into_claimed_slot` site is **structurally immune** and
needs nothing.

Refused now: `copy_file_onto_no_follow` (manifest-chosen name — the measured hole),
`archive::entry_sink_action` (archive-entry-chosen name; rows 16/19-22 of the CPE-1733 table),
`transfer::download_tree`'s leaf (server-chosen name). `batch_media::open_output_verified` already did,
with a kinder directory census.

**Accepted explicitly**, per row, with reasons in the table: the six compressors and the two `.gz`
extract leaves (destination typed into a Save dialog — no untrusted input picks it); `secure_shred`
(writing through the inode *is* the operation); the Windows ADS write (a hard link shares the inode and
therefore the streams — inode semantics, not a redirected write); the app's private JSON stores and
journals (fixed names in app-data; planting a link at one needs local write access, outside a threat
model whose premise is that a manifest can only *aim at* a link, never create one).

**Flagged, not fixed:** `backup.rs::copy_one_verified` — a bare `std::fs::copy(src, dst)` onto a
user-named backup destination with **no** link guard of any kind, not even for symlinks. `safe_join`'s
own doc calls itself "the cheap first filter, NOT the guarantee", and `contained_under` is applied only
to the delete list, never to copy/update. Out of this ticket's threat model (the plan's `rel` comes from
our own scan, not from untrusted data), but it deserves its own ticket rather than a line edited in
passing here.

**Evidence — before (today's code, vulnerability reproduced).** `left` is `CHECKPOINT-H`:

```text
cpe_1857_a_hard_linked_destination_is_never_written_through
  HARM: the restore wrote the checkpoint's bytes through a hard link, into a file outside
        the reverted root that nothing in the plan ever named
  left:  Some([67, 72, 69, 67, 75, 80, 79, 73, 78, 84, 45, 72])          "CHECKPOINT-H"
  right: Some([86, 73, 67, 84, 73, 77, 32, 67, 79, 78, 84, 69, 78, 84])  "VICTIM CONTENT"

cpe_1857_an_overwrite_through_a_hard_link_never_reaches_the_outside_file
  RestoreReport { applied: 3, skipped: [], held_back: None }
```

That second line is the ticket's own measurement reproduced through the engine: applied, nothing
skipped, and the outside file holding the checkpoint's bytes.

**Evidence — after.**

```text
test archive::tests::cpe_1857_a_zip_entry_aimed_at_a_hard_link_never_writes_the_outside_file ... ok
test fsutil::tests::cpe_1857_a_hard_linked_destination_is_never_written_through ... ok
test revert_engine::tests::cpe_1857_an_overwrite_through_a_hard_link_never_reaches_the_outside_file ... ok
test transfer::tests::cpe_1857_download_tree_never_writes_through_a_preexisting_hard_linked_leaf ... ok
test result: ok. 2368 passed; 0 failed; 8 ignored
```

**Red-proof (AC4), each guard neutralised on its own.** `fsutil.rs` `facts.links > 1` → both the fsutil
and the engine test red on their HARM axis (`applied: 3, skipped: []`, outside file = `CHECKPOINT-H`).
`archive.rs` `entry_sink_action`'s check → `ARCHIVE PAYLOAD` lands outside the extraction folder with
`Ok(...)` returned. `transfer.rs`'s check → `pwn` lands outside the download root. And the *classifier*
alone (`name_is_multiply_linked` in `apply_write`) → `outcome: SkippedByPlan`, `next_step: "This one is
temporary … run the revert again"`, i.e. the CPE-1845 loop, with the harm assertions still green.

**Fixture liveness is asserted before every verdict**, the only way a hard link can be proved live:
content written through the OUTSIDE name and read back through the IN-TREE one. A link count alone is
not enough and two unrelated files would certify nothing. Every test skips loudly via `skip_notice!`
("this run verified NOTHING") on a filesystem that will not create a hard link.

**What the new write path costs — stated plainly, because PURPOSE.md's tiebreaker is fast/small/predictable.**

- **The restore writer: nothing.** No new syscall, no new allocation, no change to the bytes written.
  `links` was already in the `HandleFacts` the function already read.
- **Archive extraction and downloads: one extra name probe per file entry** — a `symlink_metadata` on
  Unix, a `CreateFileW` with `FILE_READ_ATTRIBUTES` on Windows. Attribute-only calls, against loops that
  already do a `File::create` plus an `io::copy` of the entry's whole payload for every entry accepted.
- **Behaviour anyone could notice: a real capability loss, and it is the price of the property.**
  Restoring, extracting onto, or downloading onto a file that legitimately has a second name — a
  deduplicating backup (`rsync --link-dest`, Time Machine), a package-manager store, some sync clients —
  now **refuses that one entry** instead of writing it. Per file, with a reason, never a silent skip and
  never an aborted run: every other entry still applies. Remedy: break the link (copy the file over
  itself) and re-run.
- **Nothing changes for link counts, inode identity, ADS, permissions or timestamps**, because the write
  path itself is unchanged — this refuses before the write rather than writing differently. That is the
  whole reason the create-new-then-rename route was not taken.

**Assumptions logged.**

1. **`handle_facts == None` does not fire the rule.** On a platform whose identity model `batch_media`
   does not know there is no count to read, and the rule is skipped rather than failing closed — the
   same tolerance the reparse-point and directory checks in the same function already have. Failing
   closed there would stop restore working entirely on a platform rather than protect anything on the two
   this ships on.
2. **The count is read before the write, so an actor who creates a *new* name for the object we hold, in
   the window between the read and the last byte, still gets the write.** The same irreducible window
   `open_output_verified` records for its own census. Out of this ticket's threat model, which is a
   planted manifest: a manifest can only aim at a link that already exists.
3. **`open_output_verified`'s directory census was NOT adopted for the restore writer**, though it is
   strictly kinder (it allows the write when all of the object's names are inside the allowed region).
   Two reasons, the second decisive: that census scans one *directory* and a revert's allowed region is a
   whole *tree*, so containment costs an O(tree) walk per multiply-linked file and reusing one walk is the
   memo-replay CPE-1667 finding 2 killed; and even an all-inside answer is not clean, because
   `plan_restore` enumerated those names as separate entries with separate blobs, so writing through one
   clobbers the other's result and the outcome depends on apply order.
4. **`transfer`'s skip is an `eprintln`, not an `undelivered` entry** — decided, not defaulted.
   `undelivered` makes the whole `download_tree` call return `Err`; a hard-linked leaf is a per-entry
   policy skip exactly like the symlinked leaf beside it, and one collided name must not cost the user
   every other file in the tree. The first cut used `undelivered` and its own test reddened on it.

**Docs.** `confined_to`'s "What this does NOT cover" now records that it cannot see a hard link, that the
limit is unchanged, and that the answer lives at the write — with an explicit "do not add a hard-link
check here". `copy_file_onto_no_follow`'s residual bullet that declined this refusal is struck through
and corrected: it conflated `create_new` (refuses *every* existing name — a categorical break) with
`links > 1` (refuses only a destination that genuinely has a second name — in an ordinary tree, no files
at all). That conflation is why this sat open.

**Verification.** `cargo clippy --all-targets -- -D warnings` and `--features index` both clean;
`cargo test --lib` 2368 passed / 0 failed; `checkpoint_roundtrip`, `archive_panic_safety`,
`sample_fixtures` green.

### 2026-08-23 (later) — CI caught the cross-platform half: directories have a link count too

First push went red on `Server crates (ubuntu-latest)` and `(macos-latest)` and **green on
`windows-latest`** — the exact asymmetry this repo has been bitten by before, in the other direction.

```text
archive::tests::cpe1759_a_link_entry_overwrites_an_ordinary_file_but_a_directory_is_a_failure FAILED
  a link entry that cannot displace a DIRECTORY is the write failing, not a guard refusing - it
  aborts, like `File::create` on the same path would:
  ArchiveReport { done: 1, failed: 0, skipped: 1, errors: ["good_link: ".../out/good_link" is a
  hard link ..."] }
```

**Cause.** On Unix every directory has `nlink >= 2` **by construction** — its own `.` entry, plus the
entry its parent holds for it, plus one more per subdirectory. On Windows `nNumberOfLinks` for a
directory is 1. So `facts.links > 1` without an `is_dir` clause reads **every** directory on Linux and
macOS as a hard link, and **no** directory on Windows. It turned that test's tar link entry — a link
entry that cannot displace a directory, which is the *write failing* and must abort — into a hard-link
*skip*.

**Fix.** `name_is_multiply_linked` is now `!facts.is_dir && facts.links > 1`. Excluding directories costs
nothing this rule exists for: a directory is not something a file's bytes can be written into, and every
caller already refuses one on its own terms. `copy_file_onto_no_follow` and `open_output_verified` were
never exposed — both run their `is_dir` refusal *before* the count — so the defect was confined to the
new path-based helper.

**Pinned** by `batch_media::tests::cpe_1857_the_link_count_rule_answers_no_for_a_plain_file_and_for_a_directory`,
which asserts all three answers (plain file: no; **directory: no**, with the platform asymmetry spelled
out in the failure message; absent name: no) plus the positive leg with its liveness proof. It runs on
every platform, but only the Unix legs can red on the directory row — stated in the test rather than left
for the next person to rediscover, since a Windows-only local run cannot reproduce this class at all.

### 2026-08-23 (round-trip 2/3) — Security Auditor finding 1: the gate failed OPEN

**The finding.** `name_is_multiply_linked` read its answer through `probe_no_follow`, which funnels every
probe through `facts_or_unreadable` and downgrades to `Probe::Unreadable` whenever
`FileIdentity::is_degenerate()` — a zero volume serial or zero file index. This repo already documents
(CPE-1642 finding F2, on `is_degenerate` itself) that `GetFileInformationByHandle` **succeeds and hands
back a zero index** on several network redirectors. In that case `nNumberOfLinks` is present and correct
**and was thrown away**; the function answered `false`; the write proceeded. On any destination whose
volume reports a degenerate identity — **a network share, a first-class destination for this app** —
extraction and download wrote through a pre-existing hard link exactly as before, with the guard present
and silent.

**Two changes, because the finding has two halves and each is independently load-bearing.**

1. **Decouple the link count from the identity gate.** `probe_no_follow` (identity questions, still
   gated) is now a thin wrapper over `probe_facts_no_follow` (ungated facts). `name_links` reads the
   latter directly. A link count is **not** an identity — the degeneracy gate answers "can I compare this
   object with another one", which is a question nobody asked here — so gating it discarded a good
   answer. This is also what makes change 2 payable: before the split, a degenerate identity produced
   `Unreadable` for *every object on the volume*, so failing closed would have refused every entry
   extracted to a share. Afterwards `Unknown` means only "the probe genuinely could not read the name".

2. **Three-valued `NameLinks` (`One` / `Many(u64)` / `NoFileHere` / `Unknown`), and the two PRE-WRITE
   gates refuse on `Unknown`.** `archive::entry_sink_action` **aborts**, on exactly the terms
   `entry_slot_action`'s own `Unknown` arm already aborts an unreadable *link* verdict — same condition,
   same answer, and `cpe1759_an_unreadable_slot_aborts_both_tar_paths_rather_than_being_skipped` already
   pins the shape. `transfer::download_tree` records it in **`undelivered`**, the route
   `LeafProbe::Uninspectable` already takes, so the transfer cannot report `Ok(n)` for a tree it did not
   deliver.

**Why the three sites differ — the reasoning the Foreman asked to have written down.**

| site | kind | answer to `Unknown` | why |
|---|---|---|---|
| `revert_engine::apply_write`'s refusal mapper | **classifier** | fold into "no" (`name_is_multiply_linked`) | The write is already settled — refused or done — so this chooses WORDING and can no longer choose where a byte goes. An unknown answer picks `transient`, the classification that path had for **every** refusal before CPE-1857, so it degrades to the previous behaviour rather than to a new wrong one. |
| `archive::entry_sink_action` | **gate** | abort | The bytes have not moved. A gate that answers "no" when it cannot tell is a guard that is not there. `resolve_output_containment` is the in-repo precedent for the other direction (`Probe::Unreadable → Containment::Unverifiable`, refuse). |
| `transfer::download_tree`'s leaf | **gate** | record in `undelivered` | Same reason; the `undelivered` route rather than abort because that is what this function already does for an uninspectable leaf, and it keeps the count honest. |

The **restore writer was never affected**: `copy_file_onto_no_follow` reads `HandleFacts` from
`handle_facts`, which does not gate on degeneracy at all. Verified by reading it, not assumed.

**Red-then-green, and each half proved separately so neither can be carried by the other.**

*Sabotage A — the exact shape at HEAD `d13715db` (gated probe **and** the bool fold at both gates):*

```text
cpe_1857_a_degenerate_identity_must_not_silently_disable_the_hard_link_guard   FAILED
  HARM: on a volume whose identity is degenerate - a network share - the extraction wrote an
  archive entry's bytes through a hard link into a file outside the extraction folder. The
  link COUNT was readable the whole time; it was discarded by an identity gate that has
  nothing to do with this question: Ok("...\out")
  left:  Some([65,82,67,72,73,86,69,32,80,65,89,76,79,65,68])   "ARCHIVE PAYLOAD"
  right: Some([79,85,84,83,73,68,69,32,67,79,78,84,69,78,84])   "OUTSIDE CONTENT"

cpe_1857_the_transfer_gate_refuses_when_it_cannot_read_the_link_count          FAILED
  HARM: ... the download wrote the remote server's bytes through a hard link into a file
  outside the download root.
  left:  Some([112,119,110])                          "pwn"
  right: Some([111,114,105,103,105,110,97,108])        "original"

cpe_1857_an_unreadable_probe_refuses_the_entry_rather_than_writing_it          FAILED
  HARM: the gate could not read how many names this file has and wrote the entry anyway - a
  guard that answers "no" when it cannot tell is a guard that is not there: Ok("...\out")
  left:  Some([65,82,67,72,73,86,69,32,80,65,89,76,79,65,68])   "ARCHIVE PAYLOAD"
  right: Some([65,76,82,69,65,68,89,32,72,69,82,69])            "ALREADY HERE"

test result: FAILED. 5 passed; 3 failed
```

*Sabotage B — the probe split kept, only the gates folding `Unknown` into "no":* the two degenerate legs
go **green from the split alone**, and only the two unreadable legs red. `test result: FAILED. 6 passed;
2 failed`. So change 1 closes the network-share half and change 2 closes the unreadable half, and neither
is redundant.

*Green, both restored:* `test result: ok. 8 passed; 0 failed` for the `cpe_1857` set; **2372 passed / 0
failed** for the whole lib.

**The seam, and why it exists.** Neither shape can be staged on CI or a developer's box: a real SMB/NFS
redirector that zeroes the file index cannot be conjured, and the auditor confirmed a denied
`FILE_READ_ATTRIBUTES` ACE does not reach this path on this host. `ProbeInjection` +
`ProbeReset::arm(..)` (thread-local, RAII-reset even on a panic, `#[cfg(test)]` only, `pub(crate)` so
`archive` and `transfer` drive it through their **real** entry points) is the only instrument that can
drive this fail-open red at all — and a fail-open no test can reach is one that comes back.

**Finding 2 (docs).** `copy_file_onto_no_follow`'s residual section is now a numbered list of **four**
ways past the rule, replacing a sentence that claimed it "cannot be defeated by a path swap" — true, and
narrower than it read:

1. a **new** name created for the held object mid-write (the irreducible window, out of the manifest
   threat model);
2. a platform with **no** identity model, where there is no count to read — explicitly narrowed to a
   *missing* count, which is what this row used to be read as also covering and does not;
3. a filesystem that reports `nlink == 1` **inaccurately** (some FUSE/network mounts) — the rule silently
   does not fire and nothing reports that it could not answer, because as far as every layer is concerned
   it *did*. No portable way to ask a filesystem whether its count is trustworthy, so recorded rather than
   defended against — and it is exactly why `NameLinks::Unknown` matters at a gate: an honest "I cannot
   tell" is recoverable and a confident wrong number is not;
4. a Linux **bind mount** at the destination (`mount --bind /outside/victim /root/h.txt`) — `st_nlink`
   stays 1 and `canonicalize` resolves the in-tree name to itself, so nothing fires. Needs mount
   privilege, so recorded and deliberately not defended against.

**Out of scope, confirmed not touched:** CPE-1879 (`backup.rs::copy_one_verified` unguarded) and
CPE-1881 (the 420-byte-per-entry revert refusal, and `download_tree`'s `eprintln`-only skip leaving the
user a silently lower count). Both real; neither widened into here.

**Verification.** `cargo clippy --all-targets -- -D warnings` and `--features index` both clean;
`cargo test --lib` 2372 passed / 0 failed; `checkpoint_roundtrip` 21, `archive_panic_safety` 2,
`sample_fixtures` 16 — all green.
