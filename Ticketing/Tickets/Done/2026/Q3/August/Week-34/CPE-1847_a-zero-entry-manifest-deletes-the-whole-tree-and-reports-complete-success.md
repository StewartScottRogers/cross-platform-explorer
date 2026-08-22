---
id: CPE-1847
title: a planted zero-entry manifest deletes the whole tree and reports complete success
type: bug
priority: Critical
status: Done
tags: ready
estimate: M
created: 2026-08-21
closed: 2026-08-22
---

## Problem

A checkpoint manifest whose `files` map is **empty** describes a tree with nothing in it. Revert against
a real tree therefore plans a delete for every file, executes them all, and returns success.

Confirmed by the independent Reviewer during CPE-1823's round-4 review — measured, not reasoned:

```
empty checkpoint vs a five-file tree
  -> RestoreReport { applied: 5, skipped: [] }
  -> survivors = 0
```

Five files gone, nothing skipped, **complete success reported**.

CPE-1823's stand-down cannot help here. It arms on a checkpoint entry that cannot be restored on this
platform; a zero-entry checkpoint has no entries at all, so there is nothing to stand down on. The guard
is structurally blind to this shape.

## Why Critical

Every other manifest attack CPE-1823 closed required a crafted key that survived a guard. This one
requires **deleting text**. It is the cheapest possible tamper — truncate the map to `{}` — and its blast
radius is the entire tree rather than one named file.

### It also reaches through cherry-revert, which removes the mitigation entirely

Measured by the independent Security Auditor during CPE-1823's round-5 audit:

```
CMD revert[empty manifest]:     applied=5 skipped=0   survivors = []
CMD revert_one[empty manifest]: applied=1 skipped=0   survivors = [f1,f2,f4,f5]
```

The same emptied manifest destroys files **one at a time through `checkpoint_revert_one`**, behind a
per-file confirm that says nothing about a mass delete and **never consults `checkpoint_preview_revert`**.
So the mitigation everyone assumed — that the UI previews first (`AgentTimeline.svelte:483`,
`CheckpointDialog.svelte:138`), and an attentive user would see five deletes and no creates — does not
apply on that route at all.

On the whole-tree route the preview is still real but partial: `checkpoint_revert` is callable without it,
and "the UI happens to ask first" is not a guard, it is a habit.

## The judgement call this ticket must settle

An empty checkpoint is **legitimately representable**: capturing an empty directory produces one. So the
fix cannot simply refuse `files: {}`.

Options to weigh and decide explicitly, with the reasoning recorded:

- Require a positive assertion that the capture was of an empty tree, so an emptied map and a genuinely
  empty capture are distinguishable (a count, a checksum over the entry set, or a signed/derived field).
- Refuse a revert whose plan is **all deletes and no writes** above some threshold, without confirmation
  carrying the count.
- Recompute rather than trust — the shape CPE-1823 landed on repeatedly, and the shape CPE-1844 asks for
  on the same store's `index.json`.

Prefer whichever makes the harm impossible over whichever makes the manifest look valid.

## Acceptance criteria

- [ ] A zero-entry manifest cannot silently delete a populated tree. Whatever the chosen mechanism, the
      test asserts **the files still exist** before asserting the `Result`.
- [ ] A genuine capture of an empty directory still round-trips. This is the constraint that makes a
      naive refusal wrong — pin it.
- [ ] The all-deletes-no-writes plan shape is surfaced to the caller structurally, not only in prose.
      See CPE-1845, which is adding exactly that kind of discriminant to `OpResult`.
- [ ] Enumerate any other whole-manifest shape with the same property — valid on its face, catastrophic in
      effect — rather than fixing only the empty case. CPE-1823 found its third, fourth and fifth sinks by
      enumerating instead of trusting the ticket.
- [ ] Red-proof every test with the minimal realistic change, observe red, revert, record the line.
- [ ] Assert each new test's fixture is live (that the tamper actually took effect) before asserting the
      harm. CPE-1823 caught **six** inert tests, and in every one the fixture never reached the harm.

