---
id: CPE-1731
title: The FTP and SFTP rigs accept an empty rename destination, and both remove non-empty directories on an empty-only verb
type: bug
priority: Medium
status: In Progress
tags: ready
estimate: S
created: 2026-08-14
closed:
---

## Problem

Found by the **UAT of PR #902 (CPE-1726)**, 2026-08-14, by taking CPE-1726's structural-immunity claim
and testing it instead of accepting it. The claim was wrong, and this ticket is the half that was split
out rather than fixed in that PR.

CPE-1726 fixed `cpe-webdav`'s rig, where an absent/empty `MOVE` `Destination` resolved to the served root
and was answered `201 Created`. It argued FTP and SFTP were **structurally immune**, because they take
source and destination from the same resolver in one message rather than from a second header. **That is
true of the source and false of the destination.** Both rigs' resolvers map empty *and* `/` to the served
root, so both express the identical shape:

```
[FTP]  RNFR /  -> 350 Ready for RNTO
       RNTO <no argument at all>  -> 250 Renamed
[SFTP] rename("/", "")  -> Ok(())
       rename("/", ".") -> Ok(())
```

An `RNTO` carrying no argument being answered `250 Renamed` is verbatim the WebDAV defect, in a crate
declared immune to it.

## A second defect, by the PR's own "model the wire" standard

Both rigs implement an **empty-directory-only** verb with a **recursive** primitive:

```
[FTP]  RMD /sub (non-empty) -> 250 Removed    sub/ gone, nested.txt gone
[SFTP] delete("/nonempty")  -> Ok(())         dir gone, inner.txt gone
```

RFC 959 `RMD` and `SSH_FXP_RMDIR` both remove an *empty* directory and error otherwise. WebDAV's `DELETE`
is correctly recursive — the mirror image of CPE-1726's thesis, which had WebDAV as the one that got its
verb semantics wrong. A test double that deletes a tree where a real daemon answers "directory not empty"
lets a client test pass against behaviour no real server has.

## Scope

`crates/ftp/src/lib.rs` (`RNTO`, `RMD`) and `crates/sftp/src/lib.rs` (`Handler::rename`,
`Handler::rmdir`), both inside the `#[cfg(test)]` rigs.

- Reject a rename destination that **resolves to the served root**, the way CPE-1726 ended up doing for
  `MOVE`. **Do not enumerate spellings.** CPE-1726 tried that three times: round 1 rejected nothing;
  round 2 rejected `""` before trimming, so `//` and `///` survived; round 3 trimmed first and rejected
  `""` and `"."` — and this ticket's own UAT then measured four more (`/./`, `/.//`, `//./`, `/./.`)
  returning `201 Created`, because `/./` trims to `./`, which is neither literal. It shipped that round
  under a doc calling its table *"exhaustive over the shapes that resolve to the served root."*

  A denylist closes the members someone thought of; asking whether the destination resolves to the root
  closes the family.

  **Reuse `same_place`, not `normalise_lexically` alone.** This bullet has now been wrong twice, and the
  second time is the more instructive:

  - It first said *"reject a destination that resolves to the served root (empty, `/`, `.`)"* — a literal
    denylist, which the UAT had just measured insufficient in webdav. Corrected 2026-08-14.
  - The correction then prescribed `normalise_lexically(&dest_real) == normalise_lexically(root)`,
    dropping `CurDir` and empty components — **and one commit later that exact shape was measured open on
    two more families**: `..` landing *on* the root (`/nonexistent/..`, `/sub/..`, `/./sub/../.` → `201
    Created`), and spellings the filesystem calls equal but bytes do not (Windows case-insensitivity,
    trailing dots). So the ticket corrected *for propagating a defective shape* went on to propagate the
    next one.

  The shape that survived round 5 is `same_place` in `crates/webdav/src/lib.rs`: `canonicalize` both
  sides when they resolve — the filesystem's own opinion, rather than a per-platform table of its rules —
  falling back to a lexical comparison that also **pops `..`** over a preceding ordinary component. Read
  its doc before reusing it; the `..` popping errs safe by proof, and the fallback triggers on *any*
  `canonicalize` error rather than only on non-existence.

  The lesson under both corrections: **do not carry a shape forward on the strength of it being the
  newest one.** Name the property, point at the implementation that currently satisfies it, and let the
  implementer check that it still does.
