# cross-platform-explorer Ticket System — Wiki

## Purpose

Single source of truth for bugs, tasks, and feature requests for cross-platform-explorer
(the Cross-Platform Explorer app). Filed by the user, worked by the Claude Code agent using
the rules below. Tickets are plain markdown files versioned in git — there is no external
tracker or IDE project integration to keep in sync.

---

## Folder Structure

```
Ticketing/
  wiki.md        <- workflow rules (you are here)
  _template.md   <- copy to Tickets/Backlog/ to start a new ticket
  Epics/         <- umbrella trackers, decomposed just-in-time (SIBLING queue — see "Epics" below)
    Backlog/       <- Proposed: dormant briefs, not yet decomposed
    Doing/         <- In Progress: activated epics (several may be active at once)
    Blocked/       <- epics gated EXTERNALLY (normally empty)
    Deferred/      <- epics WE postponed by choice / on an internal prereq (normally empty)
    Done/          <- closed epics — FLAT, never dated-nested
  Sprints/       <- time-boxed batches of tickets (SIBLING queue — see "Sprints" below)
  Tickets/       <- the status-flow queue (folder = status)
    Backlog/       <- open tickets waiting to be worked
    Doing/         <- ticket the agent is currently working (one at a time)
    Blocked/       <- tickets deferred on an EXTERNAL gate (can't be worked until it clears)
    Deferred/      <- tickets WE postponed by choice / on an internal prereq (pickable anytime)
    Done/          <- closed tickets — never deleted
```

The folder a ticket lives in IS its status. The `status:` frontmatter field mirrors it.

`Tickets/` and `Epics/` deliberately have the **same five status folders** (CPE-1676) so both queues
read the same way and the same mental model applies to each; only the status vocabulary differs
(`Backlog/` means `Open` for a ticket and `Proposed` for an epic). Two guard tests in
`src/lib/epicsQueueLayout.test.ts` hold the Epics queue to it: nothing loose in `Ticketing/Epics/`,
and no `status:` that disagrees with its folder.

---

## ID Scheme

Format: `CPE-NNN` (zero-padded three digits — `CPE-001`, `CPE-042`, `CPE-100`).
Sequential. To find the next ID: scan all folders for `CPE-*.md`, read the highest NNN, add 1.

**Sprints use a separate sequence:** `SPR-NN` (zero-padded two digits — `SPR-01`, `SPR-02`). Scan
`Ticketing/**/SPR-*.md` for the highest NN, add 1. Sprints never take a `CPE` id.

### Allocating an ID while other work is in flight

"Scan for the highest, add 1" is only correct against a tree nobody else is writing to. During a sprint —
or any time several agents or both surfaces are active — **each worker scans its own branch and they all
pick the same number.** This happened twice in one day (2026-08-13), both times because a ticket was filed
on `main` while a worker was filing one on a branch.

Rules that prevent it:

1. **`git pull` immediately before allocating**, and allocate against `main`, not your branch. A number is
   only reserved once it is on `main`.
2. **Push the new ticket file promptly** — a ticket sitting unpushed on a branch has not reserved anything.
3. **If you are coordinating workers, say which numbers you have taken** when you take them. A worker
   cannot see your unpushed or just-pushed file.
4. **On a collision, the copy already on `main` keeps the number.** The other renumbers — and must then
   grep the whole repo for references: the PR body, work logs, `Related:` lines, and code comments. A stale
   reference now pointing at a *different* ticket is worse than no reference at all.

## File Naming

`CPE-NNN_short-kebab-title.md` — the filename never changes when a ticket moves folders.

---

## Ticket Frontmatter

```yaml
---
id: CPE-NNN
title: Human-readable title (sentence case)
type: Bug | Defect | Task | Feature | Test
status: Proposed | Open | In Progress | Blocked | Deferred | Done | Won't Fix | Duplicate
priority: Low | Medium | High | Critical
component: Frontend | Backend | Updater | CI | Packaging | Docs | Multiple
tags: [<disposition tag>, ...]   # at least one — see Disposition Tags below
estimate: 15m | 30m | 1h | 1-2h | 2-3h | 3-4h | 4h+
created: YYYY-MM-DD
closed: YYYY-MM-DD
epic: CPE-NNN                     # optional — present on a child ticket, naming its parent epic
sprint: SPR-NN                    # optional — present when the ticket is assigned to a sprint
---
```

