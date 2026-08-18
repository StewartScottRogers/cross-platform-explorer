//! Small shared filesystem utilities used across the Server's domain logic (CPE-815): epoch-ms time
//! conversion and streaming SHA-256 hashing. Pure and Tauri-free; re-exported into the app so its
//! many call sites resolve unchanged.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Convert a `SystemTime` into epoch milliseconds, if representable.
pub fn to_epoch_ms(t: SystemTime) -> Option<u64> {
    t.duration_since(UNIX_EPOCH).ok().map(|d| d.as_millis() as u64)
}

/// Render Unix-epoch seconds as an RFC 3339 UTC timestamp (`YYYY-MM-DDTHH:MM:SSZ`) — hand-rolled since
/// this crate carries no `chrono`/`time` dependency. Shared by [`crate::jwt_preview`] (`exp`/`iat`/`nbf`
/// claims, CPE-1418) and [`crate::cert_decode`] (`notBefore`/`notAfter`, CPE-1419) so both humanize
/// timestamps identically instead of each hand-rolling their own copy.
pub fn unix_to_rfc3339(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let secs_of_day = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Days-since-1970-01-01 -> (year, month, day). Howard Hinnant's `civil_from_days`
/// (<https://howardhinnant.github.io/date_algorithms.html>), valid for the entire representable `i64`
/// range with no overflow (it stays within `i64` arithmetic throughout).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d)
}

/// Three-state answer to *"is this path free for me to write?"* — the shared vocabulary behind every
/// refuse-to-clobber guard in the codebase (CPE-1705).
///
/// The whole bug class this fixes (CPE-1678 → 1687 → 1692 → 1696 → 1705) is a **two**-state answer to a
/// **three**-state question. [`Path::exists`] is `metadata().is_ok()`: it folds "provably absent" and "I
/// could not tell" into the same `false`, and a guard written as `if target.exists() { refuse }` therefore
/// *proceeds* on "I could not tell". At a site whose next statement is [`std::fs::rename`] that is not a
/// wrong error message — `fs::rename` **replaces the destination silently on both Windows and Unix**, so
/// the file that was there is destroyed with no warning and no error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetSlot {
    /// Provably nothing there — safe to write.
    Free,
    /// Provably something there.
    Occupied,
    /// The stat failed for a reason other than absence (permission denied along the resolved path, a
    /// dead network mount, `EIO`, a link that will not resolve). Not provably free ⇒ **not free**, but
    /// kept distinct from [`TargetSlot::Occupied`] so a caller can tell "this name is taken" from "I
    /// cannot see this directory at all" and word its refusal — or, like `unique_target`, advance to the
    /// next candidate — accordingly.
    Unknown,
}

/// Classify one target slot from a [`Path::try_exists`] outcome.
///
/// Pure, and taking the *outcome* rather than doing the stat, so the taxonomy is unit-testable on every
/// OS and CI account — the same reason [`crate::dispatch`]'s `classify_path_error` and
/// [`crate::split_join`]'s `part_stat_error` are split out from their callers. Permission bits are
/// platform- and privilege-dependent (inert as root; on Windows no deny ACE **on the target alone**
/// refuses `Path::exists()`, because `fs::metadata` falls back to a parent-directory read — see
/// [`deny_stat_of`]), so an ACL-based test alone would leave this taxonomy unverified on some machines.
///
/// [`Path::try_exists`], not [`Path::exists`], is the required probe: it returns `io::Result<bool>` and so
/// still *carries* the distinction this enum preserves.
pub fn classify_target_slot(stat: &std::io::Result<bool>) -> TargetSlot {
    match stat {
        // `try_exists`'s `Ok` payload is "it is THERE".
        Ok(true) => TargetSlot::Occupied,
        Ok(false) => TargetSlot::Free,
        // `try_exists` already folds a genuine `NotFound` into `Ok(false)`; be explicit anyway.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => TargetSlot::Free,
        Err(_) => TargetSlot::Unknown,
    }
}

/// **The one refuse-to-clobber guard.** Probe `target` and return the refusal message, or `None` meaning
/// *proceed* (CPE-1705).
///
/// ## Why this is one helper and not eighteen open-coded guards
///
/// CPE-1705's acceptance criteria asked the question outright — *"consider whether `fs::rename`'s
/// silent-replace semantics warrant a shared helper rather than twelve independent guards; twelve copies
/// of the same check is how the thirteenth gets missed"* — and the five preceding rounds answered it
/// empirically. CPE-1678, 1687, 1692 and 1696 each fixed a *subset* of the same open-coded shape and each
/// left more behind, because the shape is only recognisable by reading every site. There is now exactly
/// one implementation of the decision, so the next site that needs it is a call rather than a
/// re-derivation, and a `grep` for `.exists()` next to a `rename` is a review-able invariant instead of a
/// recurring sweep.
///
/// `occupied` supplies the **site's own** wording for the provably-occupied case, because that message is
/// part of each command's contract and is what the UI shows on an ordinary name collision. Only the
/// unknown case is worded here, and it is worded uniformly on purpose: every site's answer to "I could not
/// tell" should read the same way, and none of them had one before.
///
/// ## What this does NOT cover
///
/// - **Symlink-following.** [`Path::try_exists`] follows symlinks and [`std::fs::rename`] does not, so on
///   a *dangling* symlink `try_exists` answers `Ok(false)` — genuinely, correctly "nothing resolves
///   there" — and the rename then destroys the link. That is a CPE-1461-family symlink-following issue,
///   not a stat collapse, and **this helper does not close it.** A site that must not clobber a dangling
///   link needs a [`std::fs::symlink_metadata`] check as well. Recorded here so a `clobber_refusal` call
///   is never mistaken for having fixed it.
/// - **TOCTOU.** Nothing between this probe and the write is atomic. Where the platform offers an atomic
///   alternative (`create_new`, `open_no_follow`) that remains strictly better and this is not a
///   substitute for it.
pub fn clobber_refusal(target: &Path, occupied: &str) -> Option<String> {
    let stat = target.try_exists();
    match classify_target_slot(&stat) {
        TargetSlot::Free => None,
        TargetSlot::Occupied => Some(occupied.to_string()),
        TargetSlot::Unknown => Some(unknown_slot_message(target, &stat)),
    }
}

/// The wording for [`TargetSlot::Unknown`], split out so the sites that cannot use [`clobber_refusal`]
/// wholesale (because they probe a slot they will then *advance past* rather than refuse at) still phrase
/// the unknown identically.
///
/// Says three things the pre-CPE-1705 message said none of: which path could not be read, what the OS
/// actually said, and that the refusal is a refusal *to guess* rather than a claim about the file. The
/// last matters most — the user is being told the operation did not happen, and the wrong reading ("it
/// says the file is there, but I can see it isn't") is exactly what sent people looking for a file that
/// was never gone in CPE-1687.
pub fn unknown_slot_message(target: &Path, stat: &std::io::Result<bool>) -> String {
    let cause = match stat {
        Err(e) => e.to_string(),
        // Unreachable via `classify_target_slot`; kept total rather than panicking in a guard.
        Ok(_) => "the check did not complete".to_string(),
    };
    // Deliberately avoids the substring "already exists". That is the *occupied* verdict's wording at
    // almost every call site, and a message that contains it reads — to a user skimming, and to a test
    // asserting "this must not claim the file is there" — as exactly the claim this function exists to
    // avoid making. Caught by this ticket's own tests, which is the mildest possible way to catch it.
    format!(
        "could not check what is at \"{}\", so nothing was written — refusing to guess rather than risk \
         overwriting it: {cause}",
        target.display()
    )
}

/// A **second and different** guard for the same rename sites: refuse when `target`'s name is occupied by
/// a **symlink**, including a dangling one (CPE-1705, CPE-1461 family).
///
/// [`clobber_refusal`] cannot see this and no amount of stat-collapse fixing will make it: [`Path::exists`]
/// **and** [`Path::try_exists`] both *follow* symlinks, so on a dangling link both answer "nothing there"
/// — `try_exists` returns `Ok(false)`, which is not a collapsed failure but a genuinely, correctly
/// negative answer to the question it was asked. [`std::fs::rename`], meanwhile, does **not** follow the
/// final component, so it renames straight over the link and the link is destroyed. Measured end to end by
/// the CPE-1705 predecessor's reviewer: `exists() = false`, `symlink_metadata = Ok`, `fs::rename` → `Ok`.
///
/// **It is recorded here, and kept a separate function called separately at each site, precisely so that a
/// `try_exists` swap is never mistaken for having closed this route.** The two failures look identical
/// from the user's chair (a file vanished) and have nothing in common underneath. Round six of the
/// stat-collapse chain existed partly because a fix for one shape was reported as covering another.
///
/// Failure policy matches [`classify_target_slot`]: `NotFound` is the only answer that means free.
///
/// The decision is split into the pure [`classify_symlink_slot`] for exactly the reason
/// [`classify_target_slot`] is split out of [`clobber_refusal`] — **and round 2 of CPE-1705 proved the
/// point the hard way.** While this function did its `symlink_metadata` inline, its non-`NotFound` `Err`
/// arm could not be unit-tested at all, was believed unreachable, and quietly accumulated a garbled
/// message that nobody had read. Doing the stat inline is what made an arm unreadable *and* unread.
pub fn symlink_slot_refusal(target: &Path) -> Option<String> {
    classify_symlink_slot(&std::fs::symlink_metadata(target).map(|m| m.file_type().is_symlink()), target)
}

/// The pure decision behind [`symlink_slot_refusal`]. `stat` is the `symlink_metadata` outcome reduced to
/// "is it a link?", so every arm — including the one no ACL was thought able to reach — is unit-testable
/// on every OS and CI account.
pub fn classify_symlink_slot(stat: &std::io::Result<bool>, target: &Path) -> Option<String> {
    match stat {
        Ok(true) => Some(format!(
            "\"{}\" is a link, and renaming onto a link destroys it — the link is removed and its target \
             is left orphaned. Nothing was changed; remove the link first if that is what you meant",
            target.display()
        )),
        // Not a link: either free, or occupied by a real entry `clobber_refusal` already judged.
        Ok(false) => None,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        // CPE-1705 round 2 — a real user-facing bug, fixed. A bulk rename of the *other* helper's wording
        // clipped this string into "could not check what is at "…\final.txt" is a link, so nothing was
        // changed": ungrammatical nonsense that would have reached a user. Nobody read it because this arm
        // was *believed unreachable* — an earlier round concluded no ACL could make `symlink_metadata`
        // fail. Correction 4's parent `(RD)` deny makes it fail, so the arm is reachable, is now covered by
        // a test, and now says what it means. An "unreachable" branch is exactly where an unread string
        // hides.
        Err(e) => Some(format!(
            "could not check whether \"{}\" is a link, so nothing was changed — refusing to guess rather \
             than risk destroying one: {e}",
            target.display()
        )),
    }
}

/// **CPE-1715.** Probe a name-picking candidate the caller intends to *advance past* on collision, not
/// refuse at — the sibling of [`symlink_slot_refusal`] for a loop such as `unique_target` or
/// `resolve_conflict` (both `src-tauri`), whose whole job is to pick a *different* name, so a refusal is
/// the wrong verdict for them.
///
/// [`Path::try_exists`] follows symlinks, so a name occupied by a **dangling** link answers `Ok(false)` —
/// correctly, to the question it was asked — and a caller that stops there reads that as "this name is
/// free". A `fs::rename`/`fs::copy` onto that name does not follow the final path component, so it
/// destroys the link (or, for a live one, writes through it).
///
/// When `try_exists` says "nothing resolves here", this additionally takes a [`std::fs::symlink_metadata`]
/// reading of the **same** candidate — which does not follow the final component, so it sees a dangling
/// link that `try_exists` stepped straight through — and folds a successful stat there into `Ok(true)`
/// (occupied), same as `try_exists` itself would answer for an ordinary occupied name.
///
/// **Deliberately broader than "is it a link".** The stat's payload is discarded (`.map(|_| true)`): *any*
/// entry `symlink_metadata` can see but `try_exists` could not resolve counts as occupied, not only a
/// symlink. Two routes reach that with something other than a link sitting there: a TOCTOU race (a plain
/// file created between the two stats), and a non-symlink reparse point — a cloud-storage placeholder or a
/// dedup stub — whose `file_type().is_symlink()` can read `false` while `try_exists`'s underlying stat
/// still resolves it as absent. A version of this probe that fed [`classify_link_presence`] the narrower
/// `is_symlink()` bit would answer `Free` for exactly that entry, which is the same "provably nothing
/// there" mistake this whole ticket exists to close, just for a rarer trigger. Only a confirmed
/// `NotFound` — nothing at all, of any kind, at that name — is allowed through as free.
pub fn name_pick_slot_probe(candidate: &Path) -> std::io::Result<bool> {
    match candidate.try_exists() {
        Ok(false) => classify_link_presence(std::fs::symlink_metadata(candidate).map(|_| true)),
        other => other,
    }
}

