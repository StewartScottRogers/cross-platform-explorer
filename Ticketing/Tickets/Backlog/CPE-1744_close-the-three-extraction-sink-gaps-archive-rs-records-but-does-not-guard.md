---
id: CPE-1744
title: Close the three extraction-sink gaps archive.rs now records but does not guard
type: task
priority: High
status: Backlog
tags: ready
estimate: L
created: 2026-08-14
closed:
---

## Why this exists

CPE-1733 enumerated every `File::create`/`fs::write`/`create_dir_all` destination in `archive.rs` and
guarded the link question at the ones it measured. Its PR (#906) review then measured **three hazards the
enumeration had recorded as absent, understated, or wrongly excused**. All three are now written down at
the sites, with real output, and **none of them is fixed**. CPE-1718 established that an unrecorded absence
is indistinguishable from an overlooked one; the #906 review added the corollary this ticket answers — *a
recorded absence with no ticket is a recorded absence nobody is scheduled to fix.*

Read `archive.rs`'s "Archive creation & extraction" section comment first. It carries the 18-row table, the
measurements below, and a `LEAF ONLY` marker on rows 15–16 that exists because of item 2.

## The three gaps, each with the measurement already taken

### 1. `entry_name_is_safe` accepts what `transfer::is_safe_name` rejects — including the NTFS ADS shape

`guarded_join` does not only answer traversal: it applies `is_safe_name` per segment, which fails closed on
a `:` anywhere and on a leading `..` (CPE-1461/1709), and on Windows sanitises through
`local_safe_segment`. `archive.rs`'s `entry_name_is_safe` has no equivalent to either.

```text
[M7] entry_name_is_safe("file:stream") = true    entry_name_is_safe("..evil") = true
     entry_name_is_safe("con") = true            entry_name_is_safe(" sp ") = true    ("x." = true)
[M8 fs::write to "adsbase:stream"] = Ok(())
     adsbase_len = Some(4) (unchanged)   a plain file named "adsbase:stream" exists = false
```

A ZIP entry named `file:stream` passes the check, reaches rows 15–16's `File::create`, and the bytes vanish
into an alternate data stream of a neighbouring file — **the user is shown a successful extraction and has
no file**. This is CPE-1709's bug at a sink CPE-1709 did not cover.

The delta is pinned by `archive::tests::entry_name_is_safe_accepts_shapes_transfers_is_safe_name_rejects`,
which asserts **both** functions. Closing this gap will turn that test red — **that is the intended
signal**, not a regression: update it and the section comment's table together.

### 2. The rows 15–16 link guard is LEAF-ONLY, and `create_dir_all(parent)` follows a directory link

`entry_name_is_safe("sub/x.txt")` is `true`, and both ZIP write loops run `create_dir_all(parent)` *before*
the leaf guard. A directory symlink — or a **junction, which needs no privilege on Windows** — already at
`dest/sub` redirects everything beneath it, and the leaf guard never sees a link because the leaf does not
exist yet:

```text
[M9] entry_name_is_safe("sub/x.txt") = true
[M9] create_dir_all(parent) = Ok(())
[M9] guard verdict for the LEAF (symlink_metadata of dest/sub/x.txt) = Err(NotFound)   -> no refusal
[M9 File::create through a symlinked intermediate dir] = Ok(())   landed_outside = true
```

Closing it needs **per-component** resolution rather than a leaf check — a different guard from the one
CPE-1733 measured, which is why it was scoped out rather than widened into.

Note while here: CPE-1729's finding was that `create_dir_all` is not **destructive**. It is not
hazard-free — a live directory link **redirects** — and CPE-1733's first draft used the former to excuse
the latter. That correction is already in the code comment; the behaviour is not fixed.

### 3. `tar` destroys a link, and one-shot ZIP aborts where streamed ZIP skips

`tar::Archive::unpack`/`Entry::unpack_in`, `zip::ZipArchive::extract` and
`sevenz_rust::default_entry_extract_fn` create their files **inside those crates**, so `archive.rs` has no
create site to guard and cannot reach one without reimplementing each crate's extraction.

CPE-1733's PR first claimed a pre-existing link "is still followed on the tar, 7z and one-shot-zip paths".
**That was inference, not measurement, and it is false for two of the three** (its UAT measured all three
on Windows and Linux; reproduced independently before this ticket was written). The 7z case is real and
has its own ticket, **CPE-1746**. The other two land here:

```text
[tar ONE-SHOT and STREAMED]  outcome = Ok(..)   victim bytes = Some("VICTIM ORIGINAL")
                             slot is link = Ok(false)   slot is file = Ok(true)
[zip ONE-SHOT]               outcome = Err("invalid Zip archive: Invalid symlink target path")
                             victim bytes = Some("VICTIM ORIGINAL")   b.txt extracted = false
[zip STREAMED, guarded]      outcome = Ok(ArchiveReport { done: 1, errors: ["a.txt: ... is a link ..."] })
                             b.txt extracted = true
```

- **tar does not follow — it *destroys*.** It unlinks the symlink and writes a regular file in its place.
  The victim's bytes are safe; the **user's link is silently gone** and the call reports success. A real
  hazard of a different shape from the one CPE-1733 guarded, recorded nowhere before this.
- **One-shot ZIP aborts the entire extraction** where streamed ZIP skips the entry and extracts the rest.
  Two shipped paths, opposite behaviours on the same input. `extract_archive` is a registered Tauri command
  with **no current Svelte caller**, so this is an API/doc inconsistency rather than a live UI regression —
  but it is a real divergence and the in-app docs can only describe one of them.

**Both behaviours ARE pinned by characterization tests** (CPE-1733 round 3 — an earlier draft of this
ticket said the opposite, which was round 2's stance and is now false):

- `archive::tests::tar_extraction_destroys_a_link_at_an_entry_name_rather_than_following_it` — asserts the
  link is replaced by a regular file holding the entry's bytes, with the victim intact and `Ok` returned,
  on **both** tar paths.
- `archive::tests::one_shot_zip_extraction_aborts_everything_when_an_entry_lands_on_a_link` — asserts the
  whole-run abort (`b.txt` absent is what separates "skipped an entry" from "abandoned the run") and
  requires the zip crate's *symlink* refusal rather than any I/O error.

Fixing either gap **will turn its test red — that is the intended signal**, the same arrangement item 1
already uses. Re-point the test at the new behaviour in the same commit as the fix; do not delete it.

### 4. Row 17's dangling-link wording

`create_dir_all` on a **dangling** link at the extraction `dest` does not "do nothing" — it fails, and
fails the whole extraction with the OS's misleading phrasing: Windows `Err(os error 183, "Cannot create a
file when that file already exists.")`, Linux `Err(os error 17, "File exists")`. That is the same wording
defect row 7 (`create_empty_zip`) got a guard for. CPE-1733 left it because the *live*-link case at that
site should keep following the link (the user pointed at that folder — `fsutil`'s claiming-vs-editing
rule), so the fix is wording-only and needs to not disturb the live case.

## What to do

- [ ] Decide the shape first and write it down before coding: a shared pre-write path validator (name
      segments **and** resolved components) is the obvious candidate, and it would serve items 1 and 2
      together. Check whether `transfer::guarded_join` can simply be adopted here rather than a third
      implementation grown — CPE-1733 declined to add it *for traversal only*, and that reasoning does not
      extend to this.
- [ ] Item 3 probably wants a **pre-extraction sweep of the destination** (refuse, or report, links found
      in `dest` before handing off to a crate that will follow them) since the write itself is unreachable.
      Confirm that is true before assuming it — the `tar` crate has `set_overwrite`/`unpack_in` options
      that may already offer a hook.
- [ ] Every guard broken **on its own**, a **distinct** test red, real output pasted, restored with
      `git checkout --`. Assert on the filesystem and the bytes, never the returned `Result` — every bug in
      this family returned `Ok` while destroying something — and **assert the effect before unwrapping**.
- [ ] Pin a **distinctive refusal**, not `is_err()`. On an unprivileged Windows runner a dangling junction
      makes `File::create` fail by itself (`Access is denied`, os error 5, measured for CPE-1733), so an
      `is_err()`-only leg passes straight through a deleted guard.
- [ ] Update `archive.rs`'s table and `src/docs/explorer-archives.md` in the same change — both currently
      state these gaps as open — **and re-point the two characterization tests named under item 3**
      (`tar_extraction_destroys_...`, `one_shot_zip_extraction_aborts_...`) plus item 1's
      `entry_name_is_safe_accepts_shapes_transfers_is_safe_name_rejects`. Those three going red is how you
      know the fix landed; leaving them red, or deleting them, is how the description drifts from the code
      again.
- [ ] **Docs correction owed regardless of the fix** (measured in PR #906's UAT):
      `src/docs/explorer-archives.md:89-92` tells the user that if the destination "already contains a
      folder shortcut and an entry is addressed through it, the entry still follows it". That is true for
      the ZIP paths (item 2) but **false for TAR**, which refuses with *"trying to unpack outside of
      destination path"*. The sentence is stated for extraction generally, so it currently misdescribes one
      of the two formats it covers.
- [ ] **While in `create_empty_zip` for item 4's wording:** row 7's guard reworded only the *link* case.
      Onto a plain existing file it still returns `Err("The file exists. (os error 80)")` — naming neither
      the path nor which of the two files is meant, which is the same defect one step over.

## Notes

Filed by the CPE-1733 worker from the PR #906 review, 2026-08-14. Related: **CPE-1733** (the enumeration
and the link guards), **CPE-1709**/**CPE-1461** (`is_safe_name`/`guarded_join`, the ADS shape at the
transfer sink), **CPE-1729** (`create_dir_all` is not destructive — and what that does *not* mean),
**CPE-1718** (recorded absences), **CPE-1745** (the `src-tauri` temp-path mirror, found in the same review).
