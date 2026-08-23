//! Shared filesystem model types used across the explorer's commands (CPE-815): the directory-listing
//! [`DirEntry`], the `list_dir` command's [`ListDirResult`] envelope (CPE-1708), the Properties-dialog
//! [`EntryInfo`], the sidebar [`Place`], and the bulk-operation [`OpResult`], plus the pure
//! `extension_of` / `is_hidden` helpers. Pure and Tauri-free; re-exported into the app so its many
//! construction/usage sites resolve unchanged.

use std::fs;
use std::path::Path;

use serde::Serialize;

/// One entry in a directory listing. Fields serialize by name to match the frontend `DirEntry`.
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    /// Last-modified time as milliseconds since the Unix epoch. `None` when the platform or filesystem
    /// does not report one.
    pub modified: Option<u64>,
    /// Lowercased file extension without the dot ("png"), empty for directories and extensionless files.
    pub extension: String,
    /// Hidden per the OS convention: the hidden attribute on Windows, a leading dot on POSIX.
    pub hidden: bool,
    /// True when the entry itself is a symbolic link (not a link *target* check — the target is resolved
    /// lazily by the frontend on badge render, CPE-1208). Sourced from the entry's own `file_type()`
    /// (which does not follow the link), never from a following `metadata()` call, so listing a folder
    /// costs no extra syscall per entry whether or not it contains links (epic CPE-715).
    pub is_symlink: bool,
}

/// The `list_dir` command's response (CPE-1708): the entries to render, plus how many were left out
/// because their name could not be shown safely. `filtered` reaches the frontend as this typed field —
/// never as a synthetic row mixed into `entries`. CPE-1704 round 2 tried exactly that (a fake
/// `⚠ N filtered` `DirEntry`) and review found it worse than the silent drop it replaced: a REAL object
/// can be named the marker's own text (nothing stops it, and the marker's own name was itself refused by
/// the same guard, so the only such row a user could ever see WAS an attacker-planted one); the fake
/// entry's `is_dir`/`size` fields were dishonest; and deleting it "succeeded" without deleting anything
/// (S3 `DELETE` of a nonexistent key returns 204). `filtered` here can't be spoofed by anything a server
/// sends — it's computed in-process from what the provider's own listing pass (and the shared name
/// guard) actually dropped; see `cpe_vfs::connect::RemoteListing` for the source of the count on the
/// remote side.
///
/// Always `0` for a local listing and for a remote backend whose listing needs no filtering (SFTP,
/// WebDAV, FTP) — see `FileSystemProvider::list_with_filtered_count`'s default, which delegates to
/// `list` and reports `0`. Only a backend with its own keyspace rule (e.g. `cpe-s3`, whose `:`-bearing
/// keys are legal but an embedded `/`/literal `..` genuinely is not) can ever produce a non-zero count.
///
/// `unreadable` (CPE-1780) is a DIFFERENT fact from `filtered` above and is never added to it: `filtered`
/// means "a remote provider refused to show this name at all" (the name is never even seen); `unreadable`
/// means "the local walk saw this row but could not stat it" (`crates/server/src/listing.rs`'s
/// `DirWalkStats`) — always `0` for a remote listing, since that failure mode is local-walk-specific (out
/// of scope here; see CPE-1780's ticket notes). Same non-spoofable, typed-field-not-a-synthetic-row
/// convention as `filtered`.
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ListDirResult {
    pub entries: Vec<DirEntry>,
    pub filtered: usize,
    pub unreadable: usize,
}

