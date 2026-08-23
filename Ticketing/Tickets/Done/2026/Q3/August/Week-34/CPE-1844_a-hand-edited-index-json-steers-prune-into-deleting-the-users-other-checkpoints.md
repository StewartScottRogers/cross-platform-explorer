---
id: CPE-1844
title: a hand-edited index.json steers prune into deleting the user's other checkpoints
type: bug
priority: Medium
status: Done
tags: ready
estimate: M
created: 2026-08-21
closed: 2026-08-22
---

## Problem

`store_total_bytes` (`crates/server/src/snapshot_capture.rs:560`) reads its figure from `index.json`.
The retention byte cap (`crates/server/src/snapshot_prune.rs:107-121`) turns that number into real
`prune` deletions of the user's **other** checkpoints, floored at one survivor.

`index.json` is exactly as hand-editable as the manifest that CPE-1823 spent four review rounds
hardening, and it receives **none** of that ticket's validation. Inflate the recorded total and the
retention policy concludes the store is over its cap and starts deleting checkpoints that should have
been kept.

This is the same shape as CPE-1823 — *a hand-editable file steers a destructive decision* — one file
over.

## Why it is Medium rather than High

The damage is confined to the snapshot store: it deletes checkpoints, not user data, and the floor
guarantees one survivor. It also needs the same precondition as CPE-1823 (write access to the store),
and the same threat premise: a store copied between machines, restored from a shared drive, or synced
by a cloud client.

But losing checkpoints silently is losing the user's ability to undo — and CPE-1823 established that
this store's inputs are not trustworthy.

## Acceptance criteria

- [x] `index.json`'s numeric fields are validated or recomputed before any retention decision uses them.
      Prefer **recomputing** from what is actually on disk over validating a claim — CPE-1823's diff cap
      had the same shape (it gated on the manifest's claimed `size` and was defeated by a manifest
      claiming `size: 1`), and the fix there was to measure the real thing, not to sanity-check the
      claim. If recomputing is too expensive to do every time, say what it costs and gate it.
- [x] A prune driven by a tampered or stale `index.json` cannot delete a checkpoint that the real
      on-disk state says should be kept.
- [x] Every other field of `index.json` that reaches a decision is enumerated and either validated or
      explicitly recorded as harmless. CPE-1823 found its third, fourth and fifth sinks by enumerating
      rather than trusting the ticket — do the same here.
- [x] Tests stage a tampered `index.json` for each shape and assert **the harm did not happen** —
      the checkpoint that should survive is still there — before asserting the `Result`.
- [x] Red-proof each test with the minimal realistic change, observe red, revert, record the line.

## Notes

Found by the independent Reviewer during CPE-1823's round-3 review, which correctly declined to absorb
it — nothing in that PR made this worse, and scope creep on a ticket already four rounds deep would
have been the wrong call.

Read CPE-1823's final Work Log before starting. It carries the attack record that matters here: which
shapes defeat textual checks, why `canonicalize` cannot see a hard link, and the rule that a guard
belongs where callers inherit it rather than at each call site. That ticket needed four rounds largely
because guards kept landing on the path with no callers while the shipping path went unguarded — check
which functions here are actually reachable from a registered command before deciding where to put
anything.

## Work Log

### 2026-08-22 — fixed, branch `cpe-1844-index-json-steers-prune`

**Everything below was reproduced first, on unmodified `origin/main` (`dc3a0b95`), through
`checkpoint_store::checkpoint_prune_apply` / `checkpoint_create` / `checkpoint_revert` — the functions
the registered `snapshot_prune_apply`, `snapshot_prune_preview`, `checkpoint_create` and
`checkpoint_revert` commands dispatch into — before a line was changed.** CPE-1823's round-1 lesson
(a guard landed on a function with no production caller) is why the reproduction starts at that layer
and not at `snapshot_prune::apply`.

The ticket's shape reproduces exactly. My first fixture did **not**, and that is worth recording:
`hourly: 100, daily: 100, weekly: 100, monthly: 100` still put all five captures in one hour bucket, so
the GFS pass pruned four and the byte-cap loop broke immediately on `kept.len() <= 1`. It "showed" the
harm while the byte cap had done nothing. Re-staged with `created_ms` a day apart and `daily: 100`, so
`preview().prune` is empty and the byte cap is the only thing in the fixture that can delete anything:

```text
BEFORE (origin/main), 5 captures, real blob bytes on disk = 45, cap = 1,000,000
  preview  total_bytes=45   keep=5  prune=[]
  index.json: every blob's "size" -> 1000000000      (one text edit; no bytes written anywhere)
  preview  total_bytes=5000000000
  CMD checkpoint_prune_apply(cap=1_000_000)
        kept   = ["…942"]
        pruned = ["…924", "…928", "…936", "…940"]     bytes_freed = 4000000000
        manifests left on disk = 1 of 5
```

Twenty thousand times under its cap, and four of the user's five checkpoints deleted, down to the
one-survivor floor, with a fabricated `bytes_freed` reported as success.

