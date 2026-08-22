---
id: CPE-1861
title: a manifest's inner id can disagree with its filename, and the obvious fix destroys blobs
type: bug
priority: High
status: Done
tags: ready
estimate: M
created: 2026-08-22
closed: 2026-08-22
---

## Problem

Every checkpoint manifest carries an `id` field, and nothing checks it against the filename it is stored
under. Retention reads that field. Two shapes, both measured through `snapshot_prune::apply`:

```
inner id -> a sibling's id      : apply Ok(kept=[id,id], pruned=[])   nothing pruned, manifest immortal
inner id -> "no-such-manifest"  : apply Err(...)                      the whole retention pass dies
```

The second is the worse one: **one tampered manifest wedges retention permanently** and nothing is ever
thinned again.

## Why this is its own ticket, and why the obvious fix is wrong

CPE-1847 fixed this as an enumeration extra by **deriving the id from the filename**. Its Security Audit
then measured two new regressions that fix introduces, and CPE-1847 was split rather than carry them.

**Regression 1 — a duplicated manifest file destroys the surviving checkpoint.**

```
cp <id>.json <id>-backup.json

before: preview keep=[id,id]  prune=[]           apply pruned=[]
        blobs=[f7e3...]   restore(id)=Ok(())     tree=["a.txt"]
after:  preview keep=[id]     prune=[id-backup]  apply pruned=[id-backup]
        blobs=[]          restore(id)=Err(".../blobs/f7e3...: cannot find the file")   tree=[]
```

The two copies get distinct ids, retention prunes one, `release` drops the **shared** blob refcounts to
zero, the blobs are deleted, and the **kept** manifest can no longer restore anything.
`RetentionApplyResult` reports it as `kept`.

`snapshot_schedule::snapshot_run_due` retention-prunes after every scheduled capture, so this fires
**unattended, with no UI and no user action**. The triggers are ordinary: Explorer copy/paste
(`X - Copy.json`), a cloud-sync conflict copy, a backup script, a partial restore-from-backup — and
"a store synced by a cloud client" is CPE-1823's own stated threat premise.

Content destroyed, complete success reported. The same failure grammar CPE-1847 exists to close.

**Regression 2 — a crafted filename wedges the pass.**

```
plant a..b.json (a copy of any manifest)
before: apply -> Ok(kept=[id,id], pruned=[])
after:  apply -> Err("a..b: not a valid manifest id")    every pass, forever
```

That is the *original* harm, relocated from the inner field to the filename rather than removed. `..` in
a stem suffices on any platform; on Unix `:` or `\` does too.

## The design choice this ticket must settle

The Auditor tested a candidate and it is **not a drop-in**:

`if m.id != id { continue; }` in `list_manifests` — a **skip**, matching that function's own documented
skip-the-unparseable guardrail, and deliberately *not* the `load_manifest` refusal (which would wedge the
pass). Measured: the duplicate case returns to `pruned: []` / `restore = Ok(())` / tree restored,
`a..b.json` returns to `Ok`, and an inner-id lie neither steers nor wedges.

It costs CPE-1847's prune test, which asserts the liar **is** pruned. So the real question is
**skip-and-leak versus prune-the-liar**, and it has to be decided rather than defaulted.

The Auditor's alternative, and the better shape if it holds: fix `prune` instead — **do not release refs a
surviving manifest still holds**. That closes regression 1 at its cause rather than by declining to prune,
and it would protect against any future path that prunes something sharing blobs.

## Acceptance criteria

- [x] An inner id disagreeing with its filename neither steers retention nor wedges the pass.
- [x] **The duplicated-manifest fixture must show `restore(<kept id>) = Ok(())` with its tree intact after
      a retention pass.** This is the gate; nothing merges without it.
- [x] `a..b.json` (and a filename with `:` or `\` on Unix) must not turn the pass into `Err`.
- [x] Decide skip-and-leak versus prune-the-liar versus fixing `prune`'s refcount release, and record why.
      If refs are the fix, state the invariant plainly: one manifest, one refcount, and a release must not
      drop a blob another manifest still names.
- [x] Assert each test's fixture is live — that the tamper landed on disk **and** reached the planner —
      before asserting harm. CPE-1823 caught six inert tests; CPE-1847's three-sabotage liveness check is
      the pattern to copy.
- [x] Red-proof every test with the minimal realistic change, observe red, revert, record the line.

## Notes

Found by CPE-1847's worker while enumerating, fixed there, and split back out after that ticket's Security
Audit measured the two regressions above. Read CPE-1847's Work Log first — it carries the measurements and
the reason the split was taken.

Related: CPE-1847 (the zero-entry stand-down that shipped), CPE-1844 (`index.json` steering prune — the
same store, the same "a hand-editable file steers a destructive decision" shape).

## Work Log

### 2026-08-22 — fixed, branch `cpe-1861-manifest-id-vs-filename`

**Everything below was reproduced first, on unmodified `origin/main`, through `snapshot_prune::apply`
and the registered commands — before a line was changed.** All four figures came back as the ticket
states them; the two regressions were re-measured by *temporarily re-applying* CPE-1847's reverted
`file_stem` line and then reverting it (`git diff --numstat` clean afterwards), so the "after" column is
measured on this branch rather than copied from the other ticket's record.

```text
BEFORE (origin/main)                     harm
  inner id -> a sibling's id             Ok(kept: [m3, m2, m3], pruned: [])   nothing thinned at all
  inner id -> "no-such-manifest"         Err(".../no-such-manifest.json: cannot find the file")
  CMD checkpoint_prune_apply, same       Ok(kept: ["no-such-manifest"])  -- reports keeping a
                                         checkpoint that does not exist