/// **Why** a bulk-operation entry ended up where it did — the structural discriminant a consumer reads
/// instead of string-matching [`OpResult::error`] (CPE-1845).
///
/// Before this existed, `OpResult` said only `ok: bool`, so a **deliberate, correct, fail-safe
/// hold-back** and a **genuine failure** arrived through the same field carrying the same shape, and the
/// only way to tell them apart was to match the prose prefix `"not deleted:"`. Measured on a staged
/// checkpoint (CPE-1823 round-4 review): `applied=1 skipped=201`, of which **200 were deliberate
/// hold-backs** and one was a real failure. A UI reading `ok` alone reports 201 problems.
///
/// The four states are deliberately the ones a *user-facing decision* turns on:
///
/// | variant | what happened | what the user can do |
/// |---|---|---|
/// | [`Applied`](OpOutcome::Applied) | the operation was performed | nothing |
/// | [`Failed`](OpOutcome::Failed) | it was attempted and it failed | fix the named cause, try again |
/// | [`SkippedByPlan`](OpOutcome::SkippedByPlan) | not attempted — something else in the same run failed, so this one's premise is unproven | fix that, **re-run**: this one then applies |
/// | [`HeldBackByCheckpoint`](OpOutcome::HeldBackByCheckpoint) | not attempted — the *input* cannot be trusted on this platform | **re-running here can never help**; see [`OpOutcome::retryable`] |
///
/// The last two are both "held back", and collapsing them is exactly the bug this type exists to stop:
/// the recorded UI wording *"held back, re-run after fixing"* is right for `SkippedByPlan` and **wrong**
/// for `HeldBackByCheckpoint`, where a Linux capture holding one colon-named file will never
/// delete-clean on Windows no matter how many times it is re-run.
///
/// Serialised `snake_case` so the TS side reads `"applied" | "failed" | "skipped_by_plan" |
/// "held_back_by_checkpoint"` — a discriminated union, not a prefix.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum OpOutcome {
    /// The operation was performed.
    Applied,
    /// The operation was attempted and failed (locked file, permission denied, source gone, a
    /// path-safety refusal of *this* item). Retrying after fixing the named cause is meaningful.
    Failed,
    /// **Deliberately not attempted, retryable.** Something else in the same run failed, so this item's
    /// premise is unproven — fixing that and re-running performs it.
    SkippedByPlan,
    /// **Deliberately not attempted, NOT retryable on this platform.** The checkpoint/manifest driving
    /// the run cannot be read correctly here, and nothing about re-running on this machine changes that.
    /// A consumer must not tell the user to "re-run"; see the accompanying next step.
    HeldBackByCheckpoint,
}

impl OpOutcome {
    /// Of the operations that did **not** happen: can running this again, on this machine, make it
    /// happen? [`HeldBackByCheckpoint`](OpOutcome::HeldBackByCheckpoint) is the one where it cannot —
    /// the whole reason the variant is separate from [`SkippedByPlan`](OpOutcome::SkippedByPlan), and
    /// the reason a consumer must not print "re-run after fixing" for it.
    ///
    /// [`Applied`](OpOutcome::Applied) answers `false` because it already happened and there is nothing
    /// to retry — this is a convenience for phrasing the *unfinished* entries, not the discriminant.
    /// The discriminant is the variant itself; never infer state from this bool alone.
    pub fn retryable(self) -> bool {
        matches!(self, OpOutcome::Failed | OpOutcome::SkippedByPlan)
    }
    /// `true` for the two states that mean "we chose not to do this", as opposed to "we tried and could
    /// not". A held-back item is a *safety* outcome, never an error.
    pub fn is_held_back(self) -> bool {
        matches!(self, OpOutcome::SkippedByPlan | OpOutcome::HeldBackByCheckpoint)
    }
}

/// The subset of [`OpOutcome`] that means "deliberately not attempted" — the only thing
/// [`OpResult::held_back`] accepts.
///
/// This is a type rather than a `debug_assert!` because a `debug_assert!` is compiled out of release
/// (review round 2): `held_back(p, OpOutcome::Applied, …)` would have SHIPPED `ok: false` alongside
/// `outcome: Applied`, with the derivation test passing in CI and the inconsistency reaching users. Now
/// it does not compile.
///
/// Deliberately NOT serialised and NOT a `specta::Type`: it is a constructor parameter, and adding a
/// second enum to the wire for something the wire never carries would be noise in `bindings.gen.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeldBackOutcome {
    /// Retryable — see [`OpOutcome::SkippedByPlan`].
    SkippedByPlan,
    /// Not retryable on this platform — see [`OpOutcome::HeldBackByCheckpoint`].
    HeldBackByCheckpoint,
}

