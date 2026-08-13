---
id: CPE-1709
title: Downloading an S3 key containing ":" silently writes a 0-byte file on Windows
type: bug
priority: High
status: Done
tags: ready
estimate: M
created: 2026-08-13
closed: 2026-08-13
---

## Problem

Found by the PR #890 reviewer (CPE-1704 round 4), 2026-08-13, while checking the blast radius of relaxing
the S3 name guard. **Measured through the real sink** â€” `guarded_join` + `std::fs::write`, which is exactly
what `download_tree` does â€” not reasoned about:

```
|SINK|"colon:name.txt"| joined â€¦\downloadroot\colon:name.txt |WROTE|
|SINK|"file:$DATA"    | joined â€¦\downloadroot\file:$DATA     |WROTE|
|SINK|inside|"colon"|0 bytes|
|SINK|inside|"file" |0 bytes|
```

The write **succeeds**. The content goes into an NTFS **alternate data stream**. What the user sees in the
folder afterwards is a **0-byte file named `colon`** â€” the part before the colon. The bytes are on disk but
unreachable through any ordinary means, and nothing anywhere reported a problem.

`guarded_join` has no `:` rule. It didn't need one while every remote listing was pre-filtered by
`cpe_server::transfer::is_safe_name`, which refuses `:`. CPE-1704 correctly relaxed that for S3 â€” `:` is an
ordinary, legal S3 key byte â€” and in doing so removed the accidental protection `guarded_join` was leaning
on without saying so.

## This is the CPE-1704 philosophy biting one layer further along

CPE-1704 existed because a legitimate key silently vanished from a listing. This is the same class, moved
downstream: the key now shows up correctly in the listing, and then **silently loses its contents on the way
to disk**. A file the user can see but cannot read is arguably worse than one they never saw, because
nothing prompts them to go looking.

## It is NOT a security bug â€” that was checked

The reviewer chased the worse version and it does not exist:

- `..::$DATA` and `..:stream` both **fail at `CreateFileW` with os error 123**.
- Nothing landed on the parent of the download root.
- **SEC PASS stands.** No traversal escape by this route.

What remains is a contained **data-integrity** bug.

## Scope

`guarded_join` and `download_tree` â€” CPE-1461's code, deliberately **out of scope** for CPE-1704, which is
why this is a separate ticket rather than another round on PR #890.

Note this is a **local-sink** problem, not an S3 problem. Any provider that can legally produce a `:` in a
name reaches the same sink. Fix it at the sink, where the platform rule actually applies, rather than by
re-tightening a provider guard that is now correct.

## Gate

**This must land before CPE-1685**, exactly as CPE-1704 does. CPE-1685 is what wires `s3` through
`cpe_vfs::open` and makes both bugs reachable by a real user. A note has been added there.

Nothing can hit this today.

## Acceptance criteria

- [x] Downloading a key containing `:` either writes the **full contents** to a safely-transformed local
      name, or **fails loudly**. A 0-byte file with the bytes hidden in an ADS is not an acceptable
      outcome of either choice.
- [x] If the answer is name transformation, the mapping is **visible and reversible enough for the user to
      understand what they got** â€” record the choice and the reasoning. Do not invent a scheme that silently
      collides two distinct keys onto one local name; if two keys can map to the same file, that is a second
      silent-loss bug, so detect it and say so.
- [x] Decide and record what happens for the other Windows-illegal name characters (`< > : " | ? *`,
      trailing dot/space, and the reserved device names `CON`, `PRN`, `AUX`, `NUL`, `COM1`â€“`COM9`,
      `LPT1`â€“`LPT9`). `:` is the one that was measured, but it is not plausibly the only one â€” enumerate
      rather than fix the single reported case.
- [x] A test proves the bytes arrive, **through the real sink**, and that breaking the fix turns a
      **distinct** test red. Per Evidence Rule 2, assert on the file the user would open â€” not on the
      return value of the write.
- [x] The traversal property is unchanged: `..::$DATA` / `..:stream` still fail, nothing escapes the
      download root. Re-run PR #890's measurements.
- [x] Platform-gate correctly. This is a **Windows/NTFS** failure; on Unix `:` is an ordinary filename byte
      and the same test would prove nothing. Provide an ungated sibling so the Linux and macOS CI legs
      still assert something real â€” CI runs a 3-OS matrix and a Windows-only test is invisible on two of
      the three.

## Work Log