BEFORE (origin/main + the reverted file_stem line)     regression
  cp <id>.json <id>-backup.json          preview keep=[id] prune=[id-backup]
                                         apply pruned=[id-backup] bytes_freed=1  blobs=[]
                                         restore(id)=Err(".../blobs/ca97…: cannot find the file")  tree=[]
  CMD, same fixture                      prune_apply kept=[id] pruned=[id-backup]; then
                                         checkpoint_revert -> Ok(applied: 0, skipped: [a.txt: blobs/06
                                         82…: cannot find the file]) and a.txt still reads "damaged"
  plant a..b.json                         apply -> Err("a..b: not a valid manifest id")   every pass
```

The command leg is the one worth keeping: a **successful-looking** revert that leaves the damaged file
damaged, after a retention pass the user never asked for.

### The design choice, settled: neither (a) nor (b) alone — both, because they answer different questions

The ticket framed it as skip-and-leak **versus** fixing `prune`. Measured, they are not alternatives:
each closes shapes the other cannot, and the pair is what makes the result hold.

**Half 1 — identity, in `list_manifests`: a manifest must agree with the name it is filed under.**
The filename is *already* the operative identity everywhere else in the module (`load_manifest`,
`restore`, `prune`, `manifest_snapshot` all resolve by it; `save_manifest` writes by it). The inner `id`
is a redundant copy, and `list_manifests` was the one place reading the copy instead. The two coherent
repairs are "trust the filename" (round 1) and "require the two to agree" (this). Round 1 failed because
a filename is chosen by *whoever put the file there* — the OS copy dialog, a sync client, a backup
script — so trusting it **invents a checkpoint out of a stray file**, and retention then prunes the
invention. Requiring agreement keeps the filename authoritative without ever letting a name mint an
identity: a copy, a liar and a crafted name are simply not checkpoints, and the rest of the store is
thinned normally — which the duplicate-id collapse used to prevent outright.

Two conditions ride along, and both earn their place with their own red-proof:

- `validate_manifest_id(stem)` — **a hole the ticket's candidate did not cover.** `m.id != stem` alone
  passes a *self-consistent* crafted file (`a..b.json` whose inner id is also `"a..b"`), which then
  wedges the pass inside `prune`. That is regression 2 with one extra step. Red-proofed:
  `HARM: planting a..b.json killed the whole retention pass: a..b: not a valid manifest id`.
- `file_count_disagreement` — **CPE-1847's own recorded, unfixed wedge, closed here.** That ticket added
  the self-consistency refusal and wrote next to it that one tampered manifest therefore stops the
  retention pass permanently. It is the identical failure to a missing file, so it gets the identical
  treatment: the *same predicate*, factored into one function, used as a **refusal** by `load_manifest`
  and as a **skip** by `list_manifests`, so the two can never drift.

That yields the invariant the module now carries, pinned by its own test: **every id `list_manifests`
hands out is one `load_manifest` accepts.** Its only caller feeds those ids straight to `prune` and
propagates the error with `?`, so an id that cannot be loaded does not fail one manifest — it kills that
store's retention for good.

**The cost, stated as a decision rather than discovered later: a file that fails these checks is never
reclaimed.** It leaks. That is this module's chosen failure direction everywhere else (`prune`'s
documented "leak over corruption", `capture`'s skip-on-error), and it is pinned by assertion so nobody
"fixes" it back without meeting the reasoning.

**Corrected in review, and the correction is the important part, because the first version of this
record was wrong in the flattering direction.** I wrote that the leak is "bounded — a duplicate shares
its original's blobs and costs one small JSON file". It costs the *snapshot's stored content*. The two
halves interact, and neither comment said so: `prune` protects a blob **because** a manifest file still
names it, and half 1 guarantees that file is never listed — so "prune the last namer", the escape my own
`prune` comment leaned on for its "no permanent leak" claim, is **unreachable through retention**.
Measured here, driving the real `apply`:

```text
3 captures (m1 oldest, unique 12-byte blob) + an Explorer copy "<m1> - Copy.json"; hourly=2
  pass 1       pruned=[m1]  kept=[m3, m2]  bytes_freed=0
               m1's unique blob still on disk after its owner was pruned: true   (index refs: 1)
  passes 2-4   pruned=[]    freed=0        every time
  final        blob present; manifests/ = ["<m1> - Copy.json", "<m2>.json", "<m3>.json"]
  prune("<m1> - Copy") by id  ->  freed=12, blob gone
