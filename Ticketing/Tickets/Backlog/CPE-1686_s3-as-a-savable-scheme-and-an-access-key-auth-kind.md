---
id: CPE-1686
title: "Frontend: s3 as a savable scheme and an access-key auth kind in the connection form"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1503
estimate: M
created: 2026-08-12
closed:
---

## What

Let a user actually save an S3 connection. Today `src/lib/network.ts` declares
`SUPPORTED_SCHEMES = ["sftp", "webdav", "smb", "ftp"]` and `NetworkConnectionForm.svelte` offers exactly two
auth radios — `password` and `key`. Neither fits an object store, so however good the backend gets, there is
no way to enter a bucket.

## Why this is independent of the whole backend chain

`network.ts` is pure, and `src/lib/network.test.ts` already tests it headlessly under vitest. The scheme
list, the default port, the URI derivation and the form's validation can all be built and proven before a
single S3 request exists — and the file's own comment says as much: *"when a provider lands … and joins
`SUPPORTED_SCHEMES`, every gate built on this helper opens automatically — no per-scheme changes."* This is
that landing. It is pickable now, in parallel with CPE-1681.

## Scope

- Add `s3` to `SUPPORTED_SCHEMES` and to `DEFAULT_PORTS`. That constant is a deliberate hand-mirror of
  Rust's `cpe_server::connections::default_port` — its comment says so — so it must match whatever
  CPE-1685 sets there. If the two disagree, a saved profile's location string disagrees with the backend's,
  which is a silent-drift bug, not a cosmetic one.
- Widen the form's `authKind` union to carry an access-key kind, and collect the **access key id** (not
  secret, stored in the profile) separately from the **secret access key** (goes to the keychain, never to
  the profile). The existing password/key radios must be unaffected.
- The unsupported-protocol message is currently hard-coded: *"choose sftp, webdav, smb, or ftp."* Adding a
  scheme must not leave that string stale — derive it from `SUPPORTED_SCHEMES` so the next protocol cannot
  reintroduce the same drift.
- Whatever endpoint/region convention CPE-1685 settles, the form collects it. Coordinate: if CPE-1685 has
  not landed, pick the shape, state it here, and hand it to that ticket rather than guessing twice.
- The scheme `<select>` and any new fields follow the existing form's styling — theme variables only, no
  hard-coded colours. If new pills or chips appear, they reflow per the standing tick-tack rule.
- **Docs.** `src/docs/31-network.md` exists — extend it with S3, including the honest limits users will hit
  first: there is no rename, and directories are a naming convention rather than real objects. This adds no
  new `Section`, so `sectionDocs.ts` needs no new entry — confirm that rather than assuming it.

## Acceptance criteria

- [ ] `isSavableScheme("s3")` is true and `buildConnection` accepts an `s3` profile, with vitest coverage in
      `src/lib/network.test.ts` alongside the existing per-scheme cases.
- [ ] `connectionUri` renders an `s3://` profile correctly, and `DEFAULT_PORTS.s3` equals what Rust's
      `default_port` returns — asserted against the literal value, so a change on either side breaks a test.
- [ ] The rejection message for an unsupported protocol is derived from `SUPPORTED_SCHEMES`; adding a
      seventh scheme requires no edit to that string, and a test proves it.
- [ ] The form collects an access key id and a secret separately, and the secret is never written into the
      saved profile — a test asserts the built `Connection` carries no secret material.
- [ ] Password and key auth still build the same `Connection` they build today — the existing tests pass
      untouched.
- [ ] `31-network.md` documents S3 including the no-rename and virtual-directory limits.
- [ ] `npm run check` and the vitest suite are green.

## Notes

Filed by the sprint PM at the CPE-1503 activation, 2026-08-12. No prereq — the pure frontend model can land
before any backend. The one coupling is the endpoint/region field shape shared with **CPE-1685**; settle it
in whichever lands first.

`src/lib/network.ts`, `src/lib/network.test.ts`, `src/lib/components/NetworkConnectionForm.svelte`,
`src/docs/31-network.md`.