### The design: recompute, and recompute the *whole* number, not a bound on it

`store_total_bytes` now measures `blobs/` on disk — `read_dir` plus one `metadata()` per entry — and no
longer opens `index.json` at all. Validating was never on the table: a recorded size **is** the claim,
so any sanity bound on it is another claim. This is the shape CPE-1823 paid for twice (its diff cap
gated on the manifest's claimed `size` and fell to `size: 1`; the fix was to measure the file) and the
one CPE-1861 reached independently for `manifests_naming`.

**Can it reuse CPE-1861's `manifests_naming`? Asked, answered no, and the reason is instructive.** That
function answers *"does any manifest file still name this blob?"* — an identity question over the
manifests. This one answers *"how many bytes are on disk?"* — a size question over the blobs. Reusing it
would mean summing the `size` fields **inside the manifests**, which are the same kind of hand-editable
claim as `index.json`'s, one file over. The two recomputations share the principle and correctly do not
share code.

**Three deliberate under-counts, all in the safe direction**, because being wrong about a delete-driving
total is only non-destructive downwards: only regular files (a `DirEntry::metadata` does not follow a
symlink, so a link into a huge file counts 0); only names passing `validate_blob_name` (a `Thumbs.db`,
a sync client's `deadbeef (1)` conflict copy, a hex-named *directory* — none are blobs this store
wrote, and letting an attacker-or-OS-chosen filename feed the total is the defect being fixed); and an
entry whose `metadata()` fails is skipped. An **absent** `blobs/` is `Ok(0)`; an **unreadable** one is
`Err`, following `load_store`'s CPE-1705 rule.

**What it costs an attacker, stated at full strength rather than implied away.** Inflating the figure now
requires creating files in `blobs/` whose *logical* length is the inflated number, under plain hex
names. That is a real cost increase and it is **not a barrier**: a sparse file (`fsutil sparse`,
`truncate -s`) has a huge logical length and near-zero allocation. Written into the code as a limit. Two
things bound it — the tamper is now visible as files in the store rather than a number in a config, and
what it can still buy is bounded by CPE-1863.

### Cost, measured — no gating needed

Release build, this machine, `store_total_bytes` vs. the `index.json` read it replaces:

```text
n blobs   index.json read   blobs/ scan
     50        572.8 us        111.2 us
    200        601.0 us        242.5 us
   1000       1185.5 us       1331.2 us
   5000       2671.8 us       5719.1 us
```

The scan is **cheaper** than the read it replaces up to roughly a thousand blobs and about 2× more
expensive at five thousand — 5.7 ms in absolute terms, against `manifests_naming`'s already-accepted
18.3 ms on a 200×50 store. Called once per `preview` and once per byte-cap iteration; the scheduled path
(`snapshot_run_due`) passes `max_total_bytes: None`, so it pays for one scan per `preview` and nothing
in the loop. Nothing to gate.

### Interaction with CPE-1863, stated because that ticket is open and this loop is its subject

CPE-1863 records that `apply`'s byte-cap loop treats `freed == 0` as no progress and runs to the
`kept.len() <= 1` floor. **Untouched, and deliberately not fixed here.** What changed is that the loop's
progress test now measures the disk: when a prune really frees nothing, the re-measured `total` really
has not moved, so the loop behaves exactly as CPE-1863 describes — the difference is that it is now
describing reality rather than an accounting artifact. It is not made worse: every case where the old
subtraction advanced `total` faster than the disk did was a case of *over*-crediting a prune, which
ended the loop **early**. And this ticket removes one way to reach CPE-1863's floor for free (an
inflated `index.json`), leaving the ones that need real bytes or a genuinely unprunable store.

### The enumeration — every field of `index.json`, walked on BOTH axes

Axis 1, every reader of every field. Axis 2 — the one CPE-1861's worker recorded as the axis it failed
to walk — **`prune`'s own list of gates ahead of its point of no return**. Axis 2 is what found row 11,
which no amount of looking at fields would have.

