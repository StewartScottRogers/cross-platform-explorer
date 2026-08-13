---
id: CPE-1714
title: Download-then-reupload renames the remote object, because the download name rewrite is never undone
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-13
closed:
---

## Problem

Raised by **both** the reviewer and the UAT on PR #894 (CPE-1709), each independently, and each judging it
out of scope for that PR. The author disclosed the asymmetry in the docs, which is the honest minimum;
closing the loop is this ticket.

CPE-1709 rewrites Windows-unholdable leaf names at the **download** sink (`local_safe_segment`, reached via
`guarded_join`) and ships `decode_windows_safe_segment` as the exact inverse. That decoder currently has
**no production caller** — it appears only in its own definition, a doc link, and test assertions.
`upload_tree` builds remote paths through the separate `join(&str, &str)` helper and never decodes.

Consequence on Windows:

```
S3 key  colon:name.txt
  -> downloads as  colon%3Aname.txt
  -> re-uploads as colon%3Aname.txt      <- a NEW key; the original is not updated
```

**Download-then-reupload silently renames the object.** Anyone using download+upload as a copy or move
between locations gets a differently-named object and no indication of it.

This is **not a regression**: before CPE-1709 the file was lost entirely on the way down, so it could not
round-trip at all. It is newly *reachable* precisely because the download now works.

## The decision to make — and why it is not obvious

Do not reflexively "just decode on upload". The author declined to wire the decoder for a real reason, and
that reason has to be answered rather than ignored:

> Encoding is **compelled** by the filesystem — we had no choice, the name could not be written otherwise.
> Decoding would be a **guess about provenance**. `report%3Afinal.txt` is a perfectly storable local name
> that a user may simply have typed. Nothing in the app tracks whether a given local file arrived via a
> rewrite, so decoding on upload would silently rename *their* remote key.

So the options are:

- **(a) Decode on upload.** Restores the round trip, but makes a genuine local `%253A` ambiguous with an
  encoded name, and silently renames a hand-typed one. Needs a provenance answer to be safe.
- **(b) Leave it and document.** Already partly done. Cheapest, honest, and leaves the copy/move footgun.
- **(c) Carry the original name out-of-band** — a sidecar record, an extended attribute, or a transfer
  manifest — so the decode is driven by *known* provenance rather than by guessing from the name's shape.
  Most correct, most work.

**Whichever is chosen, record the reasoning**, and update `decode_windows_safe_segment`'s "shipped, not
test-only" justification so it matches reality. Right now it ships `pub` claiming a production role it does
not have.

## Acceptance criteria

- [ ] A download-then-reupload of a key containing `:` either restores the original key, or **tells the
      user the name changed**. Silently creating a differently-named object is not acceptable.
- [ ] If decoding is wired: a local file the user genuinely named `report%3Afinal.txt` must **not** be
      silently renamed on upload. Prove that case specifically — it is the whole reason this was deferred.
- [ ] `decode_windows_safe_segment`'s doc reflects its actual role.
- [ ] The CPE-1709 invariants are untouched: `decode(encode(x)) == x`, injectivity across the adversarial
      corpora, and `guarded_join` containment. Re-run those brute forces rather than assuming — the
      reviewer's sweep was 1,235,632 inputs and the UAT's 629,161, and both are cheap to repeat.
- [ ] Platform-gate correctly. The rewrite only happens on Windows, so a Unix leg must still assert
      something real. CI runs a 3-OS matrix.
- [ ] Each guard broken **on its own** turns a **distinct** test red, real output pasted in the PR, per the
      Evidence Rules in `Ticketing/wiki.md`.

## Notes

Filed by the Foreman from the PR #894 review and UAT, 2026-08-13.

Not urgent: it needs a user to download and then re-upload the same object, and nothing is lost when they
do — the bytes are intact under a different name. It is a correctness and least-surprise problem, not a
data-loss one.

Related: **CPE-1709** (which created the rewrite and the decoder), **CPE-1704** (the listing guard that
stopped imposing filesystem rules on every backend), **CPE-1685** (which makes S3 reachable at all).