impl HeldBackOutcome {
    /// Widen to the wire enum. Total and infallible in this direction, which is the whole point.
    pub fn as_outcome(self) -> OpOutcome {
        match self {
            HeldBackOutcome::SkippedByPlan => OpOutcome::SkippedByPlan,
            HeldBackOutcome::HeldBackByCheckpoint => OpOutcome::HeldBackByCheckpoint,
        }
    }
    /// Mirrors [`OpOutcome::retryable`] without the widening round trip.
    pub fn retryable(self) -> bool {
        self.as_outcome().retryable()
    }
}

/// Per-item outcome of a bulk operation. Bulk file operations must NOT be all-or-nothing and must not
/// abort on the first failure: if 9 of 10 files copy and one is locked, the user needs to know exactly
/// which one failed.
///
/// `outcome` (CPE-1845) is the field to branch on. `ok` is kept as the one-bit summary every existing
/// caller already reads, and is exactly `outcome == Applied` — it is derived, never set independently.
#[derive(Serialize, Debug)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct OpResult {
    pub path: String,
    pub ok: bool,
    pub error: String,
    /// Structural discriminant (CPE-1845). Branch on this, **never** on `error`'s wording.
    pub outcome: OpOutcome,
}

impl OpResult {
    pub fn ok(path: &Path) -> Self {
        Self {
            path: path.to_string_lossy().to_string(),
            ok: true,
            error: String::new(),
            outcome: OpOutcome::Applied,
        }
    }
    pub fn err(path: &Path, e: impl std::fmt::Display) -> Self {
        Self {
            path: path.to_string_lossy().to_string(),
            ok: false,
            error: e.to_string(),
            outcome: OpOutcome::Failed,
        }
    }
    /// A deliberate hold-back: not attempted, and **not** a failure. `error` carries only what is
    /// specific to *this* path (often empty) — the shared explanation is stated once by the caller's
    /// summary rather than copied per path, which is what CPE-1847 measured at ~185 KB for 500 deletes.
    pub fn held_back(path: &str, outcome: HeldBackOutcome, detail: impl Into<String>) -> Self {
        Self {
            path: path.to_string(),
            ok: false,
            error: detail.into(),
            outcome: outcome.as_outcome(),
        }
    }
}

/// Detailed metadata for the Properties dialog.
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct EntryInfo {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<u64>,
    pub created: Option<u64>,
    pub readonly: bool,
    pub hidden: bool,
}

/// One item sitting in the OS Recycle Bin / Trash (epic CPE-1486, slice 1: browsable in-app Trash).
///
/// This DTO is deliberately `trash`-crate-free: the crate's `os_limited` listing API only exists on
/// Windows + Linux (see `docs/design/SERVER-ARCHITECTURE.md`), so the adapter in `src-tauri/src/lib.rs`
/// owns the `trash` dependency and maps its `TrashItem`/metadata into this plain, cross-platform-safe
/// struct via [`trash_entry`].
#[derive(Serialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TrashEntry {
    /// Platform-specific identifier of the trashed item (Windows: `IShellItem` display name; Linux: the
    /// `.trashinfo` path) — opaque to the frontend, round-tripped back for restore/purge.
    pub id: String,
    pub name: String,
    /// Full original path (parent + name) the item was trashed from.
    pub original_path: String,
    /// Unix seconds since epoch when the item was moved to the trash.
    pub time_deleted: i64,
    /// Size in bytes, when known. `None` for a directory (the `trash` crate reports a directory's own
    /// trash-metadata as an entry count, not a byte size) or when metadata couldn't be read for this item.
    pub size: Option<u64>,
}

