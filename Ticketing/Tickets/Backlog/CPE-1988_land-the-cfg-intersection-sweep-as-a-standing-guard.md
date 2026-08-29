---
id: CPE-1988
title: land the cfg-intersection sweep as a standing guard — a Windows-gated call in cross-platform code is invisible to every pre-push check this crew runs
type: task
priority: Medium
status: Open
tags: ready
estimate: S
created: 2026-08-28
---

## Why this exists

**PR #1103 (CPE-1959) went red on CI after `APPROVE` + `SEC PASS`**, on a one-line defect:

```
error[E0425]: cannot find function `make_guid_reparse_point` in module `crate::fsutil`
    --> src/batch_media.rs:2137:33
note: found an item that was configured out
    --> src/fsutil.rs:7506:8   #[cfg(windows)]
error: could not compile `cpe-server` (lib test) due to 1 previous error
```

Four jobs red — `Server crates` on **ubuntu and macos**, `MSRV`, and the verdict rollup. **Windows green.**

**Three careful parties missed it.** The author ran the full suite and both clippy feature modes; an
independent Reviewer re-ran every sabotage, re-derived the enumeration and audited the security posture;
the Foreman read both reports. **All three were on Windows, and all three were green.** Nothing in the
review loop *asks* the platform question, so every leg was rigorous within its platform and blind across
them.

The repo already knows this — *CI runs a 3-OS backend matrix; local Windows-only `cargo test` misses
platform failures* — and it happened anyway, which is the argument for a mechanism rather than another
reminder.

## The check, which already exists and is not committed

That PR's worker built it while fixing the defect, and it is the useful artefact:

> Enumerate every `cpe-server` item declared under a **windows-only `cfg`** with **no `not(windows)`
> sibling** (**96** of them). Tokenise every **added non-comment line** in the branch diff (**291**
> identifiers). **Intersect.** Five real hits, each confirmed gated at its call site; one false positive
> (`stage`, inside an assertion string literal).

**It needs no cross toolchain and no second machine** — it is a derivation over the source, so it runs
anywhere, in seconds, and it would have caught this from the machine that shipped it. It currently lives
in a scratchpad, which by this repo's own rule means **nothing a reviewer can check**.

## What this needs

- [ ] **Commit the sweep** — as a test or a script with a CI job, decided deliberately and stated. If a
      job, cap it with a **measured** `timeout-minutes` per CPE-1967 rather than a round number.
- [ ] **Enumerate at run time and fail loudly on a near-empty enumeration** (CPE-1932). "96 items" and
      "291 identifiers" are today's numbers, not constants; a sweep that silently finds zero gated items
      reads as clean.
- [ ] **Handle the false-positive class honestly.** The one miss was an identifier inside a **string
      literal**. Anchor on code, not on text (CPE-1933 rule 2) — `src/lib/rustSource.ts` already owns
      `stripRustComments` / `rustStringLiteralAfter` / `rustStrSliceAfter` for exactly this, and a second
      private stripper is refused rather than grown (CPE-1950). Whatever residue remains, write it as
      **"at least these"**, never a closed count.
- [ ] **Red-proof it against the real defect.** Re-introduce PR #1103's exact call — an ungated call to a
      `#[cfg(windows)]` item — and confirm the sweep names it. **A guard that cannot reproduce the failure
      it was built for is not a guard**; two on this crew this week were structurally unable to fire for
      the thing they were named after.
- [ ] **Cover the other direction too, or say you did not.** A `#[cfg(unix)]`-only item called from
      cross-platform code fails on Windows — the same defect mirrored, and the one this crew is *least*
      likely to catch locally, since every machine here is Windows.
- [ ] **Say what it does NOT cover.** It is a name-intersection, not a compile: a call reached through a
      trait, a macro, or a re-export may not appear as a bare identifier on an added line. State that at
      the site.

## The habit this replaces, and the one it does not

**Do not let this become a substitute for the matrix.** The sweep is a *cheap pre-push* check; the 3-OS
matrix is what actually compiles. What the sweep buys is the ~50-minute round trip that PR #1103 spent
discovering a one-line error.

And worth stating in any dispatch brief that touches `crates/server`: **"clippy clean in both feature
modes" is a claim about *features*, and it reads as a claim about *platforms*.** A worker who cannot
cross-compile should say **"I could not check non-Windows targets"** plainly — as PR #1103's worker did on
its second attempt, naming the four native C dependencies (`zstd-sys`, `lzma-sys`, `ring`, `bzip2-sys`)
whose build scripts need `x86_64-linux-gnu-gcc`.

## Notes

Filed 2026-08-28 by the sprint Foreman from PR #1103's post-CI fix, at that worker's own suggestion.

Related: **CPE-1959** (PR #1103 — where the defect landed and where the sweep was written), **CPE-1967**
(measured job timeouts), **CPE-1932** (enumerate, don't recall; fail loudly on a near-empty enumeration),
**CPE-1933** (anchor on code, never on prose; do not name a backstop without checking it can fire),
**CPE-1950** (one stripper, not a sixth).