**2026-08-13 â€” measured first, through the real sink.** Before writing any fix, ran a throwaway probe
(`crates/server/tests/cpe1709_probe.rs`, deleted afterwards) that drove `download_tree` â€” the actual
production path, root canonicalized to a `\\?\` verbatim path and all â€” over the whole enumerated set,
then reported what the *directory* contained afterwards rather than what the write returned. Cross-checked
every result with ordinary Win32 tooling (`cmd /c dir /r`, `cmd /c type`), because Rust's `fs` is not what
the user's applications use. Windows 11 Pro 26200, NTFS.

They do **not** group. Four distinct behaviours:

| Remote leaf | `download_tree` said | Directory afterwards | Opened by ordinary Win32 name |
|---|---|---|---|
| `normal.txt` | `Ok(1)` | `normal.txt` 5B | 5 bytes |
| `colon:name.txt`, `file:$DATA` | `Ok(1)` | **`colon` 0B** | empty â€” `dir /r` shows `colon:name.txt:$DATA` 5B |
| `< > " \| ? *`, ctrl `U+0001` | `Ok(0)` skipped | *(empty)* | n/a |
| `trailingdot.`, `trailingspace ` | `Ok(1)` | `trailingdot.` 5B | **"The system cannot find the file specified"** |
| `NUL` | `Ok(1)` | `NUL` 5B | **empty** (still the null device) |
| `CON` `PRN` `AUX` `CON.txt` `nul.txt` `COM1`â€“`COM9` `LPT1`â€“`LPT9` | `Ok(1)` | 5B | 5 bytes *(on this build)* |

Three separate silent-loss mechanisms, not one: the ADS diversion, the trailing-run strip (file exists,
no Win32 path can reopen it), and `NUL` resolving to the null device. The `< > " | ? *` group was already
being skipped â€” but only *accidentally*, and misdiagnosed: the call that failed was the CPE-1696 leaf
`symlink_metadata` probe, so the transfer announced "could not be inspected for a pre-existing symlink"
for a name that had nothing to do with symlinks.

**Decision: transform, not refuse.** Refusing would have been simpler and is explicitly allowed by the
AC, but `:` is a first-class S3 key byte â€” a bucket of ISO-8601-named objects is completely ordinary â€”
so refusing would replace *silent partial* loss with *loud total* loss for a large class of legitimate
buckets. Percent-encoding is self-evidently mechanical, ASCII-only, and reversible by eye.

**Collisions are impossible by construction, not by luck.** The encoder first rewrites any pre-existing
`%HH` to `%25HH`, so `decode(encode(x)) == x` for every `x`, which forces injectivity. Without that pass
the remote keys `a:b` and `a%3Ab` both land on `a%3Ab` â€” proved by deleting the pass and watching
`cpe_1709_windows_name_mapping_is_injective` print exactly that collision. One residual collision is
recorded rather than fixed because no leaf rewriting can address it: NTFS is case-insensitive, so
`A.txt` and `a.txt` are one file (measured â€” writing both left a single `A.txt` containing the second
write). Pre-existing, independent of this mapping, and noted in the user docs.

**Fixed at the sink**, in `guarded_join`/`local_safe_segment`, never in a provider guard: the rule belongs
to the local filesystem, and every provider that can legally produce such a name arrives at the same sink.
CPE-1704's S3 relaxation is untouched.

Five guards broken **individually**, each shown to red a distinct test and each restored with
`git checkout --` and confirmed by a real `Compiling cpe-server`; full output in the PR body. Traversal
re-measured: nothing escapes the download root, and the property is now asserted directly rather than
resting on `CreateFileW` happening to refuse a name.

User-facing docs updated: `src/docs/31-network.md` gains "Downloaded names Windows can't hold", with the
mapping table and the case-insensitivity caveat. No new app `Section`, so `sectionDocs.ts` is unchanged.