/// Pure mapping from plain fields to a [`TrashEntry`] — no `trash` crate dependency, so it's
/// unit-testable in `cpe-server` even off Windows/Linux. The adapter passes `original_parent`/`name`
/// separately (joining them here) because that's the shape `trash::TrashItem` exposes; keeping the join
/// on this side means the join logic itself is covered by a plain unit test.
pub fn trash_entry(
    id: String,
    name: String,
    original_parent: String,
    time_deleted: i64,
    size: Option<u64>,
) -> TrashEntry {
    let original_path = Path::new(&original_parent).join(&name).to_string_lossy().to_string();
    TrashEntry { id, name, original_path, time_deleted, size }
}

/// Result of the collect-to-vec Trash listing (`list_trash`). Wraps `entries` with a `degraded` flag so a
/// listing that could not be fully read is distinguishable, on the frontend, from a genuinely empty
/// Trash (CPE-1803) — the same pair of fields the streamed command reports via [`TrashStreamSummary`].
/// Set by the adapter in `src-tauri/src/lib.rs`, which owns both incompleteness sources; this DTO
/// carries no `trash` crate dependency of its own.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TrashListing {
    pub entries: Vec<TrashEntry>,
    /// True when this pass could not fully read the OS trash — NOT the same as a genuinely empty Trash.
    ///
    /// Deliberately ONE flag for BOTH ways a pass can come back incomplete, because the frontend's
    /// decision ("is what I am about to render the whole truth?") is the same for both:
    /// - `trash::os_limited::list()` panicked and was caught at the boundary, degrading the whole pass
    ///   to zero entries (CPE-1791/CPE-1803);
    /// - one or more individual items were skipped because their id/name/original_parent isn't valid
    ///   UTF-8, so `entries` holds only *some* of what is really in the Trash (CPE-1804).
    ///
    /// The second cause is the reason `degraded` must never be read as "and therefore empty":
    /// degraded-with-entries is the ordinary shape of a partially-decodable Trash (CPE-1805).
    pub degraded: bool,
    /// CPE-1804: how many items this pass dropped because a field wasn't valid UTF-8 — `0` for a clean
    /// pass, and `0` for a caught panic too (that route degrades wholesale rather than per item, so it
    /// has no per-item count to report). Non-zero always implies `degraded`.
    ///
    /// Carried as a count rather than folded into the bool so the UI can say "3 items couldn't be shown"
    /// instead of an unqualified warning — the difference between a user knowing how much is missing and
    /// only knowing that *something* is (the CPE-1704 counting-contract precedent).
    pub skipped: usize,
}

/// Result of the streamed Trash listing (`list_trash_stream`), returned once every batch has gone out
/// over the channel. `count` is the total number of entries streamed; `degraded`/`skipped` are the same
/// incompleteness signals as on [`TrashListing`], carried separately here because the streamed command
/// can't attach them to the (already-sent) entries themselves.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TrashStreamSummary {
    pub count: usize,
    /// See [`TrashListing::degraded`].
    pub degraded: bool,
    /// See [`TrashListing::skipped`].
    pub skipped: usize,
}

/// A sidebar quick-access location (special folder or drive).
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Place {
    /// Display name, e.g. "Documents" or "Local Disk (C:)".
    pub name: String,
    pub path: String,
    /// Logical kind, used by the UI to pick an icon:
    /// "desktop" | "documents" | "downloads" | "pictures" | "music" | "videos" | "drive" | "home".
    pub kind: String,
}

/// Recognized executable/object container formats a [`BinaryInfo`] can describe (CPE-1572, epic
/// CPE-1562 "Binary Inspector" slice 1).
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum BinaryFormat {
    Pe,
    Elf,
    MachO,
}

/// One section/segment entry (name + virtual address/size) in a [`BinaryInfo`] listing.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BinarySection {
    pub name: String,
    /// Virtual address: PE's RVA, ELF's `sh_addr`, Mach-O's `addr`.
    pub address: u64,
    /// Virtual/in-memory size in bytes.
    pub size: u64,
}