| # | Field | Sink | Reached from | Disposition |
|---|---|---|---|---|
| 1 | `blobs[h].size` (sum) | `store_total_bytes` → `RetentionPreview::total_bytes` | `snapshot_prune_preview` | **Fixed** — measured from `blobs/` |
| 2 | `blobs[h].size` (sum) | `store_total_bytes` → `apply`'s byte cap → **real deletes** | `snapshot_prune_apply` | **Fixed** — the ticket's bug |
| 3 | `blobs[h].size` | `release` → `freed` → `total.saturating_sub(freed)` → keeps the delete loop running | `snapshot_prune_apply` | **Fixed** — the loop re-measures instead of subtracting |
| 4 | `blobs[h].size` | `release` → `prune`'s return → `RetentionApplyResult::bytes_freed`, **reported** | `snapshot_prune_apply`, `snapshot_run_due` | **Fixed** — measured from the files actually removed |
| 5 | `blobs[h].size` | `plan_capture`'s `projected_total` → `max_total_bytes` → `SkipReason::Budget` → a checkpoint silently missing content | **nothing** — `checkpoint_store.rs:329` captures with `CaptureBudget::UNLIMITED`, whose `0` disables the gate | Recorded at the site, not fixed: `plan_capture` is pure and is handed a `BlobStore`, so it has nothing to re-measure. The note says what to do if a real budget is ever wired |
| 6 | `blobs` map **key** | `BlobStore::contains` → `plan_capture`'s dedup → the blob's bytes are **not written** | `checkpoint_create`, `snapshot_run_due` | **Fixed** — a `reused` blob whose file is missing is written |
| 7 | `blobs` map **key** | `store.contains(hash)` in `prune`'s blob-delete loop | `snapshot_prune_apply` | Harmless **by CPE-1861**: the hash is the *manifest's*, and `manifests_naming` vetoes any delete a surviving manifest still names. Pinned by test |
| 8 | `blobs[h].refs` | `prune`'s `at_risk` filter (`refs <= 1`) | same | Inflate → the witness scan is skipped, `release` never reaches 0, blob kept → **space leak only**. Deflate → the hash enters `at_risk` → CPE-1861's disk witness protects it. Both directions measured in one test |
| 9 | `blobs[h].refs` | `release`'s decrement + GC-at-0 → the blob file delete | same | as 8 |
| 10 | index keys **as path components** | none — never joined onto a path. `prune` joins the *manifest's* hash (CPE-1823-validated); `capture` joins a hash it computed from disk | — | Structurally harmless |
| 11 | **the file as a whole** (parse / type / stat) | `load_store` — CPE-1705's `Fresh`/`Present`/`Refuse` — sitting **below** `prune`'s point of no return | `snapshot_prune_apply`, `snapshot_run_due` | **Fixed** — see below. Found on axis 2 |
| 12 | unknown extra keys | serde ignores them (`PersistedIndex` has no `deny_unknown_fields`) | — | No reader; harmless |

### Row 11 — the one the field-walk could not have found, and it was destructive

`load_store` is fail-closed by design (CPE-1705). It sat one line *below* `prune`'s `remove_file`. So an
`index.json` that merely fails to parse — a torn write, a truncation, a hand-edit that dropped a brace —
cost **one checkpoint per retention pass**, each pass reporting failure, blobs leaked because `release`
never ran. Measured on `origin/main`, four checkpoints, `index.json` truncated to `{"blobs": {`:

```text
pass 1  Err(index.json: EOF while parsing…)   manifests left = 3
pass 2  Err(same)                             manifests left = 2
pass 3  Err(same)                             manifests left = 1
pass 4  Ok                                    manifests left = 1
```

Hoisted above the point of no return, the refusal is total: nothing is touched, all four survive, every
pass. The stall it leaves is the right direction and cannot be mirrored into `list_manifests` the way
CPE-1861's four per-manifest gates are — it is a property of the store's one ledger, not of any
manifest, so there is no per-file predicate to skip on. `list_manifests`'s rustdoc now says exactly
that, rather than leaving its "mirror of `prune`'s gates" invariant quietly false.

### Row 6 — dedup is an optimisation, not a promise

`plan_capture`'s `reused` verdict is `index.json` claiming it holds those bytes, and `capture` honoured
the claim by writing nothing. Reproduced through the commands with **no attacker in it** — the residue
is `prune`'s own documented leak-over-corruption window (blob files removed, then a failure before
`save_store`), and equally what a partial restore-from-backup of a store leaves:

```text
blobs/<hash> deleted, index.json's entry for <hash> left in place
  checkpoint_create         -> Ok, a "second" checkpoint recorded; blob file still absent
  checkpoint_revert(second) -> Ok(applied: 0, skipped: [a.txt: blobs/<hash>: cannot find the file])
  a.txt still reads "damaged"
```

The write loop now walks `plan.to_store.iter().chain(plan.reused.iter())`, and the slot probe already
there is the disk question — a blob whose file exists is `Occupied` and skipped for one stat. So a
reused blob is written when, and only when, its bytes are genuinely missing. `added_bytes` deliberately
still reports the plan's figure: a repair write is content the store was already supposed to hold.

### Evidence — red-proofs, observed red then reverted

Every tampering test asserts its fixture is **live** — the tamper read back off disk, *and* that it
reached the decision (the planner still sees all the checkpoints; the tampered claim really does exceed
the cap; the GFS pass really would prune nothing, so only the byte cap can) — before it asserts harm,
and the harm before the `Result`.