### Components
| Component | Area |
|-----------|------|
| Frontend | Svelte UI in `src/` |
| Backend | Rust / Tauri commands in `src-tauri/` |
| Updater | auto-update pipeline (updater plugin, signing, latest.json) |
| CI | GitHub Actions workflows |
| Packaging | installers, bundling, icons |
| Docs | README, CLAUDE.md, RELEASING.md, website |
| Multiple | spans more than one of the above |

### Types
| Type | When to use |
|------|-------------|
| Bug | Worked before, now broken |
| Defect | Never worked correctly |
| Task | Implementation, refactor, cleanup, infrastructure |
| Feature | New capability |
| Test | Adding or fixing tests |

### Priority
| Priority | Meaning |
|----------|---------|
| Critical | App crashes, data loss, or release/updater pipeline fails |
| High | Core feature broken; workaround is painful or absent |
| Medium | Feature works but behaves incorrectly |
| Low | Cosmetic, minor inconvenience, nice-to-have |

### Disposition Tags

`tags:` is a **controlled vocabulary** describing a ticket's *disposition* — why it is or isn't
workable now — orthogonal to status (folder), priority, type, and component. **Every ticket carries
at least one** disposition tag, and it is shown as a **Tags** column whenever tickets are listed.
Keep tags current: when the situation changes (a prereq lands, a decision is made), retag.

| Tag | Meaning |
|-----|---------|
| `ready` | Actionable now with resources on hand — no blocker. Mutually exclusive with the blocked/prereq/decision tags below. |
| `big-design` | Substantial; needs a design pass (decisions baked into the design) before coding. |
| `needs-decision` | Blocked on a product/UX decision from the user — record the open question in Notes. |
| `needs-prereq` | Depends on another unbuilt ticket/feature — name it in Notes. |
| `epic` | Umbrella tracker, not a single unit of work; lives in `Ticketing/Epics/` (a separate queue), decomposed just-in-time, closes when its children do. See "Epics" below. |
| `resource-blocked` | Needs something the agent can't access in this environment. **Always pair with a qualifier below.** |

Qualifiers for `resource-blocked` (add alongside it):

| Qualifier | Requires |
|-----------|----------|
| `needs-macos-linux` | A macOS/Linux machine to build or verify. |
| `needs-cert` | Purchased / identity-verified certificates. |
| `needs-reference` | An external reference repo or data source. |
| `needs-device` | Specific hardware / a physical device. |
| `needs-heavy-dep` | A non-pure-Rust / native / bundle-heavy dependency that can't be validated headlessly here. |

Rules:
- Exactly one *primary* disposition (`ready` · `big-design` · `needs-decision` · `needs-prereq` ·
  `epic` · `resource-blocked`); qualifiers are additive.
- `resource-blocked` MUST carry ≥1 qualifier so the listing says *what* is needed.
- New primary/qualifier tags are added here first, then used — don't coin ad-hoc tags in tickets.

---

## Status Lifecycle

```
Backlog/ (Open) -> Doing/ (In Progress) -> Done/ (Done | Won't Fix | Duplicate)
                        |
                        +-> Blocked/ (Blocked)   <- EXTERNAL gate; returns to Backlog/ when cleared
                        |
                        +-> Deferred/ (Deferred) <- OUR choice / internal prereq; pick up anytime
```

Only one ticket in Doing/ at a time under normal circumstances.
To reopen: move from Done/ back to Backlog/, set `status: Open`, add a Work Log note.

**Blocked vs Deferred** — both are non-terminal side states, but they differ by *cause*:
`Blocked/` is an **external** gate we can't clear by working (certs, macOS/Linux hardware, a paid
plan, a third party, a date) — not pickable until it clears. `Deferred/` is a **deliberate
postponement** by us — usually waiting on an *internal* prerequisite ticket, or deprioritized to
revisit later — and it stays pickable at any time (`/ticketing-work` un-defers it). Never close
either as Won't Fix; they are postponed, not declined. See each folder's `wiki.md`.

---

## Epics (a separate queue, decomposed just-in-time)