```

So a copy pins the whole pruned snapshot's unique content indefinitely. The leak *is* bounded — by that
snapshot's blob set — but the bound I stated was wrong by however large the snapshot is, and it is
reached by the ordinary unattended copy/paste or cloud-sync trigger this ticket is written around.
Reclaiming it needs the file removed by hand, or `prune` driven by that id directly — the last line
above, which is what `cpe_1861_prune_never_frees_a_blob_another_manifest_file_still_names` already
exercises. The trade still goes the same way (a leak of one snapshot's blobs is recoverable by deleting
one file; a stray file steering a delete is not), but the whole stated value of pinning it was that a
future maintainer meets the reasoning — and they were being handed a few hundred bytes. Corrected in all
four places rather than softened: `prune`'s comment, `list_manifests`'s comment, the in-app docs page,
and here. CPE-1847's prune test, which asserted the liar **is** pruned, is not
reinstated; it went with its fix in that ticket's revert, so nothing is lost, only not restored.

**Half 2 — blob safety, in `prune`: one manifest, one refcount, and a release must not drop a blob
another manifest still names.**

The ticket asks whether the refcount bookkeeping can even answer that question. **It cannot, and that is
measured in the fixture rather than argued.** `refs` is a counter bumped by `apply_capture` at capture
time, not a count of the manifest files on disk, so every way the two can diverge is a way for `prune` to
delete live content:

```text
cp <id>.json <id>-backup.json   ->   index.json says   refs: 1
                                     manifests/ holds  2 files naming that blob
```

A copy adds a namer without ever going through a capture. So the index is asked only the *cheap*
question — which of this manifest's hashes could even hit zero (`refs <= 1`, or absent from the index at
all, since the blob-delete loop keys off `!store.contains`) — and the authoritative question is answered
by **recomputing from the manifests actually on disk**, the shape CPE-1823 kept landing on. Nothing is
scanned unless a blob is genuinely about to be freed.

The repair is to **skip the decrement**, not to decrement and restore: the survivor's hold is what the
count should have been, so leaving it is the correct value, and it is self-correcting — prune the last
namer and nothing protects it, so the blob is freed then. Pinned in both directions: the honest
two-captures-share-a-blob case never even reaches the scan (`refs` is 2), and an over-tightened version
that protects everything reds three tests.

The survivor scan is **deliberately more permissive than `list_manifests`**, and the asymmetry is the
point. `list_manifests` asks "may this file *steer* a destructive decision?" and demands a
self-consistent record. `manifests_naming` asks "would deleting these bytes destroy something that still
points at them?", where any parseable manifest file counts — including the duplicate, the liar and the
crafted name the other rule refuses to list. Applying the strict rule here would re-open exactly the hole
it closes.

**Why both halves.** Half 2 alone makes regression 1 impossible but leaves both original harms (nothing
thinned; the pass dead forever). Half 1 alone meets the gate but leaves `prune` able to destroy shared
content down any future path. Measured, not asserted: with half 2 in place, re-applying round 1's
`file_stem` line leaves **the two duplicate-manifest gate tests green** and reds the other five — the
data-loss regression really is gone at its cause, and what still needs half 1 is the steering and the
wedge.

```text
AFTER
  inner id -> a sibling's id      Ok; kept has no duplicate and every kept id restores; m2/m3 restore
                                  byte-for-byte; the liar is left on disk, unlisted, unpruned
  inner id -> "no-such-manifest"  Ok(pruned: [m2]) -- and the NEXT pass is Ok too
  a..b.json (+ a:b, a\ b on unix) Ok(pruned: [m1]); the crafted name is never a checkpoint
  file_count contradiction        Ok -- CPE-1847's recorded permanent stall, closed
  cp <id>.json <id>-backup.json   apply Ok; blobs intact; restore(kept) = Ok(()) tree=["a.txt"]
  CMD, same fixture               prune_apply Ok; checkpoint_revert -> Ok(applied: 1, skipped: [])
                                  and a.txt reads "original" again
