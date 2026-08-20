---
id: CPE-1826
title: "Security: carry the Mark-of-the-Web (and other NTFS streams) through a handle, not by path"
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-20
closed:
---

## Problem

A copy through `do_copy_into` drops every NTFS alternate data stream, including
**`Zone.Identifier`** — the Mark-of-the-Web. Losing it means SmartScreen stops prompting and Office
stops using Protected View **on the copy** of a file that was downloaded from the internet. That is a
security control being removed without saying so, and `fs::copy` (`CopyFileExW`) did carry it.

CPE-1765 attempted the carry inline and **withdrew it**, because the attempt was defeated three
separate ways — each independently sufficient, all measured:

1. **It re-opened the destination by path.** `std::fs::write("<dst>:Zone.Identifier", ..)` violates
   `claim_file_slot`'s own rule ("write through the returned handle, never re-open the path"), and the
   window spans the **entire copy**. The Security Auditor won that race on its first attempt: the copy
   returned `Ok(272629760)` while the destination was a zero-length symlink, the user's bytes went to
   an orphaned handle, and the victim file's `ZoneId=3` was rewritten to `ZoneId=0` — i.e. the carry
   became a way to *strip* MotW from a file the attacker chose.
2. **A read-only source silently defeated it.** `set_permissions` ran before the carry, so the ADS
   write hit `ACCESS_DENIED` and was swallowed by `let _ =`. Measured: `copy result = Ok(7); dest MotW
   = None`. The test used a writable source, so it passed green.
3. It gave away the very never-re-open-by-path property CPE-1765 accepted a ~34% throughput cost to buy.

## Acceptance criteria

- [ ] The stream is written **through the destination handle already held** by
      `copy_file_into_claimed_slot`, never by re-opening the path. If `std` cannot open a named stream
      relative to an existing handle, say so explicitly and design around it — do not fall back to a
      path write.
- [ ] Ordering is fixed: streams are carried **before** `set_permissions`, so a read-only source's copy
      still receives them.
- [ ] The result is not swallowed. A failed carry is either propagated or surfaced, never `let _ =`.
- [ ] Tests cover, at minimum: a writable source, a **read-only** source, a source with no MotW (must
      not fabricate one), and a source carrying both `Zone.Identifier` and a second custom stream.
- [ ] Decide and record whether streams beyond `Zone.Identifier` are carried. Enumerating them needs
      `FindFirstStreamW`; if that is out of scope, the doc must say exactly which streams survive.
- [ ] The carry must never be able to ADD a zone marker to a file that had none, or strip one from a
      file other than the copy's own destination.
- [ ] Whatever ships, `src/docs/03-explorer.md` states accurately what a copy preserves and what it drops.

## Notes

Split out of CPE-1765 after the third review round. The core claim-then-write mechanism there is sound —
the Security Auditor could not break it across 13+ distinct attacks — and every defect found in that
ticket lived in the metadata-carrying code layered around it. Bundling this back in would repeat that.

Check whether the **modified-timestamp carry** shipped by CPE-1765 has the same by-path shape; if it
does, it belongs here too.
