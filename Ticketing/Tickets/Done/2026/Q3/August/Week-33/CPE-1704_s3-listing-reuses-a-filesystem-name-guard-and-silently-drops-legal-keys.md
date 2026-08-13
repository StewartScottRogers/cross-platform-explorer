---
id: CPE-1704
title: S3 listing reuses a filesystem name guard, so a legal S3 key silently vanishes from the explorer
type: bug
priority: High
status: Done
tags: ready
estimate: M
created: 2026-08-13
closed: 2026-08-13
---

## Problem

Found by the independent UAT on PR #888 (CPE-1683), which drove `S3Provider::list` with keys that are legal
in S3 but awkward for a filesystem.

`S3Provider::list` reuses `crates/server`'s `is_safe_name` â€” the traversal guard written for local paths,
SFTP and WebDAV (CPE-1461). It is correctly conservative about escaping a prefix, and **the security
property holds: no key can produce an entry outside the listed prefix.** That is verified and not in
question here.

The problem is that it imports filesystem semantics into a keyspace that does not have them, and the
failure mode is silent.

### 1. A key containing `:` disappears with no error

`is_safe_name` rejects any leaf containing `:` â€” a Windows drive-letter / NTFS alternate-data-stream
hardening rule. **S3 has no ADS concept and `:` is a completely legal key character.**

So an object named `colon:name.txt`, sitting in the bucket, is **absent from the listing** with no error, no
warning, and no indication anything was filtered. From the user's side that is indistinguishable from data
loss: the file is there, they cannot see it, and nothing says why.

### 2. A key containing a literal `../` segment becomes a phantom empty folder

`a/../b.txt` is a **real, distinct key** in S3 â€” not a path to normalise. Today it produces a
seemingly-empty virtual directory `a/` at the root; descending into it shows nothing, and `b.txt` is
unreachable anywhere in the tree.

The guard is doing its job (the deeper leaf is literally `..`, correctly refused, so nothing escapes) â€” but
the user-facing result is a legitimate object vanishing behind what looks like an ordinary empty folder.

Both are the failure the CPE-1683 UAT brief named explicitly as the one to flag: **a legitimate key that
vanishes is worse than an ugly one that shows up.**

## Why this is High but not yet user-visible

`crates/s3` is not wired into the app yet â€” **CPE-1685** is the ticket that routes `s3` through
`cpe_vfs::open`. So no user can hit this today, which is why PR #888 merged with it open rather than being
blocked.

**It must be fixed before CPE-1685 lands.** A note has been added to that ticket making this a prerequisite.
Shipping a file explorer that silently hides files in a connected bucket is not an acceptable first
impression of S3 support.

## THE GUARD IS AT TWO LAYERS â€” fixing only the provider does nothing

Found by the PR #890 reviewer, 2026-08-13, after a first attempt fixed `S3Provider::list` alone and
achieved nothing observable. **The Foreman's brief caused this** by scoping the work to `crates/s3`.

`crates/vfs/src/connect.rs:244` `remote_dir_entries` re-filters **every** `ProviderEntry` through
`cpe_server::transfer::is_safe_name` before it becomes a `DirEntry`. That is the only path a user's remote
listing takes (`src-tauri/src/lib.rs:7697` `remote_list_dir_impl` â†’ `remote_dir_entries`), and it is
exactly what **CPE-1685** will route `s3` through. Measured:

```
| "colon:name.txt" | S3 guard ACCEPT | vfs is_safe_name REFUSE |
| "x:y"            | S3 guard ACCEPT | vfs is_safe_name REFUSE |
| "..evil.txt"     | S3 guard ACCEPT | vfs is_safe_name REFUSE |
```

So relaxing the provider's guard alone leaves the file just as invisible. **The fix must reach
`remote_dir_entries`** â€” a `ProviderCapabilities` flag or a per-provider leaf predicate â€” so it asks the
provider which rules apply instead of imposing filesystem rules on every backend.

`crates/server`'s `is_safe_name` still must **not** be loosened: SFTP and WebDAV need the `:` rule.

## Do NOT add a percent-decode pass

The first attempt added one to catch `..%2f`. It refused **nine** classes of legal key, including two that
are common in real buckets:

| leaf | what it is |
|---|---|
| `report%2ffinal.txt` | literal text â€” the ticket's own bug, reintroduced |
| `https%3A%2F%2Fexample.com%2Findex.html` | URL-keyed archive object |
| `city=A%2FB` | Hive/Athena/Glue partition value â€” the tooling encodes `/` as `%2F` |
| `%2e%2e`, `%2e`, `%00`, `%0a`, `%5cfoo` | literal escape text |

