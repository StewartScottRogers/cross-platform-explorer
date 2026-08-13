---
id: CPE-1685
title: "Route s3 through cpe_vfs::open — AccessKey credentials from the keychain, and the missing-secret guard"
type: Feature
status: Backlog
priority: Medium
component: Backend
tags: [needs-prereq]
epic: CPE-1503
estimate: S
created: 2026-08-12
closed:
---

## What

Close the hole the codebase has been holding open for this epic. Today `cpe_vfs::open` ends with:

```rust
other => Err(format!("vfs: unsupported scheme '{other}'")),
```

and all three shipped providers answer `AuthMethod::AccessKey` with *"reserved for a future S3/cloud
provider"*. This ticket makes `s3` a real arm and `AccessKey` a real credential, so an `s3://bucket/prefix`
connection browses through the same CPE-1511 command routing SFTP, WebDAV and FTP already use — with no
new commands and no new frontend plumbing.

## Scope

- An `s3` arm in `cpe_vfs::open` building an `S3Config` from the `Connection` and handing back a boxed
  `S3Provider`. ~~`conn.host` is the bucket (`location.rs` already parses `s3://bucket/key` that way —
  check it, do not re-derive it), `conn.path` the prefix.~~ **⚠ SUPERSEDED by CPE-1686 — do not build
  this.** `host` is the **endpoint**, `user` the **region**, and the **bucket is the first segment of
  `path`**. "Host is the bucket" leaves no field for the endpoint or the region, making custom endpoints
  (MinIO/B2/Wasabi/GCS) inexpressible. Full convention and the reasoning: **"Handed over from CPE-1686"**
  below — read that section before writing any of this ticket.
- `AuthMethod::AccessKey { id, secret_ref }` → the access key id and, from the keychain, the secret. Note
  that `secret_ref` is currently **declared and used nowhere** — this is its first consumer, so decide
  deliberately how it relates to the existing `secret_for(access, &conn.name)` lookup rather than
  inheriting a convention that does not exist yet, and write that decision down.
- **The missing-secret guard.** `connected_provider` already refuses a `Password` connection with no stored
  secret rather than attempting a blank-password connect, with the comment saying exactly why. `AccessKey`
  needs the same treatment: signing with an empty secret produces a valid-looking request that fails with
  `SignatureDoesNotMatch`, which sends the user hunting for a clock-skew or policy problem that does not
  exist. Refuse it up front with a message that says the secret is missing.
- Endpoint and region: where do they come from for a saved connection? AWS can be inferred from the
  region, but MinIO/B2/Wasabi cannot. Resolve this — a `path` convention, a new optional field, or a
  documented default — and state the answer in the ticket. CPE-1686 needs to know what the form collects.
- `default_port("s3")` in `connections.rs` (it currently falls through to `0`).
- The three sibling providers' `AccessKey` error messages say "reserved for a **future** S3/cloud provider".
  It is no longer future — reword them.

## Acceptance criteria

- [ ] A test proves `cpe_vfs::open` dispatches an `s3://` connection to `cpe-s3`, mirroring the existing
      per-scheme dispatch tests in `crates/vfs/src/lib.rs`.
- [ ] The `unsupported scheme 's3'` test in `crates/vfs/src/lib.rs` is replaced — not deleted — by one
      asserting the new behaviour.
- [ ] An `AccessKey` connection with no stored secret fails with a message naming the missing secret, and
      **no HTTP request is issued** — asserted, so a future refactor cannot quietly reintroduce a
      blank-secret connect attempt.
- [ ] The secret never appears in an error message, a log line, or a `Debug` output.
- [ ] `default_port("s3")` returns a sensible value and the connection's derived location string matches
      what `src/lib/network.ts`'s `DEFAULT_PORTS` will produce (CPE-1686 keeps the two in sync — that
      mirroring is deliberate and already documented in `network.ts`).
- [ ] `default_port("s3")` returns **443**, pinned by a Rust test — the TS side asserts the same literal
      (`network.test.ts`), but nothing pins the Rust side today, so the two are currently in *silent*
      disagreement (`0` vs `443`) with the whole suite green. That is exactly the drift the mirror exists
      to stop, and only a test on this side closes it.
