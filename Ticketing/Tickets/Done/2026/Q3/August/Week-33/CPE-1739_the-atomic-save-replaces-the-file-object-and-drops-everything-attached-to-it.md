---
id: CPE-1739
title: The atomic save replaces the file object, dropping mode, attributes, ADS and open-handle identity
type: bug
priority: High
status: Done
tags: ready
estimate: M
created: 2026-08-14
closed: 2026-08-15
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

- [x] Item 1 (a private file becoming world-readable) is fixed, whichever option is taken. This one is not
      a documentation question.
- [x] Each of items 2, 3 and 4 is either fixed or its remaining exposure is recorded at
      `replace_file_contents` **and** in `src/docs/25-metadata-studio.md`, matching what actually ships.
- [x] Tests assert on the **filesystem**, never on the returned `Result`: mode before/after on Unix; on
      Windows the attribute word and a real ADS written with `path:stream` syntax; and an open-handle case
      for item 4. The property behind all four — *is it still the same file object?* — is already pinned
      cross-platform by `cpe_1725_an_ordinary_save_keeps_the_same_file_object_and_its_mode`; reuse the
      open-handle technique rather than `MetadataExt::file_index`, which is still unstable
      (`windows_by_handle`, rust#63010).
- [x] Whatever lands, `write_file_text` stays on `fs::write` unless *all four* are closed — the narrowing
      is what keeps the ordinary text save clean, and re-routing it is a regression until then.

## Notes

Filed by CPE-1725 from PR #904's UAT round, 2026-08-14. Related: **CPE-1716** (introduced
`replace_file_contents`), **CPE-1725** (shared the classifier, not the write strategy), **CPE-1738** (the
`.cpe-tmp` a killed save strands — same function, different gap).

## Work Log

**2026-08-15 — the design call: option 2 (`ReplaceFileW` on Windows, mode + xattrs on Unix), measured
first.**

The ticket's four options were weighed against `PURPOSE.md`'s fast/small/predictable tiebreaker. Option 4
("do nothing, keep the docs") was rejected outright by the ticket's own AC — no doc line makes a silent
`0600 → 0644` acceptable. Option 3 (write in place + backup) was rejected because it trades away the
atomicity a half-rewritten media file is the entire reason for, and replaces one stray-file problem with
another CPE-1738 has just finished cleaning up. That left options 1 and 2, and **option 2 turned out to
cost less than option 1**, which decided it: `windows` 0.56 with `Win32_Storage_FileSystem` is *already* a
dependency of `crates/server` (CPE-1546/CPE-1642), so `ReplaceFileW` is **zero new dependencies** — while
option 1's Windows half would have meant hand-rolling `SetFileAttributesW` plus
`FindFirstStreamW`/`FindNextStreamW` stream enumeration and still never reaching the ACL.

**The split is deliberately two mechanisms, not one abstraction**, because the two platforms lose
different things at different moments. Unix's loss (the mode) belongs to the *replacement* and is repaired
before the swap, on the staged file, while it is still empty — so a `0600` file's bytes are never even
briefly sitting at a world-readable staging name. Windows' loss (attributes, DACL, named streams) belongs
to the *destination* and can only be repaired at the swap.

Measured on this machine before writing any of it (throwaway probe, Windows 11, NTFS):

```text
before             attrs=0x802 (HIDDEN)      ADS=Ok("[ZoneTransfer]\r\nZoneId=3\r\n")
after fs::rename   attrs=0x820 (HIDDEN lost) ADS=Err(NotFound)
after ReplaceFileW attrs=0x822 (HIDDEN kept) ADS=Ok("[ZoneTransfer]\r\nZoneId=3\r\n")

foreign SHARE_READ|WRITE handle:  fs::rename -> Err(os 5)  ReplaceFileW -> Err(sharing violation)
std::fs::File::open handle held:                           ReplaceFileW -> Ok(())
read-only target:                 fs::rename -> Err(os 5)  ReplaceFileW -> Err(os 5)   (unchanged)
absent target:                                             ReplaceFileW -> Err(NotFound)
target mtime after ReplaceFileW:  fresh (2.5ms) — CPE-1738's sweep reference clock still works
cost, 200x64KiB, local NVMe:      fs::rename 0.22ms/commit | ReplaceFileW 6.2ms/commit | stat 11us
```

**Answers to the questions the ticket asked:**

- *Which attributes are worth carrying, and which are out of reach with `std` alone?* Carried: the Unix
  mode (items 1 and 2), Unix extended attributes, and — via `ReplaceFileW` — the Windows attribute word,
  DACL and named streams (item 3). Out of reach: **ownership**, because `chown` is not in `std` and an
  unprivileged process cannot give a file away regardless. Extended attributes were not in the ticket's
  list (the UAT ran on Windows) but are its exact Unix analogue, and the stakes are higher than they look:
  **this app stores its own metadata there** — macOS Finder tags (CPE-826/829, which is why the `xattr`
  dependency exists) and `com.apple.quarantine`, macOS's own Mark of the Web. A Metadata Studio save that
  dropped them would be destroying the feature next door.