**And it protects nothing.** `sigv4::encode_query_component` escapes `%`, so a leaf reaches the wire as
inert literal text (measured: `..%2f` â†’ `prefix=photos%2F..%252f%2F`). S3 prefix matching is byte-literal
and does not normalise `..`. And `ListObjectsV2` returns raw key text unless `encoding-type=url` is
requested, which this crate never requests â€” so percent-decoding a `<Key>` is a **category error**. A
decode-once guard does not even stop a double-decoding consumer: `%252e%252e%252f` sails through.

## A synthetic "N keys hidden" row is not the answer either

The first attempt appended one. The reviewer judged it **worse than the silent drop**:

- **Spoofable.** A real object can be named exactly like the marker (measured: accepted, emitted as a
  normal 7-byte file). Because the genuine marker contains a `/` and is itself refused by `is_safe_name`,
  **the only such row a user could ever see is an attacker-planted one.**
- **Dishonest.** `is_dir: false, size: 0` â€” it claims to be a zero-byte file.
- **Delete reports success.** S3 `DELETE` of a missing key returns **204**, so deleting it would say it
  worked and it would still be there on refresh.
- Off-by-one in item counts, included in select-all, and it slips past `MAX_LIST_ENTRIES`.

Since the fix has to reach `crates/vfs` anyway, carrying a filtered-count on a real field â€” or returning an
error when a listing is entirely filtered â€” is available and honest. Pick deliberately and record why.

## Scope

`crates/s3/src/provider.rs`, **and `crates/vfs/src/connect.rs`'s `remote_dir_entries`** (see above â€” fixing only the provider is a no-op). **Do not loosen `crates/server`'s
`is_safe_name`** â€” it guards local paths, SFTP and WebDAV, where the `:` rule is correct and load-bearing.
This needs a sibling that encodes S3's rules, not a weakening of the shared one.

## Acceptance criteria

- [ ] A key containing `:` appears **through `remote_dir_entries`**, not merely out of `S3Provider::list`.
      That distinction is the whole ticket â€” the first attempt satisfied the provider boundary and changed
      nothing a user could see. Pin it with a test that goes through the real path, per Evidence Rule 2.
- [ ] The security property is **unchanged**: no key can produce an entry that escapes the listed prefix.
      Re-run PR #888's own traversal test (`a_content_key_with_a_traversal_segment_or_embedded_slash_is_dropped`)
      plus the UAT's set â€” `%2e%2e/`, a key that is exactly `..`, a leading `/`, a backslash key, an
      embedded NUL, an embedded newline. **Breaking the guard must still turn a distinct test red.**
      Note `..%2f` is now expected to be **accepted** â€” it is a legal key and cannot escape (see above).
- [ ] Decide what happens to a key the guard genuinely must refuse, and make it **not silent**. Options:
      surface it under a visibly-escaped display name, or report that N entries were filtered. Either is
      acceptable; dropping it invisibly is not. Record the choice.
- [ ] Decide what `a/../b.txt` should look like to a user, and write the reasoning down. It is a real key
      and it has to be *reachable* or *visibly explained* â€” a phantom empty folder is neither.
- [ ] `crates/server`'s `is_safe_name` is untouched, or if it is touched, SFTP and WebDAV are re-verified
      against their own traversal tests.
- [ ] **No dead arm.** The reviewer's mutation showed the first attempt's raw-bytes check was provably
      unreachable â€” deleting it left all 121 tests green â€” because the decode pass subsumed it. Whatever
      guard ships, confirm each arm is independently reachable and independently red-able.
- [ ] Each guard broken **on its own** turns a **distinct** test red, real output pasted in the PR, per the
      Evidence Rules in `Ticketing/wiki.md`.

## Notes

Filed by the Foreman from the PR #888 UAT, 2026-08-13. The UAT correctly judged it non-blocking for
CPE-1683 â€” that ticket's AC5 only requires that no entry escapes the listed prefix, which is true â€” and
recommended exactly this follow-up.

Related: **CPE-1683** (which introduced the reuse), **CPE-1685** (which would make it user-visible â€”
blocked on this), **CPE-1461** (the traversal guard being reused), **CPE-1684** (the sibling object-ops
ticket, which will hit the same key-shape questions for stat/read/write).

## Work Log

**Closed 2026-08-13, merged as PR #890 (`775eaa97`).** Four rounds. The recurring failure was not in the
fix â€” it was in *where each round proved the fix*.

