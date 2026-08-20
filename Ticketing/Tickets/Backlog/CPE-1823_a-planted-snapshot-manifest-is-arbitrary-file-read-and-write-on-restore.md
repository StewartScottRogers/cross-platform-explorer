---
id: CPE-1823
title: "Security: a planted snapshot manifest is arbitrary file read and write on restore"
type: bug
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-20
closed:
---

## Problem

`crates/server/src/snapshot_capture.rs:102-108` — `root_relative_to_abs` builds the restore target by
`p.push(part)` for each `/`-split segment of the manifest's stored path, with **no rejection of `..`
and no rejection of an absolute component**. `Path::push` with an absolute component *replaces* the
whole path, so a single crafted segment relocates the write anywhere on the volume; `..` walks up
from the restore root.

`restore` (`:218-224`) uses that function for the **write target**, and `blobs_dir.join(&file.hash)`
for the **read source** — `hash` being another unsanitised manifest field.

So a hand-edited or planted manifest JSON yields **arbitrary file write** (restore writes attacker-chosen
content to an attacker-chosen path) and **arbitrary file read** (the blob source is pulled from an
attacker-chosen path) at the privilege of the app.

## Why it matters

The manifest is *trusted downstream* while being an ordinary on-disk JSON file the user — or anything
running as the user, or anything that can write into the snapshot directory — can edit. A snapshot
directory copied from elsewhere, restored from a shared drive, or synced by a cloud client is enough.
There is no signature, no canonicalisation, and no containment check between reading the manifest and
writing the files it names.

Every other write path in this crate is being hardened right now (CPE-1765 claims the picked name so
a copy cannot land outside the chosen folder). This one bypasses the question entirely by letting the
*input* choose the path.

## Acceptance criteria

- [ ] `root_relative_to_abs` rejects any segment that is `..`, is absolute, contains a drive
      prefix/root component, or is otherwise not a plain single component — returning an error, not a
      silently-sanitised path.
- [ ] After building the target, the result is canonicalised and asserted to be **inside** the restore
      root, so a link planted mid-path cannot redirect the write either. Reuse the containment helper
      the crate already has rather than writing a second one — check `is_self_or_descendant` and the
      `transfer::is_safe_name` family first.
- [ ] `file.hash` is validated as a plain hex blob name before it is joined onto `blobs_dir`, so the
      read source cannot escape either.
- [ ] A restore that hits a rejected entry fails **loudly and per-entry**, naming the offending path —
      it must not silently skip, because a silently-skipped restore entry is a file the user believes
      was restored.
- [ ] Tests stage a genuinely malicious manifest for each shape: `..` traversal, an absolute component,
      a drive-relative component on Windows, a link planted at an interior component, and an escaping
      `hash`. Each asserts **the harm did not happen** (nothing written or read outside the root)
      before asserting the `Result`.
- [ ] Red-proof each test: remove the guard it covers, observe red, revert, record the line.

## Notes

Found 2026-08-20 by the independent Security Auditor while auditing PR #968 (CPE-1765) — it audits
`snapshot_capture::save_manifest`, which CPE-1765 fixed correctly, and answered the "is the manifest
trusted downstream?" question with "yes, and here is why that is a problem". **Pre-existing, not
introduced by CPE-1765.** Filed separately so it is not absorbed into that ticket's scope.
