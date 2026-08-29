---
id: CPE-1986
title: 'Security: the vault wipe never overwrites an **alternate data stream**, so the lock reports success over retained plaintext'
type: bug
priority: High
status: Done
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

- [x] **Reproduce first, under the production policy, asserting on BYTES.** `wipe_session_dir` passes
      `UnlinkAliasesInsteadOfOverwriting`, **not** `ShredEveryFile` — CPE-1957's own new test used the
      latter and the Auditor had to re-run the production one by hand to confirm the fix held. Do not
      repeat that: test the policy the app actually uses, and say which you ran.
- [x] **Enumerate streams with `FindFirstStreamW` / `FindNextStreamW`** and shred each, or state the
      residual at the site with its reason. **Do not leave it silent** — an unstated residual in a wipe
      path is the defect, not the missing feature.
- [x] **Decide what an unshreddable stream should do**, and say so at the site: refuse the lock (loud,
      matching how a file held by another process already behaves — a refusal, retryable, not a skip), or
      report it. **Never skip silently.** CPE-1957 established that over-refusing at a *wipe* costs
      retained plaintext, so weigh a refusal against a partial wipe deliberately rather than defaulting.
- [x] **Check the sibling paths, don't recall them** (CPE-1932). `shred_tree` has two callers —
      `wipe_session_dir:1265` and `create_vault:264` (`ShredEveryFile`, a user-picked folder) — and
      `shred_through` may have others. Report a verdict per call site, including the ones that are fine.
- [x] **Cross-platform:** streams are an NTFS concept. Say what the Unix arm does and whether anything
      analogous exists there (resource forks / xattrs on macOS are the obvious question). A guard that is
      Windows-only must say so at the site — this shift found a CPE-1929 pair that was **split across
      platforms**, green on one and red on the other.
- [x] **Run the CPE-1929 sabotage pair on any refusal you add** and **write both numbers at the site**,
      naming the platform. Note CPE-1957's lesson that `if true ||` is the **wrong instrument** when the
      predicate is also consulted on the common path — there, the measurement that answered reachability
      was a third sabotage nobody prescribed (disable everything upstream and see who reports the failure).
- [x] **Update `docs/design/VAULT-SECURITY.md`** either way. Whatever is decided, the document should stop
      being silent about streams.

## Work Log

**2026-08-28 — fixed.** Reproduced first, on Windows 11, under the **production** policy
`AliasPolicy::UnlinkAliasesInsteadOfOverwriting` (driven through `shred_dir_pinned`, as CPE-1957's test is,
because `wipe_session_dir` removes the tree and leaves nothing to read back):
`wipe_ok=true main_all_zero=true ads_still_secret=true` — the Auditor's reading, confirmed. The same probe
found the **same defect on a directory** (`sub:dirsecret` survived identically), which the ticket did not
name.

Closed by `vault_manager::shred_alternate_streams`: `FindFirstStreamW`/`FindNextStreamW`
(`FindStreamInfoStandard`) enumerate the object's streams and each **named** `$DATA` stream is overwritten
through `overwrite_pinned_file` — the same function, and therefore the same refusals, the default stream
already uses. That reuse is sound because a stream is not a second object: measured, a handle at
`file:name` reports the file's own volume serial + file index, its link count, no directory bit (even on a
directory's stream), no reparse tag, and a `metadata().len()` that is the **stream's** length. Called from
`shred_dir_pinned` for each file **and for the directory itself** — never from inside
`overwrite_pinned_file`, which would recurse. `same_object_or_refuse` now returns the probe it took, so the
directory's streams are pinned to the identity step 2 verified rather than to a fresh re-probe.

Decisions, each stated at the site and in `docs/design/VAULT-SECURITY.md`:
- **An unshreddable stream refuses the whole wipe**, retryable, exactly as a busy default stream already
  does — and before `remove_dir_all`, so what is retained is plaintext still in the session directory,
  visible and retryable, rather than plaintext in extents with no name.
- **An aliased file's streams are left alone** like its default stream (same file record, reachable through
  the other name), asked *before* enumeration so a listing failure cannot refuse over an untouched object.
- **A listing failure splits by alias policy**, exactly as `same_object_or_refuse`'s `Unknown` arm does:
  refused for the session tree, waved through for `create_vault`'s user-picked folder (possibly FAT/exFAT).
  `ERROR_HANDLE_EOF` is "no streams", measured. **Not measured:** no FAT volume was available here.
- **Only `$DATA` streams are shredded** — an unexercised safety valve; `FindStreamInfoStandard` returned
  nothing else in any measurement (EFS-encrypted file: `::$DATA` alone; GUID reparse point: same).

