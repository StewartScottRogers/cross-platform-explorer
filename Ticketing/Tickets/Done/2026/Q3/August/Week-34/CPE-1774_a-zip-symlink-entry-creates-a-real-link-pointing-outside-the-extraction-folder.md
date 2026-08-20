---
id: CPE-1774
title: A zip symlink entry creates a real link pointing outside the extraction folder, and reading it reads the outside file
type: bug
priority: High
status: Done
tags: ready
estimate: M
created: 2026-08-17
closed:
---

## Problem

**Demonstrated**, not theorised, by the Security Auditor on PR #926 (CPE-1758) while auditing that PR's
name guard. It is unrelated to CPE-1758 and was not introduced by it — it is a pre-existing live bug that
audit happened to walk into.

A `.zip` can carry a **symlink entry**: an entry whose stored content is a path, flagged as a link. The
one-shot `extract_archive` path calls the `zip` crate's own `archive.extract(dest_path)`
(`crates/server/src/archive.rs:1345`) directly, with none of our guards. That extractor materialises stored
symlink entries as **real OS symlinks** (`zip-8.6.0/src/read.rs::make_symlink_impl`, read directly), using
the entry's raw content bytes as the link target, with **no traversal or canonicalisation check on the
target at all.**

The measurement — a zip whose entry is named `evil_link` (a name our guard accepts, correctly: it is a
perfectly ordinary name) with stored target `..\outside_secret.txt`:

```
symlink_metadata(dest/evil_link).is_symlink() = Ok(true)
fs::read_link(dest/evil_link)                 = Ok("..\\outside_secret.txt")
fs::read_to_string(dest/evil_link)            = Ok("SECRET")
```

A real symlink was created pointing **outside the extraction root**, and reading the "extracted file"
returns the content of a file the user never chose to touch.

## Why the existing guards do not cover it

`entry_name_is_safe` inspects the entry's **name**. This attack uses a perfectly legitimate name; the
payload is the **target**. Nothing in `archive.rs` validates a symlink entry's target on any path.

Our own guarded write loops (rows 15/16/19/20) are not affected — they treat every non-directory entry as a
regular file and copy bytes in, never materialising a link. So the hazard is confined to the crate-native
extractors, which is precisely why it survived: it lives in the one place our guards were never applied.

CPE-1746 covers 7z link **targets** specifically, by an explicit design decision in that ticket. There is no
equivalent for zip or tar.

## Scope of exposure

The `extract_archive` command **is registered** (`src-tauri/src/lib.rs:8290`) but currently has no frontend
caller — the UI uses the streamed path. That reduces the exposure today; it does not close it. A registered
command with no caller is one commit away from having one, and "nothing calls it yet" is not a guard.

**Tar is likely the same and is unconfirmed.** Reading the `tar` crate source: `unpack_in` explicitly
canonicalisation-validates a **hard link**'s target (`entry.rs:924`, `validate_inside_dst`), but the
**symlink** branch calls `symlink(&src, dst)` with the raw target and no equivalent check. That was reasoned
from source, **not measured** — confirming it is the first task here.

## What to do

- Confirm the tar symlink case by building a crafted tar and measuring it. Do this before designing the
  fix, so the fix covers what is actually broken.
- Decide the policy and write it down: refuse link entries outright, refuse only those whose target escapes
  the extraction root, or extract them as regular files containing the target text. Each is defensible;
  silently creating an escaping link is not. Note that "refuse" runs into CPE-1775 — a refusal the user
  never sees is its own problem.
- Apply it wherever an archive extractor can materialise a link: the one-shot zip fallback, the tar paths,
  and anywhere else `extract()`/`unpack()` is handed control of the write.
- Validate the **target**, resolved against the destination, not the literal string. A target of `x/../..`
  is as much an escape as `..`, and an absolute target is one too.

## Acceptance criteria

- [ ] A zip containing a symlink entry whose target escapes the extraction root does **not** produce a link
      that resolves outside it. Demonstrate with the auditor's reproduction and show the new behaviour.
- [ ] The equivalent tar case is measured and covered, or explicitly ruled out with the measurement shown.
- [ ] An absolute target, a `..`-chain target, a target with mixed separators, and a link-to-a-link chain are
      each covered.
- [ ] A **legitimate** relative symlink pointing inside the extraction root still works, or the decision to
      drop those is recorded with its reason — do not silently break valid archives.
- [ ] Whatever happens to a refused link entry is **reported**, not swallowed (see CPE-1775).
- [ ] Each test asserts the harm — where the link points, and what reading through it returns — **before**
      unwrapping the `Result`.
- [ ] `archive.rs`'s section comment and CPE-1733's sink table record which extractors can materialise links
      and what guards them.

## Notes

Demonstrated by the Security Auditor on **PR #926 / CPE-1758**, 2026-08-17, during the batched sprint.
Related: CPE-1746 (7z link targets — the existing precedent), CPE-1773 (tar bypasses the name guard),
CPE-1775, CPE-1744, CPE-1710.
