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
    classify_create_slot(&std::fs::symlink_metadata(target).map(|m| m.file_type().is_symlink()), target)
        .or_else(|| clobber_refusal(target, occupied))
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
/// - **Permissions/ownership of the original are not carried onto the replacement.** Unchanged from the
///   open-coded save this replaces; noted rather than silently inherited.
/// - **Durability across a power loss is NOT provided, only atomic visibility** (PR #899 Reviewer). The
///   bytes are `sync_all`ed before the rename, so no observer can ever see a half-written file and a
///   failed save leaves the original exactly as it was. The **directory entry** the rename creates is a
///   different question and is not synced: closing that needs a parent-directory `sync_all` on Unix and
///   `MOVEFILE_WRITE_THROUGH` on Windows. `vault_manager::sync_parent_dir` does the Unix half for the
///   vault and its own comment records that Windows is only *narrowed*, not closed — so adding the Unix
///   call here would buy a guarantee this crate cannot state platform-uniformly. The user docs are scoped
///   to what is actually provided instead of claiming power-cut safety.
/// - **TOCTOU.** The link is resolved and then written; nothing is atomic across those two steps.
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
pub fn create_exclusive(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new().write(true).create_new(true).open(path)
}

fn stage_and_replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let target = resolve_write_target(path)?;
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
    // [`create_exclusive`] — **this** function's opener, not a copy of it. The temp name carries
    // pid+nanos, so racing the real one is not the way to test it; extracting the opener is.
    {
        use std::io::Write as _;
        let mut f = create_exclusive(&tmp).map_err(|e| format!("{}: {e}", display_path(&tmp)))?;
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
    #[allow(clippy::disallowed_methods)]
    std::fs::rename(&tmp, &target).map_err(|e| {
        let _ = std::fs::remove_file(&tmp); // never leave the temp behind on a failed rename
        format!("{}: {e}", display_path(&target))
    })
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
            // two alternatives — inventing the target, or renaming over the link — are respectively a
            // surprise and the CPE-1716 bug.
            Err(e) => Err(format!(
                "\"{}\" is a link and what it points at could not be opened, so nothing was written — \
                 editing it would have destroyed the link and left the edit nowhere: {e}",
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

fn make_dangling_link_inner(link: &Path) -> bool {
    let missing = link.with_file_name(format!(
        "{}-target-that-does-not-exist",
        link.file_name().unwrap_or_default().to_string_lossy()
    ));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_ms_of_unix_epoch_is_zero() {
        assert_eq!(to_epoch_ms(UNIX_EPOCH), Some(0));
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

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-fsutil-{}-{}-{}", tag, std::process::id(), n));
        std::fs::create_dir_all(&d).unwrap();
        d
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
    /// choice is load-bearing rather than decorative. Swapping `create_new(true)` for `create(true).truncate(true)` at [`replace_file_contents`]'s call site reds THIS test, because it calls [`create_exclusive`] -- that function's own opener -- rather than a copy of it.
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
        // **`create_exclusive` is the opener `replace_file_contents` actually uses.** The first version
        // of this test built its own `OpenOptions` closure here, so swapping `create_new(true)` for
        // `create(true).truncate(true)` at the call site left this green — a test named after a guard,
        // passing with the guard removed. Call the real one.
        let staged = create_exclusive;

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