/// One imported symbol.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BinaryImport {
    pub name: String,
    /// Owning library/DLL/dylib, when the format ties an individual import to one (PE, Mach-O).
    /// `None` for ELF — its dynamic-symbol table doesn't record a per-symbol source library.
    pub library: Option<String>,
}

/// One exported symbol.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BinaryExport {
    pub name: String,
    /// Virtual address of the export, when known.
    pub address: Option<u64>,
}

/// One entry from a format's own symbol table (ELF `.symtab`, Mach-O's `LC_SYMTAB`). PE carries no
/// equivalent for a typical EXE/DLL (only object files/PDBs do), so [`BinaryInfo::symbols`] is
/// always empty for [`BinaryFormat::Pe`].
#[derive(Serialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BinarySymbol {
    pub name: String,
    pub address: Option<u64>,
}

/// Cap on the number of entries collected into any one [`BinaryInfo`] list (sections / imports /
/// exports / symbols). Guards against a malformed or hostile header claiming an enormous count —
/// without this a crafted section/symbol-count field could drive an unbounded allocation/loop
/// before the pane ever renders anything. Real-world binaries stay far below this.
pub const MAX_BINARY_LIST_ENTRIES: usize = 4096;

/// One decoded machine-code instruction (CPE-1581, epic CPE-1562 "Binary Inspector" slice 2).
#[derive(Serialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BinaryInstruction {
    /// Virtual address of this instruction's first byte.
    pub address: u64,
    /// The instruction's raw encoded bytes, as lowercase hex (e.g. "48 89 e5" — space-separated
    /// byte pairs, matching the conventional disassembler-listing look).
    pub bytes: String,
    /// Formatted mnemonic + operands (e.g. "mov rbp, rsp"), via iced-x86's NASM-style formatter.
    pub text: String,
}

/// Cap on the number of instructions decoded into one [`BinaryInfo::disasm`] list. Guards against a
/// huge or hostile code section driving an unbounded decode loop before the pane ever renders
/// anything — mirrors [`MAX_BINARY_LIST_ENTRIES`]'s role for the other bounded lists. Chosen well
/// above what a single preview view usefully shows; a real disassembly UI would page/stream past
/// this (out of scope this slice — see [`crate::binary_preview::disassemble`]'s doc comment).
pub const MAX_DISASM_INSTRUCTIONS: usize = 2048;

/// Structured summary of a PE/ELF/Mach-O binary (CPE-1572, epic CPE-1562 "Binary Inspector" slice
/// 1): format + architecture, plus bounded Sections/Imports/Exports/Symbols tables. Populated by
/// [`crate::binary_preview::binary_info`] via goblin, with parity across all three formats. Each
/// list is capped at [`MAX_BINARY_LIST_ENTRIES`] and built skip-on-error per entry, so a
/// truncated/adversarial file degrades to a partial (or empty) `BinaryInfo` rather than failing the
/// whole parse.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BinaryInfo {
    pub format: BinaryFormat,
    /// Human-readable CPU architecture label(s) from [`crate::bin_arch::detect_arch`] (e.g.
    /// "x86-64", "ARM64"; a Mach-O fat/universal binary joins each slice's label with ", "). `None`
    /// when the leading bytes didn't decode to a recognized architecture.
    pub arch: Option<String>,
    pub is_64: bool,
    /// `true` when this is a managed .NET/CLR image — a [`BinaryFormat::Pe`] whose data directory
    /// #14 (`IMAGE_COR20_HEADER`, the "CLI header") is present and non-empty (CPE-1596, epic
    /// CPE-1562 "Binary Inspector" slice 3). Always `false` for [`BinaryFormat::Elf`]/[`BinaryFormat::MachO`].
    /// This is the flag that lets a caller tell a managed PE apart from a native one — the concrete
    /// motivation was UAT on CPE-1585 finding that x86 disassembly of `mscorlib.dll` produced 2,048
    /// "instructions" that were really the decoder chewing on CIL bytecode, not native machine code;
    /// a caller can check this flag before trusting `disasm` on a PE. See
    /// [`crate::dotnet_metadata::read`] for the structured CLR metadata itself (assembly identity,
    /// `AssemblyRef`s, `TypeDef`/`MethodDef` names), fetched separately since it's a heavier parse.
    pub is_managed: bool,
    pub sections: Vec<BinarySection>,
    pub imports: Vec<BinaryImport>,
    pub exports: Vec<BinaryExport>,
    pub symbols: Vec<BinarySymbol>,
    /// x86/x64 disassembly of the format's code section (CPE-1581), capped at
    /// [`MAX_DISASM_INSTRUCTIONS`]. Empty (never an error) for a non-x86/x64 architecture, a format
    /// with no locatable code section, or a code section iced-x86 can't decode from.
    pub disasm: Vec<BinaryInstruction>,
}

