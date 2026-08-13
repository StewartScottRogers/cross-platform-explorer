---
id: CPE-1709
title: Downloading an S3 key containing ":" silently writes a 0-byte file on Windows
type: bug
priority: High
status: In Progress
tags: ready
estimate: M
created: 2026-08-13
closed:
---

## Problem

Found by the PR #890 reviewer (CPE-1704 round 4), 2026-08-13, while checking the blast radius of relaxing
the S3 name guard. **Measured through the real sink** — `guarded_join` + `std::fs::write`, which is exactly
what `download_tree` does — not reasoned about:

```
|SINK|"colon:name.txt"| joined …\downloadroot\colon:name.txt |WROTE|
|SINK|"file:$DATA"    | joined …\downloadroot\file:$DATA     |WROTE|
|SINK|inside|"colon"|0 bytes|
|SINK|inside|"file" |0 bytes|
```

The write **succeeds**. The content goes into an NTFS **alternate data stream**. What the user sees in the
folder afterwards is a **0-byte file named `colon`** — the part before the colon. The bytes are on disk but
unreachable through any ordinary means, and nothing anywhere reported a problem.

`guarded_join` has no `:` rule. It didn't need one while every remote listing was pre-filtered by
`cpe_server::transfer::is_safe_name`, which refuses `:`. CPE-1704 correctly relaxed that for S3 — `:` is an
ordinary, legal S3 key byte — and in doing so removed the accidental protection `guarded_join` was leaning
on without saying so.

## This is the CPE-1704 philosophy biting one layer further along

CPE-1704 existed because a legitimate key silently vanished from a listing. This is the same class, moved
downstream: the key now shows up correctly in the listing, and then **silently loses its contents on the way
to disk**. A file the user can see but cannot read is arguably worse than one they never saw, because
nothing prompts them to go looking.

## It is NOT a security bug — that was checked

The reviewer chased the worse version and it does not exist:

- `..::$DATA` and `..:stream` both **fail at `CreateFileW` with os error 123**.
- Nothing landed on the parent of the download root.
- **SEC PASS stands.** No traversal escape by this route.

What remains is a contained **data-integrity** bug.

## Scope

`guarded_join` and `download_tree` — CPE-1461's code, deliberately **out of scope** for CPE-1704, which is
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
      understand what they got** — record the choice and the reasoning. Do not invent a scheme that silently
      collides two distinct keys onto one local name; if two keys can map to the same file, that is a second
      silent-loss bug, so detect it and say so.
- [x] Decide and record what happens for the other Windows-illegal name characters (`< > : " | ? *`,
      trailing dot/space, and the reserved device names `CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`,
      `LPT1`–`LPT9`). `:` is the one that was measured, but it is not plausibly the only one — enumerate
      rather than fix the single reported case.
- [x] A test proves the bytes arrive, **through the real sink**, and that breaking the fix turns a
      **distinct** test red. Per Evidence Rule 2, assert on the file the user would open — not on the
      return value of the write.
- [x] The traversal property is unchanged: `..::$DATA` / `..:stream` still fail, nothing escapes the
      download root. Re-run PR #890's measurements.
- [x] Platform-gate correctly. This is a **Windows/NTFS** failure; on Unix `:` is an ordinary filename byte
      and the same test would prove nothing. Provide an ungated sibling so the Linux and macOS CI legs
      still assert something real — CI runs a 3-OS matrix and a Windows-only test is invisible on two of
      the three.

## Work Log

**2026-08-13 — measured first, through the real sink.** Before writing any fix, ran a throwaway probe
(`crates/server/tests/cpe1709_probe.rs`, deleted afterwards) that drove `download_tree` — the actual
production path, root canonicalized to a `\\?\` verbatim path and all — over the whole enumerated set,
then reported what the *directory* contained afterwards rather than what the write returned. Cross-checked
every result with ordinary Win32 tooling (`cmd /c dir /r`, `cmd /c type`), because Rust's `fs` is not what
the user's applications use. Windows 11 Pro 26200, NTFS.

They do **not** group. Four distinct behaviours:

| Remote leaf | `download_tree` said | Directory afterwards | Opened by ordinary Win32 name |
|---|---|---|---|
| `normal.txt` | `Ok(1)` | `normal.txt` 5B | 5 bytes |
| `colon:name.txt`, `file:$DATA` | `Ok(1)` | **`colon` 0B** | empty — `dir /r` shows `colon:name.txt:$DATA` 5B |
| `< > " \| ? *`, ctrl `U+0001` | `Ok(0)` skipped | *(empty)* | n/a |
| `trailingdot.`, `trailingspace ` | `Ok(1)` | `trailingdot.` 5B | **"The system cannot find the file specified"** |
| `NUL` | `Ok(1)` | `NUL` 5B | **empty** (still the null device) |
| `CON` `PRN` `AUX` `CON.txt` `nul.txt` `COM1`–`COM9` `LPT1`–`LPT9` | `Ok(1)` | 5B | 5 bytes *(on this build)* |

Three separate silent-loss mechanisms, not one: the ADS diversion, the trailing-run strip (file exists,
no Win32 path can reopen it), and `NUL` resolving to the null device. The `< > " | ? *` group was already
being skipped — but only *accidentally*, and misdiagnosed: the call that failed was the CPE-1696 leaf
`symlink_metadata` probe, so the transfer announced "could not be inspected for a pre-existing symlink"
for a name that had nothing to do with symlinks.

**Decision: transform, not refuse.** Refusing would have been simpler and is explicitly allowed by the
AC, but `:` is a first-class S3 key byte — a bucket of ISO-8601-named objects is completely ordinary —
so refusing would replace *silent partial* loss with *loud total* loss for a large class of legitimate
buckets. Percent-encoding is self-evidently mechanical, ASCII-only, and reversible by eye.

**Collisions are impossible by construction, not by luck.** The encoder first rewrites any pre-existing
`%HH` to `%25HH`, so `decode(encode(x)) == x` for every `x`, which forces injectivity. Without that pass
the remote keys `a:b` and `a%3Ab` both land on `a%3Ab` — proved by deleting the pass and watching
`cpe_1709_windows_name_mapping_is_injective` print exactly that collision. One residual collision is
recorded rather than fixed because no leaf rewriting can address it: NTFS is case-insensitive, so
`A.txt` and `a.txt` are one file (measured — writing both left a single `A.txt` containing the second
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

## Notes

Filed by the Foreman from the PR #890 review, 2026-08-13, on the reviewer's own recommendation to gate it
alongside CPE-1704.

Related: **CPE-1704** (which correctly relaxed the guard this was leaning on), **CPE-1685** (which makes it
reachable — gated on this), **CPE-1461** (`guarded_join` / the download sink), **CPE-1708** (the other
CPE-1704 follow-up, also gating CPE-1685).
