---
id: CPE-1976
title: the swarm-activity **read** path is unhardened — a planted link makes the console read and serve an **attacker-chosen directory** over the loopback API
type: bug
priority: High
status: Open
tags: ready
estimate: M
created: 2026-08-28
---

## Summary

Found by PR #1086's Reviewer while verifying CPE-1964's hardening of the *create* and *sweep* paths.

`handle_swarm_activity` does `temp_dir().join(mission)` and reads `mailbox.jsonl` and `memory/*` with
**no `symlink_metadata` check**. So a planted directory link at a `cpe-swarm-<alnum>` name makes the AI
Console **read an attacker-chosen directory and serve its contents over the loopback API**.

**Create and delete are now hardened; read is one line away from the same treatment.**

- **Create** (CPE-1964, PR #1086) — `std::fs::create_dir`, atomic `AlreadyExists` on anything at the
  path including a reparse point.
- **Sweep** (same PR) — `symlink_metadata` + `!is_dir()`; a planted link is **never followed and never
  removed**, verified on both platforms including a link nested *inside* a genuine mission directory.
- **Read** — nothing.

## Why it is High

**This is exfiltration, not corruption.** The other three tickets in this family were about writes
landing where they should not (CPE-1952 data, CPE-1964 scaffolding, CPE-1972 deletes). This one hands
an attacker's file contents to a caller that believes it is reading the app's own mission state, over
an API the console exposes.

It is **pre-existing**, but CPE-1964 **widened the accepted id space from digits to alnum**, so the
name space an attacker can plant into is larger than it was.

**Threat model, both halves stated separately** (per the correction CPE-1964 carries): on **Windows**
`std::env::temp_dir()` is the **per-user** `%LOCALAPPDATA%\Temp`, so the attack needs a same-user
process. *"Predictable path in a shared namespace"* is fully true of **Unix `/tmp`**.

## Acceptance criteria

- [ ] **Reproduce first**, on both platforms, and **assert on what the API returns** — the attacker's
      bytes reaching the response — not on a verdict enum. Junction on Windows (`junction::create`, no
      admin); symlink on **real ext4** (`/tmp` on WSL is **tmpfs** — override `TMPDIR`).
- [ ] **Harden the read the way the sweep was hardened**: `symlink_metadata` and refuse a
      non-directory, before anything reads through the path. Match the sweep's shape rather than
      inventing a second one, and check whether the **nested**-link case matters here too — the
      Reviewer proved `remove_dir_all` does not recurse through a reparse point, but a *read* walk is a
      different operation and needs its own answer.
- [ ] **Keep a sensitivity control** — with the fix disabled the read must escape — as a normal CI test
      on all three OSes, **not `#[ignore]`d**, planting at the **real** path (a stand-in is
      unfalsifiable, CPE-1929). **ADR 0001 puts `skip_notice!` out of a sidecar's reach**, so write to
      the real stderr handle with `writeln!`, and **panic** rather than returning when the plant fails
      — a control that goes green because it could not set itself up proves nothing, invisibly.
- [ ] **Run the CPE-1929 pair on every refusal you add** and **write both numbers at the site**.
      CPE-1964's third pair is why: an `is_symlink()` arm was written, measured, and **deleted** because
      `!meta.is_dir()` answered the same fact first on both platforms. Expect one of yours to be
      shadowed.
- [ ] **Enumerate the other readers of that directory.** `swarm_mcp_server::run` reads
      `members.json` / `mailbox.jsonl` / `memory/*.md` from a **separate process**, and CPE-592's
      `/api/swarm/activity` from a **third**. Derive the list at run time (CPE-1932) and report a
      verdict per reader — fixing the one the Reviewer named is the enumeration defect this repo keeps
      finding.
- [ ] Say what the **residual** is. Hardening the read still leaves the directory in a shared namespace;
      the stronger answer — relocating out of `%TEMP%` into an app data dir — is noted in CPE-1964's
      review as unavailable today because **ai-console has no data-dir helper**. If that is the real
      fix, say so and let it be its own ticket rather than half-doing it here.

## Notes

Filed 2026-08-28 by the sprint Foreman from PR #1086's Reviewer (**APPROVE**, F2), which attacked all
five of that PR's sweep conditions, ran the CPE-1929 pair on three the worker had not, and then noticed
the read side had been left out of the hardening entirely.

Related: **CPE-1964** (PR #1086 — the create and sweep hardening, and the model for the control and the
pairs), **CPE-1975** (the session-daemon port file at a fixed `<temp>` path — the other control-channel
in this family), **CPE-1952** (delete the seam rather than defend it, where available), **CPE-592** (the
coordination panel that is the third reader), **CPE-1929**, **CPE-1932**.
