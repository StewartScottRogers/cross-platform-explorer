---
id: CPE-1975
title: the session-daemon **port file** lives at a **fixed** `<temp>/cpe-ai-console/` path created with `create_dir_all` — redirect it and the console talks to the attacker's "daemon"
type: bug
priority: High
status: Open
tags: ready
estimate: M
created: 2026-08-28
---

## Summary

Found by CPE-1964's worker while re-deriving the `temp_dir()` enumeration. Three sites build the
**same fixed** path; **two** of them create it with **`create_dir_all`**, and the third only reads
and deletes (corrected in CPE-1964 round 2 — the original wording put `create_dir_all` on all three):

- `sidecar/ai-console/src/session_diag.rs:33` builds it; `:52` creates it with `create_dir_all`
- `sidecar/ai-console/src/session_supervisor.rs:151` builds it; `write_port_file` at `:144` creates it
  with `create_dir_all`
- `sidecar/host/src/reaper.rs:61` builds it — **no `create_dir_all` here**; `reap_orphan_session_daemons`
  only tests `port_file.exists()` and `remove_file`s it (`:79`). It is still in scope: it is a *reader
  and deleter* of the same redirectable path, so a planted link makes it consult — and unlink —
  something inside the attacker's directory.

`create_dir_all` is the primitive CPE-1952 established will **follow a junction/symlink into an
attacker-chosen directory**, and this path is **not even timestamped** — `cpe-swarm-<millis>` was at
least guessable-within-a-window; `cpe-ai-console` is a constant.

**What makes it worse than the two leaks fixed alongside it:** that directory holds the session-daemon's
**port file**. CPE-1952's catalog staging leaked *data*; CPE-1964's mission directory leaked *scaffolding*.
**This one is a control channel.** An attacker who redirects the directory controls the port file, and
the console then connects to **their** "daemon" instead of the real one.

## Threat model, stated in both halves

Per the correction CPE-1964 carries: on **Windows**, `std::env::temp_dir()` resolves to the **per-user**
`%LOCALAPPDATA%\Temp`, so the Windows attack needs a same-user process. *"Predictable path in a shared
namespace"* is fully true of **Unix `/tmp`**. Both halves are real; do not collapse them.

## The fix shape is already established in-tree — read it before designing

**CPE-1964 (PR #1086)** solved the same class one file over, and its reasoning transfers:

- **`std::fs::create_dir`, not `create_dir_all`** — one `mkdir(2)` / `CreateDirectoryW`, `AlreadyExists`
  on **anything** at the path including a reparse point, **atomically with the create**, so there is no
  check-then-use window.
- **No `exists()` pre-check.** That would be a shadowed guard; CPE-1964 refused it explicitly and said so
  at the site.
- **CPE-1952's stronger answer — delete the directory — is probably unavailable here too**, since a port
  file is a rendezvous by construction. Say so explicitly rather than leaving it implied, and say what
  hardening does and does not buy: the directory still exists in a shared namespace.

**But a port file is a control channel, so hardening the directory may not be enough.** Consider what
the *consumer* does: does anything verify the daemon it connects to is the one it started? A redirected
port file is only exploitable if the reader trusts the endpoint it names.

## Acceptance criteria

- [ ] **Reproduce the redirect first**, on both platforms, and **assert on the filesystem** — where the
      port file lands — never on a returned verdict. Junction on Windows (`junction::create`, no admin);
      symlink on **real ext4**, not `/mnt/z`. Note `/tmp` on WSL is **tmpfs**: override `TMPDIR`.
- [ ] **Then demonstrate the consequence**, not just the redirect: show a console connecting to a port
      file the attacker placed. If that turns out not to be reachable, **that is a real result** and it
      changes the priority — say so with the evidence.
- [ ] **Keep a sensitivity control** — with the fix disabled the redirect must succeed — as a normal CI
      test on all three OSes, **not `#[ignore]`d**, planting at the **real** path (a stand-in is
      unfalsifiable — CPE-1929). And heed the leg #1075 lost twice: a control that returns green because
      it could not plant its link proves nothing, **invisibly**. Panic; and note **ADR 0001 puts
      `skip_notice!` out of a sidecar's reach**, so write to the real stderr handle with `writeln!` as
      CPE-1964 did.
- [ ] **Run the CPE-1929 pair on every refusal you add** and **write both numbers at the site**.
      CPE-1964's third pair is the reason: an `is_symlink()` arm was written, measured, and **deleted**
      because `!meta.is_dir()` answered the same fact first on both platforms. Expect one of yours to be
      shadowed.
- [ ] **All three sites, or a stated reason why not.** They share the path; fixing one is the enumeration
      defect this repo keeps finding.
- [ ] Decide what to do with any existing `cpe-ai-console` directories, using CPE-1964's five-condition
      fail-closed shape if a sweep is warranted — and CPE-1972's rule: *an absence of information must
      never license a delete.*

## Notes

Filed 2026-08-28 by the sprint Foreman from CPE-1964's enumeration (PR #1086), which found these three
while re-deriving the `temp_dir()` site list with the corrected recipe and flagged them as wanting their
own ticket.

Related: **CPE-1964** (PR #1086 — the same class, and the model for the fix, the control and the CPE-1929
pairs), **CPE-1952** (delete the seam rather than defend it, where that is available), **CPE-1929**
(shadowed guards — one was found and deleted in CPE-1964's own sweep), **CPE-1972** (an absence of
information must never license a delete).
