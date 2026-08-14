---
id: CPE-1724
title: Measure the four unrouted staging mechanisms so their legs can go red instead of skipping quietly
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-13
closed:
---

## Problem

CPE-1717 routed every staging attempt it could through `cpe_server::fsutil::require_staged`, so a leg
that cannot stage its condition on a platform where the mechanism is *supposed* to work goes **red
under CI** instead of printing a notice into a green log.

Four sites were deliberately **left out**, and they are "Group F" in CPE-1717's enumeration:

| Site | Mechanism | Why it was not routed |
|---|---|---|
| `crates/server/src/split_join.rs` — `make_unstattable` (notice at ~`:826`) | three fallbacks in order: an `(RA,RD)` ACL deny, a symlink loop, … | the Windows behaviour of the fallbacks is **not measured**; `(RA)` is on the "does not refuse `try_exists`" side of `fsutil::deny_stat_of`'s table, and the symlink-loop fallback needs a privilege an unprivileged runner may not have |
| `crates/server/src/organize_apply.rs` (~`:291`, `:320`) | live-symlink creation, plus an `exists()`-on-the-link premise check | same: unmeasured on an unprivileged Windows runner, and it creates a *live* link, so `make_dangling_link`'s junction fallback does not apply unchanged |
| `crates/server/src/vault_crypto.rs` (~`:726`) | creating a **directory** link for the `promote` live-link leg | same |
| `src-tauri/src/lib.rs` (~`:15792`) | `cpe_1705_rename_entry_refuses_onto_a_dangling_symlink`'s own inline symlink creation | predates `make_dangling_link`; may simply be repointable at it |

**Nothing regresses today.** All four already use the capture-proof `writeln!(std::io::stderr(), ..)`
emitter, so their notices do reach the CI log — they are simply not *consequential*: the leg still
reports green. That is the second half of CPE-1717's finding, unfinished for these four.

Routing them on a guess is the thing to avoid. `require_staged(.., supported_here: true, ..)` on a
mechanism that genuinely cannot work on an unprivileged Windows runner produces exactly the **false
red** CPE-1717's own acceptance criteria forbid, and a false red teaches people to ignore the guard.

## What to do

For each of the four, in this order:

1. **Measure** what the mechanism actually does on an unprivileged Windows runner, a Linux runner and
   a macOS runner. `fsutil::deny_stat_of`'s doc comment is the model: a table of deny → observable
   effect, with the measurement conditions stated. Do not reason from the existing comments — this
   family has now recorded four separate "correct measurement of an incomplete setup" errors.
2. If it works everywhere → route it through `require_staged(.., true, ..)`.
3. If it genuinely cannot work on a platform → route it through `require_staged(.., cfg!(unix), ..)`
   (or whatever the measurement says) so the *other* platforms still go red.
4. If `src-tauri`'s inline symlink creation turns out to be equivalent to `fsutil::make_dangling_link`,
   delete it and call the helper — one implementation, per CPE-1710's own reasoning.
5. Add each newly-routed leg to the `skip-visibility guard (CPE-1717)` step's filter list in
   `.github/workflows/ci.yml`, so the routing is proved by neutralisation rather than asserted.

## Acceptance criteria

- [ ] Each of the four mechanisms has a **measured** per-platform verdict written at the site, with the
      conditions of the measurement stated (non-elevated? local NTFS? which deny?).
- [ ] Each is routed through `require_staged` with a `supported_here` that the measurement supports, or
      carries a comment saying why it must remain a notice-only skip on every platform.
- [ ] Breaking the routing turns a **distinct** test or CI step red — add the leg to the guard step's
      filter list and paste the real CI output (Evidence Rules, `Ticketing/wiki.md`).
- [ ] No leg that legitimately cannot stage on a platform becomes a false red there.

## Notes

Filed out of the PR #898 (CPE-1717) round-2 review, 2026-08-13. The reviewer assessed leaving these
four unrouted as **honest rather than half-done** — but noted the follow-up existed only as prose in a
ticket that was about to close, which is how intent gets lost.

Related: **CPE-1717** (the mechanism and the enumeration), **CPE-1705** / **CPE-1710** (the staging
helpers), **CPE-1687** (`split_join`'s unstattable-part leg specifically).
