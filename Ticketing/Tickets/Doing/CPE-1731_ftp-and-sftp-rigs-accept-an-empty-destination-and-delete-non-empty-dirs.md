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

- [ ] A rename whose destination **resolves to the served root** is refused with the protocol's own
      error rather than answered success — the property, not a list of strings. `RNTO` with no argument
      and SFTP `rename` to `""` / `"."` / `"/"` are the *observed* instances and belong in the test as
      regression rows, but a row is never the fix; the resolved comparison is.
- [ ] **All three families webdav needed five rounds to close** are checked here, in whatever form each
      protocol expresses them — they are the cheapest available evidence that the comparison closes
      families rather than members:
      1. `.`-and-`/` spellings — `/./`, `/.//`, `//./`, `/./.` (what the round-3 denylist let through);
      2. `..` landing **on** the root — `/nonexistent/..`, `/sub/..`, `/./sub/../.` (what the round-4
         lexical comparison let through, having deliberately preserved `..`);
      3. spellings the **filesystem** calls equal but bytes do not — case differences and trailing dots
         on Windows (what round 4 let through by comparing `PathBuf`s directly).
- [ ] `RMD` / `rmdir` on a **non-empty** directory is refused, and the directory and its contents are
      still there afterwards — asserted on the filesystem, not on the status code.
- [ ] Each guard broken on its own turns a distinct test red, real output pasted (Evidence Rules,
      `Ticketing/wiki.md`).

## Notes

Filed from PR #902's UAT, 2026-08-14. Related: **CPE-1726** (the WebDAV half, and the immunity claim this
ticket corrects), **CPE-1730** (the same rigs' unconfined path resolvers — a different escape through the
same naive join, so the two will likely touch the same lines and are worth doing together).