- *Does carrying them cost a syscall on every save?* Yes: one extra `fs::metadata` (~11 µs), plus on Unix
  one `fchmod` and one `listxattr`, and on Windows `ReplaceFileW` in place of `MoveFileEx` — **~6 ms
  instead of ~0.22 ms per commit**, measured. Accepted, for a bounded reason: the only caller is
  `metadata_write`, a user-initiated single save that has already spent far longer parsing and rewriting a
  media file. The path that would have made this a general tax — `write_file_text`, the ordinary text save
  — does not use this function at all and pays nothing.
- *What if the source attributes cannot be READ?* **Refuse the save.** This is what makes item 1 a fix
  rather than a best-effort improvement: if we cannot tell whether the file is `0600`, staging anyway
  hands back whatever the umask gives, so the one case where it matters most is precisely the one a
  "carry it if you can" policy would silently downgrade. Same posture as `classify_write_target` —
  *not provably safe ⇒ do not write*. `NotFound` is the deliberate exception (a brand-new file at a free
  name has nothing to carry, and Save-As depends on it).
- *What if they cannot be RE-APPLIED?* Mode: **fail the save** — continuing past a failed `fchmod` ships
  the exact downgrade this closes. Extended attributes: **best effort, per attribute, never fails the
  save** — `listxattr` also returns kernel-managed `security.*`/`system.*` entries that an unprivileged
  owner frequently cannot set even when nothing is wrong, and refusing there would make the app unable to
  write files on an ordinary hardened Linux box to protect a value the kernel re-derives itself. Recorded
  as the one silent residual.
- *No silent fallback.* If `ReplaceFileW` fails the save fails; falling back to `fs::rename` would restore
  the exact loss this fixes and do it invisibly, only on the filesystems where nobody is watching.

**Result against the acceptance criteria:** item 1 fixed (mode carried on Unix, DACL carried on Windows);
item 2 fixed; item 3 fixed on Windows (attributes + ADS) and closed on Unix too via xattrs; **item 4 not
fixed** — measured as still failing, with its exposure recorded at `commit_replacement`, at
`replace_file_contents`, in `write_file_text_impl`, and in `src/docs/25-metadata-studio.md`.
`write_file_text` stays on `fs::write`, since the criterion for re-routing it was all four and it is
three.

**Tests, and the mutation that reds each (every assertion is placed before the `Result` is unwrapped —
all four defects fail by returning `Ok`):**

| Guard removed | Test that reds | Observed |
|---|---|---|
| `set_permissions` in `carry_protections` | `cpe_1739_a_save_carries_the_mode_so_a_private_file_stays_private` | `0600` → `0644`, `result was Ok(())` (Linux) |
| `carry_xattrs` call | `cpe_1739_a_save_carries_extended_attributes_where_the_filesystem_has_them` | attribute gone, `result was Ok(())` (Linux) |
| `ReplaceFileW` branch disabled | `cpe_1739_windows_a_save_keeps_the_hidden_attribute_and_the_zone_identifier_stream` | `attrs=0x20`, `result was Ok(())` |
| `target_exists` condition dropped | `cpe_1739_a_save_to_a_free_name_still_creates_the_file` | `Err(... cannot find the file specified)` |
| `classify_carryover` never refuses | `cpe_1739_classify_carryover_refuses_a_target_it_cannot_read_but_allows_an_absent_one` | returns `Ok(false)` |
| `carried_mode` always keeps setuid | `cpe_1739_carried_mode_keeps_every_bit_but_drops_setuid_when_the_owner_changes` | `0o4755` where `0o755` required |

Each mutation redded **one** distinct test and nothing else. The Unix legs were run for real on Linux (a
`rust:1` container over this worktree), not left to CI — including the xattr leg, which printed no skip
notice and therefore genuinely exercised the property.

One judgement not asked for but forced by the work: `carried_mode` **drops `setuid`/`setgid` when the
replacement would have a different owner**. Ownership cannot be carried, so a surviving `setuid` bit no
longer means "runs as the original owner" — it means "runs as whoever saved it". That is not preservation,
it is a quietly re-pointed privilege bit. Not an escalation being prevented (anyone able to replace the
file could have created such a file anyway) — a misrepresentation being prevented.