| Guard | Line broken | Observed |
|---|---|---|
| `store_total_bytes` measures the disk | `measure_blobs_dir(&blobs_dir(Path::new(store_dir)))` → `Ok(load_store(Path::new(store_dir))?.total_bytes())` | **5 red**, each on its harm axis: `HARM: a hand-edited index.json deleted checkpoints — left ["…389.json"]` (the command-level test, the reproduced figure exactly: 1 of 5), the same at store level, `HARM: … still dictates the store's reported footprint`, `HARM: a deflated index.json kept the byte-cap loop deleting past the cap — 4 left`, and the corrupt-index test on its `preview` leg |
| only hex-named regular files count | `if validate_blob_name(&name).is_err() {` → `if false && …` | red **alone**: `HARM: something that is not a content-addressed blob file contributed to the footprint` (15 expected, the decoys are 15 000) |
| `prune` measures what it removed | `freed = freed.saturating_add(size);` → `… .saturating_add(0);` | 2 red: `HARM: prune's freed figure does not describe the files it removed`, and the honest-cap pin |
| the pre-fix accounting **pair** (`prune` returns `release`'s figure **and** the loop subtracts it) — the realistic re-introduction, 2 lines | `let claimed = release(…)` + `Ok(claimed)`; `total = store_total_bytes(…)?` → `total = total.saturating_sub(freed)` | `HARM: prune reported index.json's claim as bytes freed` and `HARM: a deflated index.json kept the byte-cap loop deleting past the cap — 1 left` (of 4, when exactly 1 should have been pruned) |
| `capture` writes a reused blob whose file is gone | `for blob in plan.to_store.iter().chain(plan.reused.iter())` → `for blob in plan.to_store.iter()` | 2 red: `HARM: the checkpoint just taken cannot restore its content: …\blobs\313df9…: cannot find the file` and, through the commands, `HARM: a checkpoint reported as created held none of the file's content` |
| `load_store` above the point of no return | moved the `let mut store = load_store(store_path)?;` line back below the `remove_file` | red **alone**: `HARM: pass 1 destroyed a checkpoint on an unreadable index.json — 3 left` |

**Stated rather than glossed: the loop's re-measure is not independently red-proofable.** Once `prune`
reports the bytes it actually removed, `total.saturating_sub(freed)` and a fresh measurement agree by
construction, so breaking only the re-measure reds nothing. Its red-proof is the pair row above, which
is also the realistic regression (the two lines are exactly the pre-fix code). It is kept because it
removes a carried-forward number rather than bounding it — this ticket's own rule — and because it is
what holds if `prune`'s figure ever regresses. Recorded here so nobody reads that row as a one-line
proof it is not.

**The fixtures cannot pass against a dead tamper** — CPE-1861's three-sabotage check, run over all of
them. Making every tamper silently inert (the `index.json` size rewrite in all three helper copies, both
`refs` edits, both blob-file removals, the index truncation) gives **2 passed / 9 failed**, and all nine
fail on a `LIVE:` assertion, never on the harm. The two that pass are the two tests that have no tamper
by design: the measurement's exclusion list, and the over-tightening pin below.

**Over-tightening is pinned too**, because "measure the disk" would satisfy every test above by simply
never pruning: `cpe_1844_the_byte_cap_still_thins_a_store_that_is_genuinely_over_it` puts 4 × 200 bytes
against a 700-byte cap and asserts **exactly one** deletion, a 600-byte footprint after it, and
`bytes_freed == 200`.

### Gates

`crates/server`: `cargo clippy --all-targets -- -D warnings` → **exit 0**. `cargo test` (every target) →
**2339 lib** passed, 4 ignored + `ticket_mcp` 0 + `archive_panic_safety` 21 +
`binary_data_preview_panic_safety` 22 + `checkpoint_roundtrip` 2 + `finder_tags_os_interop` 1 +
`native_meta_os_interop` 1 + `parser_panic_safety` 45 + `sample_fixtures` 16 + `thumb_svg_panic_safety`
32 + doc-tests 0 — **0 failed**. The lib delta is **+11** on CPE-1861's 2328, accounted for rather than
asserted: this branch adds exactly 11 tests (5 in `snapshot_capture`, 4 in `snapshot_prune`, 2 in
`checkpoint_store`). Every integration binary is unchanged.

`src-tauri`, **both** feature modes: clippy default → **0**, `--features sidecar-platform` → **0**;
`cargo test` → **214**, `--features sidecar-platform` → **269** — identical to CPE-1861, as expected,
since nothing in `src-tauri` was touched.

**`bindings.gen.ts` regenerated and committed.** No struct or command *signature* changed, but the
`RetentionPreview::total_bytes` and `RetentionApplyResult::bytes_freed` **doc comments** did, and specta
emits field docs into the bindings — so CI's typed-bindings drift guard would have failed. Regenerated
with `cargo run --bin export_bindings --features "specta-bindings sidecar-platform"`; the diff is those
two comments and nothing else.

Frontend guards, after the markdown edit: `vitest run src/lib/docs.test.ts src/lib/sectionDocs.test.ts`
→ **11 passed** (9 + 2); `src/lib/mojibakeGuard.test.ts` → **62 passed**.

In-app docs: `src/docs/16-checkpoints.md` gains "How big the store thinks it is" under the existing
"Copying files inside the snapshot store" subsection — that the size figure driving deletions is
measured from the stored content rather than read from a bookkeeping file, that a snapshot taken over
missing content stores it again rather than assuming it is there, and that a damaged bookkeeping file
stops cleanup instead of guessing. No new `Section`, so `sectionDocs.ts` is unchanged.

### Not verified on this machine

- Everything here runs on Windows and there are no `#[cfg(unix)]` legs in this change, but the
  `blobs/` measurement's symlink behaviour differs by platform (`DirEntry::metadata` does not follow a
  symlink on Unix; on Windows the reparse point is reported), and the fixtures use ordinary files. The
  merge gate is `Server crates` green on **ubuntu and macOS**, as it was for CPE-1823/1847/1861.
