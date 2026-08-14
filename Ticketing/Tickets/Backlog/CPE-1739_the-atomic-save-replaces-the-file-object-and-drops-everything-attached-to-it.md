---
id: CPE-1739
title: The atomic save replaces the file object, dropping mode, attributes, ADS and open-handle identity
type: bug
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-14
closed:
---

## Problem

Measured by the PR #904 (CPE-1725) independent UAT, 2026-08-14. Side-by-side, same file, same run, plain
`fs::write` versus `cpe_server::fsutil::replace_file_contents` (temp sibling + rename):

```text
0600 private:    fs::write -> 0o600 | replace_file_contents -> 0o644
0755 executable: fs::write -> 0o755 | replace_file_contents -> 0o644
fs::write:             attrs=0x22 (HIDDEN) ADS=Ok("ZoneId=3\r\n")
replace_file_contents: attrs=0x20 (HIDDEN lost) ADS=Err(NotFound)
SHARE_READ|WRITE open:  fs::write -> Ok(()) | replace_file_contents -> Err("Access is denied. (os error 5)")
```

One cause, four consequences: **a rename replaces the file object**, so everything attached to the object
rather than to the bytes is left behind on the file that gets unlinked.

1. **A private file becomes world-readable.** `0600` → `0644`. A security downgrade, silent, reported as a
   successful save.
2. **An executable loses its executable bit.** `0755` → `0644`. Editing a shell script breaks it.
3. **Windows attributes and alternate data streams are destroyed**, including `Zone.Identifier` — the Mark
   of the Web. Editing a downloaded file silently clears a trust signal other software acts on.
4. **A save that used to work now fails.** With another handle open `SHARE_READ|WRITE` — what an ordinary
   Windows application holds — the rename returns `Access is denied`.

Item 4 is the one that rules out the easy fix: copying mode and attributes onto the staged file closes 1–3
but does nothing for 4, because nothing about the staged file changes the *target's* sharing mode.

## Who is affected today

**`metadata_write` (Metadata Studio) only.** This has been live since CPE-1716.

CPE-1725 briefly extended it to `write_file_text` (the preview pane's text editor and three Save-As
exports) and then **narrowed back** on this UAT's finding: that command now shares only the dangling-link
*classifier* with `metadata_write` and keeps writing with `fs::write`, so it has none of the four. See
`write_file_text_impl`'s doc comment for the reasoning and
`cpe_1725_an_ordinary_save_keeps_the_same_file_object_and_its_mode` for the guard that keeps it that way.

The behaviour is **no longer silent**: `replace_file_contents`' doc states all four, and
`src/docs/25-metadata-studio.md` now spells them out for the user, including the security one. That makes
today's state a documented decision rather than a defect — but it is still the wrong default for item 1.

## Options

1. **Copy mode + attributes onto the staged file before the rename.** Straightforward on Unix
   (`set_permissions` from the original's metadata). On Windows, attributes need `SetFileAttributesW` and
   streams need `FindFirstStreamW`/`FindNextStreamW` enumeration and per-stream copying. Closes 1–3, not 4.
2. **`ReplaceFileW` on Windows.** This is the OS primitive built for exactly this job: it preserves the
   destination's attributes, ACLs, streams and creation time, and it exists precisely because the
   rename-based idiom loses them. Unix keeps option 1's `set_permissions` + rename. Best coverage, and the
   only route that plausibly improves item 4 as well.
3. **Write in place, keep a backup copy** — inverts the trade: nothing about the object changes, and
   crash-safety comes from the backup rather than from the rename.
4. **Do nothing, keep the documentation.** Defensible for 2–4; not for item 1, which no doc line makes
   acceptable as a default.

## Acceptance criteria

- [ ] Item 1 (a private file becoming world-readable) is fixed, whichever option is taken. This one is not
      a documentation question.
- [ ] Each of items 2, 3 and 4 is either fixed or its remaining exposure is recorded at
      `replace_file_contents` **and** in `src/docs/25-metadata-studio.md`, matching what actually ships.
- [ ] Tests assert on the **filesystem**, never on the returned `Result`: mode before/after on Unix; on
      Windows the attribute word and a real ADS written with `path:stream` syntax; and an open-handle case
      for item 4. The property behind all four — *is it still the same file object?* — is already pinned
      cross-platform by `cpe_1725_an_ordinary_save_keeps_the_same_file_object_and_its_mode`; reuse the
      open-handle technique rather than `MetadataExt::file_index`, which is still unstable
      (`windows_by_handle`, rust#63010).
- [ ] Whatever lands, `write_file_text` stays on `fs::write` unless *all four* are closed — the narrowing
      is what keeps the ordinary text save clean, and re-routing it is a regression until then.

## Notes

Filed by CPE-1725 from PR #904's UAT round, 2026-08-14. Related: **CPE-1716** (introduced
`replace_file_contents`), **CPE-1725** (shared the classifier, not the write strategy), **CPE-1738** (the
`.cpe-tmp` a killed save strands — same function, different gap).
