---
id: CPE-1916
title: `apt-get update` fails a whole CI job when a third-party repo 403s — repos we never install anything from
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-26
---

## Summary

The `MSRV (1.88.0) compiles on every manifest` job failed after 20 seconds on PR #1035, with nothing
to do with the code under test:

    E: Failed to fetch https://packages.microsoft.com/repos/azure-cli/dists/noble/InRelease  403  Forbidden
    E: Failed to fetch https://packages.microsoft.com/ubuntu/24.04/prod/dists/noble/InRelease  403  Forbidden
    ##[error]Process completed with exit code 100.

Both failing repos are **`packages.microsoft.com`** — the azure-cli and Microsoft-prod feeds that ship
preinstalled on GitHub's `ubuntu-latest` image. This project installs **nothing** from either. The job
needs `dbus-1` and `glib-2.0` from Ubuntu's own archives, and it never got to ask for them, because
`apt-get update` returns a hard failure when *any* configured source fails.

So a transient outage on a Microsoft feed we do not use fails our build, on a job whose purpose is to
check that Rust compiles.

**This is not a regression of CPE-1787 — it is that ticket working correctly.** CPE-1787 hardened five
unhardened `apt-get` sites that could silently hang. This one failed **loudly and fast**, which is the
improvement. The remaining question is different: it should not fail *at all* for a repo we do not need.

## Acceptance criteria

- [ ] Stop a third-party source's failure from failing the job. Options, in rough order of preference:
      scope `apt-get update` to the sources we actually use; remove the unused Microsoft lists before
      updating (a common, well-understood one-liner on GitHub runners); or tolerate per-source failures
      while still failing hard if a package we genuinely need cannot be found.
- [ ] Whatever you choose, **a missing package we do need must still fail loudly.** The value CPE-1787
      added was that these steps stop hanging and start reporting; do not trade that away for quiet.
      Red-proof it: ask for a package that does not exist and confirm the job still fails clearly.
- [ ] Apply the fix at every `apt-get update` site, not only the MSRV job's. CPE-1787 found five; check
      whether they all carry this exposure.
- [ ] Record which sources the runner image ships preinstalled and which this project actually needs.
      That list is the justification for the fix and the thing a future reader will want.

## Notes

Filed 2026-08-26 by the sprint Foreman after the failure above. This is the **fourth** distinct
environmental CI-flake class observed in a single day, alongside:

- **CPE-1910** — GUI-smoke shard 2 dying on a WebDriver socket failure (three occurrences).
- **CPE-1914** — the layout guard's 15 s CDP timeout being too tight for a cold navigate on a loaded
  machine.
- The GitHub Actions outage itself (a database failure on their side, several hours, unfixable here).

Each is small on its own; together they are the reason a merge queue needs manual re-runs, and on an
unattended overnight run each one is a stall rather than an inconvenience. Worth someone looking at the
set rather than only at the members — the common shape is *an external dependency's bad day failing a
job that did not need it*.

Related: **CPE-1787** (the apt-get hardening that made this fail fast instead of hanging — read it
first), **CPE-1910**, **CPE-1914**.