### One lesson, learned four times

Every round was verified at a boundary the real caller does not use:

| Round | Fixed at | Why it changed nothing |
|---|---|---|
| 1 | `S3Provider::list` | `crates/vfs::connect::remote_dir_entries` re-filters every entry through `cpe_server::transfer::is_safe_name` â€” the file stayed invisible |
| 2 | provider boundary + a synthetic `âš  N hidden` row | the marker was spoofable, lied about `is_dir`/`size`, and `DELETE` of it would report success (S3 returns 204 on a missing key) |
| 3 | the concrete `S3Provider` type | `list_with_filtered_count` was an **inherent** method; production holds `&dyn FileSystemProvider` and reached the trait default, which hardcodes `0` |
| 4 | the vtable | â€” |

**The Foreman caused rounds 1 and 3.** Round 1's brief said "you own `crates/s3`, stay out of
`crates/server`", which put the fix at the wrong layer. Round 3 was accepted without asking how production
dispatches.

Round 3's bug is the one worth remembering. Rust resolves inherent methods before trait methods, so **all
122 tests and all of CI were green on the broken build** â€” the reviewer confirmed the count matches exactly.
The only artifact in the tree that can see the bug is the test added in round 4.

### What shipped

- `is_safe_s3_leaf`, an S3-appropriate **sibling** of `cpe_server::transfer::is_safe_name` â€” six raw-byte
  checks (empty / `..` / `.` / contains `/` / contains `\` / any control char). `is_safe_name` itself is
  **untouched**; SFTP and WebDAV still need the `:` rule and still double-guard inside their own `list`.
- `is_safe_leaf_name` as a trait method, so `remote_dir_entries` asks the provider which rules apply
  instead of imposing filesystem semantics on every backend.
- An honest `usize` filtered count on `RemoteListing`, computed in-process and not spoofable â€” plus the
  vtable forwarder that makes it reachable through `&dyn`.
- **No percent-decode pass.** The round-1 attempt added one; it refused nine classes of legal key,
  including `city=A%2FB` (a Hive/Athena partition value â€” the tooling encodes `/` as `%2F`) and
  `https%3A%2F%2Fâ€¦` URL-keyed objects. It also protected nothing: `sigv4::encode_query_component` escapes
  `%`, so `..%2f` reaches the wire as `prefix=photos%2F..%252f%2F` â€” inert literal text. Measured, not
  argued.

### Evidence

Every guard broken on its own reds a distinct test. The reviewer's independent mutation pass found two
arms (`""` and `"."`) that redded **nothing** â€” genuinely dead while the parser was the only caller, but
live once `is_safe_leaf_name` became a public trait method called on every name. Both are now pinned.

A near-miss worth recording: the first attempt to mutate the empty-leaf arm hit
`parse_list_bucket_result`'s separate `if leaf.is_empty() { continue }` instead of the arm in the `&&`
chain. All 124 tests stayed green and the new assert looked toothless. **Same shape as the headline bug** â€”
two things that read as the same check, only one of which the caller reaches. The test's doc comment now
says which is which.

I also reverted my own uncommitted test with `git checkout --` mid-probe, for the third time this sprint.
Commit before you probe.

### Completeness, established rather than assumed

Asked whether `list_with_filtered_count` was the only method with the shadowing shape, the reviewer
**enumerated all six providers** and gave the structural reason the rest are safe: the seven *required*
trait methods cannot have this bug, because omitting one is a compile error, so there is nothing for an
inherent method to silently shadow. Only `S3Provider::list_with_filtered_count` collided. Nothing to file.

### Deliberately deferred, with tickets

- **CPE-1708** â€” the count reaches an `eprintln!`, not the UI. Real scope cut, disclosed, gates CPE-1685.
- **CPE-1709** â€” downloading a key containing `:` writes a **0-byte file** on Windows; the bytes land in an
  NTFS alternate data stream. Found by the reviewer, measured through the real sink. Same disease one layer
  downstream: the key now lists correctly and then loses its contents. **Not** a security bug â€” `..::$DATA`
  and `..:stream` both fail at `CreateFileW` (os error 123) and nothing escapes the download root. Gates
  CPE-1685.
- Renaming the inherent method to `list_pages_counting_filtered` (deleting the collision class rather than
  pinning it) â€” agreed in principle, declined here to avoid churning ~20 intra-doc-link references on a
  12/13-green PR.

Verdicts: Reviewer **APPROVE + SEC PASS**, UAT **PASS**. All CI green.