/// The pure half of [`name_pick_slot_probe`]'s fallback check, split out — the same reason
/// [`classify_target_slot`] and [`classify_symlink_slot`] are split from their callers — so the
/// dangling-link arm, which only reproduces with a real filesystem entry, is unit-testable without
/// touching disk. `is_link` is a [`std::fs::symlink_metadata`] outcome reduced to a `bool`, matching the
/// shape [`classify_symlink_slot`] uses — [`name_pick_slot_probe`] always passes `true` on a successful
/// stat (see its doc comment for why), but this function is kept general so a future caller that legitimately
/// has an `is_symlink()` bit, rather than a bare "did the stat succeed", can still use the same taxonomy.
///
/// A link — dangling or live — occupies its slot (`Ok(true)`). A confirmed absence agrees with the
/// `try_exists` probe that produced it (`Ok(false)`), including the explicit `NotFound` case (mirroring
/// `classify_target_slot`'s own `NotFound` handling). Any other stat failure cannot prove the slot free, so
/// it is threaded through as `Err` — `classify_target_slot` folds that into [`TargetSlot::Unknown`], the
/// same "cannot tell, so not free" verdict an unreadable slot already gets.
pub fn classify_link_presence(is_link: std::io::Result<bool>) -> std::io::Result<bool> {
    match is_link {
        Ok(true) => Ok(true),
        Ok(false) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

/// **The one guard for a slot that is about to be `fs::rename`d onto** (CPE-1710): both
/// [`clobber_refusal`] and [`symlink_slot_refusal`], in the order the two sites that got it right already
/// used, as a single call that cannot be half-applied.
///
/// ## Why this exists rather than two calls at every site
///
/// CPE-1705 gave twelve `rename`-destructive sites [`clobber_refusal`] and gave exactly two of them
/// [`symlink_slot_refusal`] as well — and its own doc comment (see "What this does NOT cover" above) wrote
/// down that a `rename`-destructive site needs both. CPE-1710's reviewer then found, by enumerating rather
/// than spot-checking, that `copilot::apply_op`, `copilot::transfer_entry`, `organize_apply` and the
/// board's ticket move were all the exception to the rule the same PR had just written down. Four sites
/// out of six is not a memory failure by four authors; it is a guard whose two halves are separable when
/// the hazard is not.
///
/// So the pairing is now a **call, not a convention**. A future `fs::rename` site calls this and gets both
/// checks or it calls neither, and `clippy.toml`'s `disallowed-methods` ban on bare `std::fs::rename` is
/// what makes skipping it impossible. (An earlier version of this sentence credited a
/// `guards_are_paired_at_every_rename_destructive_site` source scan in this module's tests. That scan was
/// replaced by the clippy ban in round 3 — see [`rename_into_slot`] on why the scan was oversold — so the
/// citation named an enforcement mechanism that no longer exists. Corrected under CPE-1718.)
///
/// ## Order is load-bearing, and it is the existing order
///
/// [`clobber_refusal`] runs first, so an ordinary occupied name still reports the **site's own** wording
/// ("\"notes.txt\" already exists") rather than a link message. The link wording is reachable only for a
/// **dangling** link, which is precisely the case [`clobber_refusal`] answers "free" for and cannot ever
/// judge. This preserves, byte for byte, the messages `rename_entry_impl` and `move_exact_impl` already
/// produced when they open-coded the two calls in this order.
///
/// Callers that must probe a slot they will *advance past* rather than refuse at (name-picking loops such
/// as `unique_target`) still cannot use this — refusal is not their verdict — and they are tracked
/// separately; see the test's allow-list for the current inventory.
pub fn rename_slot_refusal(target: &Path, occupied: &str) -> Option<String> {
    clobber_refusal(target, occupied).or_else(|| symlink_slot_refusal(target))
}

/// **The one guard for a slot a caller is about to CREATE a file at** (CPE-1718) — the create-shaped
/// sibling of [`rename_slot_refusal`], and the third member of the family alongside
/// [`replace_file_contents`].
///
/// ## Which of the three a site wants, in one question
///
/// CPE-1716 settled it: *"am I claiming this name, or editing this file?"* [`replace_file_contents`] is
/// the **editing** answer and follows a link, because the user opened a file and a link is how they
/// reach it. This and [`rename_slot_refusal`] are the **claiming** answers and refuse a link, because
/// the user typed a name for a file that does not exist yet and following the link would put their
/// bytes somewhere else entirely.
///
/// ## Why a create site needs its own helper rather than reusing `rename_slot_refusal`
///
/// The two differ in what the link does to you, and therefore in what the message must say. At a
/// `fs::rename` site the link is **destroyed** (rename does not follow the final component). At a
/// `File::create`/`fs::write` site the link is **followed**: the bytes land at its target and the link
/// survives, so the operation reports success about a file the user never named. `rename_slot_refusal`
/// would refuse correctly and then explain it wrongly — *"renaming onto a link destroys it"* is a
/// confident false statement about what was going to happen here, which is the failure mode this repo
/// has filed four tickets about.
///
/// ## Order is the OPPOSITE of `rename_slot_refusal`'s, and that is deliberate
///
/// There the occupancy half runs first, so an ordinary collision keeps the site's own wording and the
/// link wording is reachable only for a **dangling** link — the one case occupancy cannot judge. Here
/// the link half runs first, because at a create site **both** kinds of link are the same hazard:
///
/// - a **dangling** link reads as a free name ([`Path::try_exists`] follows links, so it answers
///   `Ok(false)`) and `File::create` then creates the link's target — measured on Windows for CPE-1718:
///   `clobber_refusal = None`, `File::create -> Ok`, 4096 bytes at the target, slot still a link;
/// - a **live** link is already refused by the occupancy half, but as *"already exists"*, which sends
///   the user to delete a file at that name when what is there is a link to somewhere else.
///
/// Both are write-through, so both deserve the write-through message. A plain occupied name still gets
/// the site's own wording: a regular file answers `Ok(false)` to the link question and falls through.
pub fn create_slot_refusal(target: &Path, occupied: &str) -> Option<String> {
    create_slot_link_refusal(target).or_else(|| clobber_refusal(target, occupied))
}

/// **The link half of [`create_slot_refusal`], alone** (CPE-1733) — for a create site whose *occupancy*
/// answer is legitimately "overwrite it" but whose *link* answer is still "refuse".
///
/// This is not a weaker `create_slot_refusal`; it is the other half of a split that CPE-1733's
/// enumeration of `archive.rs` forced. Every archive-creation destination there (`compress_to_zip` and
/// its five siblings) writes a whole new archive at a caller-supplied path, and overwriting an existing
/// archive at that path is a **legitimate, long-standing behaviour** — the app's own compress flow even
/// depends on re-running onto a name it picked. Wiring `create_slot_refusal` in wholesale would have
/// changed that contract as a side effect of a link fix, which is the "one step past the evidence"
/// failure this family keeps filing tickets about. The link hazard is separable and was measured on its
/// own, so it is guarded on its own.
///
/// The link verdict itself is unchanged and shares one implementation with [`create_slot_refusal`]:
/// both are [`classify_create_slot`] over one [`std::fs::symlink_metadata`], and `create_slot_refusal`
/// is now literally this function plus [`clobber_refusal`]. A site that also wants the occupancy half
/// must call [`create_slot_refusal`] — do not stack this with a hand-rolled existence check.
///
/// Measured on Windows for CPE-1733, with the guard removed, at a `fs::File::create` destination:
///
/// ```text
/// [M1 File::create on DANGLING file symlink] result = Ok(())
///       target now exists = true, bytes = Some([78, 69, 87, 32, 66, 89, 84, 69, 83])
///       slot still a symlink = Ok(true)
/// [M2 File::create on LIVE file symlink] result = Ok(())
///       victim bytes = Some("CLOBBERED")
///       slot still a symlink = Ok(true)
/// ```
pub fn create_slot_link_refusal(target: &Path) -> Option<String> {
    match create_slot_link_verdict(target) {
        CreateSlotLink::NotALink => None,
        CreateSlotLink::Link(m) | CreateSlotLink::Unknown(m) => Some(m),
    }
}

/// The three answers a create-slot link check can give — **for the callers that must treat "it is a link"
/// and "I could not tell" differently** (CPE-1733, UAT finding 6).
///
/// [`create_slot_link_refusal`] collapses the last two into one `Some(message)`, which is right for a site
/// that refuses the whole operation either way: both mean "do not write here". It is **wrong** for a site
/// that *skips and continues*, because the two verdicts differ in what the skip costs the user:
///
/// - `Link` is a **policy** decision. We know what is there, we know writing follows it, and dropping that
///   one entry is the considered answer — the same shape as the zip-slip skip.
/// - `Unknown` is an **I/O failure**. We could not read the slot at all, so "skip it" silently drops a
///   file for a reason that has nothing to do with the archive, and reports success. Every other I/O
///   failure in those loops aborts via `?`; this one was quietly not doing so.
///
/// The distinction costs one `match` at the two sites that need it and nothing anywhere else.
#[derive(Debug)]
pub enum CreateSlotLink {
    /// Provably not a link: free, or occupied by a real entry that is the occupancy half's business.
    NotALink,
    /// Provably a link, with the refusal wording.
    Link(String),
    /// Could not tell, with the refusal wording. Never a licence to carry on.
    Unknown(String),
}

/// [`create_slot_link_refusal`]'s verdict before it is flattened: **one `symlink_metadata` and nothing
/// else**, so that the decision it feeds is a pure function that can be tested on inputs no filesystem
/// will reliably produce on demand.
pub fn create_slot_link_verdict(target: &Path) -> CreateSlotLink {
    create_slot_link_from_stat(&std::fs::symlink_metadata(target).map(|m| m.file_type().is_symlink()), target)
}

/// **The decision UAT finding 6 was actually about, as a pure function** (CPE-1733, PR #906 review
/// round 4).
///
/// The first fix for that finding put this `match` inside [`create_slot_link_verdict`], next to the
/// `symlink_metadata` call, and tested `archive::entry_slot_action` instead — which only re-labels an
/// **already-classified** verdict. The review's mutation proved the gap: flipping the `Err(_)` arm here
/// to `CreateSlotLink::Link` reinstates the original bug exactly (an unreadable slot skipped silently,
/// the run reporting `Ok`) and the whole suite stayed green, because nothing exercised *this* choice.
/// The reasoning for a pure classifier was right; it was applied one level too low.
///
/// Splitting it out costs nothing at runtime and buys the one arm that cannot be staged on demand: a
/// slot whose `symlink_metadata` fails with something other than `NotFound` needs a permission or I/O
/// fault that no test can portably arrange, so with the decision inline the arm that was wrong would
/// again be the arm nothing could reach. Pinned by
/// `an_unreadable_slot_is_unknown_never_a_confirmed_link`.
///
/// [`classify_create_slot`] still owns every message, so there is exactly one copy of each; this adds
/// only the split of its `Some` into the two verdicts it collapses (a confirmed link, and a stat that
/// failed for a reason other than `NotFound`).
pub fn create_slot_link_from_stat(stat: &std::io::Result<bool>, target: &Path) -> CreateSlotLink {
    match classify_create_slot(stat, target) {
        None => CreateSlotLink::NotALink,
        Some(msg) => match stat {
            Ok(true) => CreateSlotLink::Link(msg),
            _ => CreateSlotLink::Unknown(msg),
        },
    }
}

/// The pure decision behind [`create_slot_refusal`], split out for the reason
/// [`classify_symlink_slot`] and [`classify_write_target`] are: **a live *file* symlink cannot be staged
/// without privilege on an unprivileged Windows account**, so with the decision inline the live-link arm
/// would go unverified on exactly the machines most likely to hit it.
pub fn classify_create_slot(stat: &std::io::Result<bool>, target: &Path) -> Option<String> {
    match stat {
        Ok(true) => Some(format!(
            "\"{}\" is a link, and creating a file at a link's name writes THROUGH it — the bytes would \
             land at the link's target, a path you did not name, and a failure part-way would then \
             delete the link itself. Nothing was written; remove the link first if that is what you meant",
            target.display()
        )),
        // Not a link: either free, or occupied by a real entry the occupancy half will judge.
        Ok(false) => None,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        // Same failure policy as everywhere else in this module: not provably a non-link ⇒ do not write.
        Err(e) => Some(format!(
            "could not check whether \"{}\" is a link, so nothing was written — refusing to guess rather \
             than risk writing through one: {e}",
            target.display()
        )),
    }
}

/// **Guard and rename in one call — the only sanctioned way to `fs::rename` onto a user-named slot**
/// (CPE-1710 round 3, on the PR #895 reviewer's recommendation).
///
/// [`rename_slot_refusal`] on `target`, then the rename. Returns the guard's refusal or the OS's error,
/// both already stringified, because every call site did exactly that mapping by hand.
///
/// # Why this exists when `rename_slot_refusal` already did the deciding
///
/// The refusal helper made it possible to guard correctly; it did nothing to make guarding *happen*. The
/// source scan in this module's tests was round 1's answer to that and it was oversold: the PR #895
/// reviewer and UAT independently smuggled a destructive rename past it three ways (>25 lines from the
/// guard, `use std::fs::rename as move_entry`, one call deep in a helper), and it is structurally blind to
/// the more dangerous case — a rename with **no** guard at all.
///
/// `clippy.toml`'s `disallowed-methods` closes both. It resolves the **path**, not the text, so an alias,
/// a distance or a helper indirection cannot dodge it, and it fires on *every* `std::fs::rename` rather
/// than on a recognised-wrong shape. This function carries the single `#[allow]` for the guarded path, so
/// a site either calls this or writes its own `#[allow(clippy::disallowed_methods)]` **with a reason** —
/// which is the real win: the out-of-class justification ends up in the code, at the site, permanently,
/// instead of in a PR description nobody reads twice. Round 1's lived in a PR description and was wrong
/// twice.
///
/// **Known gaps, stated rather than papered over — and the first one was itself stated wrongly once.**
///
/// 1. `disallowed_methods` resolves paths, not `dyn` dispatch. An **`impl` body IS covered** — measured:
///    removing the `#[allow]` at `provider.rs:162` gives `error: use of a disallowed method
///    std::fs::rename`. What is **not** covered is a **caller** reaching rename through a trait object;
///    a `fn(p: &mut dyn FileSystemProvider) { p.rename(a, b) }` produces zero diagnostics. An earlier
///    version of this very comment said the *implementation* was uncovered, which is false and errs
///    toward complacency about impls. The CPE-1710 round-4 review also probed an alias, a re-export, a
///    bound-but-uncalled fn value, a macro expansion and a const fn-pointer field — the lint flagged all
///    of them. The `dyn` caller is the only escape.
/// 2. `disallowed_methods` is **warn-by-default**. It only fails a build because CI passes `-D warnings`
///    on every invocation; a bare local `cargo clippy` prints the warning and exits 0.
pub fn rename_into_slot(src: &Path, target: &Path, occupied: &str) -> Result<(), String> {
    if let Some(e) = rename_slot_refusal(target, occupied) {
        return Err(e);
    }
    // The one sanctioned `fs::rename` in the codebase: it is three lines below the guard that makes it
    // safe, and nothing can get between them.
    #[allow(clippy::disallowed_methods)]
    std::fs::rename(src, target).map_err(|e| e.to_string())
}

/// **Rewrite the contents of a file the user already has, atomically, without eating a symlink**
/// (CPE-1716). The counterpart to [`rename_into_slot`]: that one guards a slot the caller is *claiming*,
/// this one replaces the bytes of a slot the caller is *editing*.
///
/// Temp-sibling + `rename` is the right save shape — a crash mid-write must never truncate a user's media
/// file — but `fs::rename` does **not** follow the final path component, so renaming straight onto `path`
/// when `path` is a symlink replaces **the link** with a regular file and leaves the file the link pointed
/// at holding its old contents. CPE-1716 measured exactly that through `metadata_write`: the link was
/// destroyed, the real file was never edited, and the command returned `Ok` with the edited fields echoed
/// back, so the user got positive confirmation of an edit that did not happen.
///
/// # The decision, recorded here because the code is where it has to live
///
/// Three options were on the table — **replace** the link (what the bug did), **refuse** any symlinked
/// path, or **resolve** the link and edit its target. This function resolves.
///
/// - **Resolving is what a user means by "edit this file".** A symlink-organised media library is an
///   ordinary arrangement; every entry in it is a link, so refusing would make the Metadata Studio useless
///   for exactly the people most likely to open it. An editor that follows the link is also what every
///   other editor on the machine does.
/// - **Refusing was rejected** for that reason, not because it is unsafe. It is the safer option and would
///   be the right one if resolving could write somewhere unexpected — it cannot: the resolved path is
///   wherever the user's own link points, which is the file they opened.
/// - **Replacing is never right here.** It destroys a link the user made and drops the edit. That is the
///   bug.
///
/// This is deliberately the **opposite** of the settled decision at the vault's `.cpevault` writes (see
/// `vault_manager`, CPE-1670/VAULT-SECURITY.md §5), which replace a symlinked destination rather than
/// resolve it. The two are not inconsistent: a vault write *creates the file at a path the user named*,
/// where following a link could redirect a freshly-sealed vault somewhere the user did not choose; this
/// rewrites *a file the user already has open*, where not following the link edits the wrong file. The
/// distinguishing question is "am I claiming this name, or editing this file?", and the answer picks the
/// helper.
///
/// # A dangling link is REFUSED, and says so
///
/// If `path` is a link whose target does not resolve there is nothing to edit — "follow the link" has no
/// answer. Rather than create the missing target (inventing a file the user never asked for) or fall back
/// to replacing the link (the bug), this returns an error naming the link.
///
/// **Callers that read the file first must call [`resolve_write_target`] BEFORE the read, and read the
/// path it returns** — otherwise this refusal is unreachable and the user sees the read's bare
/// `NotFound` instead. Measured by the PR #899 UAT round 2: `std::fs::read` follows the link too, so
/// `metadata_write` failed with `The system cannot find the file specified. (os error 2)` — no path, no
/// mention of a link — while the message below, which says all of that, could never fire. It now resolves
/// first and hands the resolved path to both the read and this function, which also means the bytes read
/// and the bytes replaced provably concern the same file.
///
/// # Where the staging file lands — a behaviour change worth stating
///
/// The temp is a sibling of the **resolved target**, not of `path`. For a symlinked file that is a
/// different directory from the one the user is looking at (the library folder rather than the playlist
/// folder). It is required rather than incidental: a rename is only atomic within one filesystem, and the
/// resolved target's directory is the only one guaranteed to be on the target's volume. The temp is
/// removed on every failure path, so it is not left sitting there either way.
///
/// # What this does NOT do
///
/// - **Hard links are not preserved**, and cannot be: a hard link is not distinguishable from the file
///   itself, so the rename breaks the link and the other name keeps the old bytes. This is the standard
///   atomic-save trade-off (vim, git and every other rename-based writer do the same), and it is *not* the
///   CPE-1716 bug — with a hard link the file the user opened does receive the edit. Stated so a future
///   reader does not mistake the two.
/// - **The file OBJECT is still replaced — but what it CARRIES is now carried across with it** (CPE-1739,
///   from the four losses PR #904's UAT measured). The swap itself is inherent to the atomic-save idiom
///   and is not going away; what changed is that the things attached to the object no longer stay behind
///   on the file that gets unlinked:
///
///   | What | Before (rename only) | Now |
///   |---|---|---|
///   | Unix mode — `0600` private, `0755` executable | `0644`, both | carried exactly ([`carried_mode`]) |
///   | Unix extended attributes — Finder tags, `com.apple.quarantine` | dropped | carried, best effort ([`carry_xattrs`]) |
///   | Windows attributes (`HIDDEN`, …), DACL, creation time | dropped | carried by `ReplaceFileW` ([`commit_replacement`]) |
///   | Windows alternate data streams — `Zone.Identifier`, the Mark of the Web | destroyed | carried by `ReplaceFileW` |
///   | Unix ownership (uid/gid) | not carried | **still not carried** — `chown` is not in `std`, and an unprivileged process cannot give a file away |
///   | Open-handle identity — item 4 | see below | **still not fixed** |
///
///   **Item 4 remains open, and it is the one that cannot be bought with care on the replacement file.**
///   While another program holds the file open with `SHARE_READ|WRITE` — what an ordinary Windows
///   application holds — the save fails: `fs::rename` said `Access is denied. (os error 5)` and
///   `ReplaceFileW` says `...being used by another process`. The obstacle is the **target's** sharing
///   mode, so only writing in place would sidestep it, and that means giving up the atomicity that a
///   half-rewritten media file is the whole reason for. (Rust's own `File::open` takes
///   `FILE_SHARE_DELETE` and is unaffected either way — measured `Ok(())`.) A reader holding the file open
///   across the save likewise still sees the old bytes on every platform, because the object it refers to
///   is the one that was unlinked. Both are recorded here, in [`commit_replacement`], and in
///   `src/docs/25-metadata-studio.md`, rather than being quietly hoped away.
///
///   **`write_file_text` still shares only [`classify_write_target`] with this function and not the write
///   itself** (CPE-1725), and CPE-1739 does not change that: its traffic is ordinary text files, item 4 is
///   not closed, and the ~6 ms per save `ReplaceFileW` costs (measured, see [`commit_replacement`]) is a
///   price worth paying for a media file being rewritten in place and not for an ordinary text save that
///   `fs::write` already does correctly. Re-routing it is a regression until item 4 is closed.
/// - **Durability across a power loss is NOT provided, only atomic visibility** (PR #899 Reviewer). The
///   bytes are `sync_all`ed before the rename, so no observer can ever see a half-written file and a
///   failed save leaves the original exactly as it was. The **directory entry** the rename creates is a
///   different question and is not synced: closing that needs a parent-directory `sync_all` on Unix and
///   `MOVEFILE_WRITE_THROUGH` on Windows. `vault_manager::sync_parent_dir` does the Unix half for the
///   vault and its own comment records that Windows is only *narrowed*, not closed — so adding the Unix
///   call here would buy a guarantee this crate cannot state platform-uniformly. The user docs are scoped
///   to what is actually provided instead of claiming power-cut safety.
/// - **TOCTOU.** The link is resolved and then written; nothing is atomic across those two steps.
/// - **A second TOCTOU, between reading the target and committing** (CPE-1739 review round 1, F4 —
///   recorded because this function records everything else, not because either branch is dangerous).
///   [`classify_carryover`]'s stat happens before staging and [`commit_replacement`] happens after the
///   bytes are written, and the target can change in between. If it **appears** in that window — the name
///   was free at stat time — the commit takes the plain-`rename` branch and overwrites the newcomer,
///   destroying exactly the attributes and streams this ticket exists to preserve. If it **vanishes**,
///   `ReplaceFileW` fails `NotFound` where a rename would have succeeded, so the save reports an error
///   about a file that is no longer there. Both need someone to create or delete the user's file during
///   the milliseconds of one save; the first is silent but requires the file to have been absent when the
///   user pressed Save, and the second is loud and leaves nothing damaged. Closing them means holding the
///   target open across the whole save, which is item 4's problem from the other side.
/// - **The temp is not cleaned up at the instant the process dies** (CPE-1725, PR #904 review; decided by
///   CPE-1738). The removals below sit on the write/sync and rename **error** branches, so they run when
///   the save *fails* and not when the save is *killed*: force-quit, a crash or a `SIGKILL` between the
///   create and the rename strands a `<name>.<pid>-<nanos>.cpe-tmp` next to the user's file at the moment
///   it happens. Harmless when it does — the original is untouched, and the stamped name means the next
///   save cannot collide with it.
///
///   **CPE-1738's decision: build a narrow, opportunistic sweep, rather than leave the stray file for the
///   user to find and delete by hand.** [`stage_and_replace`] now calls [`sweep_stale_temp_siblings`] once
///   after its own rename **succeeds** — never before staging and never on a failed save, so an ordinary
///   save only pays for it in the one case where the save has already done all its real I/O. The sweep
///   only ever looks at THIS file's own `<name>.*.cpe-tmp` siblings (never a directory-wide `*.cpe-tmp`
///   glob, so it can never mistake a temp another file's concurrent save just staged for one of this
///   file's leftovers), only removes one whose age already clears a floor comfortably above any plausible
///   in-flight save, and never touches — never even opens — anything that is a symlink. See
///   [`sweep_stale_temp_siblings`]'s own doc comment for the full reasoning, including why "do nothing" and
///   "sweep on startup" (the ticket's other two options) were rejected, and how this stays cheap on a slow
///   network share. The user docs no longer tell people to delete a `.cpe-tmp` by hand; a stray one left by
///   a crash is now expected to disappear on its own the next time that same file is saved again.
pub fn replace_file_contents(path: &Path, bytes: &[u8]) -> Result<(), String> {
    stage_and_replace(path, bytes)
}

/// The staging opener [`replace_file_contents`] uses, extracted **so a test can call the real one**
/// (PR #899 UAT round 2).
///
/// The round-2 test that closed the "`create_new` is untestable" finding built its own
/// `OpenOptions` closure and never touched this module's call site — so swapping `create_new(true)`
/// for `create(true).truncate(true)` in production left the whole suite green, **including the test
/// named after the guard**. It pinned `std::fs`'s semantics, which are the standard library's problem,
/// not this crate's use of them. That is Evidence Rule 1 — a test that cannot fail is not evidence —
/// inside the change made to close a finding that was itself about testability.
///
/// One line, few callers, no behaviour change; its whole purpose is to be reachable from a test.
///
/// # It is also the belt behind [`create_slot_refusal`] (CPE-1718), and it bites on Windows too
///
/// [`create_slot_refusal`] is a probe followed by an open, so it is TOCTOU by construction. This is the
/// atomic half: `O_CREAT|O_EXCL` does not follow a symlink at the final component, so a link at the name
/// makes the *open itself* fail rather than being followed. Measured on Windows for CPE-1718, on a
/// dangling symlink where `File::create` writes 4096 bytes straight through to the target:
///
/// ```text
/// [PROBE] B create_new -> Err((AlreadyExists, "The file exists. (os error 80)"))
/// [PROBE] B post: is_link=Ok(true) target_exists=Ok(false)
/// ```
///
/// So the two are not redundant, and each is load-bearing for a different thing: **this** one keeps the
/// bytes off the link's target, and **the refusal** is what makes the failure say *"is a link"* instead
/// of `The file exists. (os error 80)` about a name `try_exists` reports as free. A site that creates a
/// file at a user-named path wants both, in that order.
///
/// # The mode argument (CPE-1739 review round 1)
///
/// This entry point takes the platform default — `0666 & ~umask` on Unix, the parent's inherited ACL on
/// Windows — which is what a site *claiming a name for a new user file* wants (`split_join`'s joined
/// output and its manifest are the callers). [`create_staging_file`] is the other entry point and asks
/// for `0600`, because a staging file holds someone else's private bytes for a moment and is not a file
/// the user will ever see. Both funnel through this one body, so there is still exactly **one**
/// `create_new(true)` in the crate and the extraction argument above is unweakened.
pub fn create_exclusive(path: &Path) -> std::io::Result<std::fs::File> {
    create_exclusive_with_mode(path, None)
}

/// The mode [`create_staging_file`] creates a `.cpe-tmp` at: readable and writable by its owner and by
/// nobody else, from the instant the file exists.
///
/// **Why a constant and not `0644`-and-hope** (CPE-1739 review round 1, the blocker). `create_new`
/// without an explicit mode is `O_CREAT|O_EXCL, 0666`, so the kernel makes the staging file
/// `0666 & ~umask` — world-readable under the ordinary `022` umask — and the [`carry_protections`]
/// `fchmod` that narrows it to the target's real mode runs *afterwards*. **POSIX checks permission at
/// `open`, not at `read`**, so a local process that opens the staging name inside that window keeps a
/// readable descriptor across the `fchmod` and goes on to read the private bytes written after it. The
/// window is small but it is not theoretical and the name does not have to be guessed: an
/// inotify/FSEvents watcher is woken by the create itself. Measured with `strace` on the real test
/// binary before the fix:
///
/// ```text
/// openat(AT_FDCWD, ".../secrets.env.382-1786791461862651861.cpe-tmp",
///        O_WRONLY|O_CREAT|O_EXCL|O_CLOEXEC, 0666) = 3
/// fchmod(3, 0600)                                 = 0
/// rename(".../secrets.env.382-....cpe-tmp", ".../secrets.env") = 0
/// ```
///
/// With the mode on the `openat` there is no window at all: the file is never, for any instant, more
/// open than `0600`. What [`carry_protections`]'s `fchmod` does to that birth mode next depends on what,
/// if anything, sat at the target name — three cases, all pinned by CPE-1755's tests
/// (`cpe_1755_a_0644_target_widens_the_staged_file_from_its_0600_birth_mode`,
/// `cpe_1755_a_0400_target_narrows_the_staged_file_from_its_0600_birth_mode`,
/// `cpe_1755_a_brand_new_name_lands_at_the_0600_staging_birth_mode_not_the_platform_default`):
///
/// - An existing `0644` target **widens** it, `0600 → 0644`. This is the case the pre-CPE-1755 wording
///   here only described — "only ever widens" — which is where the next two cases correct it.
/// - An existing `0400` target **narrows** it, `0600 → 0400`: `carry_protections`' `fchmod` takes the
///   owner-write bit *away*, because it copies the target's mode exactly rather than only adding to the
///   birth mode. "Widens" was never true in general — it happened to hold for every case CPE-1739 itself
///   exercised (a `0600` secret staying `0600`, a `0755` script gaining bits), but a locked-down `0400`
///   target falsifies it.
/// - **No target at all** — `existing` is `None`, so `carry_protections` never runs, and the file is
///   never touched again: it stays at `0600`, full stop. See the `else` of the `carry_protections` call
///   in [`stage_and_replace`] for why that is the recorded, deliberate answer and not an accident of
///   `None` skipping a branch (CPE-1755).
///
/// Note the birth mode is deliberately **more** restrictive than the eventual mode during staging rather
/// than equal to it — narrowing first and widening later is the only order that has no gap, since the
/// target's mode is not known until after the file exists.
const STAGING_MODE: u32 = 0o600;

/// The staging opener [`stage_and_replace`] actually calls, one line above the call site with nothing
/// between them — the same "extract it so a test can call the real one" argument as [`create_exclusive`],
/// which this delegates to. Pinned by `cpe_1739_the_staging_opener_creates_a_file_no_one_else_can_open`
/// and by `create_new_refuses_a_link_at_the_staging_name_where_fs_write_would_follow_it`, both of which
/// call **this** function rather than a copy of it.
fn create_staging_file(path: &Path) -> std::io::Result<std::fs::File> {
    create_exclusive_with_mode(path, Some(STAGING_MODE))
}

/// The single `create_new(true)` open in this crate. `unix_mode` is the mode the file is **created**
/// with, not one applied afterwards — see [`STAGING_MODE`] for why that distinction is the whole point.
/// Ignored on Windows, which has no mode; see [`carry_protections`]'s Windows arm for what that costs.
fn create_exclusive_with_mode(path: &Path, unix_mode: Option<u32>) -> std::io::Result<std::fs::File> {
    let _ = unix_mode; // read on Unix only
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    if let Some(mode) = unix_mode {
        use std::os::unix::fs::OpenOptionsExt as _;
        opts.mode(mode);
    }
    opts.open(path)
}

fn stage_and_replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let target = resolve_write_target(path)?;
    // CPE-1739: what the file the user is editing carries TODAY. Two different things depend on it — the
    // mode (and extended attributes) [`carry_protections`] copies onto the staged file on Unix, and, on
    // Windows, whether the target exists at all, which is what picks `ReplaceFileW` over a plain rename in
    // [`commit_replacement`]. Read once, before anything is staged: a save that cannot find out what it is
    // about to replace refuses rather than quietly handing back a default-permission file. See
    // [`classify_carryover`].
    let source = std::fs::metadata(&target);
    let existing = if classify_carryover(source.as_ref().err(), &target)? { source.as_ref().ok() } else { None };
    // A per-write pid+nanosecond stamp so two concurrent saves — or a stale temp left by an earlier crash
    // — cannot collide on the same sibling path.
    let stamp = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = target.with_file_name(format!(
        "{}.{}-{stamp}.cpe-tmp",
        target.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    // `create_new` is `O_CREAT|O_EXCL`, which refuses an existing entry **and does not follow a symlink at
    // the final component** — so the temp file cannot be written through a link somebody pre-placed at the
    // (guessable-in-principle) staging name. `fs::write` would follow one. Pinned by
    // `create_new_refuses_a_link_at_the_staging_name_where_fs_write_would_follow_it`, which calls
    // [`create_staging_file`] — **this** function's opener, not a copy of it. The temp name carries
    // pid+nanos, so racing the real one is not the way to test it; extracting the opener is.
    {
        use std::io::Write as _;
        // CPE-1739: the file is CREATED at 0600 (see `STAGING_MODE`), not created wide and narrowed
        // afterwards — POSIX checks permission at `open`, so narrowing afterwards leaves a window in
        // which another local process can take a descriptor it keeps.
        let mut f = create_staging_file(&tmp).map_err(|e| format!("{}: {e}", display_path(&tmp)))?;
        // Then widen/adjust to whatever the user's own file actually carries, while the staged file is
        // still EMPTY — before the user's bytes ever reach it. On Windows this is a deliberate no-op:
        // `commit_replacement`'s `ReplaceFileW` carries the attributes, ACL and named streams across at
        // the moment of the swap instead, which is more than anything copyable onto the staged file
        // could — but it does mean the Windows staging file carries the *directory's* inherited ACL for
        // the whole write, which `carry_protections`' Windows arm records.
        if let Some(src) = existing {
            if let Err(e) = carry_protections(&target, src, &f, &tmp) {
                drop(f);
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
        }
        // CPE-1755, THE RECORDED DECISION: no `else` here on purpose. `existing` is `None` for a save to
        // a brand-new name (Save-As, or any free-name write nothing has ever occupied), so there is
        // nothing to carry and the staged file is left exactly as `create_staging_file` made it — at
        // `STAGING_MODE`, `0600` — all the way to the rename. Every OTHER file-creating path in this app
        // (`create_exclusive`, used by `split_join`'s joined output and manifest) takes the platform
        // default `0666 & ~umask` instead, so this is a deliberate, narrow exception, not an oversight:
        //   - Chosen over matching the platform default because a private mode is the safer of the two
        //     defensible answers for a file this app is writing on the user's behalf, and because the
        //     staging file is already sitting at `0600` for CPE-1739's reasons — leaving it there costs
        //     nothing, while widening it would mean deriving "the platform default" after the fact, which
        //     on Unix means calling `umask(2)` — the ONLY way POSIX exposes the umask is by atomically
        //     SETTING it and reading back the old value, a global, process-wide, thread-unsafe side effect
        //     for a process that may be juggling other concurrent saves, in exchange for matching a default
        //     this path does not even reach today.
        //   - Low-stakes to get "wrong": `metadata_write_impl` always reads the target before writing, so
        //     the target exists and this branch cannot run from there, and `write_file_text`'s Save-As does
        //     not call this function at all. The free-name path is only reachable from tests today
        //     (`cpe_1739_a_save_to_a_free_name_still_creates_the_file`,
        //     `cpe_1755_a_brand_new_name_lands_at_the_0600_staging_birth_mode_not_the_platform_default`).
        // If a future caller wants a brand-new file at the platform default instead of `0600`, that is
        // what [`create_exclusive`] is for — this function's contract is now that a save through it never
        // hands back a file wider than `0600` unless something existed at the name to widen it from.
        if let Err(e) = f.write_all(bytes).and_then(|()| f.sync_all()) {
            drop(f);
            let _ = std::fs::remove_file(&tmp);
            return Err(format!("{}: {e}", display_path(&tmp)));
        }
    }
    // NOT `rename_into_slot`, and the reason is its FIRST half, measured rather than assumed (PR #899
    // Reviewer): `clobber_refusal` runs first and `try_exists` follows a live link, so on the user's
    // symlinked media file it refuses with `"01 - My Track.wav" already exists` and the link half is
    // never reached at all. Nor does resolving first rescue it — the resolved target is an existing file
    // by construction, so the occupancy half then refuses that instead (`Some("real.wav already
    // exists")`, measured). "Already exists" is this call's *precondition*, not its error. The link half
    // would additionally refuse a dangling symlinked path, which `resolve_write_target` has already
    // judged. So this is a genuinely different primitive, not a duplicate: the guard that matters here is
    // that resolution — `target` is a real file, never a link — and it has already run.
    commit_replacement(&tmp, &target, existing.is_some()).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp); // never leave the temp behind on a failed commit
    })?;
    // CPE-1738: the save just succeeded, so THIS save's own temp is already gone — it became `target` via
    // the rename above. Sweep for a STALE sibling a DIFFERENT, earlier save of this same file left behind
    // by being killed between its own `create_new` and `rename`. Runs only here, after success, so a
    // failed save (which already removed its own temp above) never pays for it and the sweep is never on
    // the path a slow save is already blocking on.
    sweep_stale_temp_siblings(&target);
    Ok(())
}

/// **CPE-1739, the pure decision**: the target's own metadata could not be read — does the save go ahead
/// anyway, and does it have anything to carry across?
///
/// `err` is `None` when the stat succeeded. Three arms, and the middle one is the reason this is a
/// classifier rather than a `?`:
///
/// - **`None` → `Ok(true)`.** The file exists; its mode/attributes are what [`carry_protections`] and
///   [`commit_replacement`] preserve.
/// - **`NotFound` → `Ok(false)`.** There is nothing at the name yet. Creating a brand-new file at a free
///   name is a legitimate use of this function (`resolve_write_target` already admits it deliberately, and
///   `write_file_text`'s Save-As callers depend on it), and a file that does not exist has no protections
///   to carry. On Windows this arm is also what routes the save back to a plain rename — `ReplaceFileW`
///   requires an existing file to replace and answers `NotFound` otherwise (measured, see
///   [`commit_replacement`]).
/// - **Anything else → refuse.** This is the arm that makes CPE-1739's item 1 a *fix* rather than a
///   best-effort improvement. A stat that fails for a reason other than absence means we cannot tell
///   whether the file we are about to replace is `0600` or `0644`; staging anyway produces a file with
///   whatever the process umask hands out, so the one case where the answer matters most — an unreadable,
///   locked-down file — is exactly the case a "carry it if you can" policy would silently downgrade. Same
///   posture as [`classify_write_target`] and [`classify_create_slot`]: **not provably safe ⇒ do not
///   write.** The original is untouched and the message says so.
fn classify_carryover(err: Option<&std::io::Error>, target: &Path) -> Result<bool, String> {
    match err {
        None => Ok(true),
        Some(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Some(e) => Err(format!(
            "could not read what \"{}\" currently carries — its permissions and security settings, its \
             attributes, and its alternate data streams — so nothing was written. Saving anyway would \
             replace it with a brand-new file carrying only the defaults, so a file you had made private \
             could come back readable by others. Check the file is still there and that you can read it, \
             then save again: {e}",
            display_path(target)
        )),
    }
}

/// **CPE-1739**: copy onto the freshly-staged (still empty) file everything that is attached to the
/// **file object** rather than to its bytes, and that this platform can carry with `std` plus the
/// dependencies this crate already has.
///
/// # Unix: the mode, then the extended attributes
///
/// The mode is the whole of CPE-1739's items 1 and 2, measured through `metadata_write` in PR #904's UAT:
/// a `0600` private file came back `0644` and a `0755` script came back `0644`, both reported as a
/// successful save. [`std::fs::File::set_permissions`] on the staged handle (an `fchmod`, so no second path
/// lookup and no TOCTOU against the name) fixes both. [`carried_mode`] decides *which* bits.
///
/// Extended attributes are the Unix half of item 3 — the ticket measured the Windows half (alternate data
/// streams, `Zone.Identifier`) because that is where the UAT ran, but the same "attached to the object"
/// argument applies verbatim to xattrs, and the stakes here are higher than they look: **this app stores
/// its own metadata in them.** macOS Finder tags are `com.apple.metadata:_kMDItemUserTags` (CPE-826/829,
/// the `xattr` dependency in this crate's `Cargo.toml` exists for exactly that), and macOS's own
/// Mark-of-the-Web equivalent is `com.apple.quarantine`. A Metadata Studio save that dropped them would be
/// destroying the feature next door.
///
/// **Ownership is NOT carried and cannot be**: `chown` is not in `std`, and an unprivileged process cannot
/// give a file away regardless. The staged file therefore belongs to whoever ran the save, which is what
/// [`carried_mode`] takes into account.
///
/// # Windows: nothing here, on purpose — and what that costs during the write
///
/// Everything the Windows side loses — the attribute word, the ACL, named streams, the creation time —
/// belongs to the *destination*, and the OS has a primitive that carries it across at the moment of the
/// swap. Copying attributes onto the staged file would be a strictly worse imitation of it (it cannot
/// reach the ACL at all, and enumerating streams needs `FindFirstStreamW`/`FindNextStreamW` by hand). See
/// [`commit_replacement`], which is where the Windows answer lives.
///
/// **The consequence, stated because the Unix side makes a point of not having it** (CPE-1739 review
/// round 1, F2): the Unix path creates the staging file at [`STAGING_MODE`] so it is never for an instant
/// readable by anyone else, and Windows has no equivalent here. The staged `.cpe-tmp` is created with the
/// **parent directory's inherited ACL** and holds the user's bytes under it for the whole write, only
/// acquiring the target's own ACL when `ReplaceFileW` swaps it in. In a folder whose ACL is wider than the
/// file's — a shared folder holding one restricted file — the bytes are briefly reachable by whoever the
/// *folder* lets in. Not a regression (the pre-CPE-1739 rename had the same exposure and did not even
/// end with the right ACL), and not closed here: closing it means building the target's security
/// descriptor onto the staging handle by hand, which is the `SetFileSecurity`-by-hand route
/// [`commit_replacement`] rejects `ReplaceFileW` in favour of. Recorded rather than left as an
/// unstated asymmetry.
///
/// # Failure fails the save, and that is the point
///
/// Both steps that can fail the save do so loudly, with the temp removed and the original untouched.
/// Continuing past a failed `fchmod` would ship precisely the silent security downgrade this function
/// exists to close. The xattr copy is the one deliberate exception — see [`carry_xattrs`].
#[cfg(unix)]
fn carry_protections(
    source_path: &Path,
    source: &std::fs::Metadata,
    staged: &std::fs::File,
    staged_path: &Path,
) -> Result<(), String> {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
    let staged_uid = staged
        .metadata()
        .map_err(|e| format!("{}: could not stat the staging file, so nothing was written: {e}", display_path(staged_path)))?
        .uid();
    let mode = carried_mode(source.mode(), source.uid(), staged_uid);
    staged.set_permissions(std::fs::Permissions::from_mode(mode)).map_err(|e| {
        format!(
            "could not give the replacement for \"{}\" the same permissions the original had ({mode:04o}), \
             so nothing was written rather than leaving you a file that is more readable than the one you \
             saved: {e}",
            display_path(source_path)
        )
    })?;
    carry_xattrs(source_path, staged);
    Ok(())
}

/// The Windows half of [`carry_protections`] — deliberately empty; [`commit_replacement`]'s `ReplaceFileW`
/// is where the Windows answer lives. Kept as a function (rather than `#[cfg]`-ing the call site) so
/// `stage_and_replace` reads the same on both platforms and the reason is recorded once, above.
#[cfg(windows)]
fn carry_protections(
    _source_path: &Path,
    _source: &std::fs::Metadata,
    _staged: &std::fs::File,
    _staged_path: &Path,
) -> Result<(), String> {
    Ok(())
}

/// **CPE-1739, the pure decision**: which permission bits of `source_mode` the replacement file gets.
///
/// Pure `u32` arithmetic, so it compiles and is unit-tested on **every** runner, including the Windows one
/// that never calls it — a Unix-only policy tested only on Unix legs would be the weaker arrangement, and
/// the decision here has nothing platform-specific in it.
///
/// Two things happen:
///
/// 1. **Masked to `0o7777`.** [`std::os::unix::fs::MetadataExt::mode`] returns the whole `st_mode`,
///    file-type bits (`S_IFREG` and friends) included, and POSIX leaves `chmod`'s behaviour *unspecified*
///    for bits outside the permission set. Passing them through would be relying on Linux's tolerance and
///    hoping the other two runners agree.
/// 2. **`setuid`/`setgid` are dropped when the replacement would have a different owner.** Ownership
///    cannot be carried (see [`carry_protections`]), so a set-user-ID file replaced by a *different* user
///    keeps a bit that now means something else entirely: it used to run as the original owner and would
///    now run as whoever saved it. That is not "preserved", it is a quietly re-pointed privilege bit, and
///    the honest answer is to drop it and let the ordinary permission bits stand. This is not an
///    escalation being prevented — anyone who can replace a file in a directory could have created a
///    set-user-ID-themselves file there anyway — it is a *misrepresentation* being prevented. When the
///    owner is unchanged (overwhelmingly the common case: your own files) every bit is carried, `setuid`
///    included, because then the bit still means exactly what it meant before.
#[cfg_attr(not(unix), allow(dead_code))]
fn carried_mode(source_mode: u32, source_uid: u32, staged_uid: u32) -> u32 {
    let mode = source_mode & 0o7777;
    if source_uid == staged_uid {
        mode
    } else {
        mode & !0o6000 // S_ISUID | S_ISGID
    }
}

/// Copy `source`'s extended attributes onto `staged`. **Best effort, per attribute, and never fails the
/// save** — the one place in [`carry_protections`] where a failure is swallowed, so the reason is worth
/// stating rather than assuming.
///
/// The attributes that matter to a user (and to this app: Finder tags, `com.apple.quarantine`) live in the
/// `user.`/`com.apple.` namespaces and copy fine. `listxattr` also returns kernel-managed ones —
/// `security.selinux` on an SELinux system, `system.posix_acl_access` where POSIX ACLs are in use — and an
/// unprivileged owner frequently *cannot* set `security.*` even though nothing is wrong: the kernel has
/// already assigned the staged file a context by policy. Refusing the save there would make this app
/// unable to write files on an ordinary hardened Linux box, to protect a value the kernel re-derives
/// itself. So each attribute is attempted and a failure skips that one.
///
/// **The residual, stated rather than hidden:** an attribute that cannot be re-applied is silently lost,
/// exactly as it is today. This is strictly better than the status quo (where *all* of them are), and it
/// is the one gap in this function that a caller cannot see — so `src/docs/25-metadata-studio.md` says so
/// to the user too, rather than letting the residual disappear on the page people actually read
/// (CPE-1739 review round 1, F5). Filesystems with no xattr support at all (`listxattr` → `ENOTSUP`)
/// return early and cost one syscall.
///
/// # The staged side goes through the FILE DESCRIPTOR, the source side through the path
///
/// The asymmetry is deliberate rather than an oversight (CPE-1739 review round 1, F3, which noticed the
/// mode was carried by `fd` while xattrs were carried by name on both sides). `staged` is an `&File`
/// this function already holds, so `FileExt::set_xattr` is a plain `fsetxattr` on that descriptor — no
/// second path lookup, and nothing can swap an entry in at the staging name between the `fchmod` and the
/// attribute writes. The **source** side stays path-based because `stage_and_replace` holds no handle to
/// the target and opening one purely to read attributes would cost an extra open on every save and fail
/// outright on a file the user can write but not read — for a read whose worst case is copying a stale
/// attribute onto a file that is about to be replaced anyway. The half worth hardening is the half where
/// something is written, and that half is now on the descriptor.
#[cfg(unix)]
fn carry_xattrs(source: &Path, staged: &std::fs::File) {
    use xattr::FileExt as _;
    let Ok(names) = xattr::list(source) else { return };
    for name in names {
        if let Ok(Some(value)) = xattr::get(source, &name) {
            let _ = staged.set_xattr(&name, &value);
        }
    }
}

/// **CPE-1739**: swap the staged file into `target`'s place — with `ReplaceFileW` on Windows when there is
/// a file there to replace, and with `fs::rename` everywhere else.
///
/// # Why Windows gets a different primitive rather than one abstraction
///
/// The two platforms lose different things and have different repairs, and pretending otherwise would mean
/// shipping the weaker one twice. Unix's loss is the mode, and it is repaired *before* the swap, on the
/// staged file ([`carry_protections`]). Windows' loss — the attribute word, the DACL, named streams — is
/// attached to the **destination**, and `ReplaceFileW` is the OS primitive built for precisely this job:
/// it exists because the rename-based save idiom loses them.
///
/// Measured on Windows 11 for CPE-1739, same file, same run, `HIDDEN` set and a real `Zone.Identifier`
/// stream written with `path:stream` syntax:
///
/// ```text
/// before             attrs=0x802 (HIDDEN)      ADS=Ok("[ZoneTransfer]\r\nZoneId=3\r\n")
/// after fs::rename   attrs=0x820 (HIDDEN lost) ADS=Err(NotFound)    <- CPE-1739 item 3
/// after ReplaceFileW attrs=0x822 (HIDDEN kept) ADS=Ok("[ZoneTransfer]\r\nZoneId=3\r\n")
/// ```
///
/// `REPLACEFILE_IGNORE_MERGE_ERRORS` is the documented flag for "carry the ACL and attributes across, but
/// do not fail the whole save if you cannot" — without it a save into a folder where the caller lacks
/// `WRITE_DAC` would start failing outright, which trades one silent loss for a loud regression on
/// ordinary files. No backup file is requested (`lpBackupFileName` is null): a backup is the *third*
/// option CPE-1739 listed, not this one, and asking for one would leave a second stray file next to the
/// user's on every crash — the thing CPE-1738 just finished cleaning up.
///
/// # What this does NOT fix, measured
///
/// **CPE-1739 item 4 — a save that used to work still fails while another program holds the file open with
/// `SHARE_READ|WRITE`** (what an ordinary Windows application holds, and *not* what Rust's own
/// `File::open` takes, which adds `FILE_SHARE_DELETE` and is unaffected by either primitive):
///
/// ```text
/// foreign SHARE_READ|WRITE handle held:  fs::rename   -> Err("Access is denied. (os error 5)")
///                                        ReplaceFileW -> Err("...being used by another process.")
/// std::fs::File::open handle held:       ReplaceFileW -> Ok(())
/// ```
///
/// So the failure is unchanged in kind (the error is at least now accurate about the cause), and it cannot
/// be fixed by any amount of care on the *replacement* file — the obstacle is the **target's** sharing
/// mode, which only writing in place would sidestep. That is CPE-1739's option 3, and taking it would give
/// up the atomicity a half-rewritten media file is the whole reason for. Recorded, not closed.
/// [`replace_file_contents`]'s "What this does NOT do" and `src/docs/25-metadata-studio.md` say the same
/// thing to the user. **A read-only target is refused too, by both primitives** — `fs::rename` →
/// `Access is denied. (os error 5)`, `ReplaceFileW` → `Access is denied. (0x80070005)`, the same refusal
/// rendered through the `windows` crate's `HRESULT` formatting rather than `std::io`'s (CPE-1739 review
/// round 1, F6 — an earlier version of this line called the two strings identical, which they are not).
/// So that is pre-existing behaviour this change neither fixes nor worsens, and every error this function
/// returns on Windows now carries an `0x8007NNNN` code rather than an `os error N`.
///
/// # No silent fallback to `fs::rename`
///
/// If `ReplaceFileW` fails, the save fails. Falling back to a rename would restore the exact data loss this
/// function exists to prevent and do it **invisibly** — a user's Mark of the Web would evaporate only on
/// whichever filesystems happened not to support the call, which is the least observable place for it to
/// happen. A failed save is loud, leaves the original byte-for-byte intact (measured above: the target
/// still read `old bytes` after both failure modes), and can be retried.
///
/// # The cost, measured rather than assumed
///
/// `ReplaceFileW` is materially more expensive than `MoveFileEx`, because it is doing materially more:
/// 200 iterations, 64 KiB payload, local NVMe — **~0.22 ms per commit for `fs::rename` versus ~6.2 ms for
/// `ReplaceFileW`**, plus ~11 µs for the one extra `fs::metadata` [`classify_carryover`] needs. Against
/// `PURPOSE.md`'s fast/small/predictable tiebreaker that is a real cost, and it is accepted for a bounded
/// reason: the only caller is `metadata_write`, a user-initiated single save that has already spent far
/// longer parsing and rewriting a media file, and 6 ms is not perceptible in it. The path that *would*
/// have made this a general tax — `write_file_text`, the ordinary text save — does not use this function at
/// all (CPE-1725's narrowing) and pays nothing.
fn commit_replacement(tmp: &Path, target: &Path, target_exists: bool) -> Result<(), String> {
    let _ = target_exists; // read on Windows only — see below
    #[cfg(windows)]
    if target_exists {
        use std::os::windows::ffi::OsStrExt as _;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::{ReplaceFileW, REPLACEFILE_IGNORE_MERGE_ERRORS};
        fn wide(p: &Path) -> Vec<u16> {
            p.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
        }
        let (replaced, replacement) = (wide(target), wide(tmp));
        // SAFETY: both buffers are NUL-terminated and outlive the call; `lpBackupFileName` is null (no
        // backup wanted, see above) and both reserved out-parameters are null, as documented.
        return unsafe {
            ReplaceFileW(
                PCWSTR(replaced.as_ptr()),
                PCWSTR(replacement.as_ptr()),
                PCWSTR::null(),
                REPLACEFILE_IGNORE_MERGE_ERRORS,
                None,
                None,
            )
        }
        .map_err(|e| format!("{}: {e}", display_path(target)));
    }
    // Unix always, and Windows only when there is nothing at the name to replace — `ReplaceFileW` answers
    // `NotFound` for an absent target (measured), and a brand-new file has nothing to carry across anyway.
    #[allow(clippy::disallowed_methods)]
    std::fs::rename(tmp, target).map_err(|e| format!("{}: {e}", display_path(target)))
}

/// **CPE-1738**: best-effort collection of a `.cpe-tmp` sibling stranded by an EARLIER, DIFFERENT save of
/// `target` that was killed between [`stage_and_replace`]'s `create_new` and its `rename` — never a save
/// still in flight. Called once, from [`stage_and_replace`], after its own rename has already succeeded.
///
/// ## The decision, recorded here because CPE-1738 asked the question outright
///
/// The ticket listed three options, in increasing cost: leave it (the docs already explain the stray file
/// and it is genuinely harmless); sweep opportunistically on the next save into the same directory; sweep
/// on startup across every folder the app has ever saved into. The third was rejected first — the app
/// tracks no such list today, and building one would be a second feature purely to support this one. Doing
/// nothing was rejected too, weighed against `PURPOSE.md`'s own fast/small/predictable tiebreaker:
/// "predictable" cuts both ways, and a file that silently accumulates forever in a folder the user curates
/// themselves is not the predictable, small footprint that tiebreaker is protecting — it is the opposite,
/// dressed up as inaction. So this is the middle option, kept deliberately narrow:
///
/// - **Scoped to `target`'s OWN stale siblings, matched on the STAMP SHAPE, never a loose
///   `starts_with`/`ends_with` pair.** The staged name is always `<target-file-name>.<pid>-<nanos>.cpe-tmp`
///   — [`is_valid_temp_stamp`] requires the segment between the stripped prefix and the stripped suffix to
///   be exactly two non-empty ASCII-digit runs joined by one `-`, nothing else. **This was not always true
///   and the looser version shipped once** (PR #910 review round 2, all three of UAT/Reviewer/Security
///   converging on the same finding independently): checking `starts_with(prefix)` and
///   `ends_with(".cpe-tmp")` as two *independent* conditions, with nothing checking there was a valid stamp
///   — or anything at all — between them, let two real exposures through. First, the prefix's trailing dot
///   and the suffix's leading dot are the *same character*, so a real file literally named
///   `<target-file-name>.cpe-tmp` — the exact name the shipped docs used to tell a user to keep by hand —
///   satisfied both conditions with an EMPTY middle and was permanently, silently unlinked (UAT measured
///   this against a real file). Second, a genuinely different file's own in-flight temp — saving `a.txt`
///   while `a.txt.bak` is independently mid-save, staged as `a.txt.bak.<pid>-<nanos>.cpe-tmp` — also starts
///   with `a.txt.` and ends with `.cpe-tmp`, so it was swept as if it were `a.txt`'s own leftover (Reviewer
///   measured: saving `a.txt` left `a.txt.bak`'s temp gone, `a.txt.bak`'s own rename then failing). Once the
///   middle segment is required to be exactly the stamp shape, neither survives the check: `""` fails (no
///   `-` at all) and `"bak.4242-1000000000000"` fails (the part before the `-` is not all digits).
///   **Residual, stated rather than hidden:** a file whose real name happens to be
///   `<target-file-name>.<digits>-<digits>.cpe-tmp` — deliberately mimicking the exact stamp shape, not
///   merely starting and ending right — is still indistinguishable from a genuine stale temp and would be
///   swept. Vanishingly unlikely to occur by accident (unlike the empty-middle and sibling-extension cases
///   above, which are ordinary names), and not closed by any filename-pattern check alone; recorded so it is
///   not mistaken for closed.
/// - **An age floor, not a pid-liveness check, is what tells "stale" from "live".** The ticket asked how
///   to avoid deleting a temp that belongs to a live concurrent save, and the tempting-looking answer —
///   parse the pid back out of the stamp and ask the OS whether it is still running — needs a
///   process-enumeration dependency this crate does not otherwise carry (against the "small" half of the
///   tiebreaker above) and is wrong on its own terms besides: a pid is only unique while that process is
///   alive, so "pid P is running" does not prove *that* file was written by *this* app, and "pid P is not
///   running" does not prove the save that pid made finished — a save's own process can outlive the write
///   that staged one particular temp (a second tab, a second save). An age floor answers the actual
///   question — "could a save still plausibly be writing this?" — directly, needs no extra dependency, and
///   is exactly what the ticket's own acceptance test asks for: a temp created seconds ago must survive.
///   [`STALE_TEMP_FLOOR`] is comfortably above that.
/// - **The age reference is THIS FILESYSTEM's own clock, never the client's.** PR #910 review round 2
///   (Blocker 4) measured a real SMB mount whose clock ran minutes off the client's — comparing a
///   share-stamped mtime against `SystemTime::now()` made the floor read anywhere from "minutes generous" to
///   effectively **zero**, depending on drift direction, on exactly the network target
///   (`qnap-nas-test-target`) this app is built to work against. The reference is now `target`'s OWN mtime,
///   read immediately after [`stage_and_replace`]'s rename just set it — the same filesystem stamps both
///   sides of the comparison, so client/share clock skew cancels out however large it is. If `target`'s own
///   mtime cannot be read, the sweep is skipped entirely for this call rather than falling back to the
///   client clock, which would silently reintroduce the same skew in the rare path meant to be safer.
/// - **What this does NOT close: a save that is merely SLOW, not killed, can still lose its own temp** (PR
///   #910 review round 2, Blocker 3 — stated rather than papered over, matching this crate's convention of
///   recording a known gap instead of overselling a fix, see `replace_file_contents`'s "What this does NOT
///   do"). The floor answers "could a save still plausibly be writing this?" with a duration, and a
///   duration cannot distinguish "still writing, unusually slowly" from "the process that was writing this
///   is gone" — that distinction needs OS-level open-handle detection, and Windows' own semantics defeat
///   the obvious version of it besides: Rust opens a file with `FILE_SHARE_DELETE`, and `unlink` on Unix
///   never blocks on an open handle either way, so an open handle offers no protection to check for even if
///   this reached for one. So: if save A of `target` is unusually slow (a huge file on a very slow link, or
///   a suspended machine) and takes longer than [`STALE_TEMP_FLOOR`] to reach its own rename, a SEPARATE,
///   independent save B of the SAME `target` that completes in the meantime will sweep A's still-live temp
///   out from under it — A's own rename then fails, loudly, with an error naming `target` (not the vanished
///   source, which the OS's `NotFound` does not distinguish). **The user's file is never damaged either
///   way** — that guarantee comes from the staging design (the original is never touched until a rename
///   succeeds), not from this sweep — but save A itself is lost and must be retried. Narrow in practice (it
///   needs two overlapping saves of the very same file, one pathologically slower than the floor), not
///   closed here, and not silently assumed closed: closing it needs real liveness detection, which is the
///   same dependency-cost trade-off the pid-liveness idea above was rejected for, so it is out of scope for
///   this ticket rather than fixed by a bigger floor (a bigger floor narrows the window; it cannot close it,
///   since some save is always slower than any fixed number).
/// - **Never touches a symlink.** Checked with [`std::fs::symlink_metadata`] (never [`std::fs::metadata`],
///   which would follow it) before either the age check or the removal, so a symlink whose name happens to
///   match the pattern is skipped outright regardless of its age — this module's established posture
///   everywhere else a name is about to be removed or written through (see [`entry_is_symlink`],
///   [`symlink_slot_refusal`]). The pure decision lives in [`should_sweep_temp`], split out for the same
///   reason [`classify_symlink_slot`] is: ageing a REAL symlink's own timestamp without following it needs
///   a platform call `std::fs` does not expose, so a real-IO test could only ever prove "a young link
///   survives" — which a young ordinary file would too, for an unrelated reason (the age floor). The pure
///   function lets the link-protection arm be asserted on its own, independent of age.
/// - **Runs after the rename succeeds, never before staging.** Putting it before the write would tax every
///   save — including the overwhelming majority that have no stale sibling at all — with a directory scan
///   before the user's bytes have even started moving. Running it after success means the cost is paid
///   only once the save has already done its real I/O, and a failed save (which already removes its own
///   temp, see [`stage_and_replace`]'s error branches) never reaches this at all.
///
/// ## The cost, measured rather than assumed (PR #910 review round 2)
///
/// The scan is one `read_dir` of the folder the save just finished writing into. Measured directly against
/// a plain local write (0.19ms): an EMPTY directory adds roughly 1ms; a directory of 20,000 entries adds
/// ~37.8ms (~1.9µs/entry) — real cost, not the "almost always match nothing so nothing is stat'd" the first
/// version of this comment claimed, which described the name filter and ignored that `read_dir` itself, not
/// the filter, is what dominates on a large directory. That cost is now BOUNDED: the scan stops after
/// [`SWEEP_SCAN_CAP`] entries regardless of the directory's real size, so the worst case on any single save
/// is fixed rather than growing with the folder. The trade this makes explicit: a stale temp sitting past
/// entry [`SWEEP_SCAN_CAP`] in an enormous directory may simply survive longer than it otherwise would —
/// tidiness deferred, never a correctness concern — in exchange for every save in that folder paying a
/// predictable, bounded price instead of one proportional to how many files happen to be in it. It happens
/// once per SUCCESSFUL save, never per failed one, and never blocks the write itself (it runs strictly
/// after); every error inside it (the read_dir itself, a stat, a remove) is swallowed rather than surfaced,
/// so a slow or flaky network mount can never turn an already-successful save into a reported failure.
fn sweep_stale_temp_siblings(target: &Path) {
    let Some(dir) = target.parent() else { return };
    let Some(name) = target.file_name() else { return };
    // The reference clock for "how old is this candidate?" — THIS filesystem's own idea of `target`'s
    // mtime, just set by the rename this function's caller performed. See the "age reference" bullet
    // above (Blocker 4): comparing against the client's `SystemTime::now()` instead let share/client clock
    // skew move the effective floor anywhere from "generous" to "zero". No reference, no sweep this time.
    let Ok(reference) = std::fs::symlink_metadata(target).and_then(|m| m.modified()) else { return };
    // Raw bytes, not `to_string_lossy` — two names that differ only in invalid-UTF-8 bytes both collapse
    // to the same U+FFFD replacement text under a lossy conversion, so a name comparison on the lossy form
    // could match a DIFFERENT file's temp on a non-UTF-8 filesystem (Linux allows arbitrary bytes in a
    // filename). `OsStr::as_encoded_bytes` compares the platform's own bytes, is exact, and needs no
    // allocation per entry (PR #910 review round 2, non-blocking item, fixed anyway since it was cheap).
    let name_bytes = name.as_encoded_bytes();
    let Ok(entries) = std::fs::read_dir(dir) else { return }; // best-effort: see doc comment above
    for entry in entries.flatten().take(SWEEP_SCAN_CAP) {
        let fname = entry.file_name();
        let fbytes = fname.as_encoded_bytes();
        // Prefix (`name_bytes` + the literal `.`), then suffix, then validate what's LEFT is the exact
        // `<digits>-<digits>` stamp shape — never `starts_with`/`ends_with` as two independent conditions
        // (Blocker 1: that let an empty middle and another file's own extension-suffixed temp both
        // through). See `is_valid_temp_stamp`.
        if fbytes.len() <= name_bytes.len() + 1
            || &fbytes[..name_bytes.len()] != name_bytes
            || fbytes[name_bytes.len()] != b'.'
        {
            continue;
        }
        let rest = &fbytes[name_bytes.len() + 1..];
        let Some(stamp_bytes) = rest.strip_suffix(b".cpe-tmp") else { continue };
        // A genuine stamp is pure ASCII digits and `-`, so this can only fail (and skip) on garbage that
        // could never be a valid stamp anyway — `is_valid_temp_stamp` still does the real check.
        let Ok(stamp) = std::str::from_utf8(stamp_bytes) else { continue };
        if !is_valid_temp_stamp(stamp) {
            continue;
        }
        let candidate = entry.path();
        // `symlink_metadata`, not `metadata` — a link is never followed, and its OWN timestamp (not its
        // target's) is what `should_sweep_temp` sees, which is irrelevant anyway because the link half of
        // that decision short-circuits before the age is ever consulted.
        let Ok(meta) = std::fs::symlink_metadata(&candidate) else { continue };
        let Ok(modified) = meta.modified() else { continue };
        let age = reference.duration_since(modified).unwrap_or(std::time::Duration::ZERO);
        if should_sweep_temp(meta.file_type().is_symlink(), age, STALE_TEMP_FLOOR) {
            let _ = std::fs::remove_file(&candidate); // best-effort: see doc comment above
        }
    }
}

/// How far above any plausible in-flight save [`sweep_stale_temp_siblings`]'s age floor sits. An ordinary
/// save — even a large media file on slow media — completes in, at most, low tens of seconds; this is an
/// order of magnitude past that, deliberately, so a save that is merely slow is never mistaken for one that
/// was killed. The ticket's own acceptance test needs only "a few seconds" of headroom; this gives minutes.
/// **Does not, by itself, close Blocker 3** (a save slower than this WILL still lose its temp to a
/// concurrent sweep) — see [`sweep_stale_temp_siblings`]'s doc comment; raising this number narrows that
/// window without ever closing it, since some save is always slower than any fixed floor.
const STALE_TEMP_FLOOR: std::time::Duration = std::time::Duration::from_secs(300);

/// Upper bound on how many directory entries [`sweep_stale_temp_siblings`] will examine in one call
/// (PR #910 review round 2, the measured hot-path cost). ~1.9µs/entry measured, so this bounds one save's
/// worst-case added latency to roughly `SWEEP_SCAN_CAP` * 2µs regardless of how large the directory
/// actually is, at the cost of a stale temp past this many entries surviving longer in a very large folder
/// — tidiness deferred, never a correctness concern, and no worse than doing nothing for that folder at
/// all (which was one of the ticket's own on-the-table options).
const SWEEP_SCAN_CAP: usize = 4096;

/// Whether `s` is exactly the `<pid>-<nanos>` stamp [`stage_and_replace`] writes: two non-empty runs of
/// ASCII digits joined by exactly one `-`, nothing before, after, or in between. This is what actually
/// distinguishes a genuine staged temp from an arbitrary name that merely starts and ends right (PR #910
/// review round 2, Blocker 1) — an empty middle (a real file the user kept, named exactly
/// `<target-name>.cpe-tmp`) and a different file's own extension-suffixed temp (`a.txt.bak`'s
/// `a.txt.bak.<pid>-<nanos>.cpe-tmp`, matched while saving `a.txt`) both satisfy `starts_with`/`ends_with`
/// but fail this: the first has no `-` to split on, the second's pre-`-` half is not all digits.
fn is_valid_temp_stamp(s: &str) -> bool {
    match s.split_once('-') {
        Some((pid, nanos)) => {
            !pid.is_empty()
                && !nanos.is_empty()
                && pid.bytes().all(|b| b.is_ascii_digit())
                && nanos.bytes().all(|b| b.is_ascii_digit())
        }
        None => false,
    }
}

/// The pure decision behind [`sweep_stale_temp_siblings`]: should this ONE already-matched candidate be
/// removed? Split out, and unit-tested as a truth table, for the reason given in
/// [`sweep_stale_temp_siblings`]'s own doc comment — the link arm cannot be proven by real-IO ageing alone.
fn should_sweep_temp(is_symlink: bool, age: std::time::Duration, floor: std::time::Duration) -> bool {
    !is_symlink && age >= floor
}

/// Render a path for a **user-facing message**, stripping Windows' `\\?\` verbatim prefix.
///
/// [`std::fs::canonicalize`] returns a verbatim path on Windows (`\\?\C:\music\track.wav`), and a
/// resolved path is exactly what the interesting errors here name — the read-only far end of a link, or a
/// link that resolves to a directory (PR #899 UAT round 2 nit). `\\?\` is a Win32 API escape hatch, not
/// something the user typed or will recognise, so it is noise in an error message even though the path
/// itself is correct and useful.
///
/// `\\?\UNC\server\share` is folded back to `\\server\share` rather than left as a bare `UNC\…`, which is
/// not a path at all. Everything else, including every Unix path, is returned unchanged.
pub fn display_path(p: &Path) -> String {
    let s = p.display().to_string();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s)
}

/// Which path [`replace_file_contents`] should actually write: `path` itself, or — when `path` is a
/// symlink — the file it points at. The IO half; [`classify_write_target`] is the decision.
pub fn resolve_write_target(path: &Path) -> Result<PathBuf, String> {
    let is_link = std::fs::symlink_metadata(path).map(|m| m.file_type().is_symlink());
    classify_write_target(&is_link, || std::fs::canonicalize(path), path)
}

/// The pure decision behind [`resolve_write_target`], split out for the same reason
/// [`classify_symlink_slot`] is split out of [`symlink_slot_refusal`] — and this time the reason is sharper
/// than style. **A live *file* symlink cannot be staged without privilege on Windows**
/// (`SeCreateSymbolicLinkPrivilege`): [`make_dangling_link`]'s junction fallback is directory-only, and a
/// junction reports `is_symlink = true` but fails `canonicalize` with `NotADirectory`, so it stages the
/// *dangling* verdict and cannot stand in for a live link. With the decision inline, the live-link arm —
/// the one CPE-1716's data loss actually travelled through — would be untestable on any machine without
/// that privilege. As a pure function every arm is exercised on every account. (CI's `windows-latest`
/// turns out to *have* the privilege — measured, see the live-link test — so this buys coverage for an
/// unprivileged contributor machine rather than for CI.)
///
/// `resolve` is a closure rather than a value so the canonicalisation is not performed for a path that is
/// not a link, which is the overwhelmingly common case.
///
/// # Failure policy, and the one place it is weaker than it reads
///
/// In the same spirit as [`classify_target_slot`]: **only a proven "not a link" proceeds on the original
/// path.** If the link check fails for a reason other than `NotFound` the write is refused rather than
/// guessed at, because guessing here means renaming over something we could not identify. `NotFound`
/// proceeds, because creating a brand-new file at a free name is a legitimate use.
///
/// **`NotFound` is not always proof of absence on Windows**, and saying otherwise would repeat the exact
/// over-claim this repo has filed two tickets about. Rust folds `ERROR_BAD_NETPATH` (53),
/// `ERROR_BAD_NET_NAME` and `ERROR_INVALID_DRIVE` into `ErrorKind::NotFound`, so a **disconnected UNC
/// share** reaches this arm and is classified free (PR #899 Reviewer). Nothing is destroyed — the path is
/// unreachable, so `create_new` in [`replace_file_contents`] then fails and the save reports it — but on
/// that route `create_new` is the only thing standing between the classifier and a write, which is worth
/// knowing before anyone "simplifies" it back to `fs::write`. This matches [`classify_target_slot`]'s
/// pre-existing policy rather than inventing a new one, and it is contained; it is recorded, not fixed.
pub fn classify_write_target(
    is_link: &std::io::Result<bool>,
    resolve: impl FnOnce() -> std::io::Result<PathBuf>,
    path: &Path,
) -> Result<PathBuf, String> {
    match is_link {
        // Not a link — the ordinary case. Write where the caller said.
        Ok(false) => Ok(path.to_path_buf()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(path.to_path_buf()),
        Ok(true) => match resolve() {
            Ok(real) => Ok(real),
            // A dangling link (or a link chain that does not resolve). There is no file to edit, and the
            // two ways to proceed anyway — invent the file at the far end, or write over the link itself —
            // are respectively a surprise and the CPE-1716 bug.
            //
            // **The wording describes the alternatives, not one caller's history** (CPE-1725, PR #904
            // UAT). It previously said "editing it would have destroyed the link and left the edit
            // nowhere", which is what CPE-1716's `fs::rename` did — and is **false** for the other caller
            // this message now serves: `write_file_text`'s `fs::write` left the link perfectly intact and
            // conjured the target instead (measured, `Ok(7)` with the target created). A shared message
            // that narrates one caller's bug tells the other caller's user about a danger that never
            // applied to them, so it names both hazards and asserts neither happened.
            Err(e) => Err(format!(
                "\"{}\" is a link and what it points at could not be opened, so nothing was written — \
                 saving would have had to either invent the missing file at the far end of the link or \
                 write over the link itself, and neither is the file you opened: {e}",
                path.display()
            )),
        },
        Err(e) => Err(format!(
            "could not check whether \"{}\" is a link, so nothing was written — refusing to guess rather \
             than risk destroying one: {e}",
            path.display()
        )),
    }
}

/// Whether a directory entry is a symlink (without following it). Used to avoid symlink cycles in the
/// recursive walks (CPE-609/611).
pub fn entry_is_symlink(entry: &std::fs::DirEntry) -> bool {
    entry.file_type().map(|t| t.is_symlink()).unwrap_or(false)
}

/// Stream a file through SHA-256 and return the lowercase hex digest. Shared by `hash_file` (CPE-412),
/// the folder checksum baseline (CPE-791), and the backup verifier. 64 KiB chunks — a multi-GB file
/// never loads into memory.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    // Lowercase hex — one dependency fewer than pulling in `hex` for three lines.
    let mut hex = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
    }
    Ok(hex)
}