**2026-08-15 — review round 1 (PR #913): UAT PASS, Reviewer CHANGES REQUESTED. One blocker, seven
non-blocking. All addressed.**

**BLOCKER — the staging file was made private one statement too late.** Round 1 created the `.cpe-tmp`
with `create_new`'s default and narrowed it with an `fchmod` immediately afterwards, before any bytes were
written, and the doc claimed the bytes were "never even briefly sitting at a world-readable staging name".
The bytes half was true; the **file object** half was not. POSIX checks permission at `open`, not at
`read`, so a local process that opens the staging name between the two calls keeps a readable descriptor
across the narrowing and reads the private bytes written after it — and the pid+nanos name needs no
guessing, because an inotify/FSEvents watcher is woken by the create itself. `strace` of the real test
binary, before:

```text
openat(AT_FDCWD, ".../secrets.env.382-1786791461862651861.cpe-tmp",
       O_WRONLY|O_CREAT|O_EXCL|O_CLOEXEC, 0666) = 3
fchmod(3, 0600)                                 = 0
```

Fixed by creating at the right mode instead of correcting to it. `create_exclusive` now delegates to a
single `create_exclusive_with_mode`, and `stage_and_replace` calls a new `create_staging_file` that asks
for `STAGING_MODE` (`0600`); `create_exclusive` itself keeps the platform default, because its other
callers (`split_join`'s joined output and manifest) are *claiming a name for a user's file*, not staging
one, and making those `0600` would have been an unrelated behaviour change. Both entry points funnel
through one body, so the crate still has exactly **one** `create_new(true)` and the PR #899 extraction
argument is unweakened. `strace` after, on the same test — note the ordering is deliberately
**narrow-then-widen**, the only order with no gap, since the target's mode is unknown until the file
exists:

```text
openat(AT_FDCWD, ".../secrets.env.88-1786792604872315786.cpe-tmp",
       O_WRONLY|O_CREAT|O_EXCL|O_CLOEXEC, 0600) = 3
fchmod(3, 0600)                                 = 0     <- the 0600 target file
rename(".../secrets.env.88-....cpe-tmp", ".../secrets.env") = 0
openat(AT_FDCWD, ".../build.sh.88-1786792604878333035.cpe-tmp",
       O_WRONLY|O_CREAT|O_EXCL|O_CLOEXEC, 0600) = 3
fchmod(3, 0755)                                 = 0     <- widened for the 0755 script
```

New test `cpe_1739_the_staging_opener_creates_a_file_no_one_else_can_open` drives `create_staging_file` —
the function `stage_and_replace` calls, one line above its call site. Reverting it to
`create_exclusive_with_mode(path, None)` reds that test **and nothing else**, reporting the mode as
`100644` — exactly the reviewer's `0666 & ~umask`. It carries a **umask control**: under a `0077` umask
the unfixed code would also produce `0600`, so the test first proves an ordinary `File::create` in the
same directory comes out group/other-readable, and loudly says what went uncovered if it does not. It ran
for real on Linux with no skip notice.

**Non-blocking, all seven:**

- **F2 (fixed as documentation).** Windows has no equivalent of the create-time mode: the staged
  `.cpe-tmp` carries the *parent directory's* inherited ACL for the whole write and only acquires the
  target's at the `ReplaceFileW` swap. Not a regression, and closing it means hand-building a security
  descriptor onto the staging handle — the `SetFileSecurity`-by-hand route `ReplaceFileW` was chosen over.
  Now recorded at `carry_protections`' Windows arm instead of being an unstated asymmetry.
- **F3 (fixed).** `carry_xattrs` now writes through the staged **file descriptor** (`FileExt::set_xattr`,
  an `fsetxattr`), matching the `fchmod`. The source side stays path-based and says why: no handle is held
  on the target, and opening one purely to read attributes costs an extra open on every save and fails
  outright on a file the user can write but not read — for a read whose worst case is copying a stale
  attribute onto a file about to be replaced anyway. The half worth hardening is the half that writes.
- **F4 (recorded).** The stat→commit TOCTOU is now in `replace_file_contents`' "What this does NOT do":
  a target that *appears* in the window is overwritten by the plain-rename branch (silent, but needs the
  file to have been absent when Save was pressed); one that *vanishes* makes `ReplaceFileW` fail
  `NotFound` (loud, nothing damaged). Closing them means holding the target open across the save, which is
  item 4 from the other side.
- **F5 (fixed).** `src/docs/25-metadata-studio.md` no longer lists extended attributes as an unqualified
  guarantee — it now says an attribute the OS won't let the app re-apply is dropped without warning, and
  that this is still far better than losing all of them but is not the guarantee the two entries above it
  are. The residual must not vanish on the page people actually read.
- **F6 (fixed).** The read-only-target line quoted both primitives as giving the identical string; measured,
  `ReplaceFileW` renders as `Access is denied. (0x80070005)` through the `windows` crate's `HRESULT`
  formatting, not `std::io`'s `os error 5`. Corrected, with the general point that every Windows error out
  of `commit_replacement` now carries an `0x8007NNNN` code.
- **F7 (fixed).** The item-4 pin asserted only that the message named the file, which a `classify_carryover`
  refusal would also satisfy — so the doc's "the error is at least now accurate about the cause" was held
  by nothing. It now asserts the message contains `0x80070020` (`ERROR_SHARING_VIOLATION`) and does **not**
  contain the refusal's wording. The HRESULT rather than the prose deliberately: the prose is localised by
  the OS and would red on a non-English runner for no real reason.
- **F8 (fixed).** The refusal message is no longer Unix-flavoured ("permissions and security settings, its
  attributes, and its alternate data streams" / "a file you had made private could come back readable by
  others") and now ends with a next step: check the file is still there and readable, then save again.

**Left as is on both checkers' advice:** the carry-over refusal itself. Both confirmed `fs::metadata`
succeeds against a `CreateFileW` handle at `share=0` and against an ACL denial of read-attributes, so it
cannot be tripped by an application or AV lock — it is far less reachable than the round-1 write-up feared.