- Make `RMD`/`rmdir` non-recursive (`remove_dir`, not `remove_dir_all`), answering the protocol's
  not-empty error. Check whether any existing test deletes a non-empty directory through those verbs
  first — `cpe-sftp`'s `writes_mkdirs_lists_and_deletes_round_trip` is the one to look at.

Note `SftpProvider::delete` (the **shipped** half, `crates/sftp/src/lib.rs`) already classifies dir-vs-file
and issues `remove_file` or `rmdir` accordingly; it is a reasonable model, and it is also why CPE-1726's
"neither ever classifies" phrasing is true of the *rigs* only.

## Acceptance criteria

- [x] A rename whose destination **resolves to the served root** is refused with the protocol's own
      error rather than answered success — the property, not a list of strings. `RNTO` with no argument
      and SFTP `rename` to `""` / `"."` / `"/"` are the *observed* instances and belong in the test as
      regression rows, but a row is never the fix; the resolved comparison is.
- [x] **All three families webdav needed five rounds to close** are checked here, in whatever form each
      protocol expresses them — they are the cheapest available evidence that the comparison closes
      families rather than members:
      1. `.`-and-`/` spellings — `/./`, `/.//`, `//./`, `/./.` (what the round-3 denylist let through);
      2. `..` landing **on** the root — `/nonexistent/..`, `/sub/..`, `/./sub/../.` (what the round-4
         lexical comparison let through, having deliberately preserved `..`);
      3. spellings the **filesystem** calls equal but bytes do not — case differences and trailing dots
         on Windows (what round 4 let through by comparing `PathBuf`s directly).
- [x] `RMD` / `rmdir` on a **non-empty** directory is refused, and the directory and its contents are
      still there afterwards — asserted on the filesystem, not on the status code.
- [x] Each guard broken on its own turns a distinct test red, real output pasted (Evidence Rules,
      `Ticketing/wiki.md`).

## Notes

Filed from PR #902's UAT, 2026-08-14. Related: **CPE-1726** (the WebDAV half, and the immunity claim this
ticket corrects), **CPE-1730** (the same rigs' unconfined path resolvers — a different escape through the
same naive join, so the two will likely touch the same lines and are worth doing together).

## Work Log

**2026-08-14 — implemented.**

- `same_place` moved out of `cpe-webdav`'s `handle` into `cpe_server::fsutil`, with its whole argument
  (the `..`-popping safety proof, the per-platform table, the "any `Err`, not just ENOENT" fallback
  bound). All three rigs now call one implementation. This ticket exists because a property proved for
  one crate was assumed to hold for two others, so three copies that can drift was the wrong shape.
- **Re-measured before reusing, per the ticket's closing lesson.** A probe ran both rigs' own resolvers
  (`real_path` / `FsSftp::real`) over all three families on Windows and on Linux (a musl toolchain built
  under WSL). Three differences from webdav, none of which break the property: wire paths are
  `/`-separated even on Windows (mixed-separator resolved paths — `components()` and `canonicalize`
  both accept them); both resolvers map empty → root directly, which is how `RNTO` with no argument
  reaches the guard at all; and family 3 needs an absolute destination, which the leading-`/` trim makes
  *unreachable* on Linux (an absolute path becomes relative and lands inside the root, measured
  `same_place = false`) — so that test is Windows-only for the same reason webdav's is.
- **One correction to the inherited doc.** CPE-1726's platform table said "`..` popping removed → Linux:
  red (`..` rows)". Measured for these resolvers: only `/nonexistent/..` reaches the lexical fallback on
  Linux (`sub` exists, so `/sub/..` and `/./sub/../.` canonicalize fine), and that one row is then
  stopped by `rename`'s `ENOENT` rather than by the guard. A bare "it returned an error" assertion would
  therefore have stayed green through a neutralised pop on the one row the pop exists for — so the FTP
  test pins `553` (an ENOENT answers `550`) and the SFTP test pins `SSH_FX_FAILURE` and explicitly
  rejects the `NoSuchFile` wording. The corrected table is on `fsutil::same_place`.
- Both rigs' `RMD`/`rmdir` now use `remove_dir`. No existing test deleted a non-empty directory through
  those verbs (`writes_mkdirs_lists_and_deletes_round_trip` and its FTP twin both delete an *empty*
  `newdir`), so nothing needed adjusting.
- The **source** side of both renames is deliberately left unguarded (`RNFR /` + `RNTO /elsewhere` still
  moves the served root). Recorded at both sites with the reason: it needs CPE-1730's containment check,
  and `cpe-webdav` carries the identical asymmetry.