/// True when Win32 path normalisation would silently rewrite this single path **component** — i.e. it
/// carries trailing spaces or dots (CPE-1664/CPE-1662, PR #855 security audit).
///
/// Win32 strips trailing `' '` and `'.'` from the last component of a path before opening it, so a
/// component that is *entirely* spaces/dots addresses **its own parent**: `dir\ `, `dir\...` and
/// `dir\. ` all open `dir`. A component that merely *ends* in one addresses a different sibling:
/// `dir\report. ` opens `dir\report`. Neither is ever what a caller meant, and both are catastrophic
/// where the resolved path is then handed to `remove_dir_all` — a plan entry or a transfer source name
/// spelled this way deletes the destination root instead of an item inside it.
///
/// Rust's `Path::components()` cannot be used to detect this: it special-cases exactly `.` and `..`,
/// and classifies **every** other string — including `" "`, `"..."` and `". "` — as
/// [`std::path::Component::Normal`]. That is why the containment check on the *resolved* path is the
/// real defence and this predicate is only the cheap first filter.
///
/// **The predicate is uniform; acting on it must NOT be.** `foo ` and `notes.` are legal, creatable,
/// everyday filenames on Linux and macOS, where `dir/notes.` is a real distinct path and nothing is
/// aliased. Callers must therefore gate the **refusal** on `cfg!(windows)` — the first version of this
/// change did not, and the result was that a macOS user moving a folder named `My Documents ` got an
/// error about Windows path normalisation and the move failed, while a Linux backup of `notes.` was
/// silently never copied. That is breaking a basic operation on two platforms to defend against a
/// hazard that exists only on the third, and [`contained_under`] already covers the destructive case
/// platform-independently, so this predicate is not the thing carrying the safety.
///
/// The function itself stays uniform so both legs compile and test the same shape, and so a caller can
/// report or warn on such a name without refusing it.
///
/// The empty string is **not** unstable by this rule (`"".trim_end_matches(..) == ""`); callers reject
/// empty components separately, since an empty component is a different bug with a different message.
pub fn win32_name_is_unstable(name: &str) -> bool {
    name != name.trim_end_matches([' ', '.'])
}

/// **The containment guarantee** shared by every "remove the thing already at this path" site
/// (CPE-1664/CPE-1662, PR #855 security audit): assert on the *resolved* path, never on the spelling
/// that produced it.
///
/// Canonicalise both sides and require `joined` to be strictly **inside** `root` — `starts_with(root)`
/// **and** `!= root`. That is the only formulation that holds without enumerating spellings, so it
/// covers the seven the audit found and whatever normalisation quirk, junction, case-folding share or
/// Unicode-folding filesystem produces the next one. Textual filters in front of it are a cheap first
/// pass, never a substitute.
///
/// # Failure policy — fails CLOSED on the side that matters
///
/// - `root` won't canonicalise → **`Err`**. There is nothing legitimate to remove under a container
///   that doesn't resolve, so the destructive call must not be the default when IO fails. (The first
///   version of the transfer-side copy of this check used `if let (Ok(a), Ok(b)) = …`, which fell
///   straight through to `remove_dir_all` when either `canonicalize` errored — the wrong way round for
///   the one check standing between a consented Replace and the user's folder. That is why there is now
///   exactly one implementation with one failure policy.)
/// - `joined` won't canonicalise → **`Ok`**.
///
/// # Precondition — `joined` must be an EXISTING target that is about to be removed
///
/// The `Ok` on an unresolvable `joined` is only sound because a path that does not exist cannot be
/// destroyed: the caller's `remove_*` will fail and be reported normally. **Do not reuse this to
/// validate a create/copy destination.** Such a target is *expected* not to exist yet, so this would
/// return `Ok` for exactly the case it was meant to judge — a guard that fails open every time. A
/// create-side check needs to canonicalise the target's *parent* instead.
///
/// Both current callers satisfy the precondition: the backup mirror-delete loop is about to
/// `remove_dir_all`/`remove_file` `joined`, and `resolve_conflict`'s Overwrite arm is only reached
/// after `base_target.exists()` has already returned true.
pub fn contained_under(joined: &Path, root: &Path) -> Result<(), String> {
    let Ok(real_root) = std::fs::canonicalize(root) else {
        return Err(format!("the containing directory {root:?} could not be resolved"));
    };
    let Ok(real) = std::fs::canonicalize(joined) else {
        return Ok(()); // doesn't exist — nothing to destroy; the caller's remove reports it normally
    };
    if real == real_root {
        return Err(
            "the path resolves to the containing directory itself, not to something inside it"
                .to_string(),
        );
    }
    if !real.starts_with(&real_root) {
        return Err(format!("{real:?} resolves outside the containing directory {real_root:?}"));
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────────────────────────
// CPE-1726/CPE-1731 — "does this destination name the served root?", decided in ONE place
// ─────────────────────────────────────────────────────────────────────────────────────────────────

/// Resolve `.` and `..` lexically so two spellings of the same place compare equal.
///
/// Private on purpose: it is a *step* of [`same_place`], never the answer. CPE-1731's ticket had to be
/// corrected for prescribing this function alone — it is blind to the spellings only the filesystem
/// knows are equal (Windows case-insensitivity, trailing dots), which is what [`same_place`]'s
/// `canonicalize` half exists for.
///
/// **This function does not close the `/./` family, and an earlier version of this doc claimed it
/// did.** `Path::components()` already drops every non-leading `CurDir` before anything here runs —
/// probe: `Path::new(r"C:\tmp\rig\.\.").components()` yields `[Prefix, RootDir, "tmp", "rig"]`, no
/// `CurDir` at all. Deleting the `CurDir` arm below left all of `cpe-webdav`'s tests green; deleting
/// the whole filter at the commit before `..` popping was added left all 25 green. The arm is kept as
/// a total match over `Component` rather than as load-bearing code, and the `/./` rows do not even
/// reach here now that `canonicalize` short-circuits them. Recorded because crediting a function with
/// work the standard library had already done is the same over-attribution this family keeps finding
/// in other people's comments. (PR #902 review.)
///
/// **`..` is popped, and that changed in CPE-1726's round 4.** Round 4 first *preserved* `..`,
/// reasoning that popping it would turn this into a containment check and containment is CPE-1730's
/// scope. The UAT showed the reasoning had skipped a case: `/nonexistent/..` escapes nothing and needs
/// no knowledge of the server, but lands **exactly on the served root** — so it was a destination that
/// resolved to the root, in a guard whose entire subject is destinations that resolve to the root,
/// answered `201 Created`. Same for `/sub/..`, `/./sub/../.`, and every other spelling of the shape.
///
/// So `..` pops a preceding ordinary component. What it does **not** do is claim containment: a `..`
/// with nothing to pop is *kept*, so `/../x` normalises to a path still carrying `..`, compares unequal
/// to the root, and is allowed through to the caller's primitive — which is the documented CPE-1730
/// escape, unchanged by this. ([`contained_under`] is the check for *that* question; this one answers
/// only "is it the root itself".)
///
/// **Lexical `..` resolution is not sound in the presence of symlinks** (`a/link/..` need not be `a`) —
/// and the *direction* of that unsoundness is what bounds it. It errs **safe**:
///
/// 1. If `canonicalize` succeeds on **both sides**, the filesystem decides and this function never
///    runs.
/// 2. So the lexical path runs only when at least one side failed to canonicalize. When that failure is
///    `ENOENT`, the path has no true resolution to disagree with — in particular it cannot have a
///    symlink as its *final* component, because that component does not exist. **This step covers
///    `ENOENT` only.** For the other failure modes [`same_place`] falls back on — `EACCES` on a parent,
///    `ELOOP`, `ENAMETOOLONG` — the path may well exist, may have a symlink final component, and does
///    have a true resolution we simply cannot see.
/// 3. For everything that reaches here — **for any reason, including those cases** — popping only makes
///    the path *shorter*, hence *more* likely to equal the root, hence more likely to **refuse**. Step
///    3 holds unconditionally, which is why the bound survives step 2's narrower scope.
///
/// There is no input for which popping makes a root-destination compare unequal. The unsound direction
/// refuses a legitimate move; it can never allow one onto the root.
///
/// Step 2 previously stated its `ENOENT` reasoning over *every* `canonicalize` failure. The conclusion
/// was never at risk — it rests on step 3 — but the justification was wider than its evidence, which is
/// the exact family that paragraph exists to bound, appearing inside the paragraph. It took three
/// passes to see, the third being the reviewer's. (PR #902.)
fn normalise_lexically(p: &Path) -> PathBuf {
    let mut out: Vec<std::path::Component> = Vec::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                // Pop only an ordinary name. A `..` above a prefix/root — or above another surviving
                // `..` — is kept, so this never silently claims containment.
                if matches!(out.last(), Some(std::path::Component::Normal(_))) {
                    out.pop();
                } else {
                    out.push(c);
                }
            }
            _ => out.push(c),
        }
    }
    out.iter().collect()
}

/// Does `dest` name the same place as `root`?
///
/// The property behind every "a rename/MOVE destination must not be the served root itself" guard in
/// this repo's protocol test rigs (`cpe-webdav`'s `MOVE`, `cpe-ftp`'s `RNTO`, `cpe-sftp`'s `rename`).
/// **Ask the question; never enumerate the spellings.** CPE-1726 tried a denylist three rounds running
/// (`""`, then `""`+`.`, then a table of seven) and the UAT falsified each one on the code that shipped
/// it — the third under a doc calling its table *"exhaustive over the shapes that resolve to the served
/// root"*. A denylist closes the members someone thought of; this closes the family.
///
/// # Why both halves
///
/// Lexical normalisation alone is not enough on Windows, which CPE-1726's round-4 UAT measured: the
/// filesystem matches names **case-insensitively** and **strips trailing dots**, while `PathBuf`
/// equality compares `Component::Normal` byte-wise. So a destination naming the served root as
/// `...\CPE-WEBDAV-47084` or `...\cpe-webdav-47084.` compared unequal and was answered `201 Created`.
///
/// `canonicalize` answers exactly that question and is the filesystem's own opinion rather than a table
/// of its rules — but it requires the path to **exist**, and a rename to a not-yet-existing name is the
/// ordinary case. So: canonicalize when both sides resolve, fall back to a purely lexical comparison
/// (`normalise_lexically`, private, below) when they do not. The fallback is what handles the common
/// `/`, `/./`, `/sub/..` shapes, none of which need the filesystem.
///
/// **"When they do not resolve" means any `Err`, not just "does not exist"** — `EACCES` on a parent,
/// `ELOOP`, `ENAMETOOLONG`, a Windows sharing violation. In those cases this silently becomes the
/// byte-wise comparison, with the case-insensitivity and trailing-dot blind spots that motivated
/// `canonicalize` in the first place. That degrade is **kept deliberately** (PR #902 review): the
/// tempting alternative — refuse if *either* comparison says same-place — is wrong, because for
/// `root/link/..` where `link` leaves the tree `canonicalize` is right to say "different place, allow",
/// and OR-ing the lexical answer would refuse a legitimate move. The degrade is also narrow: every
/// spelling that *needs* `canonicalize` requires the client to know the absolute root path, which is
/// CPE-1730's territory rather than this guard's.
///
/// # Each half is load-bearing on exactly one platform *class*
///
/// Measured, not reasoned (CPE-1726 on `cpe-webdav`, re-measured by CPE-1731 through `cpe-ftp`'s and
/// `cpe-sftp`'s own resolvers — see this function's tests and the ticket's probe output):
///
/// | probe | Windows | non-Windows |
/// |---|---|---|
/// | `canonicalize` removed | **red** (spelling rows) | pass |
/// | `..` popping removed | pass | **red** — but only one row, see below |
/// | both removed | **red** | **red** |
///
/// **"non-Windows", not "Linux", and the distinction is not pedantry — CI runs three OSes.** macOS is
/// the awkward one: HFS+/APFS are case-**insensitive** by default, so the intuition that "family 3 is
/// a Windows thing because only Windows folds case" is wrong there. What actually makes the column
/// hold is different and stronger: family 3 needs an **absolute** destination, and both rigs'
/// resolvers `trim_start_matches('/')` first, so on any POSIX host an absolute path becomes relative
/// and lands *inside* the root — measured `same_place = false`, independently reproduced by this
/// ticket's UAT. The spelling is therefore unreachable on macOS for a reason that has nothing to do
/// with case folding, which is why the column is safe to state over "non-Windows" rather than only
/// over the one POSIX platform that was probed.
///
/// The previous version of this table said "Windows | Linux" and was silently claiming a two-platform
/// world. Recorded because *this table is the artefact CPE-1731 already had to correct once* for being
/// broader than its evidence, and the fix for that must not introduce a narrower error in its place.
/// **Not measured:** no neutralisation run was performed on macOS at all — the column above is
/// evidenced on Linux and argued to macOS through the resolver, and that is the boundary.
///
/// Windows normalises `..` during path processing, so `root\nonexistent\..` opens as `root` and
/// `canonicalize` succeeds even though `nonexistent` does not exist — which is why the lexical pop
/// looks redundant there. Conversely `canonicalize` is what catches the case-insensitive and
/// trailing-dot spellings, which only exist on Windows.
///
/// **The Linux cell is narrower than CPE-1726's version of this table said, and CPE-1731 measured it
/// rather than copying it.** Of the three `..` rows, `canonicalize` succeeds on Linux for `/sub/..`
/// and `/./sub/../.` — `sub` exists, so the whole path resolves — and the pop is therefore *not* what
/// catches them there either. Only `/nonexistent/..` reaches the lexical fallback
/// (`canonicalize -> Err ENOENT`), so the pop is load-bearing on Linux for **exactly one row**:
///
/// ```text
/// LINUX, `..` pop REMOVED  (probe output, WSL)
///   /nonexistent/..  same_place = false -> rename Err(NotFound) -> refused BY AN ERRNO, not by the guard
///   /sub/..          same_place = true  -> guard fires
///   /./sub/../.      same_place = true  -> guard fires
/// ```
///
/// That "refused by an errno" is why the callers' tests assert the *specific* refusal — `cpe-ftp` pins
/// `553` (an `ENOENT` answers `550`) and `cpe-sftp` pins `SSH_FX_FAILURE` and explicitly rejects the
/// `NoSuchFile` wording. A bare "it returned an error" would have stayed green on Linux through a
/// neutralised pop, on the one row the pop exists for. Copying the old table's "**red** (`..` rows)"
/// without re-measuring would have hidden that.
///
/// And the Windows-wide short-circuit is **correct**, not merely tolerated, because `fs::rename` goes
/// through the same Win32 path processing — `MoveFileExW` performs the identical `..` stripping, so
/// `canonicalize`'s answer describes exactly the path the primitive will act on. Measured on both:
///
/// ```text
/// WINDOWS rename(src, "<root>\nonexistent/../landed.txt") = Ok(())   landed at root/landed.txt = true
/// LINUX   rename(src, "<root>/nonexistent/../landed.txt")  = Err     landed at root/landed.txt = false
/// ```
///
/// # What CPE-1731 checked before reusing this for FTP and SFTP
///
/// CPE-1726 declared those two rigs "structurally immune" to the defect by category, and the category
/// was wrong. So this reuse was measured rather than inherited: the ticket's probe ran both rigs' own
/// resolvers (`real_path`/`real`, which trim leading `/` and bare-`join` the remainder) over all three
/// families, on Windows and on Linux. Three differences from `cpe-webdav` came out of it, and all three
/// leave the property intact:
///
/// 1. The wire paths are `/`-separated even on Windows, so a resolved destination is mixed-separator
///    (`...\cpe-ftp-srv-0\sub/..`). `Path::components()` splits on both separators there, and
///    `canonicalize` accepts both, so every root-resolving row still compares equal.
/// 2. Both resolvers map an **empty** relative path to the root directly (and `cpe-sftp`'s maps `"."`
///    too), so `RNTO` with no argument at all arrives here as the root itself — the WebDAV
///    absent-`Destination` case, in a crate that was declared immune to it.
/// 3. The filesystem-equal spellings (family 3) need an **absolute** destination, and both resolvers
///    `trim_start_matches('/')` first — so on Linux an absolute path becomes relative and lands
///    *inside* the root (`same_place = false`, measured), whereas on Windows `C:\…` survives the trim
///    and `join` discards the base. That family is therefore Windows-only for these rigs, exactly as it
///    is for `cpe-webdav`, and for the same two reasons stacked (case-sensitive filesystem *and* an
///    unreachable spelling).
pub fn same_place(dest: &Path, root: &Path) -> bool {
    if let (Ok(a), Ok(b)) = (dest.canonicalize(), root.canonicalize()) {
        return a == b;
    }
    normalise_lexically(dest) == normalise_lexically(root)
}