An **epic** is a headline goal too big for one unit of work (a Mega-Feature, or anything that will
clearly spawn many child tickets). Epics are managed by the **`ticketing-epic`** skill and live in
their own queue, **`Ticketing/Epics/`** — never in `Tickets/`. That queue has the **same five status
folders** as `Tickets/`, and there too the folder IS the status.

**The core rule: no research, planning, or sub-ticketing until an epic is *activated*.** A dormant
epic is a one-page brief — goal, rough scope, open questions, maybe an epic-level Definition of Done —
and nothing more. Up-front breakdown rots as scope drifts and clutters the backlog with speculative
work. Pulling an epic from the queue IS the decision to invest in planning it.

Lifecycle:

| Stage | Folder / status | What exists |
|-------|-----------------|-------------|
| **Proposed** | `Epics/Backlog/`, `status: Proposed` | Just the brief. No children, no research. |
| **Active** | `Epics/Doing/`, `status: In Progress` | Activated: decisions resolved, child tickets created in `Tickets/Backlog/` (each with `epic: CPE-NNN`). |
| **Blocked** | `Epics/Blocked/`, `status: Blocked` | Gated by something EXTERNAL. Normally empty. |
| **Deferred** | `Epics/Deferred/`, `status: Deferred` | Parked by OUR choice / an internal prereq. Normally empty. |
| **Done** | `Epics/Done/`, `status: Done` | All children Done + the epic's Definition of Done met. |

- `status: Proposed` is **epics-only** — it marks a dormant, not-yet-decomposed brief.
- **`activate`** (in `ticketing-epic`) is the *only* place an epic is decomposed: research → resolve
  `needs-decision` questions with the user → create `epic:`-linked children in `Tickets/Backlog/` →
  `git mv` the epic `Epics/Backlog/ → Epics/Doing/` and set `status: In Progress` in the same edit.
- `Epics/Doing/` is **not** one-at-a-time the way `Tickets/Doing/` is — several epics can be active.
- `Epics/Done/` is **flat**: `/ticketing-organize` nests `Tickets/Done/` by date because it holds
  thousands of tickets; there are ~70 epics in total, so it must never touch `Epics/Done/`.
- Epics are **never** put in `Tickets/` and **never** built by `/ticketing-work` (it redirects to
  `ticketing-epic activate`). Only an epic's *children* are worked, as ordinary Backlog tickets.
- Every child carries an `epic: CPE-NNN` frontmatter field so progress is countable and the epic
  closes exactly when its children (and DoD) do.
- Epics closed **before** CPE-1676 were filed into `Tickets/Done/` and were left there; both boards
  therefore read epics from `Epics/**` *and* from `Tickets/Done/`.

---

## Sprints (a separate queue, time-boxed)

A **sprint** is a **named, time-boxed batch of tickets** worked together toward a near-term goal — the
"what are we doing now/next" grouping. Sprints are managed by the **`ticketing-sprint`** skill and live
in **`Ticketing/Sprints/`**, ids **`SPR-NN`** (a separate sequence from `CPE-NNN`).

Sprints are **orthogonal to epics**: an epic is a *thematic* umbrella; a sprint is a *time-boxed*
selection that can pull tickets from **any** epic or none. A ticket may belong to both at once — it can
carry an `epic:` **and** a `sprint:` field.

Lifecycle:

| Stage | Folder / status | What exists |
|-------|-----------------|-------------|
| **Planned** | `Sprints/`, `status: Planned` | A named, dated sprint queued behind the current one. |
| **Active** | `Sprints/`, `status: Active` | The current focus (convention: one Active at a time). |
| **Closed** | `Done/`, `status: Closed` | Time-box ended / all members Done; carry-overs documented. |

- **Membership is the `sprint: SPR-NN` frontmatter field** on member tickets — authoritative and
  countable (glob tickets whose `sprint:` names the sprint to get `X of Y Done`). The sprint file's
  `## Tickets` list mirrors it; keep them in step on `assign`/`remove`.
- A sprint **never works tickets itself** — its members are ordinary tickets worked via
  `/ticketing-work`; the sprint is a lens over them.
- **When listing tickets, ALWAYS show Sprints alongside Epics** (user preference) — see
  `ticketing-list`.

