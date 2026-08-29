---
id: CPE-1989
title: the user-facing **Shred** command has the same retained-plaintext residual — alternate data streams on Windows, extended attributes on Unix
type: bug
priority: High
status: Open
tags: ready
estimate: M
created: 2026-08-28
---

## Summary

Found by **CPE-1986**'s worker (PR #1106) while closing the vault wipe's alternate-data-stream leak, and
deliberately not widened into — a secure-delete change to the explorer's user-facing command belongs in its
own reviewed ticket, not folded into a vault fix.

**`secure_shred::shred_file` — the explorer's **Shred** command — carries the identical residual, on every
platform.** It overwrites the **default data stream** and then unlinks. Anything else attached to the object
survives as extents on the volume:

- **Windows: alternate data streams.** Measured in CPE-1986 on the vault path under the production policy:
  `wipe_ok=true  main_all_zero=true  ads_still_secret=true` — and the same probe found it **on a directory
  too** (`sub:dirsecret` survived a full wipe), a case that ticket had not named.
- **Unix: extended attributes**, including macOS `com.apple.ResourceFork`.

It is now **stated at that function** rather than silent, which is the difference between a declared
residual and a defect. **Closing it is this ticket.**

## Why the Windows half is cheap and the Unix half is not

**CPE-1986 already built the Windows mechanism and it is reusable.** `vault_manager::shred_alternate_streams`
enumerates with `FindFirstStreamW`/`FindNextStreamW` and overwrites each named `$DATA` stream **through
`overwrite_pinned_file`** — the same function, and the same refusals, the default stream uses.

**The reuse rests on a measurement worth re-checking rather than inheriting:** a handle opened at
`file:name` reports the file's **own** volume serial and file index, its link count, **no directory bit even
on a directory's stream**, no reparse tag, and a `metadata().len()` that is **the stream's** length. If any
of that is false in this call path, the refusals are being applied to the wrong subject.

**The Unix half was deliberately left open, with a reason that must not be quietly dropped:** an xattr
**cannot be overwritten in place through any portable API**, so "shredding" one is delete-and-hope, and
building untestable destruction logic for two platforms was judged worse than a declared gap. **If this
ticket closes it, say what "shredded" means for an xattr and how it is verified; if it does not, keep the
declaration and say why at the site.**

## What this needs

- [ ] **Reproduce first, asserting on BYTES, on a file and a directory.** `assert!(result.is_ok())` is the
      assertion that already passes today with the plaintext intact — **a skip returns `Ok`.** That is the
      single most expensive lesson of CPE-1957 and CPE-1986; do not re-learn it.
- [ ] **Decide what an unshreddable stream/xattr should do, and say so at the site.** CPE-1986 chose
      **refuse** (retryable, **before any unlink**), matching what a busy default stream already does — but
      weigh it here rather than inheriting it: **over-refusing a user-invoked Shred is a different cost from
      over-refusing a vault lock**, and CPE-1957 established that the cost function is what decides these.
- [ ] **Enumerate the callers rather than recalling them** (CPE-1932), and **report a verdict per site
      including the ones that are fine.** CPE-1986's worker did exactly this for the vault path and found
      `shred_through`'s two callers already covered.
- [ ] **Run the CPE-1929 pair on any refusal added, and write both numbers at the site**, naming the
      platform. Note the rule this crew paid for twice: **`if true ||` is the wrong instrument when the
      predicate is also consulted on the common path** — there the measurement that answers reachability is
      a third sabotage, *disable everything upstream and read who reports the failure*.
- [ ] **Check the platform axis before pushing** (CPE-1988). PR #1103 went red on CI **after** an approval
      and a security pass because a `#[cfg(windows)]` item was called from ungated code — three careful
      parties, all on Windows, all green. A real `cargo check --target x86_64-unknown-linux-gnu` is **not
      possible on this machine** (five transitive C deps need `x86_64-linux-gnu-gcc`); CPE-1986's worker
      instead **flipped its `#[cfg(windows)]` attributes to `#[cfg(any())]` and its `not(windows)` arm to
      `not(any())`, selecting the non-Windows arm on Windows**, and got a clean clippy. **Do that, and say
      plainly that "clippy clean" means Windows only.**
- [ ] **Update the user-facing docs.** A Shred that leaves data behind is a promise the UI is currently
      making and not keeping; whatever is decided, `src/docs/` should stop being silent about it. Write any
      remaining gap as **"at least these"**, never a closed list.

## Notes

Filed 2026-08-28 by the sprint Foreman from CPE-1986's worker (PR #1106), which measured the residual,
stated it at the site, and correctly declined to widen a vault fix into the user-facing delete path.

Related: **CPE-1986** (PR #1106 — the vault half, the `FindFirstStreamW` mechanism, the stream-identity
measurements and the refusal decision), **CPE-1957** (the reparse-point half of the same
skip-returns-`Ok` family, and the wipe-vs-skip cost argument), **CPE-1988** (the cfg-intersection sweep),
**CPE-1929** (sabotage pairs, and when the pair is the wrong instrument), **CPE-1932** (enumerate, don't
recall).