- **The sparse-file residual is reasoned, not measured.** I did not stage a sparse blob file to confirm
  a large logical length with near-zero allocation drives the cap; it is recorded as a limit in
  `store_total_bytes`'s rustdoc rather than claimed closed.
- CPE-1863 is untouched by design; the interaction is argued and partly measured (the pair red-proof
  shows the loop reaching the floor), not exhaustively explored.

### 2026-08-22 — round 2: the headline fix was a lateral move, and it introduced a regression

The independent Reviewer approved with four findings; the Security Auditor returned MERGE with a first
item whose framing is the one that matters — **I replaced one hand-editable steering input with
another.** Both are folded in here. Everything below was reproduced on **my own round-1 fix** through
`checkpoint_prune_apply` / `checkpoint_create` / `checkpoint_revert` before anything was changed, rather
than taken from the reports:

```text
              honest    tampered preview   CMD prune_apply                       revert(oldest)
planted        45        2000000045        kept=1 pruned=4 freed=36 left=1        Err(cannot find the file)
hardlink       45         500000045        kept=1 pruned=4 freed=36 left=1        Err(cannot find the file)
orphan         45           4000045        kept=1 pruned=4 freed=36 left=1        Err(cannot find the file)
                                                                                  a.txt = "damaged" in all three
```

Byte-for-byte this ticket's own opening outcome, with **no `index.json` edit at all**. `planted` is
`File::create("blobs/dead") + set_len(2_000_000_000)`. `hardlink` is a hard link named `beef` to a
500 MB file outside the store — and it needed **no elevation** on this machine, where a symlink does, so
the residual note that named only the sparse file understated the cheapest primitive. `orphan` is a
4 MB file that is `capture`'s own documented partial-write residue, **with no attacker in the fixture at
all**.

**`orphan` is a regression round 1 introduced, and that is the worst of the three.** Before it the
orphan contributed 0 and the pass did nothing; after it, a crash residue deletes four of five
checkpoints. It is also *permanent* where the index tamper is not — `save_store` rewrites honest sizes
on the next capture, a stray file just sits there — and `bytes_freed = 36` is now *honestly* reported,
which makes the destructive pass read as more legitimate rather than less.

### The witness — reuse of `manifests_naming`, and where my round-1 reasoning was wrong

Round 1 argued against reusing CPE-1861's `manifests_naming` because it answers *identity* while I
needed *size*. That was right about the **figure** and wrong about the **filter**: the size question has
a prior question — *whose content is this?* — and that is precisely the question `manifests_naming`
recomputes from disk, and which `prune` already pays for on every call. `store_total_bytes` now measures
only blob files some manifest on disk still names. The planted file, the hard link and the orphan all
contribute **0**, and an index that is deflated, emptied or absent cannot inflate anything.

The figure it returns is therefore **reclaimable footprint** — the bytes deleting checkpoints could
actually free. That is not a softer definition, it is the only correct one for a cap *enforced by
deleting checkpoints*: counting bytes no prune can reach is a category error, and `orphan` is what that
category error costs.

**Residual, at full strength.** Inflation now needs a **matched pair** — a hex-named file with a large
logical length *and* a manifest naming that hash. `manifests_naming` is deliberately permissive (any
parseable manifest counts), so the second half is a file to write, not a gate to defeat. What the
witness buys is that the tamper is two coordinated files instead of one number, and that every
*accidental* shape is worth zero.

**The shared-predicate hazard, written into both call sites.** `prune` is safe when
`manifests_naming` is **generous** (a blob wrongly called named is merely leaked); this site is safe
when it is **stingy** (a blob wrongly called named inflates a figure that deletes checkpoints). Same
predicate, opposite failure directions — the "one predicate, two meanings" drift CPE-1861 warns about,
now with two callers on opposite sides. Tuning it for either caller breaks the other. That is why
`store_total_bytes` checks `manifests/` is **readable itself** instead of handing the question over:
`manifests_naming`'s own failure branch answers "all of them are named", the maximal footprint, at the
one site where maximal deletes checkpoints.

### Cost — it went up, and my numbers disagree with the audit's