---

## Ticket Body Sections

| Section | Required | Who writes it |
|---------|----------|---------------|
| Summary | Always | User |
| Environment | Bugs/Defects | User |
| Steps to Reproduce | Bugs/Defects | User |
| Expected Behavior | Bugs/Defects | User |
| Actual Behavior | Bugs/Defects | User |
| Acceptance Criteria | Always | User |
| Resolution | On close | Agent |
| Work Log | Throughout | Agent (append-only) |
| Notes | Optional | Either |

**Work Log format** — one line per entry, appended throughout (not just at close):
```
YYYY-MM-DD — Short description of discovery, decision, or action.
```

---

## Evidence Rules

Four rules about *proof*, not about code. They apply to every ticket, and to the PR body and review
that close it. Each exists because the crew has broken it and paid for it.

### 1. Guard neutralisation — a test that cannot fail is not evidence

Every new guard must be **broken on its own** and shown to make a **distinct** test go red, then
restored and shown green. Break one guard at a time: a change that reds five tests at once has proved
nothing about which guard is load-bearing. Paste the **actual** red output — the real panic message,
not a description of it — into the PR body. "The test passes" says nothing until you have seen it fail
for the right reason.

**Restore with `git checkout --`, not by copying a backup over the file.** `Copy-Item` (and `cp -p`, and
most editor "revert") preserve the source's timestamp, and `cargo` decides whether to rebuild by comparing
timestamps. Restoring a backup whose timestamp is *older* than the broken build leaves the broken binary in
place, so the suite reports the state you just undid. This bites in **both** directions — a false red after
the restore, or a false green if you restore before capturing the break — which makes it worse than an
ordinary flake: the mandated "restore and confirm green" step is exactly the one it corrupts. If you must
restore by copy, touch the file first, and check the run actually printed `Compiling` before you trust it.

**Assert the harm before unwrapping the `Result` (CPE-1743).** A test of the shape
`let err = op(...).expect_err("...")` **then** `assert!(state survived, "...")` cannot fail the way its
name promises: if the guard under test ever fails by returning `Ok` instead of `Err`, the run stops at
`expect_err` and the harm assertion — the one carrying the actual damage — never executes. The test
still reds, but on "expected an error, got `()`" rather than on "the user's files are gone", which looks
like working coverage and is not. Capture the outcome, assert the harm, **then** unwrap:
`let outcome = op(...); assert!(state survived, "... (outcome was {outcome:?})"); let err =
outcome.expect_err("...");`. Interpolating `outcome` into the harm message is part of it — it names the
damage and the cheerful success in one line. The "harm" is not always a file — a request counter proving
*no request was sent*, or *how many pages were accepted*, is the same assertion and goes above the
unwrap for the same reason. This is the subtler sibling of rule 1's own headline ("a test that cannot
fail is not evidence"): CPE-1743 found **at least twelve** instances of it in one file — three
successive scans of that one file each undercounted (six, then ten, then twelve), so treat any count
here as a floor, not a total — immediately after a round that had just fixed the same shape twice
elsewhere, which is why the rule alone — already written, already known — was not enough on its own to
stop a sharp-eyed reviewer from reintroducing it. Treat "does the harm assertion run when the guard
fails the *observed* way, not just some way" as part of guard-neutralisation review, the same as picking
which guard to break.

**Beware what "neutralise the guard" means.** Flipping the boolean the test names is a faithful fault;
writing new destructive code inside the neutralisation is not — it proves the assertion would fire
against an implementation that does not exist. If a reorder can only be proven by inventing production
code, say so rather than pasting the red as evidence. CPE-1743's own review ran into this: a worker, an
independent Reviewer, and an independent UAT all initially accepted a "recursive-delete walker" invented
inside a test-only neutralisation as proof that a non-recursive, single-key `delete` would destroy a
subtree it structurally cannot reach, before a minimal-fault re-check caught it.

### 2. Verify through the channel that will carry the message

Prove a thing works **the way it will actually be used**, not the way that shows it most easily. A
check run under conditions the real caller never sets has confirmed nothing about the real caller.

