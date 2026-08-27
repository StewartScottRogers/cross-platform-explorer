---
id: CPE-1938
title: tar/7z extraction is blind to an inside-pointing junction, and the `#[cfg(unix)]` permission pass follows links — an archive can chmod (setuid included) outside the root
type: bug
priority: High
status: Open
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

Two residuals from PR #1050's independent Security Audit (CPE-1913). They were deliberately scoped
**out** of that PR — it fixed the zip leg and deliberately left tar/7z alone, for the documented
reason that those two need a third-party unpacker replaced — but both are real and neither is
recorded anywhere else.

## F-A: tar (measured) and 7z (inferred) do not see an inside-pointing junction

CPE-1913 gave the **zip** leg a handle gate. The **tar** and **7z** legs still resolve entry
destinations by **path**, so a junction planted at an entry's name inside the extraction root is
followed and the payload lands wherever it points.

Measured on the tar leg:

    [tar junction->outside]  Ok((done 1, skipped 0, errors []))
    -> the victim file outside the root holds the archive payload

The **7z** case is **inferred from the shared shape, not demonstrated** — the auditor says so
explicitly and this ticket repeats it rather than laundering it into a measurement. **Demonstrate it
before fixing it**, and if it turns out 7z is already safe for a different reason, record why.

## F-B: the `#[cfg(unix)]` permission pass follows links, with an archive-chosen mode

After writing an entry, the extraction loop makes a **path-addressed** `fs::set_permissions` call
under `#[cfg(unix)]`. `set_permissions` **follows symlinks**. So a link swapped in between the write
and the chmod re-targets the mode change at whatever the link points to — outside the root — with a
mode **the archive chooses**, and that mode can include **setuid**.

This is the same race the whole of CPE-1896/CPE-1913 exists to close, in the one call that survived
the conversion because it changes metadata rather than writing bytes. PR #1050 counted it and named
it: **two** path-addressed writes remain in that loop — this one and the symlink-entry branch.

The fix shape already exists in this repo: hold the handle from the write and set the mode through
it (`fchmod`-equivalent) rather than re-addressing by name. `open_beneath.rs` is the seam.

## Acceptance criteria

- [ ] **Demonstrate 7z** before changing it. Tar is measured; do not fix 7z on the strength of the
      analogy alone (this repo's recurring defect is a check that looks stronger than it is).
- [ ] Give the tar leg the same handle gate the zip leg got in CPE-1913, or record concretely why the
      third-party unpacker makes that impossible and what the interim containment is. "Blocked on the
      unpacker" is an acceptable answer **only** if it names the blocker and the mitigation.
- [ ] Convert the `#[cfg(unix)]` permission pass to a **handle-addressed** mode change. It is a
      smaller, self-contained fix than the tar leg and should not wait for it.
- [ ] Cover the **symlink-entry branch** — the second surviving path-addressed write — or say why it
      is safe.
- [ ] **Red-proof each fix by racing it**, not by reading it. The auditor's racer already exists and
      is the right tool; a fix that cannot be shown to change a losing trial into a refusal has not
      been demonstrated. Prove the harness can go red before trusting its green (CPE-1929's rule).
- [ ] Assert on the **filesystem** — that the victim outside the root is untouched and its mode is
      unchanged — not on an error string.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1050's Security Auditor, which raised these as F2/F3
alongside the F1 regression that PR then fixed. Its third finding became **CPE-1937** (revert delete
destroys files outside the root, with the 596-files measurement).

Related: **CPE-1913** (the zip leg, fixed), **CPE-1896** (the handle-gate family), **CPE-1935**
(a half-extracted folder with no per-entry report — same loop, adjacent symptom), **CPE-1937**
(the sibling destructive finding from the same audit).
