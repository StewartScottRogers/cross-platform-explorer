---
id: CPE-1986
title: 'Security: the vault wipe never overwrites an **alternate data stream**, so the lock reports success over retained plaintext'
type: bug
priority: High
status: Open
tags: ready
estimate: M
created: 2026-08-28
---

## Summary

Found by **PR #1101**'s Security Auditor (CPE-1957) while confirming that ticket's fix. **It is the same
failure mode CPE-1957 just closed, one layer down**, and it is pre-existing — the Auditor measured it as
such and correctly declined to widen that PR.

`crates/server/src/vault_manager.rs:1846` enumerates the session tree with `std::fs::read_dir`, which
returns **names only**. `shred_through` then overwrites **the default data stream**. So an **alternate data
stream** on a session file is never written, and `remove_dir_all` afterwards unlinks the file and **leaves
the stream's extents on the volume.**

Measured by the Auditor under the **production** alias policy
(`AliasPolicy::UnlinkAliasesInsteadOfOverwriting`, the one `wipe_session_dir:1265` uses):

```
P2 ADS: created=true  wipe_ok=true  main_all_zero=true  ads_readable=true  ads_still_secret=true
```

**`wipe_ok=true` with `ads_still_secret=true` is the whole ticket:** the lock reports success, the main
stream is genuinely zeroed, and the secret is still on disk.

**It is undocumented.** Neither `vault_manager.rs` nor `docs/design/VAULT-SECURITY.md` mentions streams at
all — so this is not a declared residual, it is an unstated one.

## Why it matters more than it looks

CPE-1957's defect and this one share the shape that makes them dangerous: **a skip is indistinguishable
from a success at the API.** Every existing assertion on that path is satisfied by not touching the data.
That is why CPE-1957's test had to assert on **bytes**, and why this one must too — `assert!(result.is_ok())`
is exactly the assertion that already passes today with the plaintext intact.

Streams are also **trivially plantable by an unprivileged process** (`type secret > file.txt:hidden`, or
any `CreateFileW` on `name:stream`), and they survive a copy onto NTFS. The same Auditor established in the
same review that Microsoft's non-surrogate reparse tags are also unprivileged-plantable — so "only cloud
sync does this" is not a safe assumption for either mechanism.

## What this needs

- [ ] **Reproduce first, under the production policy, asserting on BYTES.** `wipe_session_dir` passes
      `UnlinkAliasesInsteadOfOverwriting`, **not** `ShredEveryFile` — CPE-1957's own new test used the
      latter and the Auditor had to re-run the production one by hand to confirm the fix held. Do not
      repeat that: test the policy the app actually uses, and say which you ran.
- [ ] **Enumerate streams with `FindFirstStreamW` / `FindNextStreamW`** and shred each, or state the
      residual at the site with its reason. **Do not leave it silent** — an unstated residual in a wipe
      path is the defect, not the missing feature.
- [ ] **Decide what an unshreddable stream should do**, and say so at the site: refuse the lock (loud,
      matching how a file held by another process already behaves — a refusal, retryable, not a skip), or
      report it. **Never skip silently.** CPE-1957 established that over-refusing at a *wipe* costs
      retained plaintext, so weigh a refusal against a partial wipe deliberately rather than defaulting.
- [ ] **Check the sibling paths, don't recall them** (CPE-1932). `shred_tree` has two callers —
      `wipe_session_dir:1265` and `create_vault:264` (`ShredEveryFile`, a user-picked folder) — and
      `shred_through` may have others. Report a verdict per call site, including the ones that are fine.
- [ ] **Cross-platform:** streams are an NTFS concept. Say what the Unix arm does and whether anything
      analogous exists there (resource forks / xattrs on macOS are the obvious question). A guard that is
      Windows-only must say so at the site — this shift found a CPE-1929 pair that was **split across
      platforms**, green on one and red on the other.
- [ ] **Run the CPE-1929 sabotage pair on any refusal you add** and **write both numbers at the site**,
      naming the platform. Note CPE-1957's lesson that `if true ||` is the **wrong instrument** when the
      predicate is also consulted on the common path — there, the measurement that answered reachability
      was a third sabotage nobody prescribed (disable everything upstream and see who reports the failure).
- [ ] **Update `docs/design/VAULT-SECURITY.md`** either way. Whatever is decided, the document should stop
      being silent about streams.

## Notes

Filed 2026-08-28 by the sprint Foreman from PR #1101's Security Auditor (CPE-1957), which measured it while
verifying a different fix and flagged it as pre-existing rather than folding it in.

Related: **CPE-1957** (PR #1101 — the reparse-point half of this same shape, the two guards that hid each
other, and the bytes-not-`Ok` test discipline), **CPE-1929** (shadowed guards, and when the two sabotages
are the wrong instrument), **CPE-1959** (the `fsutil`-writes / `batch_media`-refuses split — still open),
**CPE-1932** (enumerate, don't recall), **SEC-847** (hardlinks are unlink-only under the production policy —
the existing, *declared* residual this one should be written to match in honesty).
