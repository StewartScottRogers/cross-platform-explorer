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
  `S3Provider`. `conn.host` is the bucket (`location.rs` already parses `s3://bucket/key` that way — check
  it, do not re-derive it), `conn.path` the prefix.
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
- [ ] `cargo test` green across `crates/s3`, `crates/vfs`, `crates/server`; `cargo clippy --all-targets
      -D warnings` clean in both feature modes; any `Cargo.lock` delta committed, **including
      `src-tauri/Cargo.lock`** if the app pulls the new crate.

## Notes

Filed by the sprint PM at the CPE-1503 activation, 2026-08-12. Prereqs: **CPE-1683** and **CPE-1684** (there
must be a provider to route to).

`crates/vfs/src/lib.rs` and `crates/vfs/src/connect.rs` are the two files; **CPE-1514** (the FTP scheme arm)
is the worked example to copy.