// ---------------------------------------------------------------------------------------------
// .NET/CLR metadata (CPE-1596, epic CPE-1562 "Binary Inspector" slice 3): a hand-rolled ECMA-335
// reader (`crate::dotnet_metadata`) walks the CLI header + `#~` compressed metadata tables of a
// managed PE. `dotnetdll` is GPL and cannot be used, so these DTOs and their parser are hand-rolled
// against the ECMA-335 spec — see that module's doc comment for the format walkthrough.
// ---------------------------------------------------------------------------------------------

/// The managed assembly's own identity, from the single-row `Assembly` table (ECMA-335 II.22.2).
#[derive(Serialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DotnetAssemblyIdentity {
    pub name: String,
    /// "Major.Minor.Build.Revision", e.g. "4.0.0.0".
    pub version: String,
    /// `None` for the neutral culture (an empty string in the `#Strings` heap).
    pub culture: Option<String>,
    /// The `PublicKey` blob, hex-encoded. `None` when the assembly isn't strong-named (an empty
    /// blob). This is the raw public key blob, not the derived 8-byte "public key token" (deriving
    /// that requires a SHA-1 hash, and this reader adds no new crypto dependency for it).
    pub public_key: Option<String>,
    /// Raw `Flags` column (ECMA-335 II.23.1.2 `AssemblyFlags`), exposed unparsed so a caller can
    /// decode bits like `PublicKey` (0x0001) or `Retargetable` (0x0100) as needed.
    pub flags: u32,
}

/// One row of the `AssemblyRef` table (ECMA-335 II.22.5): a managed assembly this one references.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DotnetAssemblyRef {
    pub name: String,
    pub version: String,
    pub culture: Option<String>,
    /// The `PublicKeyOrToken` blob, hex-encoded — usually the compact 8-byte token (e.g.
    /// `b77a5c561934e089` for `mscorlib`), occasionally a full public key when the `PublicKey` flag
    /// bit is set. `None` when the blob is empty (unsigned reference).
    pub public_key_token: Option<String>,
}

/// One row of the `TypeDef` table (ECMA-335 II.22.37), name-only (CPE-1596's scope stops at
/// "legible before the full decompile epic lands" — no field/method-list resolution here).
#[derive(Serialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DotnetTypeDef {
    pub name: String,
    /// Empty for a type in the global namespace.
    pub namespace: String,
}

/// One row of the `MethodDef` table (ECMA-335 II.22.26), name-only.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DotnetMethodDef {
    pub name: String,
}

/// Structured CLR metadata for a managed PE (CPE-1596), populated by [`crate::dotnet_metadata::read`]
/// via a hand-rolled ECMA-335 `#~` table-stream walk. `assembly_refs`/`types`/`methods` are each
/// capped at [`MAX_BINARY_LIST_ENTRIES`] and built skip-on-error/skip-on-overflow, so a
/// truncated/adversarial assembly degrades to a partial (or empty) result rather than failing the
/// whole read — mirroring [`BinaryInfo`]'s own contract.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DotnetMetadata {
    /// The metadata root's version string (ECMA-335 II.24.2.1), e.g. "v4.0.30319" — the CLR runtime
    /// this assembly was compiled against, not necessarily the one running it.
    pub runtime_version: String,
    /// `None` when the single-row `Assembly` table is genuinely absent from an otherwise-located
    /// `#~`/`#-` tables stream — a real module/netmodule, not an assembly manifest. (If the tables
    /// stream itself couldn't be located/parsed at all, [`crate::dotnet_metadata::read`] reports that
    /// honestly as `Ok(None)` at the outer `Option<DotnetMetadata>` level instead — see its doc
    /// comment — rather than this field alone standing in for "nothing was found here".)
    pub assembly: Option<DotnetAssemblyIdentity>,
    pub assembly_refs: Vec<DotnetAssemblyRef>,
    pub types: Vec<DotnetTypeDef>,
    pub methods: Vec<DotnetMethodDef>,
}

