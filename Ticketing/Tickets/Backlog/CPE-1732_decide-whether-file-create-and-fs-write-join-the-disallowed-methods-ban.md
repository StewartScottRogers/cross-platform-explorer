---
id: CPE-1732
title: Decide whether File::create and fs::write join the disallowed-methods ban, as fs::rename did
type: task
priority: Medium
status: Backlog
tags: needs-decision
estimate: M
created: 2026-08-14
closed:
---

## The question

CPE-1710 concluded that a **convention does not hold** and replaced its source-scanning guard with a
compiler-level ban: `clippy.toml`'s `disallowed-methods` on `std::fs::rename`, in all 17 workspace roots,
with every legitimate use carrying an `#[allow(clippy::disallowed_methods)]` and a **reason at the site**.
That worked — the exemptions became the permanent audit, and a rename site cannot skip its guard.

**`File::create` and `fs::write` are not banned.** So CPE-1718's `create_slot_refusal` — which guards a
*create* at a user-named slot exactly as `rename_into_slot` guards a rename — is back to being a
convention. A future create site at a user-named slot silently gets nothing.

Raised by the PR #901 reviewer, which noted plainly that this is *"the precise thing CPE-1710 concluded
does not hold."*

## Why this is `needs-decision` rather than `ready`

**The ban is blunter here than it was for `rename`.** `File::create` and `fs::write` are legitimate at
app-owned paths **far more often** than `rename` is — journals, indexes, caches, vaults, every temp file,
every test fixture. So:

- The allow-list will be **long**, and each entry needs a real reason rather than a rubber stamp, or the
  exemptions stop being an audit and become noise.
- A long allow-list has its own failure mode: the CPE-1710 review found that an `#[allow]` on an `if`
  **statement** silently covers its whole block. More exemptions means more of those.
- `--all-targets` covers test code, where fixture writes are everywhere. CPE-1719 measured ~10 in one small
  crate alone and judged the cost not worth it **for that crate**; this ticket asks the same question
  repo-wide, where the answer may differ.

**This is a repo-wide policy decision, and it should not wait on an investigation.** It is deliberately
split from **CPE-1733** (the `archive.rs` sweep) on the PR #901 reviewer's recommendation: *"bundled, the
cheap decisive one waits on the expensive exploratory one — and the enforcement half is the one that stops
the next instance from being written."*

## What a good answer looks like

- [ ] Count the sites first. How many `File::create` / `fs::write` / `OpenOptions::create(true)` calls
      exist, split by **production vs `#[cfg(test)]`** and by **user-named vs app-owned destination**.
      The decision turns on that ratio and nobody has measured it.
- [ ] Decide, and **record the reasoning either way.** "No, the allow-list would be longer than the
      benefit, and here is the count" is a perfectly good outcome — better than a ban nobody maintains.
- [ ] If yes: a convention for the reason text, and a check that an `#[allow]` sits on the **statement**
      rather than a block that could later grow more writes.
- [ ] If no: say what *does* stop the next unguarded create at a user-named slot, and make that visible at
      `create_slot_refusal`'s doc rather than leaving it implied. The current doc does **not** over-claim
      enforcement, which is honest — but it also does not say there is none.
- [ ] Consider a narrower ban: only `File::create`, or only in the crates that hold user-named slots, rather
      than all 17 roots. `fs::rename` earned the blanket ban because it is rarely legitimate; these are not.

## Notes

Filed by the Foreman from the PR #901 review, 2026-08-14.

Related: **CPE-1710** (which established that a convention does not hold, and the `#[allow]`-with-reason
pattern), **CPE-1718** (`create_slot_refusal`, currently unenforced), **CPE-1719** (which weighed and
**rejected** a `fs::write` ban for one crate — read its reasoning first), **CPE-1733** (the `archive.rs`
sweep this is deliberately split from).
