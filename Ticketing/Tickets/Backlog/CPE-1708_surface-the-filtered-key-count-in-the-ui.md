---
id: CPE-1708
title: When a remote listing hides keys, the count only reaches a log line the user never sees
type: task
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-13
closed:
---

## Problem

CPE-1704 fixed the bug where a legal S3 key silently vanished from a listing. Its acceptance criteria also
said that a key the guard **genuinely must** refuse should not vanish silently either — *"surface it under a
visibly-escaped display name, or report that N entries were filtered. Either is acceptable; dropping it
invisibly is not."*

That half is **deferred, not delivered**, and the ticket should say so plainly rather than read as closed.

Where it got to (PR #890, round 3):

- The count is real and trustworthy — a `usize` on `crates/vfs`'s `RemoteListing { entries, filtered }`,
  computed in-process from the provider's own filtering, **not** reconstructable or spoofable from wire data.
- It stops at the Tauri boundary. `list_dir`'s response is a `specta`-typed contract that reaches the
  frontend, and the CPE-1704 worker judged threading a new field through it out of scope for a fix already
  spanning three crates. **That was the right call** — it flagged rather than rushed.
- So today the count reaches an `eprintln!` and nothing else. **A user with a hidden key sees a listing that
  looks complete.**

### Why the earlier attempt is not the answer

Round 2 tried a synthetic `⚠ N keys hidden` entry appended to the listing. Do not revive it. The reviewer
found it **worse than the silent drop**:

- **Spoofable.** A real object can be named exactly like the marker (measured: accepted, emitted as a normal
  7-byte file). Since the genuine marker contained a `/` and was itself refused by the shared name guard,
  **the only such row a user could ever have seen was an attacker-planted one.**
- **Dishonest fields.** `is_dir: false, size: 0` — it claimed to be a zero-byte file.
- **Delete reported success.** S3 `DELETE` of a missing key returns **204**, so deleting the marker would
  have said it worked, and it would still be there on refresh.
- Off-by-one in item counts, included in select-all, and it slipped past `MAX_LIST_ENTRIES`.

The count must travel as **data**, not as a fake row in the data.

## Scope

The `list_dir` command's response type and its `specta` binding, `crates/vfs`'s `RemoteListing`, and
whatever surfaces it in the UI.

## Acceptance criteria

- [ ] The filtered count reaches the frontend as a typed field, not a synthetic entry.
- [ ] **Regenerate `bindings.gen.ts`.** Editing a `specta::Type` struct — even its doc comment — without
      regenerating fails CI's typed-bindings drift guard. Local crate-only checks miss it.
- [ ] The UI says something honest and useful when the count is non-zero. Decide the surface — a status-bar
      note, an inline banner, a toast — and record why. It must not imply the *listing* failed; the listing
      succeeded and some entries were not representable.
- [ ] The wording is for a person, not a maintainer. The round-2 attempt cited an internal ticket ID in
      user-facing text; keep ticket references in code comments where they help.
- [ ] Zero filtered entries produces **no** UI noise at all. This is the common case by an enormous margin.
- [ ] A test proves the count survives the whole path — provider → `RemoteListing` → command → frontend —
      and that breaking any link turns a **distinct** test red, per the Evidence Rules in
      `Ticketing/wiki.md`.
- [ ] Confirm every other provider (SFTP, WebDAV, FTP, local) reports zero and is unaffected. The
      `list_with_filtered_count` default delegates to `list` and reports 0; that default must stay correct.

## Notes

Filed by the Foreman from PR #890 (CPE-1704), 2026-08-13, on the worker's own recommendation.

Nothing can hit this today — `crates/s3` is not wired into the app until **CPE-1685**. Ideally this lands
before that one, so S3 support does not ship with a listing that can quietly omit an object. It is a
smaller and less urgent gap than CPE-1704's was, though: the keys being hidden now are only those a
correctly-scoped S3 guard genuinely refuses, not ordinary files with a colon in the name.

Related: **CPE-1704** (which produced the count), **CPE-1685** (which makes it user-visible),
**CPE-1683** (the listing).