**2026-08-13 â€” round 2 (PR #894 review).** The injectivity construction and containment survived a
66,429-name brute force with 0 collisions and 0 round-trip breaks, and the reviewer's own case-fold
sweep confirmed the mapping introduces no case collision â€” so the "case-insensitivity is pre-existing
and independent" claim is now measured rather than asserted. Two blockers, both of which reintroduced
this ticket's own bug class at the edges of the fix:

- **F1 â€” an over-long encoded name was silently lost and reported as success.** Encoding grows a name by
  up to 3Ã—, the component ceiling is 255 (measured: 255 writes, 256 gives os error 123), and the failure
  landed on the CPE-1696 leaf `symlink_metadata` probe â€” which `return`ed silently. So a batch of three
  keys returned `Ok(2)` and produced two files, under a stderr line blaming a *symlink* probe for a
  *length* problem. That is verbatim the message the round-1 PR body claimed was gone; it had not gone,
  it had moved. Fixed by splitting **security refusals** (traversal names, pre-existing symlinks â€” not
  writing them is correct, still `Ok`) from **delivery failures** (we meant to write it and the
  filesystem refused â€” now `Err`, naming how many were lost and why). Everything deliverable is still
  delivered first. The message now states the real cause and the measured limit; `MAX_LOCAL_COMPONENT`
  only *explains* a failure the OS already reported, never pre-rejects, because its unit differs per
  platform and guessing would risk refusing a name that would have worked.
- **F2 â€” pass 1 escaped every `%HH`, including sequences the encoder can never emit.** `%2f` is the
  clearest: `guarded_join` splits on `/` and `\` *before* the transform, so `%2F` is unproducible â€” yet
  `report%2ffinal.txt` became `report%252ffinal.txt` and `city=A%2FB` (a normal Hive/Athena partition
  value) became `city=A%252FB`. Combined with F1 this destroyed data on keys that previously worked: a
  provably-writable 254-char name grew to 256 and stopped landing. Narrowed to the emittable set only.
  Injectivity is untouched â€” the proof only ever needed "no `%HH` in the output that this encoder did
  not emit", and encoder and decoder now read that phrase the same, narrower way.

Also: **F5** completed the device list (the superscript `COMÂ¹`/`LPTÂ²` forms, `CONIN$`/`CONOUT$`, and
`COM0`/`LPT0`) â€” the same "device resolution moves between Windows releases" argument that justified
escaping all 22 covers these identically, so omitting them was inconsistent. **F4** (encoding can push a
*path* past MAX_PATH even when the component fits) is disclosed as a surfaced notice and in the docs,
deliberately not a failure: the file really is delivered and really is readable by long-path-aware
software, so calling it a failure would be its own wrong answer. **F6** records why
`decode_windows_safe_segment` is *not* wired into `upload_tree` â€” encoding is compelled by the
filesystem, decoding would be a guess about provenance that silently renames a local file the user may
simply have typed. **F3** rewrote the docs section, which had claimed outright that any `%`-bearing name
is untouched.

Three round-2 guards broken individually, each red with real output, each restored and recompiled.

Not fixed here, filed separately by the Foreman: `is_control()` is Cc-only, so `U+202E` passes through.

## Notes

Filed by the Foreman from the PR #890 review, 2026-08-13, on the reviewer's own recommendation to gate it
alongside CPE-1704.

Related: **CPE-1704** (which correctly relaxed the guard this was leaning on), **CPE-1685** (which makes it
reachable â€” gated on this), **CPE-1461** (`guarded_join` / the download sink), **CPE-1708** (the other
CPE-1704 follow-up, also gating CPE-1685).

## Work Log

**Closed 2026-08-13, merged as PR #894 (`2d4d9b28`).** Two rounds.

### Enumerating instead of patching found three loss modes, not one

The ticket said `:` was what got measured and was not plausibly the only case. It wasn't â€” and the four
classes behave **completely differently**, verified through the real sink with `cmd /c dir /r` and
`cmd /c type` rather than Rust's `fs` (the gap the original bug lived in):

| Class | What actually happened |
|---|---|
| `:` and `file:$DATA` | file created, **0 bytes**, content in an NTFS alternate data stream |
| `< > " \| ? *`, control chars | already skipped â€” but **by accident**, and misdiagnosed |
| trailing dot / trailing space | file created **full size**, and no non-verbatim path can ever reopen it |
| `NUL` | the only device name that lost data on this build |

The second is its own small scandal: the call that failed was CPE-1696's leaf `symlink_metadata` probe, so
the app announced *"could not be inspected for a pre-existing symlink"* about names with nothing to do with
symlinks. Right outcome, wrong explanation.

### Transform, not refuse â€” and why

`:` is a first-class S3 key byte; a bucket of ISO-8601-named objects is ordinary. Refusing would have traded
*silent partial* loss for *loud total* loss across a large legitimate class. Fixed at the **sink**, not at a
provider guard: the rule belongs to the local filesystem, every provider that can legally emit `:` arrives
here, and CPE-1704's S3 relaxation stays correct.

### Round 1 reintroduced the bug it exists to remove

Two blockers, both from the UAT:

**Over-long encoded names were silently lost and reported as success.** Encoding *lengthens* names, and the
component limit is 255. The PR's own motivating ISO-8601 example: a 252-char key encoded to 256 â†’
`download_tree -> Ok(0)`, empty directory. In a batch, three keys in gave `Ok(2)` and two files out. And the
only signal was **verbatim the misdiagnosed symlink message the PR claimed to have removed** â€” it had moved,
not gone.

**Pass 1 escaped every `%HH`, including sequences the encoder can never emit.** `/` is split off by
`guarded_join` *before* the transform, so `%2f` is unreachable output â€” yet `report%2ffinal.txt` â†’
`report%252ffinal.txt` and `city=A%2FB` â†’ `city=A%252FB`, both realistic S3 keys on the must-not-touch list.
Combined with the length bug, a raw 254-char name **provably writable at that root** stopped downloading.

### The round-2 fix, and the distinction that made it right

**Security refusal â‰  delivery failure.** Round 1 collapsed them. Now a traversal name or pre-existing
symlink still ends `Ok` â€” declining to write it is correct and nothing was lost â€” while a name we *meant*
to write and the filesystem refused ends `Err`, naming what did not arrive. Deliverable files are delivered
**first**, so one bad name never aborts a large download.

**`MAX_LOCAL_COMPONENT` only explains a failure the OS already reported; it never pre-rejects.** The worker's
reasoning was that the limit's *unit* differs per platform (UTF-16 units on Windows, bytes on ext4/APFS), so
guessing would risk refusing a name that would have worked. The UAT proved this with **astral-plane names**,
where char count and UTF-16 count diverge 1:2:

```
127 emoji = 127 chars / 254 UTF-16 units -> Ok(1), delivered
128 emoji = 128 chars / 256 UTF-16 units -> Err
```

A `chars > 255` pre-check would have **wrongly refused** the 127-emoji name the filesystem accepts, and
**wrongly accepted** the 128-emoji name it refuses. Wrong in both directions. Letting the OS decide and
explaining afterwards is the only correct answer â€” and that is now measured, not argued.

### Injectivity survived the narrowing â€” the property everything rests on

Pass 1 narrowed to only the codes the encoder can emit. Both checks independently re-ran their brute forces:

| Check | Corpus | Result |
|---|---|---|
| UAT | 629,161 names across 5 corpora, incl. feeding each name's own encoding back in | 0 collisions, 0 round-trip breaks, 0 case-fold collisions |
| Reviewer | 271,452-name corpus purpose-built around the narrowing, plus the round-1 37,449 | 0/0; caseless **newly introduced = 0** (pre-existing 160,342) |
| Reviewer | `guarded_join` containment, **1,235,632 inputs**, `Component::Normal` on every tail | 11 refusals, **0 escapes** |

The reviewer also explained *why* narrowing is safe rather than merely measuring it: the proof only ever
needed "the output contains no `%HH` that did not come from an escape this function emitted", and
`is_encoder_emitted_code` is the same predicate on both sides. They had identified three ways to get this
wrong; the shipped code avoids all three (case-insensitive hex match, `%` itself in the set, device
first-chars compared with `eq_ignore_ascii_case`).

### Also settled

- **Device list** completed: `COMÂ¹ COMÂ² COMÂ³ LPTÂ¹ LPTÂ² LPTÂ³`, `CONIN$`, `CONOUT$`, `COM0`, `LPT0`. Both
  checks hunted the table in **both** directions and found no over-rejection (`CONTRACT.txt`, `console.log`,
  `com1x`, `LPT10` all untouched).
- **Containment changed shape and was re-proved.** `..::$DATA` and `..:stream` used to die at `CreateFileW`;
  they are now inert contained files. The property moved from "the Win32 parser happens to refuse this" to
  an asserted invariant â€” a better place for it to live.
- **MAX_PATH** surfaced as a write-time notice, not a failure: the file really is delivered and readable by
  long-path-aware software, so erroring would be its own wrong answer.
- **NTFS case-insensitivity** confirmed as a genuinely unfixable residual â€” the encoder introduces **zero**
  new caseless collisions, so the PR's honesty on it is conservative rather than convenient.
- **The decoder is deliberately not wired.** Encoding is *compelled* by the filesystem; decoding on upload
  would be a *guess about provenance*, and `report%3Afinal.txt` is a name a user may simply have typed.
  Disclosed in the docs; see the follow-up ticket.

Verdicts: Reviewer **APPROVE + SEC PASS**, UAT **PASS**. 12/12 CI green.