## Notes

Filed from CPE-1823's round-4 Reviewer findings. That review made the case for a ticket rather than a
comment: *"'recorded, not fixed' in a code comment is where round 1's colon regression also lived."*

**The disagreement between CPE-1823's two checkers is settled, and this shape won.** The Reviewer called it
the widest destructive shape a planted manifest has left; the Security Auditor argued at round 4 that the
case alias was wider. Round 5 closed the alias, and the Auditor withdrew its own position: this is the
widest remaining — and **wider than either party said**, because of the `revert_one` route above.
Raised from High to Critical on that basis (2026-08-22).

Related: CPE-1823 (the guards this evades), CPE-1844 (`index.json` steering prune, the same
hand-editable-file-steers-a-destructive-decision shape), CPE-1845 (the reporting discriminant).

## Work Log

### 2026-08-22 — fixed, branch `cpe-1847-empty-manifest-whole-tree-delete`

**Reproduced first, through the registered commands, before touching anything.** The ticket's figures
came back byte-for-byte, and the enumeration walk added a fourth line the ticket did not name:

```text
CMD revert[empty manifest]:            applied=5 skipped=0   survivors = []
CMD revert_one[empty manifest]:        applied=1 skipped=0   survivors = [f1, f2, f4, f5]
CMD revert[4 of 5 entries removed]:    applied=4 skipped=0   survivors = ["f1.txt"]
capture(empty dir) -> revert(3 added):  applied=3            survivors = []
```

The last line is the constraint, not a bug: a genuine empty capture is `new_blobs: 0, files: {}`, and
reverting a folder that has since been filled legitimately deletes those files today. It is the flow the
fix has to cost something, so it is measured rather than argued about.

**The judgement call, settled.** There is **no evidence anywhere on disk** that separates a genuine empty
capture from a manifest whose entries were removed — an absence is unfalsifiable, and any field added to
describe the map lives in the same hand-editable file as the map. So this could not be a detection
problem, and the ticket's instruction decides it: prefer the mechanism that makes the harm impossible
over the one that makes the manifest look valid.

**Primary fix — a zero-entry checkpoint may not authorise a delete** (`revert_engine::execute_restore`,
first `hold` branch). Reuses CPE-1823's existing per-path hold-back channel, so both routes are covered
by one rule at the one chokepoint both registered commands share, and every held-back delete is named
with the count. Two arguments carry it:

1. A delete's whole justification — this function's standing premise since CPE-1823 round 3 — is "this
   path is not in the checkpoint". A checkpoint holding nothing says that of every path there is.
2. **A zero-entry checkpoint has no constructive half at all.** It can restore nothing, so every delete
   it authorises destroys content it cannot give back, and holding the destructive half back forfeits
   *no restorable state*. That asymmetry is what makes the stand-down proportionate rather than merely
   cautious, and it is why the same rule would be wrong applied to a checkpoint with entries.

Deliberately **not** a refusal: the manifest still loads, previews, diffs and lists, and reverting an
unchanged empty tree still returns `applied: 0` with no error — the plan is empty, so nothing is held
back. The cost is exactly one flow (empty capture → folder filled → revert-as-cleanup), which now reports
each file as held back instead of deleting it. Pinned by test in both directions so nobody "fixes" it
back without meeting the reasoning.

**Secondary — `file_count`, a cost-raiser explicitly not allowed to authorise anything.**
`PersistedManifest` gains a count written by `capture` from the map it is writing, cross-checked in
`load_manifest`. That is the single chokepoint every caller-supplied manifest id funnels through, so
preview, diff, `checkpoint_revert` and `checkpoint_revert_one` refuse **together** — which matters
precisely because cherry-revert never consults the preview, so a check placed on the preview would guard
the one route nobody is attacked through. `Option` + `#[serde(default)]`: absent is not zero, so every
manifest already on disk keeps working.