// ─────────────────────────────────────────────────────────────────────────────────────────────────
// CPE-1730 — "is this destination INSIDE the served root?", decided in ONE place
// ─────────────────────────────────────────────────────────────────────────────────────────────────

/// How many resolution steps [`confined_to`] will take before giving up and refusing.
///
/// One step per trailing component that does not exist, plus one per symlink hop. A path with more
/// than this many components is not a path any of this repo's callers construct, and exhausting the
/// budget **refuses** — the same direction every other failure in this function takes.
const CONFINEMENT_STEP_BUDGET: usize = 4096;

/// Does `path` resolve to somewhere **inside** `root` (or to `root` itself)?
///
/// The property behind the protocol test rigs' path resolvers (`cpe-ftp`'s `real_path`, `cpe-sftp`'s
/// `FsSftp::real`, `cpe-webdav`'s `root.join(url)`), each of which used to hand a client-supplied path
/// to `fs::write`/`remove_file`/`remove_dir_all`/`fs::rename` with **no containment check at all**
/// (CPE-1730).
///
/// **It is also the AI Copilot's guard** ([`crate::copilot`]'s `apply_op`, CPE-1750) — and that caller,
/// unlike the rigs, is the shipped app running `fs::rename`/`fs::copy`/`create_dir_all`/`trash::delete`
/// on a real user's files. It arrived here by deleting a fourth, weaker copy of this walk: a
/// `parent_confined` that inspected only the path's *parent* chain and treated a dangling link's
/// `NotFound` as "this name does not exist yet", so it answered *confined* for `root/dangling`,
/// `root/dangling/x.txt` and `root/live` while this function answers *not confined* for all three. If a
/// fifth copy is ever proposed, that measurement is the reason not to write one: containment has one
/// answer in this crate, and it is here. **Extend this; do not fork it.**
///
/// # Containment is not equality, and the difference is the whole ticket
///
/// [`same_place`] answers *"does this destination resolve **to** the served root?"* — CPE-1726/CPE-1731's
/// question, and a guard against a rename that reports success while moving nothing. This answers
/// *"does it stay **within** the served root?"*, which is a strictly harder question and a different
/// one:
///
/// - `same_place` may compare **lexically** when `canonicalize` fails, because for *equality* the
///   lexical `..` pop is proved to err safe — popping only shortens the path, hence makes it *more*
///   likely to equal the root, hence more likely to refuse (that proof is on `normalise_lexically`,
///   and it is sound).
/// - **That proof does not transfer.** In the containment direction the same pop errs *unsafe*:
///   `root/link/..` lexically pops to `root` (contained, allow) while the filesystem resolves it to
///   `link`'s parent, which may be anywhere. A guard that answers containment by popping `..` would
///   allow the escape it exists to refuse. So this function **never pops `..`** — it asks the
///   filesystem, and refuses when the filesystem cannot answer.
///
/// Do not "simplify" this into `same_place`'s shape on the grounds that `same_place` is the newer,
/// more-reviewed function. That reasoning — carry the latest shape forward because it is the latest —
/// is what CPE-1731 recorded as its own closing lesson, having been wrong twice before it was right.
///
/// # Why [`contained_under`] could not be reused, though the ticket expected it to
///
/// The ticket that filed this said `contained_under` "already returns the right shape". It does not,
/// **for these callers**, and the reason is written on `contained_under` itself: its documented
/// precondition is that `joined` is an *existing* target about to be removed, and it therefore returns
/// `Ok` when `joined` does not canonicalize. Every call site here is the opposite case — a `STOR`
/// target, a `MKD` name, a rename destination — where not existing yet is the **ordinary** state. Reused
/// as-is it would have answered "contained" for `root/../evil.txt` (which does not exist, because
/// nothing has written it yet) and then the rig would have written it. A guard that fails open on
/// precisely its subject is worse than no guard, because it reads as one. Its own doc says so in as many
/// words: *"Do not reuse this to validate a create/copy destination."*
///
/// # What it does
///
/// Walk `path` up to its **deepest existing ancestor**, canonicalise *that*, and require the result to
/// be inside the canonicalised `root`:
///
/// 1. `canonicalize(path)` — if the whole path exists, the filesystem has resolved every symlink in it
///    and the answer is exact. This is what closes the **symlinked-intermediate-directory** escape
///    (`root/link/x` where `link` points outside), which needs neither `..` nor an absolute path and is
///    invisible to any purely textual check.
/// 2. On `NotFound`, drop the last component and try again. The components dropped this way provably
///    **do not exist**, so none of them can be a symlink, so nothing about them can redirect the path
///    elsewhere — which is why appending them back to a contained ancestor is sound without further
///    checks.
/// 3. A `..` that survives into that non-existent tail is **refused, not popped** (it arrives here as
///    `file_name() == None`). Refusing is safe on both platform classes and for different reasons:
///    on POSIX such a path has no resolution at all, so the caller's primitive would `ENOENT`; on
///    Windows `canonicalize` normalises `..` *before* step 2 is ever reached (measured below), so the
///    verdict is decided by the filesystem and this branch is not reached for the same input. See the
///    measurement table.
/// 4. A **dangling symlink** is followed by hand (`read_link`, relative targets resolved against the
///    link's parent) and the walk continues on the target. `canonicalize` reports `NotFound` for a
///    dangling link, so without this it would look like a plain missing name and be allowed — while
///    `fs::write` through that link creates its target, wherever that is. Following it rather than
///    refusing it outright is deliberate: `cpe-webdav`'s dangling-link leg
///    (`cpe_1726_rename_onto_a_link_never_writes_through_it`) points its link at a sibling *inside* the
///    root, and a blanket refusal would have quietly turned that leg into a no-op instead of leaving it
///    proving what it was written to prove.
///
/// # Failure policy — every failure REFUSES
///
/// Unlike [`contained_under`], which fails open on an unresolvable target because a path that does not
/// exist cannot be *destroyed*, every unresolved case here fails **closed**: an unresolvable `root`, any
/// `canonicalize` error other than `NotFound` (`EACCES`, `ELOOP`, `ENAMETOOLONG`, a Windows sharing
/// violation), a path that exists but will not canonicalise and is not a symlink, a `..` in the
/// non-existent tail, and exhaustion of [`CONFINEMENT_STEP_BUDGET`]. The asymmetry is the precondition:
/// this validates a path about to be **created or written**, so "I could not tell" must not mean "go
/// ahead".
///
/// # Measured, not reasoned — Windows and Linux
///
/// ```text
/// probe                                      WINDOWS                      LINUX (WSL)
/// canonicalize(root/nonexistent/..)          Ok(root)                     Err(NotFound)
/// canonicalize(<dangling link>)              Err(NotFound)                Err(NotFound)
/// Path::new("a/..").file_name()              None                         None
/// Path::new("a/.").file_name()               Some("a")                    Some("a")
/// ```
///
/// The first row is why step 3's Windows leg is unreachable rather than merely safe, and why a
/// `..`-in-the-tail input can get *different verdicts* on the two platforms (Windows: "it is the root,
/// contained"; POSIX: "unresolvable, refused"). Both verdicts are safe; they are not the same verdict,
/// and a caller that needs one answer everywhere must not get it from here.
///
/// # What this does NOT cover — stated so the absence is recorded, not overlooked
///
/// - **It is not atomic with the primitive.** Between this check and the caller's `fs::rename`, a
///   component could be replaced by a symlink pointing out of the tree (a TOCTOU swap). Closing that
///   needs `openat2(RESOLVE_BENEATH)` on Linux or an `O_NOFOLLOW` walk, neither of which `std` offers,
///   which is why this is recorded rather than solved. **A real server must not treat this as
///   sufficient.** CPE-1730's own callers are single-threaded in-process test rigs where nothing else
///   touches the tree; CPE-1750's is not — [`crate::copilot`] runs this against a user's live
///   filesystem, and repeats the residual on its own `apply_op` so a reader of that path meets it there,
///   and again on its recursive-copy walk, which asks this once per link it meets (CPE-1756).
/// - **It says nothing about what the primitive then does to a link at the final component.** A
///   contained path may still *be* a symlink whose target is contained but is written *through* rather
///   than replaced — CPE-1719's shape, pinned separately by CPE-1726's tests.
/// - It answers only about `root`; the caller still decides whether the root **itself** is an
///   acceptable answer. It is allowed here, because a resolver must map `/` to the served root for
///   `LIST`/`PROPFIND`/`CWD` to work at all. "Not the root itself" is [`same_place`]'s question, and
///   the rename sites ask both.
pub fn confined_to(path: &Path, root: &Path) -> bool {
    let Ok(real_root) = std::fs::canonicalize(root) else {
        return false; // an unresolvable root confines nothing
    };
    let mut probe = path.to_path_buf();
    for _ in 0..CONFINEMENT_STEP_BUDGET {
        match std::fs::canonicalize(&probe) {
            // `starts_with` is component-wise, so `<root>2` does not start with `<root>`, and it is
            // true for `real_root` itself — the root is contained in itself, by design (see above).
            Ok(real) => return real.starts_with(&real_root),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return false, // EACCES / ELOOP / … — cannot tell, so refuse
        }
        // `NotFound` is two different situations. A dangling symlink is one of them, and it is the
        // dangerous one: the name exists, the caller's write will follow it, and its target is
        // somewhere this function has not looked yet.
        match std::fs::symlink_metadata(&probe) {
            Ok(md) if md.file_type().is_symlink() => {
                let Ok(target) = std::fs::read_link(&probe) else { return false };
                probe = if target.is_absolute() {
                    target
                } else {
                    match probe.parent() {
                        Some(parent) => parent.join(target),
                        None => return false,
                    }
                };
            }
            // It exists, it is not a symlink, and it still would not canonicalise. Nothing sensible
            // left to conclude, so refuse.
            Ok(_) => return false,
            // It genuinely is not there: drop the last component and ask about its parent. `file_name`
            // is `None` for a path ending in `..` (and for a bare root/prefix), which is step 3.
            Err(_) => {
                let (Some(parent), Some(_name)) = (probe.parent(), probe.file_name()) else {
                    return false;
                };
                probe = parent.to_path_buf();
            }
        }
    }
    false // budget exhausted — refuse, like every other thing this function cannot resolve
}

// ─────────────────────────────────────────────────────────────────────────────────────────────────
// CPE-1717 — what a FAILED staging attempt means, decided in one place
// ─────────────────────────────────────────────────────────────────────────────────────────────────

/// What to do about a leg that could not stage the condition it exists to test.
///
/// A pure classifier — no environment, no filesystem — so the policy itself is unit-testable without
/// the env-var races that make "set a variable and run a test" unreliable under a parallel harness.
/// [`require_staged`] is the thin wrapper that reads the environment and applies this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StagingVerdict {
    /// The condition is staged; the leg runs for real.
    Staged,
    /// Staging failed on a platform where the mechanism *cannot* work (an ACL deny on Linux, the
    /// `fs::metadata` traversal deny on Windows). Legitimate: announce it and move on.
    LegitimateSkip,
    /// Staging failed on a platform where the mechanism is *supposed* to work, under a harness that
    /// has asked for strictness. The runner changed under us — go red.
    Fail,
}

/// The CPE-1717 policy, as a pure function of the four facts that decide it.
///
/// `supported_here` is the caller's own claim about the platform it is running on: "this mechanism is
/// supposed to work here". It is a **compile-time** property of the mechanism (`cfg!(unix)` for the
/// traversal deny, `true` for the target deny and for link creation), never a runtime observation of
/// whether it happened to work — deriving it from the outcome would make the check vacuous.
pub fn staging_verdict(
    supported_here: bool,
    staged: bool,
    strict: bool,
    sabotaged: bool,
) -> StagingVerdict {
    // Sabotage simulates a runner that lost the capability, so the enforcement below can be broken on
    // demand and shown to go red (Evidence Rules, `Ticketing/wiki.md` → "Guard neutralisation").
    if staged && !sabotaged {
        return StagingVerdict::Staged;
    }
    if supported_here && strict {
        StagingVerdict::Fail
    } else {
        StagingVerdict::LegitimateSkip
    }
}

/// Whether a staging failure on a supporting platform is a hard failure rather than a loud skip.
///
/// **Strict under CI, lenient locally, and that asymmetry is the whole point of CPE-1717.** The bug
/// this fixes is not that the skip notices are unprintable — measured, they are not; see
/// [`require_staged`] — it is that a *passing* leg with a notice in a 2,100-test log is a green board
/// over zero coverage, and nobody reads a green log. CI is exactly where the board lies, so CI is
/// where a leg that verified nothing must be red. A developer, meanwhile, may legitimately be in an
/// environment the mechanism cannot work in (a root Docker shell, a network share, an ACL-less
/// filesystem, a Windows account with neither Developer Mode nor the junction fallback) and must not
/// be blocked by it; there the loud skip is still the right answer.
///
/// `CPE_STAGING_STRICT=1` forces strictness on (use it locally to check a leg really stages),
/// `CPE_STAGING_STRICT=0` forces it off (an escape hatch if a runner image genuinely regresses and
/// the fix has to wait); otherwise it follows `CI`, which GitHub Actions sets to `true` on every
/// runner.
pub fn staging_is_strict() -> bool {
    strict_from(
        std::env::var("CPE_STAGING_STRICT").ok().as_deref(),
        ci_from(std::env::var("CI").ok().as_deref()),
    )
}

/// Is this a CI harness? A pure function of `$CI`, because the first version asked
/// `var_os("CI").is_some()` — under which `CI=false`, `CI=0` and `CI=""` all counted as CI, flatly
/// contradicting the doc comment that said it "follows `CI`".
///
/// **Unlike [`strict_from`], an unrecognised value here is tolerated rather than refused, and the
/// asymmetry is deliberate.** `CPE_STAGING_STRICT` is *our* knob: a value we do not understand is a
/// typo by someone reaching for a documented escape hatch, and silently ignoring it hands them a green
/// run they believe they forced. `CI` is *not ours* — dozens of tools set it to whatever they like —
/// so anything present and not falsy means CI, which is the convention every other tool uses.
pub fn ci_from(var: Option<&str>) -> bool {
    match var {
        None => false,
        Some(raw) => !matches!(raw.trim().to_ascii_lowercase().as_str(), "" | "0" | "false" | "no" | "off"),
    }
}

/// The decision [`staging_is_strict`] makes, as a **pure** function of its two inputs.
///
/// # Why this is split out, which is a bug this PR shipped and its reviewer caught
///
/// The first version left the `match` inline in [`staging_is_strict`] and "tested" it against a
/// hand-written closure in the test module that duplicated the same `match`. That is not a test: the
/// reviewer inverted the real function completely — `=0` meaning strict, `=1` meaning lenient, `CI`
/// negated — and every CPE-1717 test still passed. The subtle break was worse. Changing only the
/// `None => ci` arm to `None => false`:
///
/// - an ordinary CI run (`CI=true`, no override, staging broken) went **green over zero coverage** —
///   precisely the bug this whole ticket exists to fix; and
/// - the CI guard step **still reported OK**, because it pinned `CPE_STAGING_STRICT=1` and so routed
///   around the very arm that had broken.
///
/// So the one line connecting this feature to real CI could stop working with nothing in the repo
/// noticing, including the guard built to notice. Two changes followed: this pure function, table-
/// tested over every input; and a CI assertion that runs the sabotaged leg with **no override set**,
/// so the guard exercises the `None => ci` arm rather than stepping past it.
///
/// # Values
///
/// Accepts the obvious spellings, case-insensitively and after trimming: `1`/`true`/`yes`/`on` enable
/// strictness, `0`/`false`/`no`/`off` disable it, and an empty or unset value follows `ci`. **An
/// unrecognised value panics** rather than falling through to the default — an escape hatch that
/// silently does nothing is worse than no escape hatch, because the person using it believes it
/// worked. (The first version matched only the digits `1` and `0`, so `CPE_STAGING_STRICT=true`
/// quietly did nothing on a developer's machine and `=false` quietly did nothing on CI.)
pub fn strict_from(var: Option<&str>, ci: bool) -> bool {
    let Some(raw) = var else { return ci };
    match strict_token(raw) {
        StrictToken::Unset => ci,
        StrictToken::On => true,
        StrictToken::Off => false,
        StrictToken::Unrecognised => panic!(
            "[CPE-1717] CPE_STAGING_STRICT={raw:?} is not a value this understands, and silently \
             ignoring it would leave you believing an escape hatch worked when it did nothing. Use \
             one of 1/true/yes/on or 0/false/no/off, or unset it to follow CI."
        ),
    }
}

/// How one raw `CPE_STAGING_STRICT` value classifies. Split out so the whole vocabulary is testable
/// without a `catch_unwind` per spelling — five panics in a test log is noise that trains people to
/// scroll past panics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrictToken {
    /// Empty — treat as though the variable were not set at all.
    Unset,
    On,
    Off,
    /// Not in the vocabulary. [`strict_from`] refuses this rather than guessing.
    Unrecognised,
}

/// Classify one raw value, trimmed and case-folded.
pub fn strict_token(raw: &str) -> StrictToken {
    match raw.trim().to_ascii_lowercase().as_str() {
        "" => StrictToken::Unset,
        "1" | "true" | "yes" | "on" => StrictToken::On,
        "0" | "false" | "no" | "off" => StrictToken::Off,
        _ => StrictToken::Unrecognised,
    }
}

/// Test-only sabotage hook: with `CPE_STAGING_SABOTAGE=1`, every [`require_staged`] call reports that
/// it could not stage, exactly as a runner that lost symlink privilege or stopped honouring deny ACEs
/// would. CI uses it to prove — rather than assert — that such a runner turns a step red instead of
/// green (`.github/workflows/ci.yml`, the "skip-visibility guard" steps).
pub fn staging_is_sabotaged() -> bool {
    std::env::var("CPE_STAGING_SABOTAGE").is_ok_and(|v| v == "1")
}

/// The one gate every staging attempt passes through. Returns whether the leg may proceed; the caller
/// prints its own leg-specific notice and returns early on `false`.
///
/// # Why this exists (CPE-1717), and the measurement that reshaped it
///
/// The family CPE-1678 → CPE-1692 → CPE-1696 → CPE-1705 → CPE-1710 built a pattern: a leg that cannot
/// stage its condition announces itself loudly instead of passing silently. CPE-1717 was filed on the
/// belief that the announcement never reaches CI, because libtest captures output for passing tests
/// and `.github/workflows/ci.yml` runs plain `cargo test` with no `--nocapture`.
///
/// **Half of that is true, and the half that matters is not.** Measured directly with a one-test
/// harness (`rustc --test`, run with no flags, exactly as `cargo test` runs it):
///
/// ```text
/// running 1 test
/// VIA-WRITELN-STDERR: this is the CPE-1705/1710 shape
/// VIA-WRITELN-STDOUT: control
/// test passing_test_that_announces_a_skip ... ok
/// ```
///
/// `eprintln!`/`println!` were swallowed; `writeln!(std::io::stderr(), ..)` and its stdout twin were
/// not. libtest's capture works by swapping a thread-local inside the `print!`/`eprint!` macros, so a
/// direct write to the process's stderr handle goes around it. That is why the CPE-1678 comment in
/// `dispatch.rs` calls the choice of emitter load-bearing, and it holds: **every notice in this family
/// already reaches the CI log.** `--nocapture` would add nothing to them.
///
/// So the real defect is not visibility, it is consequence. A skip that prints into a passing run of a
/// 2,100-test suite is a green board over an uncovered platform, and no one reads a green log. On a
/// platform where the mechanism is *supposed* to work, that must be red. See [`staging_is_strict`] for
/// why the strictness is scoped to CI.
///
/// `#[track_caller]` is load-bearing: the panic reports the **call site's** file and line, so a red
/// build names the exact leg that stopped staging rather than pointing back into this helper.
#[track_caller]
pub fn require_staged(mechanism: &str, supported_here: bool, staged: bool) -> bool {
    match staging_verdict(supported_here, staged, staging_is_strict(), staging_is_sabotaged()) {
        StagingVerdict::Staged => true,
        StagingVerdict::LegitimateSkip => false,
        StagingVerdict::Fail => panic!("{}", staging_failure_message(mechanism)),
    }
}

/// The message [`require_staged`] panics with, as a value rather than a `panic!` literal, so the CI
/// guard step's `grep 'CPE-1717'` has something a unit test can assert on without catching an unwind.
pub fn staging_failure_message(mechanism: &str) -> String {
    format!(
        "[CPE-1717] `{mechanism}` could not stage its condition on {os}, a platform where this \
         mechanism IS supposed to work. The leg that called this therefore verified NOTHING, and \
         under CI a leg that verified nothing must be red rather than a notice inside a green log.\n\
         \n\
         Likely causes: the runner image changed (symlink privilege, Developer Mode, the junction \
         fallback, `icacls` behaviour); the job is running elevated or as root, where a deny cannot \
         bind; the workspace moved to a filesystem that ignores ACLs and mode bits; or \
         `CPE_STAGING_SABOTAGE=1` is set, which is how CI proves this check still bites.\n\
         \n\
         Re-run with `CPE_STAGING_STRICT=0` to fall back to the loud-skip behaviour and read the \
         leg's own notice — but treat that as a diagnosis step, not a fix.",
        os = std::env::consts::OS
    )
}

/// Owns a per-test scratch directory under `%TEMP%` and removes it (recursively, best-effort) when
/// dropped — including when a test panics mid-assertion. **CPE-1693.**
///
/// Before this, every `scratch()`-style test helper across the tree (and there were ~70 near-identical
/// copies, one per test module) returned a bare [`PathBuf`] and relied on a manual `remove_dir_all` at
/// the end of the test — which never runs on a panicking assertion, and, per the CPE-1693 PR #924
/// review, is not reliable even on a green run (one of five orphaned trees the review measured leaked
/// on a passing test). The count reached **~1.29 million** leftover `cpe-*` directories in `%TEMP%`
/// before this landed, and started causing a real, non-deterministic test failure: enough leaked
/// `%TEMP%/cpe-archive/<pid>-<seq>` directories that a reused PID collided with its own scratch name.
///
/// Fixing this at the *helper* level rather than test-by-test closes the whole class at once: a new
/// test that calls `scratch()` cannot reintroduce the leak even if its author never thinks about
/// cleanup, because the directory's owner is the return value itself, not a `remove_dir_all` the author
/// has to remember to write (and to write *before* the assertions, not after).
///
/// Derefs to [`Path`], and implements [`AsRef<Path>`], so call sites that only ever read the path
/// (`d.join(..)`, `&d`, `d.exists()`, `d.display()`, …) keep compiling unchanged. Two things do *not*
/// carry over automatically:
/// - `d.clone()` — deliberately not [`Clone`]: cloning would let the directory outlive the guard that
///   is supposed to own it. Use `d.to_path_buf()` for an owned copy of the *path* (the clone doesn't
///   extend the directory's lifetime).
/// - Passing `scratch(..)` inline as a temporary that is never bound to a `let` — the guard would drop,
///   and delete the directory, at the end of that statement. Bind it (`let d = scratch(..);`) for as
///   long as the directory needs to exist.
pub struct ScratchDir(PathBuf);

impl ScratchDir {
    /// The path this guard owns.
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Wrap an **already-created** directory so it is removed on drop, without the `<prefix>-<pid>-<seq>`
    /// naming [`scratch_dir`] imposes. For callers with their own directory-naming scheme that
    /// `create_dir_all`s the path themselves — e.g. a nested per-spawn subdirectory under one shared
    /// parent (`cpe-s3`'s and `cpe-webdav`'s fixture spawners, CPE-1693) — and only need the *cleanup*
    /// half of what [`scratch_dir`] does. `path` must already exist; this does not create it.
    pub fn adopt(path: PathBuf) -> Self {
        ScratchDir(path)
    }
}

impl std::ops::Deref for ScratchDir {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for ScratchDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

/// Alongside [`AsRef<Path>`](ScratchDir#impl-AsRef<Path>-for-ScratchDir), so `&ScratchDir` satisfies the
/// same `impl Into<PathBuf>`-style bounds a plain `&Path`/`&PathBuf` does (std's blanket `From<&T> for
/// PathBuf where T: AsRef<OsStr>`) without every call site needing its own `.to_path_buf()`. Borrowing
/// only — it cannot consume the guard, so it doesn't touch the early-drop hazard documented on
/// [`ScratchDir`] itself (that hazard is specifically about an *owned* `ScratchDir` being dropped as an
/// unbound temporary; nothing here changes when that drop happens).
impl AsRef<std::ffi::OsStr> for ScratchDir {
    fn as_ref(&self) -> &std::ffi::OsStr {
        self.0.as_ref()
    }
}

impl std::fmt::Debug for ScratchDir {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.0, f)
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        remove_dir_all_with_retries(&self.0);
    }
}

/// `remove_dir_all`, retried a few times with a short backoff before giving up silently.
///
/// **Measured for CPE-1693, not theoretical:** a full-workspace `cargo test` run left real, freshly
/// created `cpe-*` directories behind — hundreds concentrated in the binary-fixture truncation sweeps
/// (`binary_preview`/`dotnet_metadata`), plus a handful scattered across ordinary single-scratch tests —
/// even though every one of those same tests, run in isolation or a small group, cleaned up perfectly.
/// That isolation-vs-full-suite gap, concentrated on the fixtures that most resemble real executables
/// (PE/ELF/Mach-O), is the signature of Windows Defender's real-time scanner transiently holding a
/// handle on a just-written file under heavy parallel `cargo test` load — exactly the interference this
/// repo's own `MEMORY.md` already documents ("Defender quarantines test binaries... os error 225 is
/// Defender, not a code fail"). A single `remove_dir_all` attempt swallows that as a silent failure (the
/// pre-CPE-1693 trailing `let _ = fs::remove_dir_all(..)` calls had the identical exposure — this isn't a
/// regression, it's the first time anything retries). A short bounded retry is the standard mitigation
/// for a transient Windows sharing violation and costs nothing when there's no contention (the common
/// case exits on the first attempt).
fn remove_dir_all_with_retries(path: &Path) {
    const ATTEMPTS: u32 = 5;
    for attempt in 0..ATTEMPTS {
        match std::fs::remove_dir_all(path) {
            Ok(()) => return,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(_) if attempt + 1 < ATTEMPTS => {
                std::thread::sleep(std::time::Duration::from_millis(25 * (attempt as u64 + 1)));
            }
            Err(_) => {} // Out of attempts — give up silently, same as every pre-CPE-1693 cleanup call.
        }
    }
}

/// Create a uniquely-named directory under `%TEMP%` and hand back the [`ScratchDir`] guard that owns
/// it (CPE-1693). `prefix` should already carry the caller's own module tag (e.g.
/// `"cpe-fsutil-contained"`) — this appends only the per-process, per-call disambiguator (`-<pid>-<seq>`)
/// every pre-CPE-1693 `scratch()` helper already appended, so directory names are unchanged and any
/// tooling that greps for a specific `cpe-<module>-` prefix keeps working.
///
/// **Not** `#[cfg(test)]`-gated, for the same reason [`make_dangling_link`] isn't: `cpe-server`'s
/// dependents (`src-tauri`, `cpe-net`, `cpe-webdav`, `cpe-s3`, …) need it from their *own* test builds,
/// and `#[cfg(test)]` is per-crate — an item gated on it in this crate is invisible when a downstream
/// crate compiles its own tests. One implementation, reachable everywhere a `scratch()`-style helper
/// used to hand-roll its own, is the point of this ticket.
pub fn scratch_dir(prefix: &str) -> ScratchDir {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("{prefix}-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    ScratchDir(d)
}

/// Test-only mechanism for the `fs::metadata`-based CPE-1692 guards in this crate (`links::link_status`,
/// `dangling_links_scan`'s target-resolution check): deny traversal on `dir` itself so a `stat` of
/// anything reached *through* it fails with a genuine (non-`NotFound`) error.
///
/// **This mechanism is genuinely Unix-only** — on Unix `stat()` needs no permission on the target
/// itself, only `+x` on each ancestor directory, so denying an intermediate directory blocks resolution.
/// On Windows it does **not** work, and the PR #874 review measured why in detail, correcting an
/// over-generalisation this comment used to make: `fs::metadata` opens via `CreateFileW` with a
/// desired-access mask of `0`, and Windows separately grants "Bypass traverse checking"
/// (`SeChangeNotifyPrivilege`) to Everyone by default, so neither a deny ACE on the target nor on an
/// intermediate directory blocks a `CreateFileW`-based open — confirmed live: denying `RX` on a parent
/// directory left a child's `fs::metadata` call `Ok`. Callers of THIS helper (the `fs::metadata`-based
/// sites) therefore legitimately skip on Windows and get their real coverage from CI's Unix legs.
///
/// **Sites that call [`Path::try_exists`] instead must NOT use this helper** — use [`deny_stat_of`],
/// which the same review proved *does* work on Windows, because `try_exists` is a different underlying
/// syscall (an attributes query) that a deny ACE on the target DOES refuse, even though `fs::metadata`
/// (a `CreateFileW` open) on the identical denied target still succeeds. Repointing a `try_exists`-based
/// site at this helper instead of `deny_stat_of` is worse than not testing it at all: the deny is real
/// (so the test doesn't announce a skip) but doesn't touch the call under test, so a broken guard passes
/// *silently* — the review caught exactly this on `links::link_status_does_not_report_broken_...`, which
/// passed vacuously when pointed at the wrong probe.
///
/// `probe` must be a path that requires traversing `dir` to resolve (a child of `dir` for a
/// file-under-directory check, or a child of a child for a directory-itself check — see each call
/// site). Returns whether the deny demonstrably took effect, checked by actually stat'ing `probe` and
/// requiring a non-`NotFound` error: `false` means this machine can't construct the condition (running
/// elevated, an ACL-less filesystem, non-admin without the rights to set a deny ACE, … — or simply
/// Windows), which callers MUST treat as a loud, `writeln!(std::io::stderr(), ..)` skip — never a silent
/// pass (Evidence Rules, `Ticketing/wiki.md`).
///
/// **CPE-1717:** the `false` return is routed through [`require_staged`] with `supported_here =
/// cfg!(unix)`, because this mechanism provably cannot work on Windows (see above) — so a Windows skip
/// stays a legitimate, notice-only skip on every harness, while a **Unix** runner that stops honouring
/// mode bits turns the step red under CI instead of quietly covering nothing.
#[cfg(test)]
#[track_caller]
pub(crate) fn deny_dir_traversal(dir: &Path, probe: &Path) -> bool {
    #[cfg(windows)]
    {
        if let Ok(user) = std::env::var("USERNAME") {
            if !user.is_empty() {
                let _ = std::process::Command::new("icacls")
                    .arg(dir)
                    .arg("/deny")
                    .arg(format!("{user}:(RX)"))
                    .output();
            }
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o000));
    }
    require_staged(
        "deny_dir_traversal",
        cfg!(unix),
        std::fs::metadata(probe).is_err_and(|e| e.kind() != std::io::ErrorKind::NotFound),
    )
}