```

### The enumeration — everything that reads or writes a manifest id or a blob refcount

Walked rather than trusted; `list_manifests` having exactly one production caller is itself an
enumeration result, and it is what makes the skip safe (nothing else shows these ids to a user).

| # | Sink | Which identity it uses | Disposition |
|---|---|---|---|
| 1 | `save_manifest` | writes `manifests/<manifest.id>.json` — field and filename agree **by construction** at write time | The reason "require agreement" costs nothing legitimate |
| 2 | `load_manifest(id)` | filename; ignores the inner field entirely | Unchanged. Now also the *shared* home of the `file_count` predicate |
| 3 | `restore` / `prune` / `manifest_snapshot` | filename, via `load_manifest` | `prune` gains the blob-witness rule. **This row said "Unchanged" and that is how I missed S1** — see below: I walked the *identity* sinks and never walked `prune`'s own list of fail-closed gates, one of which (`validate_blob_name`) half 1 did not mirror |
| 4 | `list_manifests` | **read the inner field** — the bug | **Fixed.** Requires agreement + a resolvable name + a self-consistent count |
| 5 | `snapshot_prune::preview` / `apply` | ids from #4, fed straight to `prune` with `?` | The only production caller. Fixed by #4; nothing else needed |
| 6 | `checkpoint_store::checkpoints.json` (`Checkpoint.manifest_id`) | its own append-only index, written from `CaptureOutcome.manifest_id`, read by `checkpoint_list` and the preview's ts lookup | **Not a sink for this bug** — never consults a manifest's inner field. Carries a separate pre-existing wart: retention prunes manifests without touching these rows, so the UI can list a checkpoint whose manifest is gone (it errors on use). Recorded, not fixed — it is a reporting gap, not a destructive one. **Now filed as CPE-1862.** (First written here as "belongs with CPE-1845"; that was the wrong home — CPE-1845 is about `OpResult` lacking a structural discriminant, a result-*shape* defect, whereas this is an append-only index nobody reconciles, and CPE-1845's own file carried no record of it, so the note would have been lost) |
| 7 | `snapshot_schedule::snapshot_run_due` | `checkpoint_prune_apply` per due root | The unattended trigger. Covered through the command-level test |
| 8 | `fresh_manifest_id` / `pick_manifest_id` | filename existence only | Unchanged |
| 9 | `snapshot::apply_capture` (**writer**, +1 per capture) | — | The origin of the drift: a manifest that arrives by copy never runs this |
| 10 | `snapshot::release` (**writer**, −1, GC at 0) | — | Still the mechanism; `prune` now decides *which* hashes reach it |
| 11 | `BlobStore::contains` in `plan_capture` (dedup) | — | Unaffected — a protected blob stays in the index, so dedup keeps working |
| 12 | `BlobStore::total_bytes` → `store_total_bytes` → `preview.total_bytes` + the byte-cap loop | — | Unaffected; the byte-cap loop goes through the same `prune`, so it inherits the rule |
| 13 | `prune`'s blob-file delete loop (`!store.contains`) | — | **Also guarded** — a hash absent from the index is treated as at-risk, which is exactly the case the refcount could never have vetoed |
| 14 | `load_store` / `save_store` (`index.json`) | — | Untouched. CPE-1844 owns tampering with that file itself |

### Evidence — red-proofs, one line each, observed red then reverted

Every fixture asserts it is **live** before it asserts harm: the tamper is read back off disk, and (for
the id shapes) the planner's own view is compared before and after, so an inert fixture reds on the
liveness assertion instead of passing quietly. Where the guard *is* the only visible change to that view
— the `file_count` shape — the liveness assertion is the independent one (the file really is unloadable
now) and the view comparison is deliberately moved **after** the harm assertion, so a removed guard reds
on the stall rather than on a proxy. That ordering bug was caught by running the red-proofs, not by
reading the tests.

| Guard | Line broken | Observed |
|---|---|---|
| `m.id != stem` | `if m.id != stem` → `if false && m.id != stem` | `steer: kept names 1787415049204 twice: ["…204", "…197", "…204"]` — the ticket's `kept=[m3,m2,m3]` — **and** `HARM: one manifest whose inner id names nothing killed the whole retention pass: …\manifests\no-such-manifest.json: The system cannot find the file specified. (os error 2)`. Both original harms, each red independently |
| `validate_manifest_id(stem)` | `\|\| validate_manifest_id(stem).is_err()` → `\|\| (false && …)` | `HARM: planting a..b.json killed the whole retention pass: a..b: not a valid manifest id` — regression 2's exact string, from the self-consistent crafted file. Plus the invariant test: `only self-describing manifests may steer a retention decision` |
| `file_count_disagreement` | `\|\| file_count_disagreement(&m).is_some()` → `\|\| (false && …)` | `HARM: a manifest contradicting its own count killed the whole retention pass: …: this manifest says it holds 2 files but its file list has 1 …` — CPE-1847's recorded stall |
| `prune`'s blob witness | `manifests_naming(store_path, &at_risk)` → `manifests_naming(store_path, &BTreeSet::new())` | `HARM: pruning one of two manifest files naming a blob deleted the blob`. Only that one test reds — correctly, because half 1 stops retention ever *reaching* the copy; the guard is proved by driving `prune` directly |
| the whole round-1 "obvious fix" (realistic re-introduction) | re-added `out.push(ManifestSummary { id: stem.to_string(), … })` for every skipped file | **5 of 8 red**: the crafted-filename and file_count stalls, the invariant test, and the two id fixtures red on `LIVE: the tamper never reached the planner` (the tamper genuinely goes inert under filename-derivation — the liveness check catches it rather than passing quietly). **The two duplicate-manifest gate tests stay green**, which is the measured proof that half 2 removes regression 1 at its cause |
| over-tightening into a permanent leak (pin) | `manifests_naming` → `return wanted.clone()` unconditionally | **3 red**, including the pre-existing `prune_gcs_blobs_no_longer_referenced_and_keeps_shared_ones` (`only-in-first.txt's blob, held only by the pruned manifest, is freed`), `apply_keeps_gfs_survivors…` (`bytes_freed > 0`), and this branch's own `the last namer's prune still frees the blob` |

