---
id: CPE-1887
title: a restore that cannot find its blob tells the user nothing, even when the bytes are sitting right there
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-25
closed:
---

## Problem

When `restore` cannot resolve a blob, the user gets:

```
...\blobs\A9C646ECFEE8A99C29E257FB9055A45D61B9EBD161CE6AAC6287CF8E9A850ED4:
the source could not be opened: The system cannot find the file specified. (os error 2)
```

and nothing else. **A reasonable person concludes their backup is gone.**

In the case that surfaced this, it isn't. The bytes are on disk the whole time — under the spelling
`capture` actually wrote (always lowercase), while the manifest names them in a different case. On a
case-folding filesystem the OS quietly reconciles the two; on a case-sensitive one it does not, and the
restore fails with the message above while the content sits inches away.

Found by PR #1023's independent UAT, which triangulated it three ways: a real ext4 filesystem inside
WSL (verified case-sensitive with `stat -f`, not the NTFS-backed mount), source inspection of
`blob_source` (a literal `blobs_dir.join(hash)` with no folding), and the PR's own `ubuntu-latest` CI
log showing the identical error. Its verdict, in its words:

> **yes, technically recoverable, but not through the app, and nothing tells the user.**

## Scope — this is not the case-mismatch bug

**CPE-1864 already fixed the dangerous half**: the prune witness no longer deletes a blob a survivor
still names, on every platform. This ticket is the *diagnostic* half, and it is deliberately broader
than the case-mismatch that exposed it.

Today the same opaque message covers every reason a blob fails to resolve: genuinely deleted, moved,
permissions, a truncated store, a manifest naming a hash that was never captured, and the case
mismatch. **The user cannot tell "your data is gone" from "your data is here under another name".**
Those deserve different answers.

## Can the case-mismatch state actually arise?

Not from any current in-app path — the UAT checked. `save_manifest` is called only from `capture`,
which always hashes via `sha256_file` (lowercase hex), and there is no import, sync, or
external-tool feature that writes into a checkpoint store. It needs direct filesystem access today,
or a future import/recovery feature.

That is *why this is Medium rather than High* — and also why it is worth doing before such a feature
exists, rather than after someone's restore fails.

## What to do

1. **Make the failure say which kind it is.** When a blob does not resolve, look before reporting:
   does something with that hash exist under a different case? Does the store contain a file of the
   right size? Is the store directory readable at all? Each answer is a different sentence to the user.
2. **If the content is there under another spelling, say so and offer the repair.** The user should not
   need to read source to learn their data is fine. A one-line "found it under a different spelling —
   repair this checkpoint?" closes the entire blind spot.
3. **Do not "fix" it by folding case in `blob_source`.** That would paper over a real integrity signal
   and make a genuinely-corrupt manifest look fine. The lookup should stay literal; the *diagnostic*
   is what is missing.
4. Consider whether the same treatment belongs on the checkpoint list — a checkpoint whose blobs
   cannot all resolve is arguably not restorable and the user could know before they try. Note
   **CPE-1862** already added a read-time filter for manifests that are *missing*; this is the same
   question one level down, for blobs.

## Acceptance criteria

- [ ] A failed blob resolution reports *why*, distinguishing gone / unreadable / present-under-another-name.
- [ ] The present-under-another-name case names the spelling found and offers a path forward.
- [ ] Demonstrated on a case-sensitive filesystem, since that is where it bites — a Windows-only
      demonstration proves nothing here.
- [ ] The lookup itself stays literal; no case folding added to `blob_source`.

## Work Log

- **2026-08-25 14:15 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`, from
  PR #1023's UAT. It passed that PR and filed this anyway, having walked into the blind spot itself
  while verifying the fix — which is the best possible provenance for a diagnostic ticket.
