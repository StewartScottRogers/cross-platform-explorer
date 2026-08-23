---
title: Checkpoints & Rollback
order: 16
category: Safety & Recovery
categoryOrder: 5
---

# Checkpoints & Rollback

**Checkpoints** capture the state of a folder tree so you can revert to it later — a manual save point
you take before an agent, a script, or your own risky edit touches a folder, so you always have a way
back.

Open it from the **command palette** (Ctrl/Cmd+K → *Checkpoint & rollback…*).

## Creating a checkpoint

1. Point the dialog at the folder to protect (it starts on the folder you're currently viewing).
2. Optionally give it a label (`"before refactor"`, `"clean state"`, …) — blank is fine too.
3. Click **Create checkpoint**. The tree is captured into your per-folder checkpoint store; identical
   file content already stored by an earlier checkpoint is reused rather than duplicated, so a series of
   checkpoints on the same folder stays cheap. Any file skipped (too large, or the capture budget would be
   exceeded) is reported by name rather than silently dropped.

## Previewing a revert

Every checkpoint in the list has a **Preview** button. It shows what reverting to that checkpoint would
do, *without changing anything*:

- **creates / overwrites / deletes** — how many files would be recreated, overwritten, or removed, and
  how many bytes would be written back.
- **drift** — paths that have changed since the checkpoint but can't be attributed to a specific agent
  session. Drift is called out separately because it's the case most likely to surprise you: reverting
  will overwrite or remove those files too.

Always preview before reverting a folder you've kept working in — the drift count is exactly the number
of files whose current state you'd lose.

## Reverting

Two ways to revert, both from the checkpoint's row in the list:

- **Revert…** — reverts the **whole tree** under the root back to that checkpoint.
- **Revert this path…** — type a path under the root and revert **only that path**, leaving the rest of
  the tree alone. Useful when preview shows drift you want to keep everywhere except one file.

Both are destructive — files are overwritten or deleted on disk directly, not moved to the Recycle Bin —
so both arm a confirmation panel first, restating what will happen. Nothing reverts on a single click.
After a revert, the dialog reports how many changes were applied and how many were skipped (e.g. a locked
or missing file); a skip never fails the rest of the revert.

### When a revert holds its deletions back

A revert deletes a file on one basis only: *that file is not in the checkpoint*. Where the app cannot be
sure it read the checkpoint correctly, it applies everything it can restore and then **holds the
deletions back** rather than performing them. Nothing is destroyed on a doubt.

The revert result counts these alongside genuine failures, as **skipped**: a revert that reports changes
applied and some number skipped, with your files still in place, has held its deletions back rather than
failed. The specific reason is recorded per file but is not shown in the dialog yet — a future update
will list it. Until then, the cases below are the ones to check against.

The cases you may meet:

- **A checkpoint that records no files at all.** Capturing an empty folder produces one legitimately, and
  so does a checkpoint file that has been edited or corrupted — on disk the two are identical. Such a
  checkpoint has nothing to restore, so it is never allowed to authorise deleting anything. Reverting an
  empty folder that is still empty works exactly as before; if the folder has since been filled, those
  files are counted as skipped, left where they are, and you can delete them yourself.
- **A checkpoint holding a name this computer cannot write** (for example one captured on Linux or macOS
  with a name Windows reserves). Everything restorable still restores; deletions wait, because a name
  spelled differently here might be the very file about to be removed.
- **A file that could not be restored this time** (locked, or its stored content is missing). Re-run the
  revert once that is fixed and the held-back cleanups apply.

A checkpoint whose stored file list contradicts its own recorded file count is refused outright, on every
route — preview, compare and both revert commands — rather than quietly acted on as a smaller tree.

## When a pre-write checkpoint fails

Several tools — Batch Media (overwriting originals in place), Metadata Studio, Declutter, and Similar
Images — take a best-effort checkpoint of the affected folder immediately before an otherwise-irreversible
write, so you have a way back even without Undo. A checkpoint failure never blocks that write (the
checkpoint is a bonus safety net, not a gate), but it's important information: it means no recovery net
was created for that attempt.

That failure now leaves a durable record right here, in this list, alongside the checkpoints that *did*
succeed — not just a banner you had to be watching for. A failed attempt is shown distinctly, with a
different marker, no timestamp-matched restore point, and **no Preview or Revert buttons at all** — it
can never be selected or mistaken for a real checkpoint, because nothing was actually captured. Hovering
it shows the reason the attempt failed.

## Scheduled snapshots

Instead of taking every checkpoint by hand, you can have a folder snapshot itself on an interval. Open
**Settings** (command palette → *Settings*) and find the **Scheduled snapshots** section.

- **Add a folder** — click **Browse…** to pick the folder, choose how often to capture it (e.g. every
  1 day), and click **Add**. It starts enabled.
- **Retention** — each folder keeps a rolling window of snapshots: so many *hourly*, *daily*, *weekly*,
  and *monthly* captures. Older snapshots outside that window are pruned automatically after each
  scheduled capture, so the store stays bounded instead of growing forever.
- **Pause / resume** — the **on / paused** toggle on a folder stops or restarts its schedule without
  losing the rule.
- **Remove** — the **✕** removes the schedule (existing snapshots in the store are left alone).

This is **opt-in and off by default**: with no folders added, the scheduler does nothing at all — no
captures, no background work. A folder you add is captured by a background timer while the app is open;
the first capture happens shortly after you add it (or after launch), then on its interval.

### Copying files inside the snapshot store

Automatic pruning deletes old snapshots, so it takes care over which files it is allowed to decide about.
A snapshot's record has to agree with the name it is filed under; anything in the store that doesn't
describe itself that way — a duplicate left by a copy/paste or a cloud-sync conflict, a file renamed by
hand, a record edited after it was written — is **ignored by pruning rather than acted on**. It stays on
disk until you remove it yourself.

**So does the content it points at**, and that is the part worth knowing: automatic cleanup never
deletes stored content that any record in the store still refers to, and it never acts on a record it
can't make sense of. An ignored file therefore holds on to its snapshot's stored content — which can be
as much space as that whole snapshot, not the size of the file itself — until you delete the file.

If something keeps *making* these files — a sync client that leaves a conflict copy every time it runs,
say — the store will keep growing, because each copy holds on to another snapshot's content and cleanup
never removes any of them. Nothing warns you about this: the cleanup reports success each time, because
from its point of view it did exactly what it was asked to. If a snapshot folder is larger than you
expect, look inside it for duplicated or renamed record files and delete them.

Two consequences worth knowing:

- One odd file can no longer stop the cleanup. Pruning keeps thinning everything else around it, instead
  of stalling on the file it can't make sense of.
- Nothing that another snapshot still needs is ever deleted. Pruning checks the snapshots that remain
  before it frees any stored content, so removing one snapshot can never leave another unable to restore.

If you want to keep a copy of a snapshot store, copy the **whole store folder** somewhere outside it
rather than duplicating individual files inside it.

### How big the store thinks it is

A snapshot store keeps a small bookkeeping file alongside the snapshots, recording how much space each
piece of stored content takes. If you set a size limit on a folder's snapshots, that limit is what
decides when older snapshots are deleted to make room — so the size figure decides deletions.

**That figure is now measured every time, from the stored content your snapshots actually refer
to** — rather than read out of the bookkeeping file. Two things follow, and the second is the one
that is easy to miss:

- If the bookkeeping file is edited, out of date, or copied in from another machine, it can no longer
  make the app believe a small store is enormous and start deleting snapshots to get under a limit
  that was never exceeded.
- Stray content in the store counts for nothing. A leftover file that no snapshot refers to — the
  kind a crash mid-snapshot or a partly-restored backup can leave behind — is not counted towards
  the limit, because deleting snapshots could never clear it anyway. Counting it would delete your
  snapshots and free nothing.

So the footprint you see in the cleanup preview is the space your snapshots are actually using: what
deleting them would genuinely give back. It can be smaller than the size of the folder on disk, and
if it is, the difference is leftovers you can delete yourself.

The same applies in the other direction. Stored content is only ever treated as present if the file
holding it is present — so a snapshot taken after part of a store went missing stores that content
again rather than assuming it is already there, and a new snapshot can always put your files back.

If the bookkeeping file is damaged outright, automatic cleanup **stops and says so** instead of
guessing. Nothing is deleted while it can't be read. That is deliberate: acting on an unreadable
record is how snapshots get lost. The same applies if the snapshot records themselves can't be read:
without them there is no way to tell which stored content still matters, so cleanup refuses rather
than assuming all of it does.

## What this is (and isn't)

This is the palette-driven, headless-friendly way to create and use checkpoints, usable on any folder at
any time. A second, richer restore surface now also exists **inside [Agent Watch](explorer-agent-watch)**:
its Replay tab shows a watched folder's checkpoints as pins right on the activity scrubber, with the same
drift-aware preview and two-step confirm as here — but only while you're watching that folder's agent
session. This dialog is the one that works everywhere else, on any folder, with or without an agent
involved.