### Round 3 — S1, the fourth gate, and the enumeration mistake that hid it

The independent Security Auditor returned **MERGE** ("on every input I found where the PR behaves badly,
`main` behaves worse") with one finding, and it is the same class as the Reviewer's blocker: **a rustdoc
of mine stating a security invariant that was false.**

`prune` has **four** fail-closed refusals before its point of no return — `validate_manifest_id`,
`load_manifest`'s parse, `load_manifest`'s `file_count` cross-check, and CPE-1823's `validate_blob_name`
on every entry hash. Half 1 mirrored **three**. So one hand-edited `hash` in an otherwise *perfectly*
self-describing manifest — inner id agrees with the stem, stem valid, `file_count` honest — still
stalled the pass permanently. Re-measured here rather than accepted:

```text
"hash": "not-a-hex-hash"
  planner view = 3 entries, contains m1: true
  pass 1 -> Err("…: refusing this manifest entry — its content hash \"not-a-hex-hash\" is not a plain
                 hex blob name")
  pass 2 -> Err(same)
  manifests still on disk: all three
```

Identical on `700ae998`, so **not a regression** — but the same permanent-stall grammar this ticket
exists to remove, at the same tamper cost, and my `list_manifests` rustdoc claimed "every id it hands
out is one `load_manifest` will accept, **and one `prune` can resolve**". The second clause was simply
untrue.

**How I missed it, stated plainly, because the mechanism matters more than the miss.** My enumeration
walked every *sink for a manifest id or a blob refcount* — which was the right axis for the reported bug
and found five things the ticket did not name. It never walked the axis the invariant actually rests on:
**`prune`'s own list of gates ahead of its `remove_file`**. Row 3 of that table says
"restore / prune / manifest_snapshot — Unchanged", and a sink I had written off as unchanged is one I
never opened. The claim was stated in terms of `prune` and verified against `load_manifest`.

**Fixed as a fourth mirrored condition, not as a caveat** — `|| m.files.values().any(|f|
validate_blob_name(&f.hash).is_err())`. Cheaper than narrowing the invariant, and it makes the stated
one true instead of nearly true. The hashes are already deserialized at that point, so it costs a map
walk and no extra I/O. The rustdoc is rewritten to say what it now structurally is: **this function's
condition list is a mirror of `prune`'s fail-closed gates, and adding a refusal to one without the other
re-opens the stall.** That is the durable form of the lesson — the previous wording invited exactly this
mistake by naming an outcome rather than a correspondence.

New test `cpe_1861_a_hand_edited_entry_hash_no_longer_wedges_the_pass`, with the liveness this ticket
demands in both directions: the tamper is read back, **and** the fixture asserts the manifest is still
flawless on the other three conditions (inner id == stem, count honest) so the harm can only be the
fourth, **and** it asserts `prune(m1)` genuinely errors with `not a plain hex blob name` — so a fixture
that stopped being a wedge could not pass quietly. Red-proofed:

| Guard | Line broken | Observed |
|---|---|---|
| `validate_blob_name` mirror | `\|\| m.files.values().any(…)` → `\|\| (false && …)` | `HARM: one hand-edited entry hash killed the whole retention pass: 1787421133938: refusing this manifest entry — its content hash "not-a-hex-hash" is not a plain hex blob name` |

### Round 3 — the "bounded" correction is worse than the Reviewer measured

The Auditor independently confirmed the blocker and then ran the **recurring** case, with no attacker in
it at all: a sync client leaving one `<id> - Copy.json` per cycle.

```text
cycle  1: listed=1  manifest files= 2  blob files= 1
cycle  6: listed=1  manifest files= 7  blob files= 6
cycle 12: listed=1  manifest files=13  blob files=12    apply=Ok, bytes_freed=0 every pass
```

Linear and **unbounded**, complete success reported, and nothing surfaces it — no `src/` code consumes
`RetentionApplyResult`, and `snapshot_run_due` runs headless. So the honest statement is "bounded per
file, unbounded per copier", and the correction now says that in all four places rather than only the
single-copy figure.

The comparison that settles the trade, and it belongs next to the admission: on `main` the same fixture
gives a permanent `Err` wedge **plus 23 phantom checkpoints**. This PR converts a loud wedge into a
silent leak — better on every axis except discoverability, and a leak is recoverable by deleting files
where a store that can never be thinned is not.

### Round 3 — what survived attack, recorded because it bounds the claim

- **Nine exotic stems planted self-consistently** — trailing dot, dot inside the stem, NFC vs NFD, mixed
  case, 180 characters, a space, a bare hyphen, an RTL override — **all listed, all pruned**, `apply`
  Ok twice, no wedge and no blob loss. There is **no fourth stem-shaped condition**; the real miss was
  hash-shaped, which is why looking harder at names would not have found it.
- **The tests cannot pass against a dead tamper.** The Auditor made all five tampers silently inert and
  got **0 passed / 8 failed**, every one on a `LIVE` assertion — the property six inert tests in
  CPE-1823 lacked.
- The both-halves separation reproduced to the exact split (3 passed / 5 failed, the gate tests green),
  and the refcount analysis was confirmed correct with half 2 judged not over-built.
- The acceptance gate holds through the registered commands: `checkpoint_create` →
  `checkpoint_prune_apply` → `checkpoint_revert` → `applied: 1, skipped: []`, `a.txt` reads `"original"`.

### Round 3 — two findings deliberately NOT fixed here (filed separately, scope held)

- `apply`'s byte-cap loop mis-accounts when a prune frees nothing: `total = total.saturating_sub(freed)`
  with `freed == 0` never sees the cap met, so it runs to the `kept.len() <= 1` floor — five checkpoints
  destroyed, zero bytes reclaimed. **Byte-identical on `main`**, and unreachable from the app because
  `snapshot_run_due` passes `None`.
- `manifests_naming` compares hashes exactly while `validate_blob_name` accepts uppercase hex, and
  Windows/macOS open both cases as one file — so an upper-cased survivor is invisible to the witness and
  loses its content. One-line hardening (`to_ascii_lowercase()`), but it is its own shape and its own
  test.

### Cost, measured

Release build, this machine. The scheduled shape — `snapshot_run_due` pruning the one capture that just
aged out — is what actually runs:

```text
                       without the witness scan   with it
50 manifests × 20 files, prune 1        6.2 ms     6.8 ms
200 manifests × 50 files, prune 1       9.6 ms    18.3 ms
200 manifests × 50 files, prune 197   1.080 s    1.814 s     (worst case: a bulk thin is quadratic —
                                                              `prune` rescans survivors per call)
```

The Auditor fitted the delta and gave the note better numbers than I had: **`3.2 µs · n(n−1)/2 +
2.9 ms · n`, within ~1% at n = 50/100/200.** The quadratic term is real, but the **linear** one dominates
until n ≈ 1800, and the shipped default `RetentionPolicy` (24/7/4/12) caps a store at roughly **47**
manifests — so the worst case is **not reachable under defaults**. It *is* reachable on the first pass
after this fix un-wedges a long-stalled store, which is precisely the scenario this ticket creates:
~1.6 s, inside `spawn_blocking`, unattended.

Recorded next to the function, and that model is what makes it a considered deferral rather than a hope:
if a store ever does grow enough for the bulk case to bite, the fix is to hoist the scan out of the
per-manifest call, not to weaken it.

### Gates

`crates/server`: `cargo clippy --all-targets -- -D warnings` → **exit 0**. `cargo test` (every target) →
**2328 lib** passed (4 ignored) + `ticket_mcp` 0 + `archive_panic_safety` 21 +
`binary_data_preview_panic_safety` 22 + `checkpoint_roundtrip` 2 + `finder_tags_os_interop` 1 +
`native_meta_os_interop` 1 + `parser_panic_safety` 45 + `sample_fixtures` 16 + `thumb_svg_panic_safety`
32 — **0 failed**. The lib delta is **+9**, accounted for rather than asserted: `cargo test --lib --
--list` counts **2323** on the stashed tree and **2332** with the branch applied, and the branch adds
exactly 9 tests (6 in `snapshot_prune`, 2 in `snapshot_capture`, 1 in `checkpoint_store`). It was +8
through review round 2; round 3's S1 fix adds the ninth.

Docs guards, re-run after each round's markdown edit: `vitest run src/lib/docs.test.ts
src/lib/sectionDocs.test.ts` → **11 passed** (9 + 2).

`src-tauri`, both feature modes: clippy default → **0**, `--features sidecar-platform` → **0**;
`cargo test` → **214**, `--features sidecar-platform` → **269** — unchanged from CPE-1847, as expected:
nothing in `src-tauri` was touched.

No `specta::Type` struct or command signature changed (`PersistedManifest` is private serde-only;
`ManifestSummary` is not a specta type; `RetentionPreview`/`RetentionApplyResult` are untouched), so
`bindings.gen.ts` is unaffected.

In-app docs: `src/docs/16-checkpoints.md` gains a "Copying files inside the snapshot store" subsection
under Scheduled snapshots — what happens to a duplicate or hand-renamed file in the store, that one odd
file no longer stalls the cleanup, that pruning can never leave another snapshot unable to restore, and
the practical advice (copy the whole store folder, not files inside it). No new `Section`, so
`sectionDocs.ts` is unchanged.

### Not verified on this machine

- The `#[cfg(unix)]` legs — a manifest filename containing `:` or `\` — cannot be created on Windows
  (NTFS refuses both), so those two shapes are exercised only by `Server crates` on **ubuntu and
  macOS**. That is the merge gate, as it was for CPE-1823 and CPE-1847. The `a..b` leg runs everywhere,
  including here.
- ~~Frontend tests were not run locally~~ — run in review round 2 after linking the repo's
  `node_modules` into the worktree: `vitest run src/lib/docs.test.ts src/lib/sectionDocs.test.ts` →
  **11 passed** (9 + 2). The change is body text in one markdown file with no frontmatter or link
  changes; the rest of the frontend suite is unaffected and CI covers it.
- The `checkpoints.json` / pruned-manifest reporting gap in enumeration row 6 is recorded, not fixed.
