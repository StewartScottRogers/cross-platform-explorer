---
id: CPE-1778
title: Nothing pins the WebDAV rig's MKCOL honesty, and StrictMkdirProvider's ENOTDIR branch is untested
type: test
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-18
closed:
---

## Problem

Two residuals from the PR #931 (CPE-1741) re-review, both about test doubles rather than product code.

### 1. The WebDAV rig's honesty is unpinned

CPE-1741 fixed the `cpe-webdav` test rig's `MKCOL` handler, which used `std::fs::create_dir_all` — quietly
succeeding on an existing collection and inventing missing parents, neither of which real WebDAV does. It
now returns **405** for an existing resource and **409** for a missing intermediate collection per
RFC 4918 §9.3.1. That was the same forgiving-double defect CPE-1731 fixed for FTP's `MKD` and SFTP's
`MKDIR`, surviving in the third protocol.

Measured by the re-review: **reverting the rig back to `create_dir_all` leaves all 32 `cpe-webdav` tests
green**, including the two new CPE-1741 regression tests. Nothing in the suite depends on the rig telling
the truth.

The reason is benign and worth stating, because it is not a product defect: `ensure_dir` `stat`s before it
ever calls `mkdir`, and the in-walk path recovers from *any* `mkdir` failure, honest or not. The production
code is robust to both rigs. The re-review separately confirmed the honest rig **does** catch the original
naive bug — reverting `transfer.rs` to the pre-CPE-1741 bare `mkdir(&base)?` turns both new tests red with
real 405/409 responses. So the rig fix is not worthless; it simply has no guard of its own.

**Why that matters anyway:** an unpinned rig drifts back. This exact class has now been fixed three times —
FTP `MKD`, SFTP `MKDIR`, and now WebDAV `MKCOL` — each time only after a real bug hid behind the
forgiveness for months. A rig with no test asserting its own semantics is one refactor from being lenient
again, and the next bug it hides will cost what the last three did.

### 2. `StrictMkdirProvider`'s `ENOTDIR` branch has no test

CPE-1741 added an `ENOTDIR` arm to `StrictMkdirProvider` (`crates/server/src/transfer.rs`) so the double
models a parent that exists as a file. **No test reaches it** — the existing "base is a file" test is caught
earlier by `ensure_dir`'s own top-level check, never getting as far as this branch. Untested code in a test
double is the same hazard as an unpinned rig: it looks like coverage and is not.

## What to do

- Add a test that asserts the **rig's own behaviour**, not a transfer outcome: `MKCOL` on an existing
  collection returns 405; `MKCOL` with a missing intermediate collection returns 409; `MKCOL` on a fresh
  path with an existing parent returns 201. Assert the status codes directly. Reverting the rig to
  `create_dir_all` must red it.
- Do the same for the FTP and SFTP rigs if they lack equivalent self-tests — check before assuming. The
  point is a standing guard on every double that models a non-idempotent verb, not a one-off for WebDAV.
- Reach `StrictMkdirProvider`'s `ENOTDIR` arm with a case that bypasses `ensure_dir`'s top-level check —
  a parent that is a file, several levels down — or, if it is genuinely unreachable through
  `upload_tree`, say so and delete the arm rather than leaving unreachable code in a double.
- Consider recording the general rule somewhere durable: **a test double that models a protocol verb must
  have a test asserting it matches the wire.** `Ticketing/wiki.md`'s Evidence Rules is where CPE-1743 put
  its equivalent lesson.

## Acceptance criteria

- [ ] Reverting the WebDAV rig's MKCOL to `create_dir_all` reds a distinct test naming the semantics it
      broke. Demonstrate with real output.
- [ ] 405, 409 and 201 are each asserted directly against the rig.
- [ ] The FTP and SFTP rigs are checked for the same gap; whatever is found is recorded, including "they
      already have one".
- [ ] `StrictMkdirProvider`'s `ENOTDIR` arm is either exercised by a test or removed as unreachable, with
      the reasoning recorded.
- [ ] No production code changes — this ticket is entirely about the doubles.

## Notes

From the re-review of **PR #931 / CPE-1741**, 2026-08-18, during the batched sprint. #931 merged on the
strength of its independently re-measured performance fix (32 → 12 round trips on a fresh tree) and its
regression tests, which do red on the real bug. Related: CPE-1741, CPE-1731 (which made the FTP/SFTP rigs
honest and thereby exposed CPE-1741), CPE-1742 (the FTP rig's STOR invents parents), CPE-1743 (the
Evidence Rules precedent).