Release build, one manifest per 20 blobs, against the `index.json` read this replaces (`dir-sum` is
round 1's witness-less version):

```text
blobs  manifests   index.json    dir-sum    witness   ratio
   50          3       550 us      72 us     318 us   0.58x
  200         10       661 us     223 us     630 us   0.95x
 1000         50      1147 us    1173 us    4189 us   3.65x
 5000        250      2394 us    6010 us   17101 us   7.14x
50000       2500     21223 us   60926 us  177226 us   8.35x
```

**This does not match the ~1.16x at n=5000 the audit reported, and I am recording both rather than
adopting the flattering one.** The ratio is driven by how much manifest JSON there is per blob, so a
lighter fixture measures far cheaper; mine is heavier. Not gated, and the arithmetic is written next to
the function: the shipped default `RetentionPolicy` caps a store at ~47 manifests (CPE-1861's figure),
about 940 blobs at 20 files each — the ~4 ms row. The unattended caller passes `None`, so it pays this
once per `preview` and never inside the loop. It also **cannot short-circuit** the way `prune`'s use
does: `prune` asks about a handful of at-risk hashes, this asks about every blob in the store.

### The four Reviewer items

1. **The over-broad claim, narrowed and then corrected.** "Nothing here makes it worse" was scoped to
   the *subtraction* and read as scoped to the whole change. It is now stated as such — and the S2
   orphan case is recorded beside it as the measured falsification of the broader reading, since an
   un-reclaimable residue is a shape the old code could not see at all rather than an over-credit. This
   is CPE-1861's round-3 blocker class (a comment of mine stating something false), caught before merge
   this time.
2. **My red-proof count was wrong.** `freed.saturating_add(0)` reds **5 lib-wide**, not 2 — I ran the
   `cpe_1844` filter and reported the filtered count. Verified myself: my two plus
   `prune_gcs_blobs_no_longer_referenced_and_keeps_shared_ones`,
   `apply_keeps_gfs_survivors_and_they_still_restore_byte_for_byte` and
   `cpe_1861_prune_never_frees_a_blob_another_manifest_file_still_names`. The guard is better protected
   than I claimed. Every count below is now stated as "within the filter / lib-wide".
3. **The honesty point moved into the code.** The comment explained why the re-measure exists but never
   said *no test will catch you if you delete it alone*. It says so now, at the line, with its red-proof
   named — in a module where every other guard carries its own. The Reviewer also found a better reason
   to keep it than I gave: the two are **not** equal by construction, they diverge whenever the initial
   measurement under-counts something `prune` then removes and credits, and the re-measure is the safer
   side. Recorded.
4. **CPE-1861's invariant sentence** now reads "…past its point of no return **(with one store-level
   exception, below)**", so a reader who stops at the bolded claim no longer reads a false one.

### Two more landed claims corrected

- **"Dedup goes back to being an optimisation rather than a promise" over-claimed.** The repair covers
  **absent** only. A blob file that is *present* but whose bytes were replaced is `Occupied` by the slot
  probe and skipped, so a restore hands back whatever is now in the file — the audit measured
  `restored bytes = "PLANTED BYTES"`. Pre-existing (that probe policy is CPE-1705's and CPE-1769's, and
  re-hashing every reused blob per capture is a different ticket's cost), but the sentence claimed a
  property the repair does not deliver. Rewritten to the one it does: *a claim about content the store
  does not hold is no longer acted on by writing nothing.*
- **The residual note named only the sparse file.** The hard link is cheaper — no API, no flag, no
  privilege — and is now named first, with the measured 500,000,000.

### Two recording-grade findings, no code

- **`preview` and `apply` now disagree** about a store whose `index.json` is corrupt: removing that file
  from `store_total_bytes` removed `preview`'s only reader of it, so `preview` succeeds while `apply`
  refuses inside `prune`. Pre-PR both refused. Non-destructive either way and the honest preview is the
  more useful half, but it is a new asymmetry rather than an intended design. Written next to
  `store_total_bytes`.
- **A thirteenth sink my walk could not have found**, because it walked `index.json`'s fields and
  `prune`'s gates: the **manifests' own** `size` field, summed into the revert preview's `bytes_written`
  at `restore_plan::summarize_plan`. An honest `12` becomes `9000000000`. Recorded at the function, not
  fixed, and the reason is this ticket's own test: **that figure authorises nothing** — it is displayed
  beside a confirm, and no deletion or write is taken from it, unlike `store_total_bytes`.

### The liveness result inverts, and the corrected number is worse than I reported

The audit applied CPE-1823's own copied-fixture trap to my helpers — read the real `index.json`, write
and verify a **decoy sibling** — and my round-1 claim of *2 passed / 9 failed* became **9 passed /
2 failed**, with three headline tests certifying nothing, including both command-path ones. My two
`snapshot_capture` tests survived only because they asserted through `load_store`, a production reader,
at their call sites.

Fixed by folding that assertion into all three tamper helpers (`load_store` is now `pub(crate)` so the
other two modules can reach the same production reader). Re-run here:

- **Decoy-sibling trap:** all **5** index-tampering tests now red on `LIVE: the tamper never reached
  index.json as the production reader sees it` / `…the record the old figure was read from`. Three of
  them previously passed.
- **Every tamper made inert** (both index helpers, both `refs` edits, both blob removals, the index
  truncation, the planted file, the orphan, the hard link, the manifest copy): **2 passed / 12 failed**,
  and after one fix all 12 fail on a `LIVE:` assertion. The one that did not —
  `cpe_1844_a_blob_named_only_by_a_duplicate_manifest_still_counts` — died on an `unwrap` instead, which
  is a crash rather than a certificate; it now carries an explicit liveness assertion that the duplicate
  really protected the blob. The 2 that pass are the 2 tests with no tamper by design (the exclusion
  list, and the over-tightening pin).

### Round-2 red-proofs — eight guards, each observed red then reverted

| Guard | Line broken | Observed |
|---|---|---|
| the witness | `manifests_naming(store_path, &candidates)` → `candidates.clone()` | 2 red: `HARM: a blob file no manifest names steers the store's footprint` and, through the commands, `HARM[planted]: a file dropped in blobs/ deleted checkpoints — 1 left` |
| `store_total_bytes` measures at all | early-`return Ok(load_store(store_path)?.total_bytes())` | **5 red** incl. `HARM: a hand-edited index.json deleted checkpoints — left ["…132.json"]` (1 of 5) |
| hex-name / type filter | `if validate_blob_name(&name).is_err() {` → `if false && …` | red alone: `HARM: something that is not a content-addressed blob file contributed to the footprint` |
| unreadable-witness refusal | `Err(e) => return Err(...)` → `Err(_e) => {}` | red alone: `HARM: an unreadable witness counted every blob file, which is the figure that deletes checkpoints`. **This guard was uncovered until this round** — the round-1 suite stayed green under it, found by running the proof rather than by reading. Now staged the way `classify_store_index` stages its own case: a **non-directory** at `manifests/`, which fails `read_dir` as not-`NotFound` on every platform without an ACL or `chmod` the two OSes disagree about |
| `prune` measures what it removed | `freed = freed.saturating_add(size);` → `…(0);` | 2 red within the filter, **5 lib-wide** (the three pre-existing ones named above) |
| `capture` writes a missing reused blob | `…iter().chain(plan.reused.iter())` → `plan.to_store.iter()` | 2 red, both layers |
| `load_store` above the point of no return | that line moved back below `remove_file` | red alone: `HARM: pass 1 destroyed a checkpoint on an unreadable index.json — 3 left` |
| the accounting **pair** (2 lines, the exact pre-fix code) | `Ok(release(..))` in `prune` + `total = total.saturating_sub(freed)` | `HARM: prune reported index.json's claim as bytes freed`; `HARM: … deleting past the cap — 1 left` of 4 |

### Round-2 gates

`crates/server`: clippy `--all-targets -- -D warnings` → **0**. `cargo test` → **2343 lib** (4 ignored)
+ ticket_mcp 0 + 21 + 22 + 2 + 1 + 1 + 45 + 16 + 32, **0 failed**. Lib delta **+4** on round 1's 2339
(**+15** on CPE-1861's 2328): this round adds exactly 4 tests — 3 in `snapshot_capture`
(no-manifest-names, duplicate-namer, unreadable-witness) and 1 in `checkpoint_store` (the command-level
dropped-file test). Every integration binary unchanged.

`src-tauri` both modes: clippy **0** / **0**; tests **214** / **269** — unchanged again.
`bindings.gen.ts` regenerated and byte-identical to round 1's commit (no `specta::Type` doc touched this
round).

Frontend guards: `docs.test.ts` + `sectionDocs.test.ts` + `mojibakeGuard.test.ts` → **73 passed**.
`src/docs/16-checkpoints.md`'s "How big the store thinks it is" is rewritten for the witness: the
figure is the space the user's snapshots are actually using, stray leftovers count for nothing because
deleting snapshots could never clear them, and unreadable snapshot records stop cleanup rather than
making it assume everything matters.

### Round-2 — not verified on this machine

- The hard-link leg is skipped rather than failed where `fs::hard_link` is unavailable (a filesystem
  without hard links); it ran here on NTFS. The planted and orphan legs run everywhere.
- The **sparse-file** residual is still reasoned, not measured — `set_len` gives a large logical length
  without a sparse flag, which is what the tests use and what the measurement reads, but I did not
  confirm near-zero *allocation* separately.
- The unreadable-`manifests/` case is staged as a non-directory, not as a permission denial; the
  permission shape is the one `classify_store_index` records as unstageable on both platforms.
- CPE-1863 remains untouched; the orphan case is handed to it as directed.

### 2026-08-22 — round 3: records only. The witness holds; three of my own numbers did not

The re-audit returned **MERGE**: both round-1 criticals are closed at the command path and the
no-attacker regression is genuinely gone, measured through the registered commands —

```text
AUDIT R2-8 [plant]               preview.total_bytes=45          pruned=0  manifests_left=5
AUDIT R2-8 [orphan]              preview.total_bytes=45          pruned=0  manifests_left=5
AUDIT R2-8 [plant-with-manifest] preview.total_bytes=2000000045  pruned=4  manifests_left=2
```

— and the decoy-sibling trap re-run against the round-2 helpers reds all five index-tampering tests on
`LIVE`, including the three that previously certified nothing, with the fix noted as being in the right
place (inside the helpers rather than patched per test). No code changed this round.

**The cost dispute is resolved, and neither figure in this log was the right one.** The driver is
**total manifest JSON bytes — manifests × files-per-tree — not blob count**. The audit's ~1.16x measured
the *dir-sum* with no manifest parsing in it at all, so it was never a witness figure. My 8.35x used
2,500 manifests, which the shipped 24/7/4/12 policy can never produce, so it overstates. Both internally
correct, neither plannable. Measured on the shape the default policy actually produces — 47 manifests,
each listing a whole tree, blobs shared:

```text
manifests x files   manifest JSON   witness    ratio
    47 x     20          68 KB        105 us    2.6x
    47 x    200         648 KB        343 us    3.7x
    47 x  2,000         6.5 MB       3199 us    4.3x
    47 x 10,000        32.9 MB      15541 us    5.1x
```

**~16 ms is the number to plan against** — a 10,000-file tree under the default policy, about 5x the
index read it replaces and just under the 18.3 ms this crate already accepts for `manifests_naming`
inside a single `prune`. My non-gating argument survives, and the mechanism it rests on was confirmed:
with `wanted` = every blob in the store the early-exit essentially never fires, so all 47 manifests are
parsed on every call. The table in `store_total_bytes`'s rustdoc is replaced with this one and cites the
policy shape; the round-2 table above is left standing as the record of a figure that was correct about
its own fixture and wrong about the store.

**My residual note understated the matched pair in two ways, both measured.**

- **The witness manifest does not scale with the plant.** It is **122 bytes** for one hash, and one
  manifest can name any number: a single **8 KB** manifest validated **200** planted blobs — 200 GB of
  claimed footprint. "A matched pair" must not be read as "a pair per blob"; the second half is a fixed
  cost regardless of how large the inflation is.
- **It is invisible and permanent.** Give the planted manifest an inner `id` disagreeing with its
  filename and CPE-1861's rule makes `list_manifests` skip it — never shown in the UI, never a prune
  candidate — while `manifests_naming`, deliberately the permissive one, still honours it. The two
  CPE-1861 halves compose into a witness nothing can see and nothing can remove. The measured row shows
  it: six manifest files in, four pruned, **two left** — one real survivor plus the planted witness,
  re-pinning the one-survivor floor on every future pass.

My "a file to write, not a gate to defeat" framing was accurate as far as it went; it is now written at
full size — an attacker who can already write into the store gets an arbitrary footprint for about 8 KB,
undetectably. That is still not a reason to tighten `manifests_naming`, which would re-open CPE-1861's
blob-deletion hole for `prune`.

**The `bytes_written` disposition was right, and the reason is sharper than I stated it.** The line is
not "does a human read it before a destructive action" — it is **whether the code branches on the
number**. `store_total_bytes` selects which checkpoints to delete; `bytes_written` is rendered and
discarded, and the destructive content of that confirm is the create/overwrite/delete counts and paths,
which come from the plan and not from `size`. So "it authorises nothing" is correct *and correctly
scoped*, and the looser test would have dragged this in wrongly. Added at `summarize_plan`, with the
follow-up framed on the inverse case: **`0` is the dangerous edit, not `9000000000`** — an implausibly
huge figure is loud and gets questioned, while a preview saying a revert writes *nothing* is the shape
that gets confirmed without being read. That is the case to test first if it is ever revisited.

**My check-then-walk note said "except in a race", which reads as a dismissal, and the race is measured
reachable.** With a thread renaming `manifests/` away and back, out of 30,000 calls the fallback fired
and returned the full 2 GB directory sum — round-1 behaviour. The bound I stated held exactly ("at most
the directory sum"), and it grants an attacker nothing they do not already have, since anyone who can
rename `manifests/` can plant the 122-byte witness instead, quieter and deterministically. **Filed
separately and deliberately not fixed here** — the close is a `manifests_naming` variant returning its
`read_dir` failure instead of falling back, so the site opens the directory once instead of twice. The
rustdoc now records it as measured rather than hypothetical.

**Also confirmed rather than trusted**, and worth keeping because it bounds the claim: every "readable
but yields nothing" and "partially readable" shape **under**-counts, which is the safe direction here;
and the framing that `manifests_naming` fails *open* for `prune` and would fail *closed-wrong* here is
correct, with the pre-check the right place to split them.

### Round-3 gates

No code changed — comments, one rustdoc table, and this log. `crates/server`: clippy
`--all-targets -- -D warnings` → **0**; `cargo test` → **2343 lib** (4 ignored) + ticket_mcp 0 + 21 + 22
+ 2 + 1 + 1 + 45 + 16 + 32, **0 failed** — identical to round 2, as expected. `src-tauri` both feature
modes: clippy **0** / **0**; tests **214** / **269**. No `specta::Type` struct or command signature
touched, so `bindings.gen.ts` is unaffected and unchanged.