**Call-site verdicts (derived by grep, not recalled).** `shred_tree` has exactly two callers —
`wipe_session_dir` (production policy) and `create_vault`'s shred-original (`ShredEveryFile`); **both are
fixed**, since the fix sits inside `shred_dir_pinned`. `shred_through` has exactly two callers, both inside
`overwrite_pinned_file`, both covered. `secure_shred::shred_open_file` has one further caller,
`secure_shred::shred_file` — the explorer's user-facing **Shred** command — which has the **identical
residual on every platform** and was deliberately **not** widened into (a different feature, whose refusal
behaviour is its own reviewed decision). Stated at that function; **wants its own ticket.**

**Cross-platform.** No Unix arm: the analogue is extended attributes (incl. `com.apple.ResourceFork`), which
has the same property and is **not** closed — an xattr cannot be overwritten in place through any portable
API, so it would buy a weaker guarantee while reading like this one. Declared, not implied.

**Review round 2 (#1106, `SEC PASS` + `CHANGES REQUESTED`) — three comment-only claim-scope fixes, no
code change.**

- **F1 — the "+5" clause was false and was inherited as boilerplate.** All three sabotage figures
  (2,465+1, 2,439+27, 2,464+2) sum to **2,466**, the *shipping* tree's total, because they were measured
  in this tree — so a re-run reproduces them exactly, and the clause telling the reader to expect five
  fewer would have fired the sibling comment's "these are stale, re-run them" instruction on an honest
  re-run. CPE-1957's "+1" clause is genuinely true there (its figures sum to the *pre-fix* total); this
  copied the shape without re-deriving it. Both sites now say the figures were measured in this tree and
  each sums to 2,466. `VAULT-SECURITY.md` never carried the clause but its paragraph now states the same
  thing positively.
- **F2 — "reachable" was the wrong word, and my own test doc said so 3,400 lines away.** The pair proves
  **covered**, not reachable. Ran the third sabotage myself rather than quoting the Reviewer's number:
  `same_object_or_refuse` returning its probe unconditionally + `overwrite_pinned_file`'s write-open
  failure returning `Ok(())` gives **2,460 / 6** with `alternate data streams could not be listed`
  appearing **0 times in the whole run** — independently reproducing the Reviewer's figure. Mechanism:
  for a file the write-open touches the same object first, for a directory the identity probe does. Now
  documented as a **deliberate, unreachable-from-the-walk backstop**, in the shape CPE-1929 requires and
  the surrogate refusal two functions up already uses. Corrected at `vault_manager.rs` **and** in
  `VAULT-SECURITY.md`, which carried the same false word and which the finding did not cite.
- **F3 — "the two residuals" → "at least these residuals"** (CLAUDE.md round-9 rule).

Re-verified after the edits: `cargo clippy --locked --all-targets -- -D warnings` clean in both feature
modes, `cargo test --lib` **2,466 / 0 / 14**.

**Numbers** (Windows 11, `cargo test --lib`, `crates/server`): baseline **2,461 / 0 / 14** at `2f7b3206`,
re-measured **identical** at `9bfb21d7` after rebasing (all three sabotage figures were re-run there and
came back the same, so #1103's 511 lines in `batch_media` moved nothing here);
**2,466 / 0 / 14** in the shipped tree — **five new tests**. Every figure below was measured in the
shipped tree and each sums to 2,466. CPE-1929 pair on the new refusal: disabled **2,465 / 1**, predicate
forced to lie **2,439 / 27** — both red, so it is **covered** (by a direct-call test), not shadowed; the
third sabotage says it is **not reachable from the walk** (**2,460 / 6**, message absent), so it is kept
as a declared backstop. Red-proof of the wiring: both `shred_alternate_streams` calls removed →
**2,464 / 2**.
`cargo clippy --locked --all-targets -- -D warnings` clean in both feature modes (default and `index`).
A real non-Windows `cargo check` is **impossible on this machine** (five transitive C deps need
`x86_64-linux-gnu-gcc`), so the platform axis was derived instead: the ten new `#[cfg(windows)]` attributes
were flipped to `#[cfg(any())]` and the `#[cfg(not(windows))]` one to `#[cfg(not(any()))]` — selecting the
non-Windows arm on Windows — and clippy finished **clean**. That is the check PR #1103 lacked.

## Notes

Filed 2026-08-28 by the sprint Foreman from PR #1101's Security Auditor (CPE-1957), which measured it while
verifying a different fix and flagged it as pre-existing rather than folding it in.

Related: **CPE-1957** (PR #1101 — the reparse-point half of this same shape, the two guards that hid each
other, and the bytes-not-`Ok` test discipline), **CPE-1929** (shadowed guards, and when the two sabotages
are the wrong instrument), **CPE-1959** (the `fsutil`-writes / `batch_media`-refuses split — still open),
**CPE-1932** (enumerate, don't recall), **SEC-847** (hardlinks are unlink-only under the production policy —
the existing, *declared* residual this one should be written to match in honesty).

## Closing record — merged as PR #1106 (`944af51c`), 2026-08-29

### The defect

`vault_manager.rs` enumerated the session tree with `read_dir` — **names only** — and `shred_through`
overwrote **the default data stream**. An **alternate data stream** was never written, then
`remove_dir_all` unlinked the file and **left the stream's extents on the volume**. Measured under the
production policy: `wipe_ok=true  main_all_zero=true  ads_still_secret=true`.

**And the worker found a case the ticket never named: the same defect on a DIRECTORY** (`sub:dirsecret`
survived a full wipe). The Reviewer wrote an **independent** directory-only probe rather than trust it —
pre-fix `still_secret=true all_zero=false`, post-fix `still_secret=false all_zero=true`. Real, and fixed.

**Why it survived: a skip returns `Ok`.** Every assertion on that path was satisfied by not touching the
data — which is why the new test asserts **bytes** (`assert_ne!` on the secret, then `all(|&b| b == 0)`),
never a verdict. Same lesson as CPE-1957, one layer down.

### The fix, and the reuse argument that carries it

`shred_alternate_streams`: `FindFirstStreamW`/`FindNextStreamW` enumerate the object's streams, and each
named `$DATA` stream is overwritten **through `overwrite_pinned_file`** — the same function, and the same
refusals, the default stream already uses.

**That reuse rests on a measurement, independently re-taken on both a file and a directory:**

| | id | links | is_dir | reparse | len |
|---|---|---|---|---|---|
| `f.txt` | vol …789 / idx …559 | 1 | false | false | 21 |
| `f.txt:hidden` | **identical** | 1 | false | false | **61** |
| `d` | idx …560 | One | **true** | – | – |
| `d:dirstream` | **identical** | 1 | **false** | false | **7** |

A stream is not a second object: it reports the file's **own** identity and link count, **no directory bit
even on a directory's stream**, and the **stream's** length. So `overwrite_pinned_file`'s refusals — wrong
object, link, directory, late alias — all apply unchanged, and `shred_through` sizes the stream rather than
the default.

Called from `shred_dir_pinned` for each file **and the directory itself**, never from inside
`overwrite_pinned_file`. `same_object_or_refuse` now **returns its probe**, so a directory's streams pin to
the identity step 2 verified — **which closes a TOCTOU rather than opening one**: on the session policy the
only reachable `Ok` arm carries that verified identity, and re-probing the path would have pinned to
whatever is there *now*.

### Decisions, all stated at the site

An **unshreddable stream refuses the wipe** — retryable, **before any unlink**, matching what a busy
default stream already does. An **aliased file's streams are left alone** like its default stream, asked
**before** enumeration. A **listing failure splits by alias policy** exactly as `same_object_or_refuse`'s
`Unknown` arm does. **Only `$DATA` streams are shredded** — declared an **unexercised** safety valve.

**The Reviewer tried five ways to exercise that valve and failed**: an EFS file (`cipher /e`, attrs
`0x6020`) → `["::$DATA"]`, **no `:$EFS:`**; an object id (`fsutil objectid create`) → `["::$DATA"]`; a
directory with 400 long-named children, i.e. a real `$I30` → `[]`; a junction → `[]`; a sparse file →
`["::$DATA"]`. **The site's claim is accurate.**

### Eight attacks, all fail-closed or correct

The root session directory's own stream: **shredded**. A 255-character stream name: **shredded**. A
**500-character path past MAX_PATH**: **shredded** — so the `verbatim_wide` claim holds. Stream names
containing `\`, `/` or a second `:`: **the OS refuses to create them** (os error 123), and feeding crafted
`:..\..\victim.txt:$DATA` straight into `overwrite_pinned_file` is **refused with the outside victim
byte-identical** — no traversal through the name append. A hard-link alias appearing **after** enumeration
(lying probe): the other name's stream **intact**, caught by the handle-side link re-read. A junction to an
outside directory carrying a stream: **not followed**. `::$DATA` spelled explicitly: **filtered, not
shredded twice**. Unprovable identity (`probe.id = None`): streams skipped — **identical to what
`overwrite_pinned_file` already does to the default stream**, so consistent rather than a new gap. A stream
created *between* enumeration and the loop is still missed — **the pre-existing "entry created after
`read_dir`" race, unchanged for ordinary files.**

### Call sites, re-derived at run time — verdict per site including the fine ones

`wipe_session_dir:1265` (production policy) **fixed**; `create_vault:264` (`ShredEveryFile`) **fixed** — the
fix lives inside `shred_dir_pinned`, which is called from `shred_tree:1827` **and its own recursion at
`:1905`**, so every subdirectory's streams come from its own invocation and the root's from the root
invocation. `shred_through`: exactly two callers, both inside `overwrite_pinned_file` → **covered**.
`secure_shred::shred_file` → the user-facing **Shred** command → **declared residual, correct** (now
**CPE-1989**).

### The platform check PR #1103 lacked

A real `cargo check --target x86_64-unknown-linux-gnu` is **impossible on this machine** — `bzip2-sys`,
`lzma-sys`, `zstd-sys`, `libsqlite3-sys` and `ring` all fail with `ToolNotFound: x86_64-linux-gnu-gcc`, and
**the author said so plainly rather than reporting "clippy clean" as coverage.** Instead it **flipped the
ten new `#[cfg(windows)]` attributes to `#[cfg(any())]` and the one `#[cfg(not(windows))]` to
`#[cfg(not(any()))]`, selecting the non-Windows arm on Windows**, and clippy finished clean. The Reviewer
counted the eleven cfgs **from the diff rather than recalling them**, re-flipped them all, and judged it
**sound within its stated scope**: it proves no ungated caller names a Windows-gated item — *"what #1103
lacked"* — and it compiles the `not(windows)` body; it does not otherwise exercise the Linux target, and
the site says so.

### Two review findings, both about sentences

**F1 — a correction inherited as boilerplate.** The sabotage sites carried *"the figures below read **five**
lower than what you will measure here"* — copied from CPE-1957, where the equivalent "+1" clause is
**genuinely true** because that ticket's figures were taken before its own test landed. **Here every figure
already sums to the shipping tree's 2,466**, so **an honest re-run contradicts the comment** and fires the
neighbouring *"these are stale — re-run, don't adjust"* instruction. Replaced with the derived statement,
and `VAULT-SECURITY.md` gained the positive version even though it never carried the false one — *"stating
the property where the numbers live is the point."*

**F2 — "reachable" was the wrong word.** The pair redding proves the refusal is **covered**, not reachable.
The **third** sabotage — neuter *every* upstream refusal in the walk — gives **2,460 / 6 with the message
appearing zero times in the whole run**, because for a file `overwrite_pinned_file`'s write-open runs first
on the same object, and for a directory the identity probe does. **Covered by a direct-call test, not
reachable from the walk**, now declared as CPE-1929 requires and as the surrogate refusal two functions up
already does — **and the author re-ran that sabotage independently rather than quoting the Reviewer's
number, then named its spelling at the site so it is re-runnable.**

**And unasked: the same false word was in `VAULT-SECURITY.md`.** The finding cited only the Rust site.
*"Fixing one and leaving the other would have left the identical claim standing in the document that exists
to be the honest record."*

**F3** — "the two residuals" → **"at least these residuals"** (CLAUDE.md's round-9 rule).

### Sabotages and gates

CPE-1929 pair on the new refusal: disabled **2,465 / 1**, predicate lying **2,439 / 27** — both red. Wiring
red-proof (both calls removed) **2,464 / 2**. Baselines **2,461 → 2,466**, five new tests, **all five
actually ran** (no skip notice). `cargo clippy --locked --all-targets -D warnings` clean in **both** feature
modes; `cargo test --locked` green; CI `completed success — total_count=26 pending=0 skipped=1 coverage=ok`.

**Declared residuals, both filed rather than left implied:** `secure_shred::shred_file` has the identical
leak **on every platform**, and there is **no Unix arm** — the analogue is extended attributes including
`com.apple.ResourceFork`, **not closed**, because an xattr **cannot be overwritten in place through any
portable API** and building untestable destruction logic for two platforms was judged worse than a stated
gap. Both are **CPE-1989**.

**One residual left open by the Reviewer, non-blocking:** `VAULT-SECURITY.md`'s "all three figures" now has
four figures beneath it — the same shape as F1, one notch smaller, harmless because the next clause carries
the correct scope and all four reproduce in the shipping tree.

**Family:** CPE-1957 (the reparse-point half of the same skip-returns-`Ok` family, and the source of the
"+1" clause this one inherited wrongly), CPE-1989 (the user-facing Shred command and the Unix xattr half),
CPE-1988 (the cfg-intersection sweep), CPE-1929 (sabotage pairs, and when the pair is the wrong
instrument), CPE-1932 (enumerate, don't recall), CPE-1933 (do not name a backstop without checking it can
fire).
