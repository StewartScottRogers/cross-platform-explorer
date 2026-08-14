---
id: CPE-1729
title: split_file's create_dir_all follows a dangling out_dir link, putting the whole part series somewhere unnamed
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-14
closed:
---

Filed by the CPE-1718 worker. CPE-1718 enumerated every destructive primitive in
`crates/server/src/split_join.rs` and fixed four of them; this is the fifth site, deliberately left out
because its verdict genuinely differs from the other four and deserves its own argument rather than being
folded into a fix whose reasoning does not apply to it.

## Problem

`split_file` calls `std::fs::create_dir_all(out_dir)` before it guards any output slot. `create_dir_all`
**follows** a link at the final component, so:

- a **live** directory link resolves to a real directory and the split lands there — which is almost
  certainly what the user meant, since a directory link is an ordinary way to name a USB stick, an
  external drive or a network share;
- a **dangling** link resolves to nothing, and `create_dir_all` then *creates the missing target
  directory* and writes the manifest and the entire numbered part series into it. Every subsequent
  per-slot guard passes honestly, because by then the final components being probed are real names inside
  a real directory — the link is one level up, where nothing looks at it.

The result is a split that reports success with its whole output in a directory the user never named.

## Why CPE-1718 did not fix it

The four sites CPE-1718 fixed are all **destructive**: they truncate (`File::create`, `fs::write`) or
delete (`fs::remove_file`), so following a link there costs the user data. `create_dir_all` destroys
nothing — it cannot truncate and cannot delete — so the cost here is *surprise*, not loss, and the fix has
to be weighed against breaking the live-link case, which is a legitimate and probably common arrangement.

That asymmetry is why it wants its own decision. See the enumeration table in `split_join.rs`'s module
doc, which records this verdict at the site so the next sweep does not have to re-derive it.

## Scope

`split_file`'s `create_dir_all(out_dir)` in `crates/server/src/split_join.rs`. Worth checking whether any
other `create_dir_all` on a user-supplied directory has the same shape before deciding — the CPE-1718
enumeration was scoped to this one module and says so.

## Acceptance criteria

- [ ] A decision, written down at the site: refuse a dangling `out_dir` link, or follow it and say so.
      A **live** directory link must keep working either way unless there is a stated reason it should not.
- [ ] If the verdict is a refusal, it names the link and nothing is created — asserted on a directory
      census (nothing new anywhere near the slot), not on the returned `Result`.
- [ ] Platform-gated with `cpe_server::fsutil::make_dangling_link`; `require_staged` so a runner that
      loses the capability goes red rather than green (CPE-1717).
- [ ] The guard broken on its own turns a distinct test red, real output pasted in the PR (Evidence
      Rules, `Ticketing/wiki.md`).

## Notes

Low priority: nothing is destroyed, and it needs a deliberate user action (naming a broken directory link
as the output folder) to reach. Filed rather than dropped because the CPE-1710 → CPE-1719 chain has now
twice shown that the site a sweep *declines* is the one the next round finds.