/// Undo whatever [`deny_dir_traversal`] did to `dir`, so a scratch tree containing it can be removed.
/// Safe to call even when the deny never took effect.
#[cfg(test)]
pub(crate) fn undo_deny_dir_traversal(dir: &Path) {
    #[cfg(windows)]
    {
        if let Ok(user) = std::env::var("USERNAME") {
            let _ = std::process::Command::new("icacls").arg(dir).arg("/remove:d").arg(&user).output();
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
}

/// Test-only mechanism for the [`Path::try_exists`]-based CPE-1692 guards in this crate and its sibling
/// crates (`disk_usage::dir_size`, `native_meta`'s Windows ADS ops, `move_exact_impl`, `crates/sftp`'s
/// `open`): make `target`'s own `try_exists()` fail with a genuine (non-`NotFound`) error while
/// `fs::metadata(target)` still succeeds — the split the PR #874 review measured and named.
///
/// **Platform-asymmetric on purpose**, because the two OSes need different targets for the SAME
/// observable effect on `target.try_exists()`:
/// - **Windows**: deny **read directly on `target` itself** (`icacls target /deny user:(R)`).
///   Measured live (three cases, non-elevated, local NTFS): denying `RX` on `target`'s PARENT (the
///   mechanism [`deny_dir_traversal`] uses) leaves `try_exists()` on a child `Ok(true)` — no effect.
///   Denying `R` directly on the target itself makes `fs::metadata(target)` stay `Ok` (unaffected,
///   confirming `deny_dir_traversal`'s doc comment) while `target.try_exists()` becomes
///   `Err(PermissionDenied)` — `try_exists` resolves to `std::fs::exists`, an attributes-query syscall
///   distinct from `fs::metadata`'s `CreateFileW` open, and an attributes query IS refused by a deny ACE
///   on the queried path itself even though an open with desired-access `0` is not.
/// - **Unix**: deny is still on the PARENT (`chmod 0o000`), matching [`deny_dir_traversal`] — `+x` on the
///   parent is still what POSIX `stat()`/`access()`-family calls need, and `try_exists()` is no different
///   from `metadata()` there. `target` itself keeps its own permission bits untouched (irrelevant on
///   Unix per CPE-1687's own finding: `stat()` needs no permission on the file itself).
///
/// **What this helper can and cannot prove — measured for CPE-1696, corrected and extended in the PR
/// #889 review, then **corrected again by the PR #893 UAT** (non-elevated, local NTFS).** The minimal
/// Windows deny that makes `try_exists()` fail is `S` (SYNCHRONIZE); `(R)`, `(RX)`, `(W)` and `(F)` all
/// work too, while `(RA)`, `(REA)`, `(RD)` and `(RC)` do **not** (they leave `try_exists()` at `Ok(true)`).
///
/// # The deny on the target is only half of it — you must also deny `(RD)` on the PARENT
///
/// **Mechanism, and the thing four separate measurements missed.** On Windows `std::fs::metadata` does not
/// give up when its `CreateFileW` open returns `ACCESS_DENIED`. It **falls back to `FindFirstFileW`**,
/// which reads the entry out of the **parent directory** instead of opening the file. That fallback is the
/// entire reason a deny on the target alone leaves `Path::exists()` answering `true`. Deny
/// **list-directory (`RD`) on the parent** and the fallback dies.
///
/// Crucially **`RD` is not `DC`**: the rename's `FILE_DELETE_CHILD` route on the parent is untouched, so
/// the rename still replaces the target. That is the combination that makes byte loss stageable — the stat
/// fails *and* the destructive operation succeeds.
///
/// | deny | `exists()` | `metadata()` | `try_exists()` | `fs::write`/`copy` | `fs::rename` onto it | unfixed `.exists()` guard clobbers? |
/// |---|---|---|---|---|---|---|
/// | `(R)` on target only | true | Ok | Err | Err | Ok | **no** — the case four rounds kept measuring |
/// | `(F)` on target only | true | Ok | Err | Err | Err | no |
/// | `(S)` on target only | true | Ok | Err | Err | Ok | no |
/// | **`(R)` target + `(RD)` parent** | **false** | **Err** | Err | Err | **Ok** | **YES — bytes destroyed** |
/// | **`(S)` target + `(RD)` parent** | **false** | Err | Err | Err | Ok | **YES** |
/// | `(RD)` parent only | true | Ok | Ok(true) | Ok | Ok | no |
/// | any target deny + **`(DC)`** on the parent | — | — | Err | Err | **Err** | no — `(DC)` cuts BOTH delete routes |
///
/// `fs::write`/`fs::copy` request SYNCHRONIZE in their own `CreateFileW` access mask, so every deny that
/// refuses `try_exists` also refuses them. **`fs::rename` is the exception, and it is the important one:**
/// replacing an existing file needs `DELETE` on the target *or* `FILE_DELETE_CHILD` on its parent, and a
/// normal scratch parent grants the latter — so the rename destroys the bytes straight through the deny
/// (measured: `"ORIGINAL"` → `"NEWDATA"`). `(F)` is the one target spec that denies the target's own
/// `DELETE`, which is why this helper uses `(R)`: `(R)` destroyed the bytes in every parent directory
/// tested, `(F)` is parent-dependent. **Never additionally deny `(DC)` on the parent** — that cuts both
/// delete routes, the rename fails for the wrong reason, and the assertion passes vacuously.
///
/// # Do not write "byte loss is not stageable via ACLs" into this comment again
///
/// It has now been the wrong conclusion **four times**, each time from a correct measurement of an
/// incomplete setup — most recently by CPE-1705's own author, who reverted `unique_target`'s probe to
/// `!candidate.exists()`, watched the test pass green, and concluded (wrongly) that the byte-loss
/// construction could not catch a `.exists()` bug. It could; the setup was denying the target only. If a
/// future round cannot stage it, **scope the null result to the exact denies applied — target *and*
/// parent — rather than stating it as a property of ACLs.**
///
/// Consequences for testing a guard in this class:
/// - At a **`write`/`copy`-destructive** site, this helper proves the guard *refuses* but cannot stage
///   byte loss — the ACL that hides the file also protects it. Assert on the guard's own message: a bare
///   `expect_err` passes vacuously, because neutralised code still errors, just with "Access is denied.
///   (os error 5)" from the write.
/// - At a **`rename`-destructive** site (`unique_target` → `do_move_into`, `move_exact`, `rename_entry`,
///   and the rest of CPE-1705's sites), a **byte-level assertion is available and is strictly stronger**,
///   and it catches the original `.exists()` bug — see
///   `cpe_1696_a_move_never_renames_over_a_target_it_cannot_stat` and
///   `cpe_1705_rename_entry_never_renames_over_a_target_it_cannot_stat` in `src-tauri`. Prefer it.
/// - `symlink_metadata` also fails under the parent-`RD` construction, so a symlink-slot `Unknown` arm is
///   reachable too — another thing an earlier round declared untestable.
/// - The **pure classifiers remain load-bearing** alongside this: they inject any `io::ErrorKind` and run
///   on all three CI legs, covering the non-permission tail (`EIO`, dead mount, stale handle) that no ACL
///   reaches. Both kinds of evidence, not one instead of the other.
///
/// Either way the unguarded overwrite is also real for stat failures the ACL model cannot stage at all — a
/// dead network mount, `EIO`, a transient resolve failure — which is why every CPE-1696 site additionally
/// has a pure classifier whose unit test can inject any `io::ErrorKind`. Unix constrains this differently:
/// its mechanism denies the *parent* (`chmod 0o000`), which refuses the rename along with everything else,
/// so the byte-loss construction above is Windows-only.
///
/// `target` is the exact path the code under test calls `.try_exists()` on. Returns whether the deny
/// demonstrably took effect, checked by calling `target.try_exists()` and requiring `Err`: `false` means
/// this machine can't construct the condition (running elevated, an ACL-less filesystem, non-admin
/// without the rights to set a deny ACE, …), which callers MUST treat as a loud,
/// `writeln!(std::io::stderr(), ..)` skip — never a silent pass (Evidence Rules, `Ticketing/wiki.md`).
///
/// **CPE-1717:** this mechanism is supposed to work on **every** platform CI runs — the target deny on
/// Windows, the parent `chmod` on Unix — so the `false` return is routed through [`require_staged`]
/// with `supported_here = true`. Under CI a failure to stage is therefore red, not a notice in a green
/// log; locally it stays a loud skip, for the root-container / network-share / ACL-less cases.
#[cfg(test)]
#[track_caller]
pub(crate) fn deny_stat_of(target: &Path) -> bool {
    #[cfg(windows)]
    {
        if let Ok(user) = std::env::var("USERNAME") {
            if !user.is_empty() {
                // `(R)`, deliberately, NOT `(F)` — see the table above. Both refuse `try_exists`, but
                // only `(F)` denies the target's own `DELETE`, which lets a parent directory that
                // withholds `FILE_DELETE_CHILD` block a `fs::rename` that this helper's callers need to
                // go THROUGH in order to observe the byte loss they assert on.
                let _ = std::process::Command::new("icacls")
                    .arg(target)
                    .arg("/deny")
                    .arg(format!("{user}:(R)"))
                    .output();
                // …and `(RD)` — list-directory — on the PARENT. Without this, `fs::metadata` falls back
                // from its refused `CreateFileW` open to `FindFirstFileW`, reads the entry out of the
                // parent directory, and answers `Ok` — which is why `Path::exists()` kept returning
                // `true` and why four rounds of this chain concluded that byte loss was not stageable.
                // `RD` is NOT `DC`: the rename's `FILE_DELETE_CHILD` route survives, so the stat fails
                // AND the destructive rename still lands. Never deny `(DC)` here — that cuts both delete
                // routes and every byte-loss assertion downstream passes for the wrong reason.
                if let Some(parent) = target.parent() {
                    let _ = std::process::Command::new("icacls")
                        .arg(parent)
                        .arg("/deny")
                        .arg(format!("{user}:(RD)"))
                        .output();
                }
            }
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Some(parent) = target.parent() {
            let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o000));
        }
    }
    require_staged("deny_stat_of", true, target.try_exists().is_err())
}

/// Undo whatever [`deny_stat_of`] did, so a scratch tree containing `target` can be removed. `parent` is
/// `target`'s parent directory — the Unix leg denies there, not on `target` itself, so both need
/// restoring on their respective platforms. Safe to call even when the deny never took effect.
#[cfg(test)]
pub(crate) fn undo_deny_stat_of(target: &Path, parent: &Path) {
    #[cfg(windows)]
    {
        // Windows denies BOTH the target `(R)` and its parent `(RD)` (CPE-1705 correction 4), so both
        // have to come off — leaving the parent's deny in place makes the scratch tree unlistable and
        // the next test in the same directory fail for an unrelated reason.
        //
        // **Parents FIRST, target last, and the order is not cosmetic.** `icacls <file>` has to resolve
        // and enumerate the file to rewrite its ACL, and it cannot do that while the containing directory
        // still denies list-directory: the call fails silently (this helper ignores its exit status), the
        // target keeps its `(R)` deny, and the caller's `fs::read` of the victim then dies with
        // `PermissionDenied` — which reads exactly like the test's own byte assertion failing, so the
        // next person debugs a guard that was never broken. Measured: reordering target-first →
        // parent-first turned four red CPE-1705 tests green in this crate. The two `src-tauri` tests that
        // stage the same denies inline (they cannot call this `pub(crate)` helper) needed the identical
        // reordering separately — see their `Restore` impls.
        if let Ok(user) = std::env::var("USERNAME") {
            let mut dirs = vec![parent];
            if let Some(real_parent) = target.parent() {
                if real_parent != parent {
                    dirs.push(real_parent);
                }
            }
            for dir in dirs {
                let _ = std::process::Command::new("icacls").arg(dir).arg("/remove:d").arg(&user).output();
            }
            let _ = std::process::Command::new("icacls").arg(target).arg("/remove:d").arg(&user).output();
        }
    }
    #[cfg(unix)]
    {
        let _ = target; // Unix denies `parent`; `target` itself is untouched on this platform.
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
    }
}

/// Put a **dangling** link (a link whose target does not exist) at `link`, for the CPE-1710 tests that
/// prove a `rename`-destructive site refuses rather than destroying one. Returns whether the slot really
/// holds a link afterwards — i.e. whether [`std::fs::symlink_metadata`] agrees, which is the exact
/// question [`symlink_slot_refusal`] asks, so a `true` here means the test is testing something.
///
/// **`false` must be treated as a loud `writeln!(std::io::stderr(), ..)` skip, never a silent pass**
/// (Evidence Rules, `Ticketing/wiki.md`): a Windows runner without Developer Mode or elevation cannot
/// create a symlink at all, and a test that quietly degrades into asserting nothing is the specific
/// failure this ticket family has spent six rounds on.
///
/// Two constructions on Windows, in order:
///
/// 1. `symlink_file` to a name that does not exist. Needs `SeCreateSymbolicLinkPrivilege` (Developer Mode
///    or elevation), which unprivileged CI runners do not have.
/// 2. Failing that, an NTFS **junction**, which needs no privilege — but `junction::create` canonicalises
///    its target, so the target has to exist at creation time and is then removed to leave the junction
///    dangling. Rust reports a junction's `file_type().is_symlink()` as `true`, which is what the guard
///    reads, so this construction stages exactly the same slot.
///
/// Unix has neither restriction: `symlink(2)` never resolves its target, so leg 1 always works.
///
/// **Why this is `pub` and not `#[cfg(test)] pub(crate)`** like its neighbour [`deny_stat_of`]: the app
/// adapter's tests need it too. `rename_entry_impl`, `move_exact_impl` and `board_move_impl` all live in
/// `src-tauri` and all rename onto a user-named slot, and in CPE-1710's first round they shipped **with no
/// test each** for exactly this reason — the helper could not be reached from there, and the alternative
/// was a third inlined copy of the construction (which is how `deny_stat_of`'s inline duplicates in
/// `src-tauri` ended up needing the parent-`RD` fix applied separately, per CPE-1705 correction 4). One
/// implementation, reachable from both crates, is the lesser evil.
/// **CPE-1717:** `supported_here = true` — between `symlink_file` and the privilege-free junction
/// fallback there is no platform CI runs on where this construction is *expected* to fail, so a
/// failure means the runner changed under us and the step goes red under CI rather than announcing
/// into a green log. Locally it remains a loud skip (see [`staging_is_strict`]).
#[track_caller]
pub fn make_dangling_link(link: &Path) -> bool {
    require_staged("make_dangling_link", true, make_dangling_link_inner(link))
}

/// **Where [`make_dangling_link`] points its link — the single derivation** (CPE-1725, PR #904 review).
///
/// A test that stages a dangling link and then asserts the save did **not** create its target has to name
/// that target, and every such assertion is a **negative**. So a caller that derives the name itself and
/// gets it wrong does not fail: `!wrong_name.exists()` is true *because the name is wrong*, and the leg
/// passes while covering nothing. The review measured exactly that — with a drifted copy of the literal
/// and the bug restored, the filesystem assertion stopped asserting and the test only reddened later, on
/// the `Result`, which is the thing this ticket's whole design says proves nothing.
///
/// The literal previously existed in four places: [`make_dangling_link_inner`], an inline copy in
/// `src-tauri`'s CPE-1716 dangling test, a second copy added by CPE-1725, and `sidecar/agent-board`'s own
/// test helper. The first three now call this. **The fourth deliberately does not and cannot**: per ADR
/// 0001 a sidecar depends only on `sidecar-contract`, never on this crate, so `agent-board` keeps its own
/// construction — and it is safe from this hazard by a different route, because its helper *returns* the
/// target path instead of re-deriving it, so its callers cannot name a different one. Stated here so the
/// remaining copy is a recorded exception rather than one this comment quietly overclaims about.
pub fn dangling_link_target(link: &Path) -> PathBuf {
    link.with_file_name(format!(
        "{}-target-that-does-not-exist",
        link.file_name().unwrap_or_default().to_string_lossy()
    ))
}

fn make_dangling_link_inner(link: &Path) -> bool {
    let missing = dangling_link_target(link);
    #[cfg(windows)]
    {
        if std::os::windows::fs::symlink_file(&missing, link).is_err() {
            // No symlink privilege — fall back to a junction, created against a real directory that is
            // then deleted so the reparse point is left pointing at nothing.
            if std::fs::create_dir_all(&missing).is_err() {
                return false;
            }
            let made = junction::create(&missing, link).is_ok();
            let _ = std::fs::remove_dir_all(&missing);
            if !made {
                return false;
            }
        }
    }
    #[cfg(unix)]
    {
        if std::os::unix::fs::symlink(&missing, link).is_err() {
            return false;
        }
    }
    // The premise, asserted rather than assumed: the slot holds a link, and it dangles.
    std::fs::symlink_metadata(link).is_ok_and(|m| m.file_type().is_symlink())
        && !matches!(link.try_exists(), Ok(true))
}

/// Put a **live directory link** at `link` pointing at the existing directory `target`, for the
/// CPE-1730 tests that drive a request path *through* a symlinked intermediate directory. Returns
/// whether the slot really holds a link afterwards, so a `false` is a leg that must announce a skip
/// rather than assert nothing.
///
/// The third escape shape needs neither `..` nor an absolute path — `/<link>/victim.txt` looks exactly
/// like an ordinary request — so it is the one no textual filter can see, and the one whose test cannot
/// be written without creating a real link.
///
/// Same two constructions, in the same order, as [`make_dangling_link`]: a real directory symlink first
/// (needs `SeCreateSymbolicLinkPrivilege` on Windows), then an NTFS **junction**, which needs no
/// privilege and which Rust reports as `file_type().is_symlink() == true`. Unix has neither restriction.
///
/// **`pub` for the same reason as [`make_dangling_link`]**: the callers are in `cpe-ftp`, `cpe-sftp` and
/// `cpe-webdav`, none of which depends on `junction`, and the alternative was three inlined copies of
/// the fallback — which is exactly how `deny_stat_of`'s duplicates ended up needing the same fix applied
/// three times.
///
/// **CPE-1717:** `supported_here = true` — between the symlink and the privilege-free junction there is
/// no platform CI runs on where this is *expected* to fail, so a failure is the runner changing under
/// us, not an ordinary skip.
#[track_caller]
pub fn make_dir_link(target: &Path, link: &Path) -> bool {
    require_staged("make_dir_link", true, make_dir_link_inner(target, link))
}

fn make_dir_link_inner(target: &Path, link: &Path) -> bool {
    #[cfg(windows)]
    {
        if std::os::windows::fs::symlink_dir(target, link).is_err() && junction::create(target, link).is_err() {
            return false;
        }
    }
    #[cfg(unix)]
    {
        if std::os::unix::fs::symlink(target, link).is_err() {
            return false;
        }
    }
    // The premise, asserted rather than assumed: the slot holds a link, and it leads to `target`.
    std::fs::symlink_metadata(link).is_ok_and(|m| m.file_type().is_symlink())
        && std::fs::canonicalize(link).ok() == std::fs::canonicalize(target).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _; // the CPE-1717 loud-skip announcements below write to stderr

    #[test]
    fn epoch_ms_of_unix_epoch_is_zero() {
        assert_eq!(to_epoch_ms(UNIX_EPOCH), Some(0));
    }

    /// **CPE-1693, the assertion the whole ticket rests on.** `scratch_dir` is armed — and must already
    /// own the directory — *before* the panic below runs, exactly the ordering every converted test
    /// module now gets for free just by calling `scratch()`. Panics mid-assertion via
    /// `std::panic::catch_unwind`, so a leak here doesn't crash the test binary and abort every other
    /// guard's own drop — it lets this one test observe, on the far side of the unwind, whether the
    /// directory it created is gone.
    ///
    /// Before CPE-1693 every `scratch()`-style helper in this tree returned a bare `PathBuf` and relied
    /// on a manual `remove_dir_all` written *after* the assertions — which this exact panic shape would
    /// have skipped, which is how the tree reached ~1.29 million leaked `cpe-*` directories in `%TEMP%`
    /// (see the ticket's Work Log for the measured before/after counts across a full `cargo test`).
    #[test]
    fn scratch_dir_guard_removes_the_directory_even_when_the_caller_panics_mid_assertion() {
        let dir = scratch_dir("cpe-fsutil-panic-proof");
        let path = dir.path().to_path_buf();
        assert!(path.is_dir(), "sanity: the guard must actually own a real directory before we panic");

        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // `dir` is moved into the closure so it drops during the unwind below, not after — the
            // ordering the whole ticket is about. If `scratch_dir` regressed to returning a bare
            // `PathBuf` again, this line wouldn't compile (nothing to move-and-drop), and if the `Drop`
            // impl regressed to a no-op, `path.is_dir()` below would still read `true`.
            let _armed = dir;
            panic!("CPE-1693 proof: deliberate panic — the guard above must already be armed");
        }))
        .is_err();

        assert!(panicked, "the proof only proves anything if the inner closure actually panicked");
        assert!(
            !path.is_dir(),
            "CPE-1693 REGRESSION: {} still exists after its owning ScratchDir panicked out of scope — \
             the guard did not clean up on the unwind path, which is the entire point of this ticket",
            path.display()
        );
    }

    /// CPE-1717. The whole policy as a table, because the policy is the deliverable and it has to be
    /// checkable without setting process-global environment variables under a parallel harness.
    ///
    /// Read the columns as: *is the mechanism supposed to work here* × *did it work* × *strict* ×
    /// *sabotaged*. The two rows that matter are the last two: a supporting platform that failed to
    /// stage is `Fail` under strictness (CI) and `LegitimateSkip` without it (a developer's machine).
    #[test]
    fn cpe_1717_staging_policy_only_fails_where_the_mechanism_was_supposed_to_work() {
        use StagingVerdict::*;
        // **All sixteen rows, not a chosen subset.** The first version listed ten and left out six as
        // "equivalent by construction" — which is true, and is exactly the argument a table test
        // exists to stop anyone having to make. With four booleans, exhaustiveness is free, and a
        // future edit that breaks the equivalence has nowhere to hide.
        //
        // supported, staged, strict, sabotaged  →  verdict
        let table = [
            // ── not supported here: a legitimate skip under EVERY harness. These are the
            // `deny_dir_traversal`-on-Windows / ACL-test-on-Linux rows, and none of them may become a
            // false red just because CI asked for strictness.
            (false, false, false, false, LegitimateSkip),
            (false, false, false, true, LegitimateSkip),
            (false, false, true, false, LegitimateSkip),
            (false, false, true, true, LegitimateSkip),
            (false, true, false, false, Staged),
            (false, true, false, true, LegitimateSkip),
            (false, true, true, false, Staged),
            (false, true, true, true, LegitimateSkip),
            // ── supported here: strictness is what separates a red from a loud skip.
            (true, false, false, false, LegitimateSkip),
            (true, false, false, true, LegitimateSkip),
            (true, false, true, false, Fail),
            (true, false, true, true, Fail),
            (true, true, false, false, Staged),
            (true, true, false, true, LegitimateSkip),
            (true, true, true, false, Staged),
            // Sabotage turns a genuine success into the failure case — how CI neutralises this guard
            // on purpose and proves it still bites.
            (true, true, true, true, Fail),
        ];
        assert_eq!(table.len(), 16, "four booleans have sixteen combinations; list all of them");
        for (supported, staged, strict, sabotaged, want) in table {
            assert_eq!(
                staging_verdict(supported, staged, strict, sabotaged),
                want,
                "supported={supported} staged={staged} strict={strict} sabotaged={sabotaged}"
            );
        }
    }

    /// The failure message is load-bearing twice over: a human has to be able to tell a changed runner
    /// from a real bug, and **the CI guard step greps it** — `.github/workflows/ci.yml`'s
    /// "skip-visibility guard" steps fail the build if a sabotaged run dies without `CPE-1717` in the
    /// output, on the grounds that a failure for some other reason is not evidence about this
    /// mechanism. So the two strings that grep depends on are asserted here, not assumed.
    #[test]
    fn cpe_1717_the_failure_message_names_the_ticket_the_mechanism_and_the_way_out() {
        let msg = staging_failure_message("deny_stat_of");
        assert!(msg.contains("CPE-1717"), "the CI guard greps for this, got: {msg}");
        assert!(msg.contains("deny_stat_of"), "must name the mechanism that failed, got: {msg}");
        assert!(msg.contains(std::env::consts::OS), "must name the platform, got: {msg}");
        assert!(
            msg.contains("CPE_STAGING_STRICT=0"),
            "must offer the escape hatch, or a runner regression blocks every PR with no way out: \
             {msg}"
        );
        assert!(
            msg.contains("verified NOTHING"),
            "must say what actually went wrong — that the leg covered nothing — rather than only that \
             a helper returned false: {msg}"
        );
    }

    /// `CPE_STAGING_STRICT` has to override `CI` in both directions, because CI is exactly where the
    /// escape hatch is needed if a runner image regresses before the fix lands.
    ///
    /// **This test used to assert against a hand-written closure that duplicated the `match` it was
    /// meant to check**, so the reviewer could invert the real function entirely and watch every
    /// CPE-1717 test pass. It now drives [`strict_from`] itself. The two `None` rows are the important
    /// ones: they are the only connection between this feature and an ordinary CI run, and breaking
    /// that arm alone was measured to turn a broken-staging CI run green while the guard step —
    /// which pinned the override — still reported OK.
    #[test]
    fn cpe_1717_strictness_reads_its_overrides() {
        // (value, ci) → strict
        let table = [
            // The arm that decides an ordinary CI run. Break it and a leg that stages nothing goes
            // green; nothing else in this file would notice.
            (None, true, true),
            (None, false, false),
            // Explicit overrides, which must win in BOTH directions.
            (Some("1"), false, true),
            (Some("0"), true, false),
            (Some("true"), false, true),
            (Some("false"), true, false),
            (Some("yes"), false, true),
            (Some("no"), true, false),
            (Some("on"), false, true),
            (Some("off"), true, false),
            // Spelling tolerance: a value typed by a human in a hurry still has to work, because an
            // escape hatch that silently does nothing is worse than no escape hatch.
            (Some("TRUE"), false, true),
            (Some("  1  "), false, true),
            (Some("Off"), true, false),
            // Empty is treated as unset — a workflow that writes `CPE_STAGING_STRICT: ""` means "leave
            // it alone", not "turn CI strictness off".
            (Some(""), true, true),
            (Some(""), false, false),
        ];
        for (value, ci, want) in table {
            assert_eq!(
                strict_from(value, ci),
                want,
                "strict_from({value:?}, ci={ci}) must be {want}"
            );
        }
    }

    /// `CI` is read by *other people's* tools, so it is parsed leniently — but `is_some()` was too
    /// lenient by half: `CI=false` counted as CI, which is not "following `CI`" by any reading.
    #[test]
    fn cpe_1717_ci_detection_treats_a_falsy_value_as_not_ci() {
        for truthy in ["true", "1", "TRUE", "yes", " true ", "azure-pipelines", "woodpecker"] {
            assert!(ci_from(Some(truthy)), "CI={truthy:?} must read as CI");
        }
        for falsy in ["", "0", "false", "FALSE", "no", "off", "  "] {
            assert!(!ci_from(Some(falsy)), "CI={falsy:?} must NOT read as CI");
        }
        assert!(!ci_from(None), "unset is not CI");
    }

    /// An unrecognised value must be **loud**, not silently folded into the default. Folding it in is
    /// the same "unknown quietly treated as the safe answer" shape CPE-1680 is about, and here it
    /// would mean someone reaches for the documented escape hatch, mistypes it, and believes it took.
    ///
    /// The spellings below are the ones an independent audit drove through the knob and found
    /// **silently lenient** in the first version, which matched the literal `1` and nothing else: a
    /// developer forcing the check got a green run and believed they had run it. That is CPE-1717's
    /// own bug — a check reporting success without having run — reintroduced inside CPE-1717's own
    /// knob. `strict` and `2` are not in the vocabulary, so they must be refused, not ignored.
    #[test]
    fn cpe_1717_an_unrecognised_strictness_value_is_refused_not_ignored() {
        // The 19 spellings the independent audit drove through the knob. Under the first version —
        // which matched the literal `1` and nothing else — every row below that is now `On` was
        // SILENTLY LENIENT, so forcing the check produced a green run the developer believed they had
        // forced.
        use StrictToken::*;
        let vocabulary = [
            ("1", On),
            ("true", On),
            ("True", On),
            ("TRUE", On),
            ("yes", On),
            ("YES", On),
            ("on", On),
            (" 1", On),
            ("1 ", On),
            (" true ", On),
            ("0", Off),
            ("false", Off),
            ("False", Off),
            ("no", Off),
            ("off", Off),
            ("", Unset),
            ("   ", Unset),
            // Not in the vocabulary — refused, never guessed at.
            ("strict", Unrecognised),
            ("2", Unrecognised),
            ("maybe", Unrecognised),
            ("enabled", Unrecognised),
            ("y", Unrecognised),
        ];
        for (raw, want) in vocabulary {
            assert_eq!(strict_token(raw), want, "CPE_STAGING_STRICT={raw:?}");
        }
    }

    /// …and `Unrecognised` really does stop the run rather than falling through to the default. This
    /// is the assertion that makes the vocabulary table above mean something.
    #[test]
    #[should_panic(expected = "CPE_STAGING_STRICT")]
    fn cpe_1717_an_unrecognised_strictness_value_panics_rather_than_defaulting() {
        let _ = strict_from(Some("strict"), true);
    }

    /// The five spellings the PR #855 audit drove through a consented `apply_backup_plan` and watched
    /// wipe the destination root, plus the milder "wrong file" variant. All must read as unstable.
    #[test]
    fn win32_unstable_names_are_recognised() {
        for name in [" ", "  ", "...", ". ", " .", "....", ".", "..", "report. ", "notes.", "a "] {
            assert!(win32_name_is_unstable(name), "{name:?} must be recognised as Win32-unstable");
        }
    }

    /// …and ordinary names, including ones with interior dots/spaces or a leading dot, must not be —
    /// otherwise the rule would refuse most of a real backup plan.
    #[test]
    fn ordinary_names_are_not_flagged() {
        for name in ["notes", "taxes.docx", "my report.txt", ".gitignore", "a.b.c", " leading"] {
            assert!(!win32_name_is_unstable(name), "{name:?} must NOT be flagged");
        }
        // The empty string is a separate error class, handled by the callers, not by this predicate.
        assert!(!win32_name_is_unstable(""));
    }

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-fsutil-{tag}"))
    }

    /// A NUL-terminated wide string for the Win32 calls the CPE-1739 tests make directly. `std` exposes
    /// *reading* a file's attribute word ([`std::os::windows::fs::MetadataExt::file_attributes`]) but no
    /// way to set one, and no way to open with a chosen sharing mode — both of which the tests need in
    /// order to stage the conditions the UAT measured.
    #[cfg(windows)]
    fn wide(p: &Path) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt as _;
        p.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
    }

    /// Mark `p` `FILE_ATTRIBUTE_HIDDEN`, the attribute PR #904's UAT watched a rename-based save destroy.
    #[cfg(windows)]
    fn set_hidden(p: &Path) {
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::{SetFileAttributesW, FILE_ATTRIBUTE_HIDDEN};
        unsafe { SetFileAttributesW(PCWSTR(wide(p).as_ptr()), FILE_ATTRIBUTE_HIDDEN) }
            .expect("this machine must be able to set FILE_ATTRIBUTE_HIDDEN");
    }

    /// The guarantee, tested as a guarantee: driven with real resolved paths rather than through any
    /// list of spellings, because enumerating spellings is exactly the approach the PR #855 audit
    /// showed cannot work.
    #[test]
    fn contained_under_admits_only_paths_strictly_inside_the_root() {
        let d = scratch("contained");
        let root = d.join("root");
        std::fs::create_dir_all(root.join("nested")).unwrap();
        std::fs::write(root.join("a.txt"), b"x").unwrap();
        std::fs::write(root.join("nested/deep.txt"), b"y").unwrap();

        // The root itself, however it is reached.
        assert!(contained_under(&root, &root).is_err(), "the root itself must be refused");
        assert!(contained_under(&root.join("nested/.."), &root).is_err(), "…and a traversal back to it");
        // Outside the root entirely.
        assert!(contained_under(&d, &root).is_err(), "the root's PARENT must be refused");
        // Real children must pass — the check must not break ordinary removes.
        assert!(contained_under(&root.join("nested"), &root).is_ok(), "a real child must be allowed");
        assert!(contained_under(&root.join("a.txt"), &root).is_ok(), "…and a real file");
        assert!(contained_under(&root.join("nested/deep.txt"), &root).is_ok(), "…and a nested file");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The failure policy, asserted in both directions — the half the first transfer-side copy of this
    /// check got backwards by using `if let (Ok(a), Ok(b)) = …` and falling through to the destructive
    /// call whenever `canonicalize` errored.
    #[test]
    fn contained_under_fails_closed_on_an_unresolvable_root_and_open_on_a_missing_target() {
        let d = scratch("contained_io");
        let root = d.join("root");
        std::fs::create_dir_all(&root).unwrap();

        // Root can't be resolved → refuse. Nothing legitimate can be removed under it.
        assert!(
            contained_under(&root.join("x"), &d.join("no-such-root")).is_err(),
            "an unresolvable root must REFUSE, never fall through to the destructive call"
        );
        // Target doesn't exist → allow (see the precondition: it cannot be destroyed, and the caller's
        // own remove reports it). This is sound ONLY for a remove target.
        assert!(contained_under(&root.join("never-existed.txt"), &root).is_ok());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **The escape shapes, enumerated** — the three CPE-1730 exists to close, driven through
    /// [`confined_to`] directly so the property is pinned independently of any rig.
    ///
    /// Enumerated rather than sampled because naming only `..` is what makes a reader believe the other
    /// two are handled: family (b) needs no `..` and family (c) needs neither `..` nor an absolute path,
    /// so a check built to close (a) alone looks complete and is not.
    #[test]
    fn confined_to_refuses_all_three_escape_shapes_and_admits_ordinary_paths() {
        let d = scratch("confined");
        let root = d.join("root");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("a.txt"), b"x").unwrap();
        let sibling = d.join("sibling");
        std::fs::create_dir_all(&sibling).unwrap();
        std::fs::write(sibling.join("victim.txt"), b"outside").unwrap();

        // Ordinary paths — the check must not break the rigs it is being added to.
        assert!(confined_to(&root, &root), "the root itself is contained (a resolver must map `/`)");
        assert!(confined_to(&root.join("a.txt"), &root), "an existing file inside must be allowed");
        assert!(confined_to(&root.join("sub"), &root), "…and an existing subdirectory");
        assert!(
            confined_to(&root.join("not-yet.txt"), &root),
            "a NOT-YET-EXISTING name inside the root must be allowed — this is the create/STOR case, \
             and it is exactly where `contained_under` fails open"
        );
        assert!(confined_to(&root.join("sub/deep/new.txt"), &root), "…including under missing parents");

        // (a) `..`-shaped.
        assert!(!confined_to(&root.join("../sibling/victim.txt"), &root), "(a) `..` to a sibling");
        assert!(!confined_to(&root.join(".."), &root), "(a) `..` to the parent itself");
        assert!(!confined_to(&root.join("sub/../../sibling"), &root), "(a) `..` through a real subdir");

        // (b) absolute — `Path::join` discards the base, so the destination replaces the root outright.
        // Spelled as the join the rigs actually perform, so this is the real shape and not a paraphrase.
        let abs = sibling.join("victim.txt");
        assert!(abs.is_absolute(), "the fixture must really be absolute or (b) tests nothing");
        assert!(!confined_to(&root.join(&abs), &root), "(b) an absolute path replaces the root");

        // (c) through a symlinked intermediate directory — neither `..` nor absolute.
        let link = root.join("outlink");
        if make_dir_link(&sibling, &link) {
            assert!(
                !confined_to(&link.join("victim.txt"), &root),
                "(c) a path THROUGH a symlinked subdirectory leaves the tree with no `..` and no \
                 absolute component — the shape no textual check can see"
            );
            assert!(
                !confined_to(&link.join("not-yet.txt"), &root),
                "(c) …and the same for a name that does not exist yet, which is the write case"
            );
        } else {
            let _ = writeln!(
                std::io::stderr(),
                "SKIP: leg (c) — this platform/account cannot create a directory link at {}",
                link.display()
            );
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Every unresolvable case **refuses** — the opposite of [`contained_under`]'s policy, and the
    /// asymmetry is the precondition (this validates a path about to be created, not one about to be
    /// destroyed).
    #[test]
    fn confined_to_fails_closed_on_everything_it_cannot_resolve() {
        let d = scratch("confined_io");
        let root = d.join("root");
        std::fs::create_dir_all(&root).unwrap();

        assert!(!confined_to(&root.join("x"), &d.join("no-such-root")), "an unresolvable root confines nothing");

        // A dangling link is `NotFound` to `canonicalize` on both platforms, so without the `read_link`
        // hop it would look like an ordinary missing name and be allowed — while `fs::write` through it
        // creates its target OUTSIDE the root.
        let escaping = root.join("escaping-link");
        let outside_target = d.join("never-created-outside.txt");
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&outside_target, &escaping).is_ok();
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(&outside_target, &escaping).is_ok();
        if made {
            assert!(
                std::fs::canonicalize(&escaping).is_err(),
                "the fixture must really be dangling, or this leg proves nothing about the hop"
            );
            assert!(
                !confined_to(&escaping, &root),
                "a DANGLING link whose target is outside the root must be refused — `canonicalize` \
                 reports it as merely missing, which is the trap"
            );
            let inside_target = root.join("also-never-created.txt");
            let inward = root.join("inward-link");
            #[cfg(unix)]
            let ok = std::os::unix::fs::symlink(&inside_target, &inward).is_ok();
            #[cfg(windows)]
            let ok = std::os::windows::fs::symlink_file(&inside_target, &inward).is_ok();
            if ok {
                assert!(
                    confined_to(&inward, &root),
                    "…but a dangling link pointing INSIDE the root stays allowed: refusing every \
                     dangling link would silently neuter cpe-webdav's dangling-link leg rather than \
                     fail it"
                );
            }
        } else {
            let _ = writeln!(std::io::stderr(), "SKIP: dangling-link legs — no symlink privilege here");
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The taxonomy CPE-1705 turns on, pinned as a taxonomy: three inputs, three distinct answers. Pure,
    /// so it runs identically on all three CI OSes and under any account — the coverage an ACL-based test
    /// cannot give (permission bits are inert as root, and on Windows no deny ACE **on the target alone**
    /// refuses `Path::exists()` — a parent `(RD)` deny is also required, see `deny_stat_of`).
    #[test]
    fn classify_target_slot_separates_absent_from_unreadable() {
        assert_eq!(classify_target_slot(&Ok(false)), TargetSlot::Free, "absent is free");
        assert_eq!(classify_target_slot(&Ok(true)), TargetSlot::Occupied, "present is occupied");
        // `try_exists` normally folds this into `Ok(false)`; an explicit NotFound must agree with it.
        assert_eq!(
            classify_target_slot(&Err(std::io::Error::from(std::io::ErrorKind::NotFound))),
            TargetSlot::Free,
            "an explicit NotFound is a genuine absence"
        );
        // Every other failure is "I could not tell" — NEVER "it isn't there". This is the whole bug.
        for kind in [
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::Other,
            std::io::ErrorKind::TimedOut,
            std::io::ErrorKind::InvalidInput,
        ] {
            assert_eq!(
                classify_target_slot(&Err(std::io::Error::new(kind, "nope"))),
                TargetSlot::Unknown,
                "{kind:?} means we could not tell, which must never read as free"
            );
        }
    }

    /// The guard itself, driven against a real filesystem for the two answers a real filesystem can give
    /// cheaply, plus the wording contract for the third.
    #[test]
    fn clobber_refusal_proceeds_only_on_a_proven_absence() {
        let d = scratch("clobber");
        let absent = d.join("nothing-here.txt");
        assert_eq!(clobber_refusal(&absent, "taken"), None, "a genuinely absent target must proceed");

        let present = d.join("there.txt");
        std::fs::write(&present, b"x").unwrap();
        assert_eq!(
            clobber_refusal(&present, "\"there.txt\" already exists").as_deref(),
            Some("\"there.txt\" already exists"),
            "an occupied target must refuse with the CALLER's wording, not this helper's"
        );

        // The unknown wording must name the path, quote the OS's own cause, and say nothing was written.
        let msg = unknown_slot_message(
            &absent,
            &Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Access is denied.")),
        );
        assert!(msg.contains("nothing-here.txt"), "must name the path it could not read: {msg}");
        assert!(msg.contains("Access is denied."), "must quote the OS's own cause: {msg}");
        assert!(msg.contains("nothing was written"), "must say the operation did not happen: {msg}");
        assert!(
            !msg.contains("already exists"),
            "must NOT claim the target exists — that is the lie CPE-1687 traced to real user confusion: {msg}"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The combined guard (CPE-1710), driven against a real filesystem for all three verdicts a
    /// `rename`-destructive slot can have. The third — a **dangling** link — is the one
    /// [`clobber_refusal`] alone answers "free" for, and it is the bug this ticket closes.
    #[test]
    fn rename_slot_refusal_covers_the_dangling_link_its_first_half_cannot_see() {
        use std::io::Write;
        let d = scratch("rename-slot");

        let absent = d.join("nothing-here.txt");
        assert_eq!(rename_slot_refusal(&absent, "taken"), None, "a genuinely absent slot must proceed");

        let present = d.join("there.txt");
        std::fs::write(&present, b"x").unwrap();
        assert_eq!(
            rename_slot_refusal(&present, "\"there.txt\" already exists").as_deref(),
            Some("\"there.txt\" already exists"),
            "an ordinary occupied name must still report the CALLER's wording — the occupancy check runs \
             first and the order is part of this helper's contract"
        );

        let link = d.join("dangling.txt");
        if !make_dangling_link(&link) {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1710] SKIPPED the dangling-link leg of rename_slot_refusal: this machine could not \
                 create a link at {} (Windows without Developer Mode / admin, and no junction either). \
                 NOTHING in this test covered the link-destruction route on this run.",
                link.display()
            );
        } else {
            // The premise: the first half of the guard genuinely sees nothing here. If this ever stops
            // holding, the test below would pass for the wrong reason.
            assert_eq!(
                clobber_refusal(&link, "occupied"),
                None,
                "premise: `clobber_refusal` follows the link, finds nothing, and reads the slot as FREE — \
                 that is why the second half exists"
            );
            let msg = rename_slot_refusal(&link, "occupied")
                .expect("a dangling link in the slot must refuse — renaming onto it destroys the link");
            assert!(msg.contains("is a link"), "the refusal must say what is in the way: {msg}");
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **The CPE-1716 decision, every arm, on every runner.** A live *file* symlink needs
    /// `SeCreateSymbolicLinkPrivilege` on Windows and [`make_dangling_link`]'s junction fallback is
    /// directory-only, so the `Ok(true)` arm — the one the data loss travelled through — has no real-IO
    /// staging on an unprivileged Windows account. Driving the pure decision covers it there.
    #[test]
    fn classify_write_target_resolves_a_link_and_refuses_a_dangling_one() {
        let p = Path::new("/tmp/song.wav");
        let real = PathBuf::from("/tmp/library/real.wav");

        assert_eq!(
            classify_write_target(&Ok(false), || panic!("must not canonicalise a non-link"), p),
            Ok(p.to_path_buf()),
            "an ordinary file is written where the caller said, and is not canonicalised at all"
        );
        assert_eq!(
            classify_write_target(
                &Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
                || panic!("must not canonicalise an absent path"),
                p
            ),
            Ok(p.to_path_buf()),
            "a path that provably holds nothing is free to create — nothing can be destroyed there"
        );
        assert_eq!(
            classify_write_target(&Ok(true), || Ok(real.clone()), p),
            Ok(real),
            "a LIVE link must resolve to its target, so the edit reaches the file the user opened rather \
             than replacing their link with it"
        );

        let msg = classify_write_target(
            &Ok(true),
            || Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
            p,
        )
        .expect_err("a dangling link has nothing to edit and must be refused, not renamed over");
        assert!(msg.contains("song.wav"), "the refusal must name the link: {msg}");
        assert!(msg.contains("nothing was written"), "and say the edit did not happen: {msg}");

        let msg = classify_write_target(
            &Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Access is denied.")),
            || panic!("must not canonicalise when the link check itself failed"),
            p,
        )
        .expect_err("an unreadable slot must refuse rather than guess — guessing means renaming over it");
        assert!(msg.contains("Access is denied."), "must quote the OS's own cause: {msg}");
    }

    /// `\\?\` is a Win32 escape hatch, not something a user typed — and [`std::fs::canonicalize`] hands one
    /// back on every Windows resolution, so it reaches error messages naming the far end of a link (PR #899
    /// round 2). Pure string work, so all four cases run on every platform.
    #[test]
    fn display_path_strips_the_windows_verbatim_prefix_including_unc() {
        assert_eq!(display_path(Path::new(r"\\?\C:\music\track.wav")), r"C:\music\track.wav");
        assert_eq!(display_path(Path::new(r"\\?\UNC\nas\media\track.wav")), r"\\nas\media\track.wav");
        // Not verbatim — left exactly as-is, including an ordinary UNC path.
        assert_eq!(display_path(Path::new(r"\\nas\media\track.wav")), r"\\nas\media\track.wav");
        assert_eq!(display_path(Path::new("/home/me/music/track.wav")), "/home/me/music/track.wav");
    }

    /// CPE-1716, end to end on a real filesystem: the ordinary save still works, and a **dangling** link is
    /// refused with the link left standing. The dangling leg runs on every runner — [`make_dangling_link`]
    /// falls back to a privilege-free NTFS junction — and it is the leg that proves the guard is wired into
    /// [`replace_file_contents`] rather than merely existing.
    #[test]
    fn replace_file_contents_rewrites_a_plain_file_and_refuses_a_dangling_link() {
        use std::io::Write;
        let d = scratch("replace-contents");

        let plain = d.join("track.wav");
        std::fs::write(&plain, b"old bytes").unwrap();
        replace_file_contents(&plain, b"new bytes").expect("an ordinary save must succeed");
        assert_eq!(
            std::fs::read(&plain).unwrap(),
            b"new bytes",
            "the file the caller named must hold the new bytes"
        );
        assert!(
            !std::fs::read_dir(&d).unwrap().flatten().any(|e| {
                e.file_name().to_string_lossy().contains(".cpe-tmp")
            }),
            "and the staging temp must not be left behind"
        );

        let link = d.join("dangling.wav");
        if !make_dangling_link(&link) {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1716] SKIPPED the dangling-link leg of replace_file_contents: this machine could not \
                 create a link at {} (Windows without Developer Mode / admin, and no junction either). \
                 NOTHING in this test covered the dangling-link route on this run.",
                link.display()
            );
        } else {
            let e = replace_file_contents(&link, b"edited")
                .expect_err("a dangling link has no file to edit — writing must be refused");
            assert!(
                std::fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()),
                "and the user's link must still be a LINK — replacing it is the CPE-1716 bug"
            );
            assert!(e.contains("is a link"), "the refusal must say what is in the way: {e}");
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **The leg where CPE-1716's loss was measured**: a *live* symlink to a real file. Pre-fix the rename
    /// replaced the link and the real file kept its old bytes, while the caller got `Ok` — so this asserts
    /// on the **file** and on the **slot**, and only then on the result.
    ///
    /// Runs for real on Unix always and on a Windows machine with Developer Mode or elevation. Where it
    /// cannot run, the `Ok(true)` arm it exercises is still covered by
    /// `classify_write_target_resolves_a_link_and_refuses_a_dangling_one` on that same runner, so this test
    /// is not the only thing standing between that arm and zero coverage.
    ///
    /// **The skip below is visible, and its absence is therefore evidence** — measured by the PR #899 UAT
    /// round 2, correcting what an earlier version of this comment claimed. Under a plain `cargo test`
    /// (no `--nocapture`, which is what CI runs) libtest swallows `println!`/`eprintln!` for a *passing*
    /// test but **not** `writeln!(std::io::stderr())`, which writes the real fd 2 directly. Every skip
    /// notice in this module and in `src-tauri/src/lib.rs` uses `writeln!(stderr)`, so a skipped leg says
    /// so in the CI log — and CI run `31772062682` shows this test passing on **windows-latest with no
    /// skip notice**, i.e. the live-link route was exercised for real on all three legs. The gap below
    /// bites an unprivileged, non-Developer-Mode dev box, not CI.
    #[test]
    fn replace_file_contents_edits_the_file_a_live_link_points_at_and_keeps_the_link() {
        use std::io::Write;
        let d = scratch("replace-live-link");
        let real = d.join("real.wav");
        std::fs::write(&real, b"old bytes").unwrap();
        let link = d.join("in-my-library.wav");

        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(&real, &link).is_ok();
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&real, &link).is_ok();
        if !made {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1716] SKIPPED the LIVE-link leg of replace_file_contents: this machine cannot create \
                 a file symlink at {} (Windows without Developer Mode / admin; a junction is \
                 directory-only and cannot stand in). The decision this leg drives is still covered on \
                 this runner by classify_write_target_resolves_a_link_and_refuses_a_dangling_one.",
                link.display()
            );
            let _ = std::fs::remove_dir_all(&d);
            return;
        }

        let r = replace_file_contents(&link, b"new bytes");

        assert_eq!(
            std::fs::read(&real).unwrap(),
            b"new bytes",
            "the REAL file the link points at must have received the edit (result was {r:?})"
        );
        assert!(
            std::fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()),
            "and the user's link must still be a LINK — pre-fix the rename replaced it with a regular \
             file and reported success (result was {r:?})"
        );
        r.expect("and the save itself must succeed");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **`create_new` on the staging file, pinned** (PR #899 Reviewer/UAT round 2).
    ///
    /// [`replace_file_contents`] stages through `create_new` rather than `fs::write` so a link pre-placed
    /// at the staging name is refused instead of written *through*. An earlier round of this PR called
    /// that untestable because the temp name carries pid+nanos and cannot be raced — true of the temp
    /// name, and the wrong conclusion: the guarantee belongs to the **primitive**, so the primitive is
    /// what this drives, with `fs::write` on an identically-staged link as the contrast that shows the
    /// choice is load-bearing rather than decorative. Swapping `create_new(true)` for `create(true).truncate(true)` at [`replace_file_contents`]'s call site reds THIS test, because it calls [`create_staging_file`] -- that function's own opener -- rather than a copy of it.
    ///
    /// The dangling leg runs on every runner ([`make_dangling_link`]'s junction fallback needs no
    /// privilege). The live leg needs a real file symlink and says so when it cannot have one; no
    /// `fs::write` contrast is asserted on the dangling leg because writing through a dangling *junction*
    /// fails on Windows for an unrelated reason, and a contrast that means two different things on two
    /// platforms proves neither.
    #[test]
    fn create_new_refuses_a_link_at_the_staging_name_where_fs_write_would_follow_it() {
        use std::io::Write;
        let d = scratch("create-new-link");
        // **`create_staging_file` is the opener `replace_file_contents` actually uses.** The first version
        // of this test built its own `OpenOptions` closure here, so swapping `create_new(true)` for
        // `create(true).truncate(true)` at the call site left this green — a test named after a guard,
        // passing with the guard removed. Call the real one. (CPE-1739 split the opener in two so the
        // staging path could ask for `0600`; both entry points delegate to the crate's single
        // `create_new(true)`, so this still drives the production primitive.)
        let staged = create_staging_file;

        // ---- dangling link in the staging slot: every runner ----
        let dangling = d.join("staging-dangling");
        if !make_dangling_link(&dangling) {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1716] SKIPPED the dangling leg of the create_new staging guard: this machine could \
                 not create a link at {}.",
                dangling.display()
            );
        } else {
            let e = staged(&dangling).expect_err("create_new must refuse a link already at the name");
            assert_eq!(
                e.kind(),
                std::io::ErrorKind::AlreadyExists,
                "and it must refuse BECAUSE the name is taken, not for some incidental reason: {e}"
            );
            assert!(
                std::fs::symlink_metadata(&dangling).is_ok_and(|m| m.file_type().is_symlink()),
                "and the link must still be a link"
            );
        }

        // ---- live link in the staging slot, with the fs::write contrast ----
        let far = d.join("far-end.txt");
        std::fs::write(&far, b"FAR ORIGINAL").unwrap();
        let live = d.join("staging-live");
        let far2 = d.join("far-end-2.txt");
        std::fs::write(&far2, b"FAR ORIGINAL").unwrap();
        let live2 = d.join("staging-live-2");
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(&far, &live).is_ok()
            && std::os::windows::fs::symlink_file(&far2, &live2).is_ok();
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&far, &live).is_ok()
            && std::os::unix::fs::symlink(&far2, &live2).is_ok();
        if !made {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1716] SKIPPED the LIVE leg of the create_new staging guard: this machine cannot \
                 create a file symlink at {} (Windows without Developer Mode / admin).",
                live.display()
            );
            let _ = std::fs::remove_dir_all(&d);
            return;
        }

        let e = staged(&live).expect_err("create_new must refuse a LIVE link at the staging name too");
        assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists, "for the same reason: {e}");
        assert_eq!(
            std::fs::read(&far).unwrap(),
            b"FAR ORIGINAL",
            "and nothing may reach the far end of somebody else's link"
        );

        // The contrast: the same staged link, written with the primitive `create_new` replaced.
        std::fs::write(&live2, b"WROTE THROUGH").expect("fs::write follows the link and succeeds");
        assert_eq!(
            std::fs::read(&far2).unwrap(),
            b"WROTE THROUGH",
            "`fs::write` writes THROUGH the link to the far end — which is exactly why the staging open \
             uses create_new instead"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    // ---- CPE-1739: the atomic save must carry across what is attached to the FILE OBJECT ----------
    //
    // Measured by PR #904's UAT: `replace_file_contents` turned a `0600` private file into `0644`, a
    // `0755` script into `0644`, and destroyed the Windows `HIDDEN` attribute and the `Zone.Identifier`
    // alternate data stream — the Mark of the Web — all while reporting a successful save.
    //
    // Every test below asserts on the FILESYSTEM before unwrapping the `Result`, because every one of
    // these defects fails by returning `Ok`: an assertion placed after an `unwrap` is unreachable in
    // exactly the run that matters.

    /// **CPE-1739 item 1, the security one, on a real filesystem.** A `0600` file that the user saves must
    /// not come back readable by everyone, and a `0755` script must not stop being executable.
    ///
    /// Unix-only because the mode is a Unix concept — not a silent skip: on Windows the equivalent (the
    /// DACL) is carried by `ReplaceFileW`, which
    /// `cpe_1739_windows_a_save_keeps_the_hidden_attribute_and_the_zone_identifier_stream` covers on that
    /// runner. The *policy* behind which bits are carried is pinned on every runner by
    /// `cpe_1739_carried_mode_keeps_every_bit_but_drops_setuid_when_the_owner_changes`.
    ///
    /// Mutation check: deleting the `set_permissions` call in [`carry_protections`] reds this test and
    /// nothing else on a Unix runner.
    #[cfg(unix)]
    #[test]
    fn cpe_1739_a_save_carries_the_mode_so_a_private_file_stays_private() {
        use std::os::unix::fs::PermissionsExt as _;
        let d = scratch("carry-mode");

        let private = d.join("secrets.env");
        std::fs::write(&private, b"TOKEN=old").unwrap();
        std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o600)).unwrap();
        let r = replace_file_contents(&private, b"TOKEN=new");
        assert_eq!(
            std::fs::metadata(&private).unwrap().permissions().mode() & 0o7777,
            0o600,
            "a 0600 private file must not come back world-readable from a save (result was {r:?})"
        );
        r.expect("and the save itself must succeed");
        assert_eq!(std::fs::read(&private).unwrap(), b"TOKEN=new", "and the bytes must have landed");

        let script = d.join("build.sh");
        std::fs::write(&script, b"#!/bin/sh\necho old\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let r = replace_file_contents(&script, b"#!/bin/sh\necho new\n");
        assert_eq!(
            std::fs::metadata(&script).unwrap().permissions().mode() & 0o7777,
            0o755,
            "and an executable script must still be executable after being edited (result was {r:?})"
        );
        r.expect("and that save must succeed too");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **CPE-1739 review round 1, the blocker: the staging file must be born private, not made private.**
    ///
    /// The first round created the `.cpe-tmp` with `create_new`'s default `0666` and narrowed it with an
    /// `fchmod` immediately afterwards, one statement later, before any bytes were written. That is not
    /// enough, and the doc claimed it was: **POSIX checks permission at `open`, not at `read`**, so a
    /// local process that opens the staging name between the `openat` and the `fchmod` keeps a readable
    /// descriptor across the narrowing and reads the private bytes written after it. The name needs no
    /// guessing — an inotify/FSEvents watcher is woken by the create itself. `strace` of the real test
    /// binary, pre-fix: `openat(..., O_WRONLY|O_CREAT|O_EXCL|O_CLOEXEC, 0666) = 3` then `fchmod(3, 0600)`.
    ///
    /// This drives [`create_staging_file`] — **the function `stage_and_replace` calls**, one line above
    /// its call site with nothing between them, not a copy of it and not the `create_exclusive` entry
    /// point that deliberately keeps the platform default for `split_join`'s user-facing output files.
    /// Dropping the mode (`create_exclusive_with_mode(path, None)`) reds this and nothing else.
    ///
    /// **The umask control makes the skip loud instead of silent.** Under a `0077` umask the unfixed code
    /// would produce `0600` too, and this test would pass while proving nothing — so it first checks that
    /// an ordinary `File::create` in the same directory *does* come out group/other-readable. If the
    /// umask has already closed those bits, there is nothing here to demonstrate and the test says so
    /// rather than claiming coverage it does not have.
    #[cfg(unix)]
    #[test]
    fn cpe_1739_the_staging_opener_creates_a_file_no_one_else_can_open() {
        use std::io::Write as _;
        use std::os::unix::fs::PermissionsExt as _;
        let d = scratch("staging-mode");

        let control = d.join("control");
        std::fs::File::create(&control).unwrap();
        let control_mode = std::fs::metadata(&control).unwrap().permissions().mode();
        if control_mode & 0o077 == 0 {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1739] SKIPPED the staging-mode test: this run's umask already strips every \
                 group/other bit (an ordinary File::create came out {:04o}), so a staging file created \
                 WITHOUT an explicit mode would look private too and this test could not tell the fixed \
                 code from the broken code. NOTHING here covered the staging-mode window on this run.",
                control_mode & 0o7777
            );
            let _ = std::fs::remove_dir_all(&d);
            return;
        }

        let staged = d.join("secrets.env.4242-1.cpe-tmp");
        let f = create_staging_file(&staged).expect("the staging opener must create the file");
        let mode = std::fs::metadata(&staged).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o077,
            0,
            "the staging file must be created with NO group or other access ({mode:04o}) — narrowing it \
             one statement later is too late, because POSIX checks permission at open() and a process \
             that got in first keeps a descriptor that can read the private bytes written afterwards"
        );
        assert_ne!(mode & 0o400, 0, "and its owner must still be able to read it back ({mode:04o})");
        drop(f);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **CPE-1739's Unix half of item 3.** Extended attributes are where this app keeps macOS Finder tags
    /// (`com.apple.metadata:_kMDItemUserTags`, CPE-826/829) and where macOS keeps its own Mark of the Web
    /// (`com.apple.quarantine`), so a save that dropped them would be destroying the feature next door.
    ///
    /// **The skip is loud and says what went uncovered**, because plenty of real filesystems have no xattr
    /// support at all (`ENOTSUP`) — notably `tmpfs`, which is where `scratch()` lands on some Linux
    /// configurations. Seeding is the probe: if the attribute cannot be set on the *source*, this machine
    /// cannot express the property under test and nothing here is asserted.
    #[cfg(unix)]
    #[test]
    fn cpe_1739_a_save_carries_extended_attributes_where_the_filesystem_has_them() {
        use std::io::Write as _;
        let d = scratch("carry-xattr");
        let p = d.join("holiday.jpg");
        std::fs::write(&p, b"old bytes").unwrap();

        if xattr::set(&p, "user.cpe.test", b"tagged").is_err() {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1739] SKIPPED the extended-attribute leg: {} is on a filesystem that will not store \
                 a user.* xattr (ENOTSUP — tmpfs and several network mounts). NOTHING in this test covered \
                 xattr carry-over on this run.",
                d.display()
            );
            let _ = std::fs::remove_dir_all(&d);
            return;
        }

        let r = replace_file_contents(&p, b"new bytes");
        assert_eq!(
            xattr::get(&p, "user.cpe.test").ok().flatten().as_deref(),
            Some(&b"tagged"[..]),
            "an extended attribute on the file the user edited must survive the save — this is where \
             Finder tags and com.apple.quarantine live (result was {r:?})"
        );
        r.expect("and the save itself must succeed");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **CPE-1739 items 2 and 3 on Windows, measured the way the UAT measured them**: the attribute word
    /// read back through [`std::os::windows::fs::MetadataExt::file_attributes`], and a real alternate data
    /// stream written with `path:stream` syntax.
    ///
    /// `Zone.Identifier` is the Mark of the Web — the record that a file came from the internet, which
    /// other software on the machine acts on. Losing it on save is a security-relevant loss, not a
    /// cosmetic one, which is why this is a test and not a doc line.
    ///
    /// Mutation check: making [`commit_replacement`] take its `fs::rename` branch unconditionally reds
    /// this test (`attrs=0x820`, `ADS=Err(NotFound)`) and nothing else.
    #[cfg(windows)]
    #[test]
    fn cpe_1739_windows_a_save_keeps_the_hidden_attribute_and_the_zone_identifier_stream() {
        use std::os::windows::fs::MetadataExt as _;
        let d = scratch("carry-attrs");
        let p = d.join("downloaded.zip");
        std::fs::write(&p, b"old bytes").unwrap();
        // The Mark of the Web, exactly as a browser writes it.
        std::fs::write(format!("{}:Zone.Identifier", p.display()), b"[ZoneTransfer]\r\nZoneId=3\r\n")
            .expect("this machine must be able to write an alternate data stream (NTFS)");
        set_hidden(&p);

        let r = replace_file_contents(&p, b"new bytes");

        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        let attrs = std::fs::metadata(&p).unwrap().file_attributes();
        assert_ne!(
            attrs & FILE_ATTRIBUTE_HIDDEN,
            0,
            "a HIDDEN file must still be hidden after being saved — a rename leaves the attribute behind \
             on the object it unlinks (attrs=0x{attrs:x}, result was {r:?})"
        );
        assert_eq!(
            std::fs::read(format!("{}:Zone.Identifier", p.display())).ok().as_deref(),
            Some(&b"[ZoneTransfer]\r\nZoneId=3\r\n"[..]),
            "and the Zone.Identifier stream — the Mark of the Web — must survive the save (result was {r:?})"
        );
        r.expect("and the save itself must succeed");
        assert_eq!(std::fs::read(&p).unwrap(), b"new bytes", "and the new bytes must have landed");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **CPE-1739 item 4, pinned as the gap it still is.** `ReplaceFileW` did not buy this back: while
    /// another program holds the file open with `SHARE_READ|WRITE` — what an ordinary Windows application
    /// holds, and *not* what Rust's `File::open` takes — the save still fails, only with an error that now
    /// names the real cause. What must NOT happen is the file being damaged on the way out.
    ///
    /// This exists so the claim in [`commit_replacement`], [`replace_file_contents`] and
    /// `src/docs/25-metadata-studio.md` stays true rather than becoming folklore: if a future change ever
    /// makes this save succeed, this test reds and those three places get corrected instead of quietly
    /// under-promising.
    #[cfg(windows)]
    #[test]
    fn cpe_1739_windows_a_foreign_share_read_write_handle_still_blocks_the_save() {
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::{
            CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        };
        let d = scratch("share-violation");
        let p = d.join("held.wav");
        std::fs::write(&p, b"old bytes").unwrap();

        // Exactly the sharing mode an ordinary Windows application takes: readers and writers welcome,
        // deleters and renamers not. Rust's own `File::open` adds FILE_SHARE_DELETE, which is why it
        // cannot stage this.
        let handle = unsafe {
            CreateFileW(
                PCWSTR(wide(&p).as_ptr()),
                0x8000_0000, // GENERIC_READ
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        }
        .expect("opening the user's file the way another application would must be possible");

        let r = replace_file_contents(&p, b"new bytes");

        assert_eq!(
            std::fs::read(&p).unwrap(),
            b"old bytes",
            "whatever happens, the user's file must be exactly as it was (result was {r:?})"
        );
        assert!(
            !std::fs::read_dir(&d).unwrap().flatten().any(|e| e.file_name().to_string_lossy().contains(".cpe-tmp")),
            "and the staging temp must not be left behind by the failure (result was {r:?})"
        );
        let e = r.expect_err(
            "CPE-1739 item 4 is NOT closed: a foreign SHARE_READ|WRITE handle still blocks the save. If \
             this now succeeds, fix the doc comments on commit_replacement and replace_file_contents and \
             src/docs/25-metadata-studio.md, which all tell the user it fails",
        );
        assert!(e.contains("held.wav"), "the failure must name the file: {e}");
        // **Naming the file is not enough** (CPE-1739 review round 1, F7): a `classify_carryover` refusal
        // names it too, so an assertion that stopped there would pass while the save failed for a
        // completely different reason and the doc's claim — that the failure is now *accurate about the
        // cause* — would be held by nothing. `0x80070020` is `ERROR_SHARING_VIOLATION`, and asserting the
        // code rather than the prose ("...being used by another process.") is deliberate: the prose is
        // localised by the OS and would red on a non-English runner for no real reason.
        assert!(
            e.contains("0x80070020"),
            "and it must be a SHARING VIOLATION (ERROR_SHARING_VIOLATION, 0x80070020) — the save failing \
             for some other reason, such as the carry-over refusal, would satisfy a name-only check while \
             leaving the documented cause unproven: {e}"
        );
        assert!(
            !e.contains("could not read what"),
            "and specifically NOT the classify_carryover refusal — fs::metadata succeeds against this \
             handle, which is why the refusal is unreachable here: {e}"
        );

        unsafe { windows::Win32::Foundation::CloseHandle(handle).ok() };
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **The Save-As shape, and the reason it is a test rather than an assumption.** `ReplaceFileW` needs a
    /// file to replace and answers `NotFound` when there is none (measured), so routing every Windows save
    /// through it would break creating a file at a free name — which `resolve_write_target` deliberately
    /// admits and this function's callers use.
    ///
    /// Mutation check: dropping the `if target_exists` condition in [`commit_replacement`] reds this test
    /// on Windows and nothing else.
    #[test]
    fn cpe_1739_a_save_to_a_free_name_still_creates_the_file() {
        let d = scratch("carry-free-name");
        let p = d.join("brand-new.json");

        let r = replace_file_contents(&p, b"{\"a\":1}");
        assert_eq!(
            std::fs::read(&p).ok().as_deref(),
            Some(&b"{\"a\":1}"[..]),
            "a save to a name where nothing exists must still create the file (result was {r:?})"
        );
        r.expect("and it must report success");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **CPE-1739, the pure decision: an unreadable target refuses the save.** This is what makes item 1 a
    /// fix rather than a best-effort improvement — if we cannot tell whether the file about to be replaced
    /// is `0600`, saving anyway hands back whatever the umask gives, and the one case where it matters most
    /// is the one a "carry it if you can" policy would silently downgrade.
    ///
    /// A truth table rather than real IO, for the reason [`classify_write_target`] is one: staging a stat
    /// that fails with something *other* than `NotFound` needs platform-specific permission gymnastics that
    /// differ on all three runners, and the arm that must not be reached (`NotFound` → refuse) cannot be
    /// staged by real IO at all. Runs everywhere.
    #[test]
    fn cpe_1739_classify_carryover_refuses_a_target_it_cannot_read_but_allows_an_absent_one() {
        let p = Path::new("/music/track.wav");
        assert_eq!(classify_carryover(None, p), Ok(true), "a readable target: carry what it has");
        assert_eq!(
            classify_carryover(Some(&std::io::Error::from(std::io::ErrorKind::NotFound)), p),
            Ok(false),
            "an absent name is a legitimate brand-new file — nothing to carry, and the save proceeds"
        );

        let msg = classify_carryover(
            Some(&std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Access is denied.")),
            p,
        )
        .expect_err("a target we cannot read must refuse — saving anyway is the security downgrade");
        assert!(msg.contains("track.wav"), "the refusal must name the file: {msg}");
        assert!(msg.contains("nothing was written"), "and say the save did not happen: {msg}");
        assert!(msg.contains("Access is denied."), "and quote the OS's own cause: {msg}");
    }

    /// **CPE-1739, the pure decision behind which mode bits travel.** Pure `u32` arithmetic, so this runs
    /// on all three legs including the Windows one that never calls [`carried_mode`] — a policy tested only
    /// where it executes would be the weaker arrangement, and there is nothing platform-specific in it.
    #[test]
    fn cpe_1739_carried_mode_keeps_every_bit_but_drops_setuid_when_the_owner_changes() {
        // st_mode as `stat` hands it over: file-type bits included. Only the permission bits may travel —
        // POSIX leaves `chmod` unspecified for anything else, so passing them through would be relying on
        // one platform's tolerance and hoping the other two agree.
        assert_eq!(carried_mode(0o100_600, 1000, 1000), 0o600, "S_IFREG must be masked off");
        assert_eq!(carried_mode(0o100_755, 1000, 1000), 0o755, "an executable keeps its exec bits");
        assert_eq!(carried_mode(0o100_644, 1000, 1000), 0o644, "and the ordinary case is unchanged");
        // Same owner: every bit means exactly what it meant before, setuid/setgid/sticky included.
        assert_eq!(carried_mode(0o104_755, 1000, 1000), 0o4755, "setuid survives when the owner is the same");
        assert_eq!(carried_mode(0o102_755, 1000, 1000), 0o2755, "so does setgid");
        assert_eq!(carried_mode(0o101_777, 1000, 1000), 0o1777, "and the sticky bit is not a privilege bit");
        // Different owner: ownership cannot be carried, so a surviving setuid bit would no longer mean
        // "runs as the original owner" — it would mean "runs as whoever saved it". Drop it; keep the rest.
        assert_eq!(carried_mode(0o104_755, 0, 1000), 0o755, "setuid must NOT be re-pointed at the saving user");
        assert_eq!(carried_mode(0o102_755, 0, 1000), 0o755, "nor setgid");
        assert_eq!(carried_mode(0o101_777, 0, 1000), 0o1777, "but the sticky bit still travels");
        assert_eq!(carried_mode(0o100_600, 0, 1000), 0o600, "and an ordinary private file is untouched by this");
    }

    // ---- CPE-1755: the three cases STAGING_MODE's doc now describes, each measured on real IO -----
    //
    // Every test here arms its cleanup with a `Drop` guard BEFORE the assertion that can panic, so a red
    // run (which this repo requires proving, not just a green one) still removes its scratch directory —
    // the pattern `split_join.rs`/`dispatch.rs` already use, chosen over a trailing `remove_dir_all` that
    // a panicking assertion would skip.

    /// **CPE-1755, case 1: widen.** A `0644` target must come back `0644` — `carry_protections`'
    /// `fchmod` opens the staging file's `0600` birth mode back out to match. This is the one case the
    /// pre-CPE-1755 doc on [`STAGING_MODE`] actually described ("only ever widens").
    ///
    /// Mutation check (run manually, not left in the tree): commenting out the `set_permissions` call
    /// inside `carry_protections` reds this test — see the Work Log for the actual red output.
    #[cfg(unix)]
    #[test]
    fn cpe_1755_a_0644_target_widens_the_staged_file_from_its_0600_birth_mode() {
        use std::os::unix::fs::PermissionsExt as _;
        let d = scratch("cpe1755-widen");
        struct Cleanup<'a>(&'a Path);
        impl Drop for Cleanup<'_> {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(self.0);
            }
        }
        let _cleanup = Cleanup(&d);

        let p = d.join("readme.txt");
        std::fs::write(&p, b"old").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();

        let r = replace_file_contents(&p, b"new");
        let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o7777;
        assert_eq!(
            mode, 0o644,
            "a 0644 target must come back 0644, not stay at the staging file's 0600 birth mode \
             (save result: {r:?})"
        );
        r.expect("the save itself must succeed");
    }

    /// **CPE-1755, case 2: narrow — the case [`STAGING_MODE`]'s doc got wrong before this ticket.** A
    /// `0400` target must come back `0400`: the staging file is BORN `0600` (CPE-1739) and
    /// `carry_protections`' `fchmod` copies the target's mode exactly, which here means taking the
    /// owner-write bit AWAY, not adding to what the file already has. "Only ever widens" was never a
    /// general truth — it happened to hold for every case CPE-1739 itself measured — and this is the
    /// target that falsifies it.
    ///
    /// Mutation check (run manually, not left in the tree): same as the widen test above — commenting
    /// out `carry_protections`' `set_permissions` call reds this one too, since the file then stays at
    /// its `0600` birth mode instead of narrowing to `0400`. See the Work Log for the red output.
    #[cfg(unix)]
    #[test]
    fn cpe_1755_a_0400_target_narrows_the_staged_file_from_its_0600_birth_mode() {
        use std::os::unix::fs::PermissionsExt as _;
        let d = scratch("cpe1755-narrow");
        struct Cleanup<'a>(&'a Path);
        impl Drop for Cleanup<'_> {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(self.0);
            }
        }
        let _cleanup = Cleanup(&d);

        let p = d.join("readonly.txt");
        std::fs::write(&p, b"old").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o400)).unwrap();

        let r = replace_file_contents(&p, b"new");
        let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o7777;
        assert_eq!(
            mode, 0o400,
            "a 0400 target must come back 0400 — narrower than the staging file's own 0600 birth mode \
             (save result: {r:?})"
        );
        r.expect("the save itself must succeed");
    }

    /// **CPE-1755, case 3: no target at all — the recorded decision.** `existing` is `None` for a save
    /// to a brand-new name, so `carry_protections` never runs and the file is deliberately left at the
    /// staging file's `0600` birth mode rather than widened to the platform's `0666 & ~umask` default —
    /// see the `else` of the `carry_protections` call in [`stage_and_replace`] for the full reasoning.
    /// This route is currently unreachable from production (`metadata_write_impl` always finds an
    /// existing target; Save-As does not call this function), but the free-name path is directly
    /// reachable from this test and from `cpe_1739_a_save_to_a_free_name_still_creates_the_file`, so the
    /// mode it lands at is pinned here rather than left as an unchecked accident of `None` skipping a
    /// branch.
    ///
    /// Mutation check (run manually, not left in the tree): widening the staged file unconditionally
    /// when `existing` is `None` (simulating "match the platform default instead") reds this test — see
    /// the Work Log for the red output.
    #[cfg(unix)]
    #[test]
    fn cpe_1755_a_brand_new_name_lands_at_the_0600_staging_birth_mode_not_the_platform_default() {
        use std::os::unix::fs::PermissionsExt as _;
        let d = scratch("cpe1755-free-name");
        struct Cleanup<'a>(&'a Path);
        impl Drop for Cleanup<'_> {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(self.0);
            }
        }
        let _cleanup = Cleanup(&d);

        let p = d.join("brand-new.json");
        let r = replace_file_contents(&p, b"{}");
        let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o7777;
        assert_eq!(
            mode, 0o600,
            "a save to a free name has nothing to carry and must stay at the staging file's 0600 birth \
             mode (CPE-1755's recorded decision); save result: {r:?}"
        );
        r.expect("the save itself must succeed");
    }

    /// **CPE-1738, the pure decision, as a truth table.** Split out from [`sweep_stale_temp_siblings`]
    /// precisely because the link arm cannot be proven honestly by real-IO ageing: a dangling symlink
    /// cannot be opened at all (there is nothing at the far end for `File::open` to reach), so a real-IO
    /// test can only ever show "a YOUNG link survives" — indistinguishable from an ordinary young file
    /// surviving because of the age floor, for a completely different reason. This table asserts the two
    /// axes independently, including the row real IO cannot stage: an OLD link that must still survive.
    #[test]
    fn cpe_1738_should_sweep_temp_never_removes_a_link_and_never_removes_something_young() {
        use std::time::Duration;
        let floor = Duration::from_secs(60);
        // (is_symlink, age, floor) -> remove?
        let table = [
            (false, Duration::from_secs(0), false, "brand new, ordinary file: must survive"),
            (false, Duration::from_secs(59), false, "just under the floor: must survive"),
            (false, Duration::from_secs(60), true, "exactly at the floor: may be removed"),
            (false, Duration::from_secs(3600), true, "old ordinary file: must be removed"),
            (true, Duration::from_secs(0), false, "young link: must survive (also true via age alone)"),
            // The row real IO cannot stage (see doc comment above): an OLD link. If the link check were
            // ever reordered behind the age check, only THIS row would flip, and every real-IO test in
            // this module would stay green — which is exactly why it has to be pinned here.
            (true, Duration::from_secs(3600), false, "OLD link: must still survive — never followed or removed"),
        ];
        for (is_symlink, age, want, why) in table {
            assert_eq!(
                should_sweep_temp(is_symlink, age, floor),
                want,
                "is_symlink={is_symlink} age={age:?} floor={floor:?}: {why}"
            );
        }
    }

    /// **CPE-1738, PR #910 review round 2, Blocker 1 — the pure decision, as a truth table.** The two real
    /// exposures the review round found, as the FIRST two negative rows: an empty middle (a real file kept
    /// on purpose, literally `<name>.cpe-tmp`) and a different file's own extension-suffixed temp
    /// (`bak.4242-...` — the pre-`-` half is not all digits). Neutralising the guard back to
    /// `starts_with`/`ends_with` alone would make both of those `true`; this table is what proves it stays
    /// `false`.
    #[test]
    fn cpe_1738_is_valid_temp_stamp_requires_exactly_digits_dash_digits() {
        let table = [
            ("4242-1000000000000", true, "a genuine stamp"),
            ("0-0", true, "degenerate but still two digit runs joined by one dash"),
            ("", false, "EMPTY MIDDLE — Blocker 1 exposure 1: a real file the user kept on purpose"),
            (
                "bak.4242-1000000000000",
                false,
                "a DIFFERENT file's own extension-suffixed stamp — Blocker 1 exposure 2",
            ),
            ("4242", false, "no dash at all"),
            ("4242-", false, "empty nanos half"),
            ("-1000000000000", false, "empty pid half"),
            ("4242-1000000000000-1", false, "two dashes"),
            ("42a2-1000000000000", false, "non-digit in the pid half"),
            ("4242-100000000000a", false, "non-digit in the nanos half"),
        ];
        for (s, want, why) in table {
            assert_eq!(is_valid_temp_stamp(s), want, "is_valid_temp_stamp({s:?}): {why}");
        }
    }

    /// **CPE-1738, end to end on a real filesystem.** A `.cpe-tmp` orphaned by an earlier "crash" — aged
    /// past the floor via [`std::fs::File::set_modified`] — is swept by the very next successful save of
    /// the same file, while a second one only seconds old (which could still belong to a save genuinely in
    /// flight) is left alone. This is the ticket's own acceptance test: "must never remove a temp belonging
    /// to a live save (test it with a second temp created seconds earlier)."
    #[test]
    fn stage_and_replace_sweeps_a_stale_orphan_but_spares_one_only_seconds_old() {
        let d = scratch("sweep-stale");
        let target = d.join("notes.txt");
        std::fs::write(&target, b"original").unwrap();

        // A temp shaped exactly like stage_and_replace's own stamp, orphaned by an earlier crash.
        let stale = d.join("notes.txt.4242-1000000000000.cpe-tmp");
        std::fs::write(&stale, b"half-written by a killed save").unwrap();
        let long_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        // `File::open` is read-only, which `set_modified` needs write access to change on Windows
        // (`ERROR_ACCESS_DENIED`, measured) — open for write explicitly.
        std::fs::OpenOptions::new()
            .write(true)
            .open(&stale)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();

        // A second one, freshly created — the AC's "created seconds earlier" case. No sleep needed: it
        // simply is not aged, which is the property under test.
        let fresh = d.join("notes.txt.4242-1000000000001.cpe-tmp");
        std::fs::write(&fresh, b"a save that could still be in flight").unwrap();

        let r = replace_file_contents(&target, b"new bytes");

        // Assert the filesystem effects BEFORE unwrapping the Result: if `replace_file_contents` ever
        // failed here, `r.expect` below would panic with a generic message and these — the assertions
        // that actually distinguish "swept correctly" from "swept the wrong one" or "swept nothing" —
        // would never run at all.
        assert!(
            std::fs::symlink_metadata(&stale).is_err(),
            "the STALE orphan from an earlier crash must be swept (result was {r:?})"
        );
        assert!(
            std::fs::symlink_metadata(&fresh).is_ok(),
            "a temp only seconds old must SURVIVE — it could still belong to a live concurrent save \
             (result was {r:?})"
        );
        r.expect("the save itself must still succeed");
        assert_eq!(std::fs::read(&target).unwrap(), b"new bytes", "and it must have written the new bytes");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A `.cpe-tmp` staged for a DIFFERENT file in the same directory must never be swept while saving
    /// THIS one — [`sweep_stale_temp_siblings`] matches on `<this-file's-name>.` as an exact prefix, never
    /// a directory-wide glob, precisely so two files' saves in the same folder cannot interfere with each
    /// other's leftovers.
    ///
    /// **`a.txt`/`b.txt` is the OWN-NAME-PREFIX guard's fixture, not the STAMP guard's** (PR #910 review
    /// round 2 correction): the two names share no prefix at all, so this only ever proves a different
    /// file's temp is unreachable when the names themselves don't overlap. It does NOT exercise — and
    /// passed unchanged through — the empty-middle and sibling-extension exposures Blocker 1 found, both
    /// of which need a candidate whose name genuinely starts with `target`'s own name plus a dot. See
    /// `stage_and_replace_never_sweeps_a_real_file_with_an_empty_stamp` and
    /// `stage_and_replace_never_sweeps_a_sibling_files_extension_suffixed_temp` for those.
    #[test]
    fn stage_and_replace_never_sweeps_a_different_files_stale_temp() {
        let d = scratch("sweep-other-file");
        let target = d.join("a.txt");
        std::fs::write(&target, b"a").unwrap();
        let other_target = d.join("b.txt");
        std::fs::write(&other_target, b"b").unwrap();

        let others_stale = d.join("b.txt.4242-1.cpe-tmp");
        std::fs::write(&others_stale, b"someone else's orphan").unwrap();
        let long_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&others_stale)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();

        let r = replace_file_contents(&target, b"new a");
        assert!(
            std::fs::symlink_metadata(&others_stale).is_ok(),
            "a DIFFERENT file's stale temp must never be touched by this save (result was {r:?})"
        );
        r.expect("the save itself must still succeed");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **CPE-1738, PR #910 review round 2, Blocker 1 exposure 1 (UAT).** A REAL file the user kept on
    /// purpose, literally named `<target-name>.cpe-tmp` — the exact name the shipped docs used to tell a
    /// user to give one — must survive a save of `<target-name>`, even aged well past the floor. Before
    /// the stamp-shape check landed this was permanently, silently destroyed: the prefix's trailing dot and
    /// the suffix's leading dot are the SAME character in this name, so `starts_with(prefix)` and
    /// `ends_with(".cpe-tmp")` were both true with nothing at all between them.
    #[test]
    fn stage_and_replace_never_sweeps_a_real_file_with_an_empty_stamp() {
        let d = scratch("sweep-empty-middle");
        let target = d.join("notes.txt");
        std::fs::write(&target, b"original").unwrap();

        let users_own_file = d.join("notes.txt.cpe-tmp");
        std::fs::write(&users_own_file, b"a temp copy the user deliberately kept").unwrap();
        let long_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&users_own_file)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();

        let r = replace_file_contents(&target, b"new bytes");
        assert!(
            std::fs::symlink_metadata(&users_own_file).is_ok(),
            "a REAL file the user kept, literally named <name>.cpe-tmp with no pid-nanos stamp at all, \
             must never be swept (result was {r:?})"
        );
        r.expect("the save itself must still succeed");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **CPE-1738, PR #910 review round 2, Blocker 1 exposure 2 + Blocker 2 (Reviewer).** A genuinely
    /// different file's OWN staged temp, whose name happens to extend THIS file's name plus an extension —
    /// `a.txt.bak` mid-save, staged as `a.txt.bak.<pid>-<nanos>.cpe-tmp`, matched while saving `a.txt`.
    /// `IMG_001.jpg`/`IMG_001.jpg.xmp`, `data.csv`/`data.csv.old`, `archive.tar`/`archive.tar.gz` are the
    /// same shape with ordinary, everyday names — this is not an exotic fixture. Reviewer's own probe:
    /// "saving a.txt; a.txt.bak's own temp survived = false (save result Ok(()))" before the fix.
    #[test]
    fn stage_and_replace_never_sweeps_a_sibling_files_extension_suffixed_temp() {
        let d = scratch("sweep-sibling-ext");
        let target = d.join("a.txt");
        std::fs::write(&target, b"a").unwrap();
        std::fs::write(d.join("a.txt.bak"), b"backup").unwrap();

        let siblings_temp = d.join("a.txt.bak.4242-1000000000000.cpe-tmp");
        std::fs::write(&siblings_temp, b"a.txt.bak's OWN in-flight save").unwrap();
        let long_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&siblings_temp)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();

        let r = replace_file_contents(&target, b"new a");
        assert!(
            std::fs::symlink_metadata(&siblings_temp).is_ok(),
            "a.txt.bak's OWN staged temp must never be touched while saving a.txt — its middle segment \
             (\"bak.4242-1000000000000\") is not a valid <digits>-<digits> stamp (result was {r:?})"
        );
        r.expect("the save itself must still succeed");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **CPE-1738, PR #910 review round 2, Blocker 2 (Reviewer).** A name that matches the prefix and even
    /// has a valid-LOOKING stamp shape, but is missing the `.cpe-tmp` suffix entirely, must never be swept
    /// — it is not a staging file at all. Pins the suffix check as independently load-bearing: the review
    /// round deleted the whole `ends_with(".cpe-tmp")` check, ran `cargo test --lib fsutil::`, and got
    /// "42 passed, 0 failed" — nothing in the shipped suite reds without this fixture.
    #[test]
    fn stage_and_replace_never_sweeps_a_name_with_a_stamp_shape_but_no_cpe_tmp_suffix() {
        let d = scratch("sweep-no-suffix");
        let target = d.join("notes.txt");
        std::fs::write(&target, b"original").unwrap();

        let lookalike = d.join("notes.txt.4242-1000000000000");
        std::fs::write(&lookalike, b"not a staging file at all").unwrap();
        let long_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&lookalike)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();

        let r = replace_file_contents(&target, b"new bytes");
        assert!(
            std::fs::symlink_metadata(&lookalike).is_ok(),
            "a stamp-shaped name with NO .cpe-tmp suffix is not a staging file and must survive \
             (result was {r:?})"
        );
        r.expect("the save itself must still succeed");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **CPE-1738, PR #910 review round 2, Blocker 4 (Reviewer).** The age reference must be THIS
    /// FILESYSTEM's own clock — `target`'s own just-set mtime — never the client's `SystemTime::now()`.
    /// Simulates a share whose clock reads minutes behind the client's (Reviewer's own probe: "a file
    /// created a millisecond ago whose mtime reads 6 minutes back") by stamping BOTH `target` and a
    /// same-instant candidate with an mtime 6 minutes behind real wall-clock time: from the filesystem's
    /// own point of view the two are simultaneous (age 0), so the candidate must survive. Under the OLD,
    /// client-clocked comparison this candidate would read as 6 minutes old — past the 5-minute floor —
    /// and be swept despite having been staged at the very instant `target` was renamed.
    #[test]
    fn sweep_stale_temp_siblings_uses_the_filesystems_own_clock_not_the_clients() {
        let d = scratch("sweep-clock-skew");
        let target = d.join("notes.txt");
        std::fs::write(&target, b"original").unwrap();
        // The share's clock, 6 minutes behind the client's real `SystemTime::now()` — past the 5-minute
        // floor if compared against the client's clock, and exactly what a lagging share produces for a
        // file it only just received.
        let share_now = std::time::SystemTime::now() - std::time::Duration::from_secs(360);
        std::fs::OpenOptions::new().write(true).open(&target).unwrap().set_modified(share_now).unwrap();

        // A candidate staged at the SAME instant as `target`, from the filesystem's own point of view —
        // i.e. a live save's temp, created moments ago.
        let candidate = d.join("notes.txt.4242-1000000000000.cpe-tmp");
        std::fs::write(&candidate, b"still writing").unwrap();
        std::fs::OpenOptions::new().write(true).open(&candidate).unwrap().set_modified(share_now).unwrap();

        sweep_stale_temp_siblings(&target);

        assert!(
            std::fs::symlink_metadata(&candidate).is_ok(),
            "a candidate stamped at the SAME instant as target's own reference mtime must survive \
             regardless of how far that instant sits from the client's real SystemTime::now() — proves \
             the age reference is the filesystem's own clock, not the client's"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The IO wrapper actually skips a name that matches the staging pattern but is a symlink — wired
    /// proof to go with [`cpe_1738_should_sweep_temp_never_removes_a_link_and_never_removes_something_young`]'s
    /// pure truth table. Uses [`make_dangling_link`] (privilege-free everywhere via the junction fallback);
    /// it cannot be aged past the floor by this test (see that table's own comment on why), so this proves
    /// the wiring — that a matching name is actually stat'd with `symlink_metadata` and skipped — while the
    /// pure table proves the decision holds even when it IS old.
    #[test]
    fn sweep_stale_temp_siblings_never_touches_a_link_at_a_matching_name() {
        use std::io::Write;
        let d = scratch("sweep-link");
        let target = d.join("track.wav");
        std::fs::write(&target, b"original").unwrap();

        let link = d.join("track.wav.4242-1.cpe-tmp");
        if !make_dangling_link(&link) {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1738] SKIPPED the link leg of sweep_stale_temp_siblings: this machine could not \
                 create a link at {} (Windows without Developer Mode / admin, and no junction either). \
                 The decision itself is still covered by the pure should_sweep_temp truth table.",
                link.display()
            );
            let _ = std::fs::remove_dir_all(&d);
            return;
        }

        sweep_stale_temp_siblings(&target);
        assert!(
            std::fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()),
            "a name matching the staging pattern that is actually a LINK must be left exactly as it was"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **Leg 2 of [`make_dangling_link`], exercised on its own** (CPE-1710, PR #895 UAT).
    ///
    /// On a developer machine with Developer Mode on, `symlink_file` always succeeds and the **junction**
    /// fallback never runs — so "verified locally, no skip notice printed" proves leg 1 and says nothing
    /// about the leg that unprivileged CI runners actually take. This drives leg 2 directly: build the
    /// junction, delete its target, and assert the resulting slot is the same hazard — a link by
    /// `symlink_metadata`, invisible to [`clobber_refusal`], refused by [`rename_slot_refusal`].
    ///
    /// Windows-only because junctions are; the Unix leg of `make_dangling_link` has no second path to
    /// exercise (`symlink(2)` never resolves its target, so leg 1 cannot fail for the privilege reason).
    #[test]
    #[cfg(windows)]
    fn the_junction_fallback_stages_the_same_hazard_as_a_symlink() {
        let d = scratch("junction-leg");
        let target = d.join("real-dir");
        std::fs::create_dir_all(&target).unwrap();
        let link = d.join("slot");
        // `junction::create` canonicalises its target, so the target must exist here and is removed
        // afterwards — that is exactly what leaves the reparse point dangling.
        junction::create(&target, &link).expect("creating a junction needs no privilege");
        std::fs::remove_dir_all(&target).unwrap();

        assert!(
            std::fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()),
            "Rust must report a junction as a link — that is the property the guard reads"
        );
        assert_eq!(
            clobber_refusal(&link, "occupied"),
            None,
            "and the occupancy half must still see nothing there — otherwise the fallback would be \
             staging a different, milder scenario than a dangling symlink"
        );
        assert!(
            rename_slot_refusal(&link, "occupied").is_some_and(|m| m.contains("is a link")),
            "so the paired guard must refuse it"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **A lint for one specific mistake — NOT a structural guarantee** (CPE-1710, scoped down by the
    /// PR #895 UAT and reviewer).
    ///
    /// The structural guard is `clippy.toml`'s `disallowed-methods` on `std::fs::rename` (see
    /// [`rename_into_slot`]): it resolves paths rather than text, so it survives aliasing, distance and
    /// helper indirection, and it fires on an **unguarded** rename — the case this scan is blind to. This
    /// stays as a second, cheaper net for the one shape that actually shipped four times: a bare
    /// [`clobber_refusal`] standing in for the whole guard immediately above an `fs::rename`. It also
    /// rejects a direct [`symlink_slot_refusal`] call outside this module, unless the site says
    /// `LINK-ONLY: <reason>` in the comment above it.
    ///
    /// # What it does NOT catch — measured, not guessed
    ///
    /// The UAT drove each of these through and the scan stayed green, so none of them is theoretical:
    ///
    /// - **A rename with NO guard at all**, and the pre-CPE-1705 `if dst.exists()` shape. The scan only
    ///   fires on the *half*-guarded shape, so the completely unguarded one — the more dangerous of the
    ///   two — is invisible to it. `provider.rs`'s `LocalProvider::rename` is a live example.
    /// - **Distance.** The guard and the rename more than [`SCAN_WINDOW`] lines apart.
    /// - **Aliasing.** `use std::fs::rename as move_entry;` — the scan matches the literal text
    ///   `fs::rename(`.
    /// - **Indirection.** The rename moved behind a helper function.
    ///
    /// **Do not describe this as making the pairing structurally impossible to get wrong.** The first
    /// version of this comment did, which is a claim about a *class* backed by a lint for one *shape* —
    /// and this ticket family exists because a rule was written down and then not followed. A guard that
    /// genuinely closed the class would have to find every `fs::rename` whose destination is user-named
    /// and require the pairing there; that is a different and much larger piece of work than this scan.
    ///
    /// # Scope, stated because a scan that quietly misses a directory is worse than no scan
    ///
    /// Every `.rs` under `crates/*/src/`, `src-tauri/src/` and `sidecar/*/src/` — the first version read
    /// only `crates/server/src/` plus `src-tauri/src/lib.rs`, missing nine sibling files in that same
    /// directory. Production code only: scanning stops at a file's `mod tests`, because a unit test that
    /// asserts on `clobber_refusal` while using `fs::rename` to *stage* a scenario is not a half-guarded
    /// site, and telling its author to switch to `rename_slot_refusal` would be wrong advice. Only this
    /// module is exempt by name, matched on its **full path** — the first version skipped any file called
    /// `fsutil.rs` anywhere in the tree.
    ///
    /// It asserts its own inputs — file count, and that it can still see the combined helper being called
    /// — so it cannot pass by scanning nothing. That failure mode is the whole reason this ticket family
    /// re-measured one incomplete setup four times.
    #[test]
    fn half_applied_rename_guards_are_rejected() {
        let (files, offences, combined_calls) = scan_for_half_applied_guards();
        assert!(
            files > 60,
            "the scan read only {files} files — it is not looking where it thinks it is"
        );
        assert!(
            offences.is_empty(),
            "half-applied rename guards:\n{}\n\n\
             ---\n\
             What this check does NOT cover, so that a green run is not mistaken for a proof:\n\
             - an `fs::rename` with NO guard at all, or the pre-CPE-1705 `if dst.exists()` shape;\n\
             - a guard more than {SCAN_WINDOW} lines from its rename;\n\
             - a rename reached through an alias (`use std::fs::rename as move_entry`) or one call deep\n\
               inside a helper;\n\
             - anything in a different function from the guard (the window stops at a function boundary\n\
               on purpose, to avoid reporting unrelated code);\n\
             - anything outside `crates/*/src`, `src-tauri/src` and `sidecar/*/src`, and anything inside a\n\
               `mod tests`.\n\
             `clippy.toml`'s `disallowed-methods` on `std::fs::rename` is what covers the unguarded and\n\
             aliased cases; this is a lint for one recognised-wrong shape.\n\
             **Absence of a failure here is not evidence the pairing holds.**",
            offences.join("\n")
        );
        assert!(
            combined_calls >= 6,
            "only {combined_calls} call(s) to `rename_into_slot`/`rename_slot_refusal` found — CPE-1710 \
             converted six sites, so the scan is matching nothing and would not catch a regression either"
        );
    }

    /// **The cry-wolf fix, asserted rather than claimed** (PR #895 UAT). The first version of the scan
    /// reported a guard sitting 8 lines above an `fs::rename` **in a different function** — a purely
    /// textual window with no idea where a function ends. A lint that tells an author to change correct
    /// code is worse than no lint: it teaches people to ignore it.
    #[test]
    fn the_scan_window_stops_at_a_function_boundary() {
        let same_fn = ["if let Some(e) = clobber_refusal(&d, \"x\") {", "return;", "}", "fs::rename(a, b);"];
        assert!(
            renames_within_window(&same_fn, 0),
            "a rename below the guard in the SAME function is the shape this lint is for"
        );

        let next_fn = [
            "if let Some(e) = clobber_refusal(&d, \"x\") {",
            "return;",
            "}",
            "}",
            "fn something_else(a: &Path, b: &Path) -> std::io::Result<()> {",
            "fs::rename(a, b)",
        ];
        assert!(
            !renames_within_window(&next_fn, 0),
            "a rename in the NEXT function has nothing to do with this guard — reporting it is the false \
             positive the UAT hit"
        );

        let attributed = [
            "if let Some(e) = clobber_refusal(&d, \"x\") {",
            "}",
            "#[test]",
            "fn t() {",
            "fs::rename(a, b).unwrap();",
        ];
        assert!(!renames_within_window(&attributed, 0), "an attribute also starts a new item");

        let far = {
            let mut v = vec!["clobber_refusal(&d, \"x\");"];
            v.extend(std::iter::repeat_n("let _ = 1;", SCAN_WINDOW + 2));
            v.push("fs::rename(a, b);");
            v
        };
        assert!(
            !renames_within_window(&far, 0),
            "and beyond the window it is missed — a KNOWN hole, asserted so the doc comment above cannot \
             quietly stop being true"
        );
    }

    /// How far below a guard the scan will look for an `fs::rename`. Every real guard-to-rename distance
    /// in this repo is ≤ 8 lines; the window also stops dead at the next `fn`, so it cannot reach into a
    /// neighbouring function (the UAT tripped the first version that way — a guard 8 lines above a rename
    /// *in a different function*).
    const SCAN_WINDOW: usize = 25;

    /// The scan itself, split out so the test above reads as assertions and this reads as the mechanism.
    /// Returns `(files scanned, offences, rename_slot_refusal call count)`.
    fn scan_for_half_applied_guards() -> (usize, Vec<String>, usize) {
        // `crates/server` → repo root.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
        let mut files = Vec::new();
        for group in ["crates", "sidecar"] {
            if let Ok(entries) = std::fs::read_dir(root.join(group)) {
                for entry in entries.flatten() {
                    collect_rs(&entry.path().join("src"), &mut files);
                }
            }
        }
        collect_rs(&root.join("src-tauri").join("src"), &mut files);

        // The one legitimate home of the two halves' names, matched on the full path rather than the
        // basename so a future `fsutil.rs` in another crate is still scanned.
        let exempt = root.join("crates").join("server").join("src").join("fsutil.rs");

        let mut combined_calls = 0usize;
        let mut offences = Vec::new();
        for file in &files {
            if *file == exempt {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(file) else { continue };
            let all: Vec<&str> = text.lines().collect();
            // Production code only — everything from `mod tests` on is a test module.
            let end_of_prod =
                all.iter().position(|l| l.trim_start().starts_with("mod tests")).unwrap_or(all.len());
            let lines = &all[..end_of_prod];
            for (i, line) in lines.iter().enumerate() {
                let code = line.trim_start();
                if code.starts_with("//") {
                    continue; // a doc comment discussing the guards is not a call to them
                }
                if code.contains("rename_slot_refusal(") || code.contains("rename_into_slot(") {
                    combined_calls += 1;
                }
                // A site whose occupancy rule is genuinely its own (an existing EMPTY directory is
                // legitimate at `vault_crypto::promote`, for instance) may take the link half alone — but
                // it has to say so at the site, in the same spirit as the `#[allow]` reasons. The marker
                // is deliberately ugly so it cannot be typed by accident.
                let link_only = lines[i.saturating_sub(12)..=i].iter().any(|l| l.contains("LINK-ONLY:"));
                if code.contains("symlink_slot_refusal(") && !link_only {
                    offences.push(format!(
                        "{}:{}: calls `symlink_slot_refusal` directly — use `rename_into_slot` (guard + \
                         rename) or `rename_slot_refusal`, or write `LINK-ONLY: <reason>` in the comment \
                         above if this site's occupancy rule really is its own",
                        file.display(),
                        i + 1
                    ));
                }
                if code.contains("clobber_refusal(") && renames_within_window(lines, i) {
                    offences.push(format!(
                        "{}:{}: `clobber_refusal` guards an `fs::rename` — that is half the guard. It \
                         follows links, so a DANGLING link at the destination reads as free and the \
                         rename destroys it (CPE-1710). Use `rename_slot_refusal`.",
                        file.display(),
                        i + 1
                    ));
                }
            }
        }
        (files.len(), offences, combined_calls)
    }

    /// Is there an `fs::rename(` within [`SCAN_WINDOW`] lines below `from`, **without crossing into
    /// another function**? The function boundary is what stops the false positive: a guard near the end of
    /// one function and a rename near the start of the next are unrelated, and the first version of this
    /// scan reported them as a violation.
    fn renames_within_window(lines: &[&str], from: usize) -> bool {
        let end = (from + SCAN_WINDOW).min(lines.len());
        for line in &lines[from + 1..end] {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            // A new item starts: stop looking. Covers `fn`, `pub fn`, `pub(crate) async fn`, and the
            // attributes that precede one.
            if code.starts_with("#[") || (code.contains("fn ") && code.ends_with('{')) {
                return false;
            }
            if code.contains("fs::rename(") {
                return true;
            }
        }
        false
    }

    /// The captured print macros. **All four**, not just `eprintln!`: libtest's capture is installed
    /// in `std::io::_print`/`_eprint`, which every one of these routes through, so `eprint!` hides a
    /// notice exactly as well as `eprintln!` does. The first version of this scan matched `eprintln!`
    /// alone and an `eprint!("SKIPPED …\n")` walked straight past it.
    const CAPTURED_MACROS: [&str; 4] = ["eprintln!", "eprint!", "println!", "print!"];

    /// The vocabulary this tree's skip notices actually use. **A vocabulary, not a semantic check** —
    /// naming it that way is the point, because the alternative is believing the scan understands
    /// intent.
    ///
    /// `skip` alone was not enough: `vault_manager.rs`'s `the_staging_open_is_exclusive` notice reads
    /// *"NOTE …: no symlink privilege here, so only the regular-file and hard-link forms were
    /// verified."* — a genuine, invisible, `eprintln!` skip notice that **never says "skip"**. It
    /// survived the CPE-1717 conversion pass precisely because that pass and the first scan shared one
    /// pattern, so they shared one blind spot: the same search cannot audit itself.
    const SKIP_PHRASES: [&str; 6] = [
        "skip",
        "not covered",
        "were verified",
        "verified nothing",
        "nothing to verify",
        "no symlink privilege",
    ];

    /// Does `literal_start` — the text from a `"` onwards — read like a notice that a leg did not run?
    ///
    /// **Case-insensitive `contains`, not a prefix match against a list of spellings.** The first
    /// version tested `starts_with("\"SKIP")`/`("\"skipping")`, which missed, in the reviewer's own
    /// counter-examples: `"[CPE-1692] SKIPPED …"` — **the shape 56 sites in this tree actually use**,
    /// and the richer convention a future author will copy — plus sentence-case `"Skipping …"` and a
    /// leading space `" SKIPPED …"`. Three near-misses out of five is not a lint, it is a coin flip.
    ///
    /// No length cap: the survivor above puts its phrase well into the message, and a cap is one more
    /// way for a notice to sit just outside the window.
    fn mentions_skipping(literal_start: &str) -> bool {
        let lower = literal_start.to_ascii_lowercase();
        SKIP_PHRASES.iter().any(|p| lower.contains(p))
    }

    /// CPE-1717. A skip notice written with a **captured** print macro reaches nobody, and 56 sites in
    /// this tree were written that way while two separate comments elsewhere explained why they must
    /// not be.
    ///
    /// libtest replays a test's captured output only when the test FAILS, and a skip is a pass; CI
    /// runs plain `cargo test` with no `--nocapture`. The capture is installed inside the
    /// `print!`/`eprint!` macros, so `writeln!(std::io::stderr(), ..)` — which [`skip_notice!`] wraps
    /// — goes around it. Measured, not assumed: see that macro's doc comment for the harness output.
    ///
    /// # Scope, stated because a scan that quietly misses a shape is worse than no scan
    ///
    /// **Test code only** — from each file's `mod tests` onwards, plus whole files under a `tests/`
    /// directory. That boundary is not tidiness: production code legitimately logs about skipping
    /// (`"cpe: vault-session sweep skipped: …"`, `"[agent-watch] audit dir unavailable, skipping …"`),
    /// and those are runtime diagnostics on a real stderr, not test notices. Flagging them would be
    /// the cry-wolf failure this file's other scan already had to be corrected for.
    ///
    /// **What it still misses, and this list is deliberately not reassuring:**
    /// - a notice phrased outside [`SKIP_PHRASES`] — "this environment cannot create a symlink; the
    ///   assertions below prove less than they look like they do" would pass. This is not
    ///   hypothetical: exactly one such notice (`vault_manager.rs`'s `the_staging_open_is_exclusive`)
    ///   survived the CPE-1717 conversion pass **because that pass and this scan's first version
    ///   shared one pattern**, and it took an independent audit to find. A search cannot audit itself;
    ///   the vocabulary now names that phrase, and the next one will be outside it too;
    /// - a message built into a variable, or read from a `const`, before printing;
    /// - a `write!(std::io::stdout(), ..)`-shaped notice, which is visible but goes to the wrong
    ///   stream;
    /// - a test module not literally named `mod tests`.
    ///
    /// Aliasing (`use std::eprintln as announce;`) is the one indirection that *is* caught, because
    /// it is cheap to catch and has no legitimate use here.
    ///
    /// [`skip_notice!`]: crate::skip_notice
    #[test]
    fn skip_notices_never_use_a_captured_print_macro() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
        let mut files = Vec::new();
        for group in ["crates", "sidecar"] {
            if let Ok(entries) = std::fs::read_dir(root.join(group)) {
                for entry in entries.flatten() {
                    collect_rs(&entry.path().join("src"), &mut files);
                    collect_rs(&entry.path().join("tests"), &mut files);
                }
            }
        }
        collect_rs(&root.join("src-tauri").join("src"), &mut files);
        collect_rs(&root.join("src-tauri").join("tests"), &mut files);

        // Asserted inputs: a scan that reads nothing passes vacuously, which is the exact failure this
        // whole ticket is about.
        assert!(
            files.len() > 60,
            "the scan read only {} files — it is not looking where it thinks it is",
            files.len()
        );

        let mut offences = Vec::new();
        let mut scanned_test_lines = 0usize;
        for file in &files {
            let Ok(text) = std::fs::read_to_string(file) else { continue };
            let all: Vec<&str> = text.lines().collect();
            // A file under `tests/` is test code end to end; a `src/` file's test code starts at
            // `mod tests`. Production logging about skipping is not this scan's business.
            let in_tests_dir = file.components().any(|c| c.as_os_str() == "tests");
            let start = if in_tests_dir {
                0
            } else {
                match all.iter().position(|l| l.trim_start().starts_with("mod tests")) {
                    Some(n) => n,
                    None => continue,
                }
            };
            let lines = &all[start..];
            scanned_test_lines += lines.len();
            for (i, line) in lines.iter().enumerate() {
                let code = line.trim_start();
                if code.starts_with("//") {
                    continue; // a comment explaining the rule is not a breach of it
                }
                // The scan reads its own source, so its detector strings and the fixtures that prove
                // it still catches things look exactly like offences. The marker is deliberately ugly
                // — the same reasoning as `LINK-ONLY:` above — and per-LINE rather than per-file:
                // exempting all of `fsutil.rs` would make this module the one place a real invisible
                // notice could hide.
                if line.contains("SCAN-FIXTURE") {
                    continue;
                }
                // …and the deliberate escape for a real call site whose message merely *mentions*
                // skipping — a diagnostic dump of a batch report's own `skipped` field, say — rather
                // than announcing that the test declined to run. Broad matching is the point of this
                // scan; the price of broad matching is a per-site opt-out that states its reason. A
                // lint that flags correct code teaches people to ignore it, which is the failure the
                // sibling scan in this file was already corrected for once.
                if line.contains("NOT-A-SKIP-NOTICE:") {
                    continue;
                }
                let lineno = start + i + 1;

                // Aliasing the captured macros hides a notice from every check above.
                if code.contains("use std::eprint") // SCAN-FIXTURE
                    || code.contains("use ::std::eprint") // SCAN-FIXTURE
                    || code.contains("use std::print") // SCAN-FIXTURE
                    || code.contains("use ::std::print") // SCAN-FIXTURE
                {
                    offences.push(format!(
                        "{}:{lineno}: aliases a captured print macro, which routes around this scan",
                        file.display()
                    ));
                    continue;
                }

                for mac in CAPTURED_MACROS {
                    // `print!` is a suffix of `eprint!`; only treat this as a call to `mac` when the
                    // preceding character cannot be part of an identifier.
                    let Some(at) = find_macro_call(code, mac) else { continue };
                    let rest = &code[at + mac.len()..];
                    // …the message on the same line, or — the other shape rustfmt produces — on the
                    // next one.
                    let hit = match rest.strip_prefix('(') {
                        Some(args) if args.trim_start().starts_with('"') => {
                            mentions_skipping(args.trim_start())
                        }
                        Some(args) if args.trim().is_empty() => lines
                            .get(i + 1)
                            .map(|n| n.trim_start())
                            .is_some_and(|n| n.starts_with('"') && mentions_skipping(n)),
                        _ => false,
                    };
                    if hit {
                        offences.push(format!(
                            "{}:{lineno}: `{mac}` skip notice",
                            file.display()
                        ));
                        break;
                    }
                }
            }
        }
        assert!(
            scanned_test_lines > 20_000,
            "only {scanned_test_lines} lines of test code were scanned — the `mod tests` boundary is \
             not finding the test modules, so this scan is looking at almost nothing"
        );
        assert!(
            offences.is_empty(),
            "these skip notices use a print macro whose output libtest SWALLOWS for a passing test — \
             so they announce a leg that verified nothing to nobody, on the only harness that \
             matters:\n{}\n\n\
             Fix: `cpe_server::skip_notice!(..)` (or `crate::skip_notice!` inside this crate) — same \
             arguments, writes straight to the process's stderr handle, survives the capture.\n\
             Better still, if the staging mechanism is supposed to work on that platform, use \
             `fsutil::require_staged` and let the leg go RED under CI instead of printing into a green \
             log.\n\n\
             What a green run here does NOT prove: see this test's doc comment. A notice that never \
             says \"skip\", one assembled into a variable first, or a test module not called \
             `mod tests`, all still slip past.",
            offences.join("\n")
        );
    }

    /// Byte offset of a call to the macro `mac` in `code`, requiring that the character before it
    /// cannot continue an identifier — so `eprint!` is not reported as a `print!` call.
    fn find_macro_call(code: &str, mac: &str) -> Option<usize> {
        let mut from = 0usize;
        while let Some(rel) = code[from..].find(mac) {
            let at = from + rel;
            let ok = code[..at]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_alphanumeric() && c != '_' && c != ':');
            if ok {
                return Some(at);
            }
            from = at + mac.len();
        }
        None
    }

    /// The scan's own premise, asserted rather than assumed: it must recognise the shapes the
    /// reviewer of PR #898 smuggled past its first version, and must **not** flag the visible
    /// `skip_notice!`/`writeln!(stderr)` form or production logging that happens to mention skipping.
    #[test]
    fn cpe_1717_the_capture_scan_recognises_the_shapes_that_slipped_past_it() {
        for good in [
            r#"eprintln!("[CPE-1692] SKIPPED the leg: …");"#, // SCAN-FIXTURE
            r#"eprintln!("Skipping the leg: …");"#,           // SCAN-FIXTURE
            r#"eprintln!(" SKIPPED the leg: …");"#,           // SCAN-FIXTURE
            r#"eprint!("SKIPPED the leg: …\n");"#,            // SCAN-FIXTURE
            r#"println!("skipping the leg");"#,               // SCAN-FIXTURE
            r#"print!("SKIPPED");"#,                          // SCAN-FIXTURE
            // The survivor an independent audit found: a real skip notice that never says "skip".
            r#"eprintln!("NOTE …: no symlink privilege here, so only the regular-file and hard-link forms were verified.");"#, // SCAN-FIXTURE
        ] {
            let at = find_macro_call(good, "eprintln!")
                .or_else(|| find_macro_call(good, "eprint!"))
                .or_else(|| find_macro_call(good, "println!"))
                .or_else(|| find_macro_call(good, "print!"))
                .unwrap_or_else(|| panic!("no macro call found in {good:?}"));
            let args = good[at..].split_once('(').unwrap().1;
            assert!(mentions_skipping(args.trim_start()), "must be caught: {good}");
        }
        // `eprint!` must not be mistaken for a `print!` call — that is what makes the four-macro list
        // safe to widen.
        assert_eq!(find_macro_call(r#"eprint!("x")"#, "print!"), None);
        assert!(find_macro_call(r#"eprint!("x")"#, "eprint!").is_some());
        // …and the visible forms this ticket is steering people towards are not offences.
        assert!(find_macro_call(r#"crate::skip_notice!("SKIPPED …");"#, "eprintln!").is_none());
        assert!(!mentions_skipping(r#""a message with no s-word""#));
    }

    /// Every `.rs` under `dir`, recursively — the scan's file list.
    fn collect_rs(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_rs(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    /// The symlink-slot taxonomy, including the `Err` arm the PR #893 reviewer found redding **nothing**
    /// when broken — because the stat was inline and no unit test could reach it. Splitting
    /// `classify_symlink_slot` out is what makes this assertable; the arm had silently accumulated a
    /// garbled message in the meantime.
    #[test]
    fn classify_symlink_slot_separates_a_link_from_an_unreadable_slot() {
        let p = Path::new("/tmp/final.txt");
        assert!(
            classify_symlink_slot(&Ok(true), p).is_some_and(|m| m.contains("is a link")),
            "a link in the slot must refuse — renaming onto it destroys it"
        );
        assert_eq!(classify_symlink_slot(&Ok(false), p), None, "a real entry is clobber_refusal's job");
        assert_eq!(
            classify_symlink_slot(&Err(std::io::Error::from(std::io::ErrorKind::NotFound)), p),
            None,
            "a genuine absence is free"
        );
        for kind in [
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::TimedOut,
            std::io::ErrorKind::Other,
        ] {
            let msg = classify_symlink_slot(&Err(std::io::Error::new(kind, "Access is denied.")), p)
                .unwrap_or_else(|| panic!("{kind:?} must refuse — an lstat we could not do is not a 'no'"));
            // The grammar regression this arm shipped with, pinned so it cannot come back: the message
            // must read "could not check WHETHER … IS A LINK", never "could not check what is at … is a
            // link", which a bulk rename of a sibling helper's wording once produced.
            assert!(
                msg.contains("could not check whether") && msg.contains("is a link"),
                "the unknown-link refusal must be grammatical and say what it could not determine: {msg}"
            );
            assert!(!msg.contains("what is at"), "the clipped wording must never return: {msg}");
        }
    }

    /// CPE-1715: the pure decision [`name_pick_slot_probe`] folds in when `try_exists` alone said "free".
    /// No disk touched, so every arm — including the one that only reproduces with a real filesystem
    /// symlink — is verifiable on every OS and CI account.
    #[test]
    fn classify_link_presence_treats_any_link_as_occupied_and_only_notfound_as_free() {
        assert!(
            classify_link_presence(Ok(true)).unwrap(),
            "a link -- dangling or live -- occupies its slot"
        );
        assert!(
            !classify_link_presence(Ok(false)).unwrap(),
            "confirmed non-link agrees with the try_exists probe that produced it"
        );
        assert!(
            !classify_link_presence(Err(std::io::Error::from(std::io::ErrorKind::NotFound))).unwrap(),
            "an explicit NotFound is a genuine absence -- matches classify_target_slot's own NotFound arm"
        );
        assert!(
            classify_link_presence(Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))).is_err(),
            "a slot we could not even check whether it holds a link must not collapse into Ok(false) -- \
             that is the exact stat-collapse CPE-1696 already fixed for the other half of this probe"
        );
    }

    /// CPE-1718: the create-shaped classifier, arm by arm. Pure, so the `Ok(true)` arm — the one an
    /// unprivileged Windows account cannot stage a live file symlink for — is exercised everywhere.
    #[test]
    fn classify_create_slot_refuses_a_link_and_says_it_would_write_through_it() {
        let p = Path::new("/tmp/rebuilt.bin");
        let link = classify_create_slot(&Ok(true), p).expect("a link at a create slot must refuse");
        assert!(link.contains("is a link"), "{link}");
        assert!(
            link.contains("writes THROUGH it"),
            "the whole reason this is not `classify_symlink_slot` is that a create FOLLOWS the link \
             rather than destroying it, and saying the wrong one is a confident false statement: {link}"
        );
        assert!(
            !link.contains("renaming onto"),
            "the rename wording must never leak into a create site: {link}"
        );

        assert_eq!(classify_create_slot(&Ok(false), p), None, "a real entry is the occupancy half's job");
        assert_eq!(
            classify_create_slot(&Err(std::io::Error::from(std::io::ErrorKind::NotFound)), p),
            None,
            "a genuine absence is free — creating a file at a free name is the point"
        );
        for kind in [
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::TimedOut,
            std::io::ErrorKind::Other,
        ] {
            let msg = classify_create_slot(&Err(std::io::Error::new(kind, "Access is denied.")), p)
                .unwrap_or_else(|| panic!("{kind:?} must refuse — an lstat we could not do is not a 'no'"));
            assert!(
                msg.contains("could not check whether") && msg.contains("nothing was written"),
                "the unknown arm must say what it could not determine and that nothing happened: {msg}"
            );
        }
    }

    /// **CPE-1733 UAT finding 6, at the seam where the decision is actually made** (PR #906 review,
    /// round 4).
    ///
    /// The first attempt at this test asserted `archive::entry_slot_action`, which converts an
    /// already-classified [`CreateSlotLink`] into skip/abort. That is a re-labelling; the choice the UAT
    /// filed about — *is this `Err` a confirmed link, or a slot I could not read?* — is made here. The
    /// review proved the difference with a one-word mutation: change the `Err(_)` arm of
    /// [`create_slot_link_from_stat`] to `CreateSlotLink::Link` and the original bug is back (rows 15–16
    /// drop the entry silently and return `Ok`), yet **the entire suite stayed green**. This test is what
    /// that mutation now hits.
    ///
    /// It is a pure-input test because the `Unknown` arm needs an `lstat` that fails with something other
    /// than `NotFound` — a permission or device fault no test can portably stage, which is precisely why
    /// the arm that was wrong was the arm nothing could reach.
    #[test]
    fn an_unreadable_slot_is_unknown_never_a_confirmed_link() {
        fn arm(v: &CreateSlotLink) -> &'static str {
            match v {
                CreateSlotLink::NotALink => "NotALink",
                CreateSlotLink::Link(_) => "Link",
                CreateSlotLink::Unknown(_) => "Unknown",
            }
        }
        let p = Path::new("/tmp/slot.bin");

        assert_eq!(
            arm(&create_slot_link_from_stat(&Ok(true), p)),
            "Link",
            "a confirmed link is a POLICY verdict: the skipping sites are entitled to drop that entry"
        );
        assert_eq!(
            arm(&create_slot_link_from_stat(&Ok(false), p)),
            "NotALink",
            "a real entry is the occupancy half's business, not this one's"
        );
        assert_eq!(
            arm(&create_slot_link_from_stat(&Err(std::io::Error::from(std::io::ErrorKind::NotFound)), p)),
            "NotALink",
            "a free name is the ordinary case — refusing here would break every create in the app"
        );
        for kind in [
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::TimedOut,
            std::io::ErrorKind::Other,
        ] {
            let v = create_slot_link_from_stat(&Err(std::io::Error::new(kind, "Access is denied.")), p);
            assert_eq!(
                arm(&v),
                "Unknown",
                "an lstat that failed with {kind:?} is an I/O FAILURE, not a confirmed link. Classifying \
                 it as `Link` reinstates CPE-1733's UAT finding 6 exactly: the per-entry extraction loops \
                 treat it as a policy skip, drop the file silently, and report the extraction as a \
                 success — the silent-success shape this whole ticket family is about. `Unknown` is what \
                 makes those loops abort like every other I/O failure in them."
            );
            let CreateSlotLink::Unknown(msg) = v else { unreachable!() };
            assert!(
                msg.contains("could not check whether") && msg.contains("nothing was written"),
                "and it must carry the could-not-check wording, not the link wording — a user told \
                 \"it is a link\" would go looking for a link that may not be there: {msg}"
            );
        }
    }

    /// CPE-1718: the composed guard puts the **link** half first, the opposite of
    /// [`rename_slot_refusal`] — and an ordinary occupied name must still get the site's own wording,
    /// which is the thing that ordering could plausibly have broken.
    #[test]
    fn create_slot_refusal_keeps_the_sites_own_wording_for_an_ordinary_occupant() {
        let d = std::env::temp_dir().join(format!("cpe1718-order-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let taken = d.join("taken.bin");
        std::fs::write(&taken, b"KEEP ME").unwrap();
        assert_eq!(
            create_slot_refusal(&taken, "taken.bin: already exists"),
            Some("taken.bin: already exists".to_string()),
            "a regular file answers Ok(false) to the link question and must fall through to the site's \
             own message"
        );
        assert_eq!(
            create_slot_refusal(&d.join("free.bin"), "unused"),
            None,
            "a free name must still be usable — a guard that refused everything would be as broken as \
             one that followed links"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **The leg that makes the link-first ordering evidence rather than a sentence** (PR #901 review).
    ///
    /// `create_slot_refusal` checks the link question *before* occupancy — the opposite of
    /// [`rename_slot_refusal`] — and the module doc argues for twenty lines that this is deliberate. It
    /// was pinned by nothing: swapping the order to match `rename_slot_refusal` **redded not one test**
    /// out of 2143.
    ///
    /// The reason is structural. Under **either** ordering a *dangling* link reaches the link classifier,
    /// because `try_exists` answers `Ok(false)` for it, occupancy returns `Free`, and it falls through.
    /// Every other CPE-1718 test stages a dangling link. **The ordering only changes behaviour for a
    /// *live* link**, and there was no live-link leg anywhere in the PR — so the one case the argument is
    /// about was the one case untested.
    ///
    /// Measured both ways by the reviewer:
    ///
    /// ```text
    /// link-first (as shipped): "…rebuilt.bin" is a link, and creating a file at a link's name writes
    ///                          THROUGH it — the bytes would land at the link's target…
    /// occupancy-first:         rebuilt.bin: already exists — refusing to overwrite
    /// ```
    ///
    /// That second message is exactly what this module's own table calls out as *"sends the user to
    /// delete a file at a name that actually holds a link to somewhere else"* — and it is the same
    /// failure PR #899's reviewer measured at the rename site, recorded at `fsutil.rs`'s
    /// occupancy-first comment. **The repo has been bitten by this ordering once already**; the fix for
    /// it should not ship undefended.
    #[test]
    fn a_live_link_at_a_create_slot_is_reported_as_a_link_not_as_already_exists() {
        let d = std::env::temp_dir().join(format!("cpe1718-live-order-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let real = d.join("real-target.bin");
        std::fs::write(&real, b"VICTIM ORIGINAL").unwrap();
        let slot = d.join("slot.bin");

        // A *live* file symlink is the one staging this repo cannot fake: a junction is directory-only
        // and a hard link is `is_symlink() == false` (both measured on CPE-1716). Per CPE-1717 a leg that
        // cannot stage where it is supposed to work goes red rather than green.
        let staged = {
            #[cfg(windows)]
            {
                std::os::windows::fs::symlink_file(&real, &slot).is_ok()
            }
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&real, &slot).is_ok()
            }
        };
        if !require_staged("live_file_symlink", true, staged) {
            // CI red is the consequential half and `require_staged` handles it — but Evidence Rule 3 says
            // a visible notice is the floor, not the goal, and this leg had no floor. On an unprivileged
            // local Windows machine it returned **silently green**, over zero coverage of the exact case
            // it exists for — and that contributor is precisely the audience `classify_create_slot`'s
            // pure-classifier split was designed for. Its four siblings in this PR all announce; this one
            // did not. (PR #901 review.)
            crate::skip_notice!(
                "[CPE-1718] SKIPPED the live-link ordering leg: this machine could not create a file \
                 symlink at {} (a junction is directory-only and a hard link is not a symlink, so \
                 neither can stand in). NOTHING in this test covered the link-first ordering on this \
                 run — the dangling-link legs pass under either ordering.",
                slot.display()
            );
            let _ = std::fs::remove_dir_all(&d);
            return;
        }
        assert!(
            std::fs::symlink_metadata(&slot).unwrap().file_type().is_symlink(),
            "staging must produce a link, or this test proves nothing"
        );

        let msg = create_slot_refusal(&slot, "slot.bin: already exists")
            .expect("a live link at a create slot must be refused, not written through");
        assert!(
            msg.contains("is a link"),
            "a LIVE link must be reported AS a link — occupancy-first would say \"already exists\" and \
             send the user to delete a name that actually holds a link elsewhere: {msg}"
        );
        assert!(
            !msg.contains("already exists"),
            "and it must not fall through to the site's occupancy wording: {msg}"
        );
        assert_eq!(
            std::fs::read(&real).unwrap(),
            b"VICTIM ORIGINAL".to_vec(),
            "nothing may touch the link's target"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn epoch_ms_is_monotonic_for_later_times() {
        use std::time::Duration;
        let later = UNIX_EPOCH + Duration::from_millis(1_500);
        assert_eq!(to_epoch_ms(later), Some(1_500));
    }

    #[test]
    fn unix_to_rfc3339_matches_known_dates() {
        assert_eq!(unix_to_rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(unix_to_rfc3339(-1), "1969-12-31T23:59:59Z");
        assert_eq!(unix_to_rfc3339(1_700_000_000), "2023-11-14T22:13:20Z");
    }

    /// CPE-1731. [`same_place`] answers the *property* — "does this destination name the served
    /// root?" — for a destination resolved the way the FTP and SFTP rigs resolve one: trim the leading
    /// `/`, then join the remainder onto the root. Both of those resolvers are `#[cfg(test)]` code in
    /// other crates, so the shape is reproduced here rather than imported; that it *is* the same shape
    /// is what the rigs' own wire tests check.
    ///
    /// The rows are **regression pins, not a specification**. CPE-1726 shipped a table like this three
    /// times and the UAT falsified each one — the property is what closes the family, and a table only
    /// records which members have already been observed escaping. Adding a row is welcome; a row is
    /// never the fix.
    #[test]
    fn cpe_1731_same_place_answers_the_property_for_rig_resolved_destinations() {
        fn rig_resolve(root: &Path, wire: &str) -> PathBuf {
            let rel = wire.trim_start_matches('/');
            // `cpe-sftp`'s resolver maps `.` to the root as well; `cpe-ftp`'s only maps empty. Both
            // spellings are covered below either way, since `root.join(".")` is still the root.
            if rel.is_empty() { root.to_path_buf() } else { root.join(rel) }
        }

        let root = std::env::temp_dir().join(format!("cpe-1731-same-place-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("sub").join("nested.txt"), b"deep").unwrap();

        // (wire path, is it the served root?, which family / why)
        let cases: &[(&str, bool, &str)] = &[
            // ── family 0: the shapes CPE-1726's rounds 1–2 were about.
            ("", true, "no argument at all — `RNTO` with nothing after it"),
            ("/", true, "a bare slash"),
            ("//", true, "two slashes — survived round 2's pre-trim filter"),
            ("///", true, "three slashes"),
            (".", true, "a bare dot"),
            // ── family 1: `.`-and-`/` spellings the round-3 denylist let through.
            ("/.", true, "`/.` — trims to `.`"),
            ("/./", true, "`/./` — trims to `./`, which is neither denied literal"),
            ("/.//", true, "`/.//` — a CurDir component then an empty one"),
            ("//./", true, "`//./` — leading empty component before the dot"),
            ("/./.", true, "`/./.` — two CurDir components"),
            ("//.", true, "`//.` — slashes then a dot"),
            // ── family 2: `..` landing ON the root, which round 4's lexical comparison let through
            // by deliberately preserving `..`.
            ("/nonexistent/..", true, "`..` popping a name that never existed"),
            ("/sub/..", true, "`..` popping a real subdirectory"),
            ("/./sub/../.", true, "`..` and `.` mixed, still the root"),
            // ── the other direction: over-rejection is a bug too. A guard that refuses everything
            // would satisfy every row above and break every legitimate rename.
            ("/renamed.txt", false, "an ordinary new name at the top level"),
            ("/sub/deeper.txt", false, "an ordinary new name inside a subdirectory"),
            ("/sub", false, "an existing subdirectory is NOT the root"),
            (
                "/../x",
                false,
                "a `..` with nothing to pop is KEPT — this is the CPE-1730 escape, deliberately not \
                 this guard's subject, and reporting it as the root would misattribute it",
            ),
        ];

        for (wire, want, why) in cases {
            assert_eq!(
                same_place(&rig_resolve(&root, wire), &root),
                *want,
                "[{why}] same_place({wire:?}) — resolved to {:?}",
                rig_resolve(&root, wire)
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The served root spelled in a way **only the filesystem** knows is the same place.
    ///
    /// Windows matches names case-insensitively and strips trailing dots, while `PathBuf` equality
    /// compares `Component::Normal` byte-wise — so this is the row that `normalise_lexically` alone
    /// cannot answer, and the reason [`same_place`] consults `canonicalize` at all. Removing the
    /// `canonicalize` half turns exactly this test red and leaves the lexical one above green.
    ///
    /// **Windows-only, and measured rather than assumed.** On Linux the equivalent spellings are
    /// genuinely different places (case-sensitive, no trailing-dot stripping) *and* unreachable through
    /// the rigs' resolvers, which trim the leading `/` and turn an absolute path into a relative one
    /// landing inside the root — measured under WSL: `same_place("/tmp/<root>") = false`, because it
    /// resolved to `<root>/tmp/<root>`. There is nothing to catch there.
    #[cfg(windows)]
    #[test]
    fn cpe_1731_same_place_catches_spellings_only_the_filesystem_calls_equal() {
        let root = std::env::temp_dir().join(format!("cpe-1731-spelling-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let literal = root.to_string_lossy().to_string();

        for (spelling, why) in [
            (literal.to_uppercase(), "upper-cased — Windows matches names case-insensitively"),
            (format!("{literal}."), "a trailing dot — Windows strips it during path processing"),
        ] {
            // Resolved the way the rigs resolve it: an absolute Windows path has no leading `/` to
            // trim and `join` discards the base, so this arrives as the spelling itself.
            let dest = root.join(&spelling);
            assert!(
                same_place(&dest, &root),
                "[{why}] {spelling:?} names the served root, so a rename onto it is a rename onto the \
                 root. Byte-wise path equality does not know that — which is why the check consults \
                 the filesystem"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sha256_matches_known_vector() {
        // SHA-256("hello") — a fixed vector so the hex formatting is pinned.
        let dir = std::env::temp_dir().join(format!("cpe-fsutil-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("h.txt");
        std::fs::write(&f, b"hello").unwrap();
        assert_eq!(
            sha256_file(&f).unwrap(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