/// Detailed metadata for the Properties dialog: name/size/dir + modified/created (epoch-ms) + the
/// readonly/hidden flags. A missing/unreadable path is an `Err`.
pub fn entry_info(path: &str) -> Result<EntryInfo, String> {
    let p = Path::new(path);
    let meta = fs::metadata(p).map_err(|e| format!("{path}: {e}"))?;
    Ok(EntryInfo {
        name: p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string()),
        path: path.to_string(),
        is_dir: meta.is_dir(),
        size: if meta.is_dir() { 0 } else { meta.len() },
        modified: meta.modified().ok().and_then(crate::fsutil::to_epoch_ms),
        created: meta.created().ok().and_then(crate::fsutil::to_epoch_ms),
        readonly: meta.permissions().readonly(),
        hidden: is_hidden(p, &meta),
    })
}

/// Lowercased extension without the dot; empty when there is none.
pub fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default()
}

/// Hidden per OS convention: the `FILE_ATTRIBUTE_HIDDEN` bit on Windows, a leading dot on POSIX.
pub fn is_hidden(path: &Path, meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        if meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0 {
            return true;
        }
    }
    #[cfg(not(windows))]
    {
        let _ = meta;
    }
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('.'))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_of_lowercases_and_handles_none() {
        assert_eq!(extension_of(Path::new("/a/b/Photo.PNG")), "png");
        assert_eq!(extension_of(Path::new("/a/b/archive.tar.gz")), "gz");
        assert_eq!(extension_of(Path::new("/a/b/README")), "");
    }

    #[test]
    fn entry_info_reports_metadata_and_errors_on_missing() {
        let dir = std::env::temp_dir().join(format!("cpe-entryinfo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("x.txt");
        std::fs::write(&f, b"hello").unwrap();
        let info = entry_info(&f.to_string_lossy()).unwrap();
        assert_eq!(info.name, "x.txt");
        assert!(!info.is_dir && info.size == 5);
        assert!(entry_info(&dir.join("nope").to_string_lossy()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn trash_entry_joins_parent_and_name_into_original_path() {
        let e = trash_entry(
            "id-123".into(),
            "photo.png".into(),
            "/home/user/Pictures".into(),
            1_700_000_000,
            Some(4096),
        );
        assert_eq!(e.id, "id-123");
        assert_eq!(e.name, "photo.png");
        assert_eq!(
            Path::new(&e.original_path),
            Path::new("/home/user/Pictures").join("photo.png")
        );
        assert_eq!(e.time_deleted, 1_700_000_000);
        assert_eq!(e.size, Some(4096));
    }

    #[test]
    fn trash_entry_size_is_optional() {
        // Directories (and items whose per-item metadata couldn't be read) carry no byte size.
        let e = trash_entry("id-456".into(), "Old Folder".into(), "/home/user".into(), 42, None);
        assert_eq!(e.size, None);
    }

    #[test]
    fn op_result_constructors() {
        let ok = OpResult::ok(Path::new("/x/y.txt"));
        assert!(ok.ok && ok.error.is_empty());
        assert_eq!(ok.outcome, OpOutcome::Applied);
        let err = OpResult::err(Path::new("/x/y.txt"), "locked");
        assert!(!err.ok && err.error == "locked");
        assert_eq!(err.outcome, OpOutcome::Failed);
    }

    /// CPE-1845 — `ok` must stay exactly `outcome == Applied`, on every variant. It is the one-bit
    /// summary the whole codebase already reads, and if the two ever disagree a caller reading `ok`
    /// and a caller reading `outcome` describe the same run differently.
    #[test]
    fn ok_is_derived_from_outcome_and_never_set_independently() {
        for (r, expect) in [
            (OpResult::ok(Path::new("/x")), OpOutcome::Applied),
            (OpResult::err(Path::new("/x"), "e"), OpOutcome::Failed),
            (
                OpResult::held_back("/x", HeldBackOutcome::SkippedByPlan, ""),
                OpOutcome::SkippedByPlan,
            ),
            (
                OpResult::held_back("/x", HeldBackOutcome::HeldBackByCheckpoint, ""),
                OpOutcome::HeldBackByCheckpoint,
            ),
        ] {
            assert_eq!(r.outcome, expect);
            assert_eq!(r.ok, r.outcome == OpOutcome::Applied, "ok disagrees with outcome: {r:?}");
        }
    }

    /// CPE-1845 — the whole point of the split: **only** `HeldBackByCheckpoint` is non-retryable, and a
    /// hold-back is never a failure. A UI decides between "re-run after fixing" and "re-running cannot
    /// help" off `retryable()` alone.
    #[test]
    fn only_the_checkpoint_hold_back_is_non_retryable() {
        // Applied is `false` because it already happened — see the method's own doc. The claim under
        // test is about the two hold-backs: one is retryable and one is not.
        assert!(!OpOutcome::Applied.retryable());
        assert!(OpOutcome::Failed.retryable());
        assert!(OpOutcome::SkippedByPlan.retryable());
        assert!(!OpOutcome::HeldBackByCheckpoint.retryable());

        assert!(!OpOutcome::Applied.is_held_back());
        assert!(!OpOutcome::Failed.is_held_back());
        assert!(OpOutcome::SkippedByPlan.is_held_back());
        assert!(OpOutcome::HeldBackByCheckpoint.is_held_back());
    }

    /// CPE-1845 — the four states must be four distinct **wire** tokens. A consumer written against the
    /// JSON (the TS bindings, `bindings.gen.ts`) sees only these strings; if two variants serialised the
    /// same, the discriminant would be a discriminant in Rust and a prefix-match everywhere else.
    #[test]
    fn the_four_outcomes_serialise_to_four_distinct_tokens() {
        let tokens: Vec<String> = [
            OpOutcome::Applied,
            OpOutcome::Failed,
            OpOutcome::SkippedByPlan,
            OpOutcome::HeldBackByCheckpoint,
        ]
        .iter()
        .map(|o| serde_json::to_string(o).unwrap())
        .collect();
        assert_eq!(
            tokens,
            vec![
                "\"applied\"",
                "\"failed\"",
                "\"skipped_by_plan\"",
                "\"held_back_by_checkpoint\""
            ]
        );
        let unique: std::collections::HashSet<&String> = tokens.iter().collect();
        assert_eq!(unique.len(), 4, "two outcomes collapsed onto one wire token: {tokens:?}");
    }

    #[test]
    fn is_hidden_by_dot_on_posix_paths() {
        let dir = std::env::temp_dir().join(format!("cpe-model-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // On POSIX the leading dot marks a file hidden; on Windows the dot alone doesn't, so only
        // assert the leading-dot direction where it holds.
        #[cfg(not(windows))]
        {
            let dotfile = dir.join(".secret");
            std::fs::write(&dotfile, b"x").unwrap();
            let meta = std::fs::metadata(&dotfile).unwrap();
            assert!(is_hidden(&dotfile, &meta));
        }
        // A plain file is never hidden on any platform.
        let plain = dir.join("plain.txt");
        std::fs::write(&plain, b"x").unwrap();
        assert!(!is_hidden(&plain, &std::fs::metadata(&plain).unwrap()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