The rule came out of CPE-1678 (PR #865), where the same failure appeared three times inside a PR whose
own subject was that failure:

- A sweep searched `read_to_string`/`fs::read` and concluded **"this is the only instance"**. The claim
  was wider than the search — the sibling bug was an `fs::metadata` collapse, which that search could
  not find (CPE-1687).
- A skip-notice was verified with `cargo test -- --nocapture` and a comment written asserting the CI log
  would show it. CI runs plain `cargo test`, and that notice used `eprintln!`, **whose output libtest
  replaces with a capture buffer that is replayed only for FAILING tests** — a skip is a pass, so the
  notice reached nobody. **The mechanism is the macro, not the stream:** the capture lives inside
  `print!`/`eprint!`, so a direct `writeln!(std::io::stderr(), ..)` is *not* captured and does reach the
  log. Stating it as "libtest captures stderr" is the over-generalisation that later produced CPE-1717's
  wrong premise, and propagated into an unrelated file in `main` citing CPE-1717 as its authority. See
  §3 for the measurement.
- The follow-up ticket's acceptance criteria then carried the intent ("the test must announce itself")
  forward **without the mechanism**, which would have handed the next person the same trap.

None of those was carelessness. Each was a true observation generalised one step past its evidence,
which is what makes the failure worth a rule instead of a shrug. In practice:

- **State the scope of a negative result.** "I found none" is only ever "I found none *within X*" —
  write the X down. A bare "there are no others" is a claim you have not tested.
- **Run the real invocation.** If CI runs `cargo test`, verify under `cargo test`. Flags, env vars and
  local config that make a signal visible can be the only reason it is visible.
- **A ticket must specify the mechanism, not just the goal**, wherever the obvious implementation of the
  goal is the bug. Otherwise the AC propagates the trap.

Related: the recurring product rule these keep proving — *a confident wrong answer is worse than an
honest "I don't know"* (CPE-1673, CPE-1678, CPE-1680, CPE-1687). These two rules are that same idea
applied to our own evidence rather than to the app's error messages.

### 3. A skip must be *consequential*, not merely visible (CPE-1717)

The bullet above about `--nocapture` is true history, and it was then over-generalised into "libtest
eats a skip notice, full stop". **Measured, with a one-test harness run with no flags at all:**

```text
running 1 test
VIA-WRITELN-STDERR: this is the CPE-1705/1710 shape
VIA-WRITELN-STDOUT: control
test passing_test_that_announces_a_skip ... ok
```

The same test's `eprintln!` and `println!` lines are **absent**. libtest's capture is a thread-local
swap installed *inside* the `print!`/`eprint!` macros, so a direct write to the process's stderr
handle goes around it. So:

- **`eprintln!` at a skip notice reaches nobody** — nor do `eprint!`, `println!` or `print!`; the
  capture sits in all four. Use `cpe_server::skip_notice!(..)`: same arguments, capture-proof.
  `fsutil`'s `skip_notices_never_use_a_captured_print_macro` scan fails the build on **one recognised
  shape**: in test code, a captured macro whose literal contains one of an enumerated `SKIP_PHRASES`
  vocabulary. It does **not** close the class. A notice phrased outside that vocabulary, one assembled
  into a variable first, or a test module not called `mod tests`, all still pass it — and that is not
  hypothetical twice over. Its first version let four shapes through, including
  `eprintln!("[CPE-1692] SKIPPED …")`, the one 56 sites actually use; and a real notice reading *"NOTE
  …: no symlink privilege here, so only the regular-file and hard-link forms were verified"* survived
  **both** the conversion sweep and that first scan, because the sweep and the scan shared one search
  and therefore one blind spot. **A search cannot audit itself** — that is the reusable lesson, and it
  is why the claim here is scoped rather than "the scan catches new ones".
- **A visible notice is the floor, not the goal.** A *passing* leg with a notice inside a 2,100-test
  log is a green board over zero coverage, and nobody reads a green log. Where the staging mechanism is
  supposed to work on that platform, use `cpe_server::fsutil::require_staged(..)` so the leg goes **red
  under CI** instead. Where it genuinely cannot work — the traversal deny on Windows, an ACL test on
  Linux — `supported_here = false` keeps the quiet skip, and that distinction is the whole design.
- **Prove the enforcement, don't assert it.** `CPE_STAGING_SABOTAGE=1` makes every staging attempt
  report failure; CI's "skip-visibility guard" steps run a filtered `cargo test` under it and **fail if
  the tests pass**.

### 4. Pin a verdict to the **code**, not to the commit (PR #901)

A review approves *code*. A commit sha changes when a comma moves in a Markdown file; the **tree hash**
changes only when the tree does. Recording "APPROVE at `<sha>`" therefore invalidates itself for reasons
that have nothing to do with what was reviewed — and the fix for that (re-review, note the new sha, push
the note, invalidate again) is a recursion with no base case. PR #901 spent three review rounds inside it,
each one a record *of the record* rather than of the code.

**So:** state the verdict against the hash of the subtree you actually reviewed —

```
git rev-parse HEAD:crates      # 644762d648a8b1bd87bdb28a6554c1b1d745a53f
```

— and a later commit that only edits the ticket, the PR body, or a doc outside that subtree **carries the
verdict forward unchanged**, provably, without anyone re-reading anything. If the hash differs, the code
differs and the review genuinely is stale.

Name the subtree you hashed. `HEAD:` — the **root tree** — defeats the purpose, though not because "that
is what the sha already tracks": the sha also moves on an amend, a message edit, or an empty commit, while
the root tree does not. The real reason is that **tickets and docs live inside the root tree**, so it moves
for exactly the edits you want to ignore.

**Two limits, or the rule over-promises the way the things it guards against do.**

- **It carries the code verdict, not a licence to skip reading.** A doc-only commit cannot break the build,
  but it can *misrepresent the review* — PR #901's `a2a010e9` left `crates/` untouched and asserted a
  verdict nobody had given, caught only because the reviewer read it anyway. Rule 4 closes **stale**
  approvals; **invented** ones still need eyes. #901 produced one of each.
- **CI re-runs regardless.** Every push restarts the whole matrix, whatever it touched. Writing up this
  rule discarded a 40-minute Windows job that was 40 minutes from finishing. Batch doc commits with the
  code, or accept the restart — but do not expect a green run to survive a typo fix.

### The merge gate is a guard too, and it was wrong three times in one morning

```sh
gh pr checks 901 | grep "Server crates" | grep -qv pending          # 1
[ "$(gh pr checks 901 | grep -c 'Server crates.*pending')" -eq 0 ]  # 2
c=$(gh pr checks 901) || exit 1                                     # 3
```

| # | what it actually asks | wrong when |
|---|---|---|
| 1 | is **any** leg non-pending? | one green, two queued → **opens** |
| 2 | are **no** legs pending? | `gh` prints nothing → **opens** |
| 3 | did `gh` succeed? | `gh pr checks` exits non-zero *while pending* → **false alarm** |

Each fixed its predecessor's blind spot and added its own; each tested a **proxy** for the question rather
than the question. The version that survives all three asks the whole thing — the legs are **present**,
none pending, none failed — and reads `gh`'s exit code as information about the *checks*, not about `gh`:

```sh
c=$(gh pr checks 901 2>/dev/null)
[ -n "$c" ] \
  && [ "$(grep -c 'Server crates' <<<"$c")" -eq 3 ] \
  && [ "$(grep -c 'Server crates.*pending' <<<"$c")" -eq 0 ] \
  && [ "$(grep -cE 'Server crates.*(fail|cancel)' <<<"$c")" -eq 0 ]
```

And the companion trap recorded elsewhere: **`gh pr checks --watch` exits 0 when the branch moves under
it**, so an exit-0 spanning a push is not a green signal. When you gate a merge on CI, name the **run id**
you gated on — a previous run's green does not describe the new head. Confirming the job's own
`conclusion`, not just that its steps look finished, is the same distinction one level down.

---

## When to Auto-File a Ticket

`/ticketing-new` intercepts **units of project work** transparently: a feature, a bug/defect fix
(including small live fixes), a refactor, a behavior change, or any multi-file edit. It announces
in one line, files the ticket, then works it to Done.

Do NOT intercept (just do the thing): answering questions, analysis, running build / check / commit /
push / git ops, cutting or publishing a release, managing tickets or the skill system, trivial
one-liners being iterated live, or anything the user says to "just do." If borderline, ask first.