Its scope is the **partial** tamper, where no absolute rule exists. The zero-entry stand-down does **not**
consult it, and a manifest asserting `file_count: 0` unlocks nothing — layering it the other way would
have handed the two-field tamper the win.

**Correction (round 2, and it is my claim being corrected, not the reviewer's).** I described this in
three places as raising the cost "from delete text to delete text and rewrite a number". **That is false.**
The field is `#[serde(default)] Option<usize>` and the check is gated on `Some`, because manifests written
before the field existed must keep loading. So the cheapest partial tamper is not "delete text and rewrite
a number" — it is **delete text and delete more text**: remove entries from `files`, remove the
`"file_count"` line, and the check never runs. Re-measured here through the registered commands, each leg
on a fresh five-file tree, with no number rewritten anywhere:

```text
4 of 5 entries removed + "file_count" key deleted
  checkpoint_revert_one(f3) -> Ok(RevertOutcome { applied: 1, skipped: [] })   survivors f1,f2,f4,f5
  checkpoint_revert         -> Ok(RevertOutcome { applied: 4, skipped: [] })   survivors ["f1.txt"]
```

The whole-tree figure is **4**, not the 3 first reported in review — a 3 is what a shared fixture gives
once the cherry-revert leg has already taken `f3`. Measured rather than copied, since this record has
already paid once for repeating a number instead of running it.

**Round 3 adds two more bypasses**, both found by the independent Security Auditor and both re-measured
here rather than accepted. None needs a number rewritten:

```text
4 of 5 entries removed + "file_count": null      -> Ok(applied: 4, skipped: [])   survivors ["f1.txt"]
replacement edit, "file_count": 5 UNTOUCHED      -> Ok(applied: 8, skipped: [])
  (remove f2..f5, add z1..z4 pointing at f1's blob)  survivors ["f1.txt","z1..z4.txt"]
```

`null` deserializes to `None` for an `Option`, so it is exactly as good as deleting the line. The
replacement edit is the sharper one: it keeps the count **honest**, so the check passes while the map
describes an entirely different tree — four user files deleted and four attacker-named files created.

So there are **three** ways past it: delete the field, null the field, or replace entries rather than
removing them.

So plainly: **`file_count` raises no cost against an attacker who knows the field exists.** It stops one
who does not, and it catches a tamper that removes entries and leaves the count behind. It is a
consistency check on a record that may have been edited — not a cost-raiser, not a bar, not a boundary.
The partial-tamper residual is **cheaper than the first version of this Work Log and the PR body said**.
Corrected in all four places (`revert_engine.rs`, the `file_count` field doc, the `checkpoint_store` test
doc, and here) rather than softened, on this ticket's own standard: a false claim in a security record is
worse than an honest smaller one — which is exactly why the retention claim above was corrected too. My
own field doc already said "an attacker who edits both is not stopped", so the record had been
contradicting itself.

**The security posture is unchanged, and that was measured too rather than asserted.** The Critical shape
is closed by the stand-down, which does not consult the count at all:

```text
files: {} + "file_count" key deleted
  checkpoint_revert_one(f3) -> applied: 0, skipped: 1   all five survive
  checkpoint_revert         -> applied: 0, skipped: 5   all five survive
```

**The keyed-signature ceiling, restated precisely.** "This store has no key" was wrong as written. The
repo does hold signing keys — `src-tauri/tauri.conf.json`'s updater pubkey, and
`TAURI_SIGNING_PRIVATE_KEY` plus a catalog key in `.github/workflows/release.yml`. Every one is a
**publisher** key in CI secrets signing centrally-produced artifacts, and a checkpoint manifest is written
on the user's own machine at capture time, so no publisher key can sign it — the conclusion holds, but the
honest statement is "no key that helps against a **same-user** attacker". And one vector that argument
does not dispose of: for the store-synced-or-copied-from-another-machine case this ticket's own threat
premise names, a **per-machine key in the OS keychain would be a real boundary**. Not attempted here;
recorded so the ceiling is not read as lower than it is.

**Rejected, with reasons, so they are not re-proposed:** refusing `files: {}` at load (refuses a real
capture — red-proofed, four tests); an all-deletes-no-writes *threshold* (an attacker just needs a
smaller tree; thresholds are not boundaries); corroborating the entry set against `index.json`'s blob
refcounts (a real independent witness, but equally hand-editable — CPE-1844's subject — expensive, and
blind whenever a manifest's blobs are shared, with false positives on any pre-existing refcount drift);
a keyed signature (the only actual boundary — see the round-2 correction below for the precise ceiling,
which is "no key that helps against a same-user attacker", not "no key at all").

**Structural surfacing left to CPE-1845 as instructed.** No field was added to `OpResult`/`RevertOutcome`.
The coordination is deliberate reuse: every hold-back here goes on the existing `"not deleted:"` prefix
CPE-1823 recorded the UI as matching, so CPE-1845 has one shape to make structural, not two.

### The enumeration — every whole-manifest shape walked to its destructive decision

Walked `PersistedManifest`'s aggregate properties rather than trusting the ticket's one shape.

| # | Shape | Measured effect | Disposition |
|---|---|---|---|
| 1 | `files` **emptied** | whole-tree delete, `applied=5/1 skipped=0`, both routes | **Fixed** — stand-down (harm impossible); cheap form also refused at load |
| 2 | `files` **partially** emptied | `applied=4, survivors=["f1.txt"]` — evades any zero-entry rule, strictly wider | **NOT fixed — open, and cheaper than round 1 claimed.** `file_count` only *detects* a tamper that leaves the count behind; deleting the `"file_count"` line bypasses it for free (measured). Recorded as the standing residual, unclosable without a per-machine key |
| 3 | `id` field **≠ filename** | retention decided about a different manifest: pointed at a sibling → `pruned: []`, `kept: [m3, m2, m3]`, tampered manifest immortal; pointed at nothing → the **whole pass** `Err`s and no checkpoint is ever thinned again | **FOUND AND FILED — CPE-1861. Not fixed here.** A fix was written (derive the id from the filename), measured, reviewed, and **reverted in round 3** because it introduced two regressions worse than the bug — see below |
| 4 | `created_ms` moved | steers `snapshot_retention::thin`; can prune checkpoints the user wanted kept | **Recorded** — no recomputation available (file mtime is equally forgeable and legitimately differs). Same class as CPE-1844 |
| 5 | `skipped` emptied | **inert today** — nothing reads `PersistedManifest.skipped` after capture | Recorded |
| 6 | a path the capture **skipped** is deleted on revert | absent from `files`, so `plan_restore` emits `Delete` — no attacker needed | **Recorded, not reachable today**: `checkpoint_create` captures with `CaptureBudget::UNLIMITED`, so nothing is ever skipped through a registered command. Reachable only via `scan_dir`'s unreadable-file skip (unreadable at capture, readable at revert). Deliberately not guarded — CPE-1823's own repeated mistake was landing guards on paths with no callers. Needs its own ticket if a budget is ever wired to the UI |
| 7 | every `hash` bogus/missing | writes fail → `report.skipped` non-empty → CPE-1823's stand-down arms | Already covered — verified |
| 8 | `size` inflated | diff cap is measured on the blob, never on the claim | Already covered (CPE-1823 inventory #7) |
| 9 | manifest captured from a **different root** | store dirs are keyed by `root_key(root)`, so a manifest is only reachable for the root whose store holds it; a store directory copied wholesale is the residual | **Recorded** — closing it needs the root recorded in the manifest, a format change beyond this ticket |
| 10 | **hash substitution between entries** (round 3, from the Auditor's extension of this walk) | pointing `f1.txt`'s `hash`/`size` at `f2.txt`'s blob gave `Ok(RevertOutcome { applied: 1, skipped: [] })` with `f1.txt`'s content on disk becoming `f2`'s — re-measured here | **Recorded** — count-neutral, per-entry-guard-neutral, and inside the manifest's trust model (the bytes still come from this store's own blobs, so CPE-1823's containment holds). Its value is what it proves about the count: `file_count` is **size-shaped, not content-shaped**, and gives zero protection against content substitution |

**A claim of mine that was measured false and is corrected rather than deleted.** The first version of
shape 3's test asserted retention would *delete a newer checkpoint the policy chose to keep*. It does
not: `thin` computes `prune` as the ids it did not keep, so an id that also appears in `keep` is never
pruned. The two reachable outcomes are the ones in the table — unbounded store growth and a dead
retention policy. Recorded at its real severity, because a false claim in a security record is worse than
an honest smaller one.

### Round 3 — what the independent security audit could NOT break

Recorded because it is the part that decides whether the Critical subject is actually closed. The Auditor
attacked the zero-entry stand-down directly and found no way through: every zero-entry variant held on
**both** routes, with and without `file_count`, and it could not construct a shape where the two guards
disagree. It also attacked the *tests* rather than only the code — three separate sabotages making a
tamper silently not take effect, all three red on the fixture-liveness assertions rather than passing
quietly, which is the property six inert tests in CPE-1823 lacked. Cost is nil: 23.0 ms against 21.2 ms
on a 10,000-entry manifest, with the cherry-revert spread inside machine noise.

### Round 3 — shape 3's fix is REVERTED, and this is the important lesson of the ticket

The fix (derive the id from the filename) shipped in round 1 and was **removed in round 3** after the
independent Security Auditor found it introduces a new silent, unattended data-loss regression. Both
reproduced here before reverting, and both re-run after, so the revert is verified rather than assumed.

**Regression 1 — a duplicated manifest file destroys the checkpoint that was KEPT.** Trigger: any second
file in `manifests/` that parses as a manifest — Explorer copy/paste (`X - Copy.json`), a cloud-sync
conflict copy, a backup script, a partial restore-from-backup. That is CPE-1823's own threat premise
("a store synced by a cloud client"), not an exotic case.

```text
cp <id>.json <id>-backup.json
  with m.id (reverted to)  preview keep=[id, id]  prune=[]
                           apply  Ok(kept: [id, id], pruned: [], bytes_freed: 0)
                           blobs=[3bfc…]   restore(id)=Ok(())   tree=["a.txt"]
  with file_stem (round 1) preview keep=[id]      prune=[id-backup]
                           apply  Ok(kept: [id], pruned: [id-backup], bytes_freed: 2)
                           blobs=[]        restore(id)=Err("…/blobs/3bfc…: cannot find the file")  tree=[]
```

The two copies get two distinct ids, retention prunes one, `release` drops the **shared** blob refcounts
to zero, the blobs are deleted, and the manifest it reports as `kept` can no longer restore anything.
`snapshot_schedule::snapshot_run_due` retention-prunes after every scheduled capture, so this fires
**unattended**, with no UI and no user action. Pre-PR the identical fixture was inert. It is the same
failure grammar this ticket exists to remove: content destroyed, complete success reported.

**Regression 2 — a crafted filename wedges the whole retention pass.** `a..b.json` (a copy of any
manifest) → `validate_manifest_id` refuses the `..`:

```text
  with m.id (reverted to)  apply -> Ok(RetentionApplyResult { kept: [id, id], pruned: [], bytes_freed: 0 })
  with file_stem (round 1) apply -> Err("a..b: not a valid manifest id")     # every pass, forever
```

That is precisely the harm the fix was written to remove — "pointed at nothing, the whole pass errors and
nothing is ever thinned again" — **relocated** from the inner field to the filename, not removed. `..` in
a stem suffices on any platform; on Unix `:` or `\` does it too.

**The lesson, stated plainly because it is the one worth carrying forward.** I closed an under-pruning
*leak* by converting it into *data loss*, and I did it in a bonus fix outside the ticket's Critical
subject, on an enumeration find, with a passing test that asserted exactly the behaviour that caused the
regression. The duplicate-id collapse I called a bug was **load-bearing by accident**: two files claiming
one id is what stopped retention from pruning one and freeing the other's blobs. My round-2 note that a
`load_manifest` self-consistency check was "rejected because it would wedge the pass" was right about that
check and blind to the fact that my own alternative wedged it differently.

**Left for CPE-1861, with the design choice recorded rather than pre-empted.** The Auditor measured a
candidate that is *not* a drop-in: `if m.id != id { continue; }` in `list_manifests` — a **skip**,
matching that function's own documented skip-the-unparseable guardrail, rather than the `load_manifest`
refusal correctly rejected above. It restores the duplicate case (`pruned: []`, `restore = Ok(())`, tree
restored), restores `a..b.json` to `Ok`, and leaves an inner-id lie able to neither steer nor wedge. But
it costs this branch's prune test, which asserted the liar **is** pruned. So it is a genuine design
choice — skip-and-leak versus prune-the-liar — and the Auditor notes the alternative correct fix lives in
`prune` instead: **do not release refs a surviving manifest still holds**. That reasoning belongs in
CPE-1861, not here.

What remains in this branch on shape 3 is a comment at the `list_manifests` line recording the walk, both
regressions, and the ticket — so the next person to notice the inner-`id` smell finds out why it is still
there before "fixing" it again.

**A pre-existing test whose fixture was the attack.** `revert_engine::deletes_apply_deepest_first` built
its checkpoint with `Snapshot::new()` — a zero-entry checkpoint — to exercise delete ordering, and so
went red against the new rule. Given a real (unchanged) checkpoint entry instead, with a comment saying
why: leaving it on the attack shape would have made it a test of the bug rather than of ordering.

Noted in review and worth recording, though it needs no action here: that test **never tested its own
name**, before this change or after. It asserts only `applied == 2` and that both files are gone — it
never observes ordering, and shallowest-first would pass it identically. The added entry is still a strict
improvement: it restores the test's ability to run at all, and it now exercises the round-5 case-alias
resolution pass, which was inert on a zero-entry checkpoint (`checkpoint_lands_on` was built from no
keys). Making it actually assert ordering is a separate, unrelated ticket.

The Auditor also confirmed in round 3 that this was the **only** test in the tree with that shape: every
other `Snapshot::new()` is immediately followed by an `insert`, and the two `execute_restore` call sites
outside this module build their checkpoint from a real `scan_dir`. So the new rule cost exactly one
fixture, and nothing else was silently disarmed by it.

**Recorded for CPE-1845, no code here: the hold-back reason is repeated verbatim per path.** 500 held-back
deletes emit 500 copies of the same ~370-character paragraph — roughly 185 KB in one `RevertOutcome`. The
two sibling `hold` branches in the same function name up to `NAMED_CAUSES` causes and then fall back to a
count; the zero-entry branch does not summarise at all, because its reason is about the checkpoint rather
than about any particular blocking entry. Harmless today at realistic sizes and squarely inside the
reporting rework CPE-1845 owns, so it is written down rather than patched around here.

**Recorded, not fixed:** a manifest whose count disagrees with its list is now unprunable, so one such
manifest wedges the retention pass until it is removed by hand (`snapshot_prune::apply` propagates
`prune`'s error with `?`). The same is already true of a manifest whose JSON is malformed, so this widens
an existing shape rather than introducing one; pruning a manifest whose file list is known wrong would
release the wrong blob refs and is worse. Written next to the check.

### Evidence — red-proofs, one line each, observed red then reverted

Every new test asserts **the files still exist** before it looks at the `Result`, and carries an explicit
fixture-liveness assertion. Two of them go further, on CPE-1823's round-5 pattern: a file is made to
diverge from the checkpoint *after* the capture, so a **dead tamper** becomes a different real change
(a legitimate `Overwrite`) that the `LIVE` assertion still trips — a test that only asserted "the files
exist" would have passed on an inert fixture.

| Guard | Line broken | Observed |
|---|---|---|
| zero-entry stand-down | `let hold = if checkpoint.is_empty() {` → `if false && checkpoint.is_empty() {` | `HARM: checkpoint_revert deleted f1.txt … RevertOutcome { applied: 5, skipped: [] }` and, on the cherry route, `HARM: checkpoint_revert_one deleted f3.txt … applied: 1, skipped: []` — the ticket's C1 and C2 figures exactly, each red independently. The genuine-empty-capture cost test red too |
| `file_count` cross-check | `if declared != manifest.files.len() {` → `if false && declared != …` | `HARM: entries deleted from a manifest's `files` map turned a revert into a delete of f3.txt … Ok(RevertOutcome { applied: 1, skipped: [] })` — the partial tamper through cherry-revert — plus the unit refusal leg |
| legacy exemption (over-tightening pin) | `if let Some(declared) = manifest.file_count {` → `… .or(Some(0)) {` | red: `a legacy manifest must still load: "…legacy.json: this manifest says it holds 0 files but its file list has 1"` |
| naive refusal of `files: {}` (over-tightening pin) | inserted `if manifest.files.is_empty() { return Err(…) }` in `load_manifest` | **all five** tests red, including `a genuine empty capture must still preview` and the `capture`→`restore` round trip — the fix the ticket says is wrong, pinned from four directions |
| ~~id-steers-retention~~ | **withdrawn in round 3 along with its fix** — both the test and the `list_manifests` change are reverted. The evidence itself was sound (it red before the fix with `HARM: retention left behind the manifest its policy chose to prune …`); what it did not cover was the duplicate-manifest and crafted-filename cases the fix broke, which is exactly why the fix is gone | n/a — shape 3 is CPE-1861 |

### Gates

Re-run after the round-3 revert. `crates/server`: `cargo clippy --all-targets -- -D warnings` → **exit 0**.
`cargo test` (every target) → **2318 lib** (4 ignored) + `ticket_mcp` 0 + `archive_panic_safety` 21 +
`binary_data_preview_panic_safety` 22 + `checkpoint_roundtrip` 2 + `finder_tags_os_interop` 1 +
`native_meta_os_interop` 1 + `parser_panic_safety` 45 + `sample_fixtures` 16 + `thumb_svg_panic_safety` 32
— **0 failed**. (**2318**, not round 1's 2319: the reverted shape-3 prune test went with its fix, so the
branch adds **five** tests, not six.)

`src-tauri`, both feature modes: clippy default → **0**, `--features sidecar-platform` → **0**;
`cargo test` → **214**, `--features sidecar-platform` → **269**.

No `specta::Type` struct or command signature changed (`PersistedManifest` is a private serde-only
struct; `ManifestSummary` is not a specta type), so `bindings.gen.ts` is unaffected.

In-app docs: `src/docs/16-checkpoints.md` gains a "When a revert holds its deletions back" subsection
covering all three hold-back causes and the self-contradicting-manifest refusal. No new `Section`, so
`sectionDocs.ts` is unchanged.

**Docs corrected in round 2 — they promised something no screen shows.** The first version opened with
"you are told exactly which cleanups did not happen **and why**". The reasons exist on the wire but **no
UI renders them**: `CheckpointDialog.svelte` (`:311`, `:172`, `:127`), `AgentTimeline.svelte:813` and
`CopilotDialog.svelte:299` each show only `skipped {n}`, and the `error` strings are dropped. That is
CPE-1845's fix, but until it lands the doc was pointing users at a list they cannot find, which makes the
CPE-1845 gap worse rather than neutral. Softened to what the screen actually shows — a count, plus an
explicit note that the per-file reason is recorded and a future update will list it — and the same
overstatement removed from the empty-checkpoint bullet ("listed as held back" → "counted as skipped, left
where they are"). The rest of the section is genuinely actionable and stands.

**Not verifiable on this machine:** nothing in this ticket is `#[cfg(unix)]`, but CPE-1823's `#[cfg(unix)]`
legs sit beside this code and the new zero-entry rule runs ahead of them in the same function, so
`Server crates` on **ubuntu and macOS** is the merge gate as it was there.