- [ ] The transitional paragraph in `src/docs/31-network.md` ("saving works now; the provider ships
      alongside it") is **deleted**, and the "Honest limits" list plus the `## Limits` section are flipped
      from future tense to present. See the handover note below for why both halves must happen together.
      *(This is an AC, not a footnote, because the prose version of it sat below the checklist and a worker
      who works the checkboxes would have shipped a lie.)*
- [ ] `cargo test` green across `crates/s3`, `crates/vfs`, `crates/server`; `cargo clippy --all-targets
      -D warnings` clean in both feature modes; any `Cargo.lock` delta committed, **including
      `src-tauri/Cargo.lock`** if the app pulls the new crate.

## Handed over from CPE-1686 (frontend landed first, 2026-08-12) — read before deciding

CPE-1686 shipped the savable `s3` scheme and had to settle the endpoint/region shape to do it. Its answers,
now live in `src/lib/network.ts` (see `schemeFieldHints`'s doc comment) and asserted by `network.test.ts`:

- **`host` = the endpoint host** (`s3.us-east-1.amazonaws.com`, `minio.lan`), **`port` = the endpoint port**,
  **`user` = the region** (blank ⇒ `us-east-1`), **`path` = `/bucket[/prefix]`** with the bucket as the first
  segment. This *contradicts this ticket's parenthetical* "`conn.host` is the bucket": that reading leaves a
  custom endpoint inexpressible, which would break the epic's "B2/GCS/Wasabi/MinIO come free" claim, since
  those endpoints cannot be derived from a region. `location.rs` still parses the result unchanged —
  `s3://us-east-1@minio.lan:9000/my-bucket/prefix` splits the same way it always did. No new `Connection`
  field was needed, and none was added.
- **`secret_ref` = the connection's `name`** — the same key `secret_for(access, &conn.name)` already uses. It
  is a label naming the keychain entry, never the secret. That is what the form now writes.
- **`default_port("s3")` must be `443`** — `network.ts`'s `DEFAULT_PORTS.s3` is 443 and `network.test.ts`
  asserts that literal, so anything else here reintroduces exactly the silent drift the mirror exists to stop.
- The frontend already refuses to connect an `AccessKey` connection with no stored secret
  (`secretAlwaysRequired`), prompting for "Secret access key" first — the backend guard this ticket adds is
  the second line of defence, not the only one.
- `src/docs/31-network.md` carries a transitional paragraph ("saving works now; the provider ships alongside
  it"). **Delete that paragraph as part of this ticket** — once `s3` routes, it becomes a lie.
- **And flip the tense with it.** The "Honest limits of object storage" list and the `## Limits` section
  are written in the **future** tense ("there will be no rename", "listing will be paged") because they
  describe the provider, not the form. The UAT on PR #866 caught them originally written in the present
  tense, which told a reader that S3 browsing worked with caveats when no S3 code path existed at all —
  and the `## Limits` copy repeated it out of sight of the hedge, where a reader skimming from the bottom
  would never see the qualification. Once the provider lands, those sentences become true and should read
  in the present tense. **Do both edits together**: deleting the hedge while leaving the future tense, or
  flipping the tense while leaving the hedge, each produces a page that contradicts itself.

## BLOCKED-BEHIND CPE-1704 — do not land this before it

Added by the Foreman from the PR #888 (CPE-1683) UAT, 2026-08-13.

`S3Provider::list` currently reuses `crates/server`'s `is_safe_name`, the traversal guard written for local
paths, SFTP and WebDAV. The security property holds — nothing can escape the listed prefix — but it imports
filesystem rules into a keyspace that does not have them, and it does so **silently**:

- A key containing `:` is **dropped from the listing with no error**. That rule exists for Windows
  drive-letters and NTFS alternate data streams; `:` is a perfectly legal S3 key character. The object is
  in the bucket, the explorer never shows it, and nothing says why.
- A key with a literal `../` segment becomes a **phantom empty folder** and the object is unreachable.

**This ticket is the one that makes those user-visible.** Nothing can hit them today because `crates/s3` is
not wired into the app; the moment `s3` routes through `cpe_vfs::open`, a connected bucket can contain files
the explorer silently hides. That is not an acceptable first impression of S3 support.

So: **land CPE-1704 first.** If you pick this up and CPE-1704 is still open, either do that one first or say
plainly in the PR that you are shipping a known silent-hide — do not land it quietly.

## Notes

Filed by the sprint PM at the CPE-1503 activation, 2026-08-12. Prereqs: **CPE-1683** and **CPE-1684** (there
must be a provider to route to).

`crates/vfs/src/lib.rs` and `crates/vfs/src/connect.rs` are the two files; **CPE-1514** (the FTP scheme arm)
is the worked example to copy.
