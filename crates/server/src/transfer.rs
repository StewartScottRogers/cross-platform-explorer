//! Provider-agnostic recursive walk + tree transfer (CPE-684/905, epic CPE-616). These operate over the
//! [`FileSystemProvider`] trait, so **every** backend — local disk, SFTP, WebDAV, … — gets a cancellable
//! recursive walk and bidirectional (remote⇄local) tree copy for free, with the logic living once here
//! instead of duplicated per provider.
//!
//! Paths are the provider's own convention (`/`-separated for remote backends; an empty `root` means the
//! provider's root). Every step checks a `cancel` flag so a slow/large enumeration or transfer stops
//! promptly.

use crate::provider::FileSystemProvider;
use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// What [`download_tree`] delivered, plus what a per-entry guard skipped on purpose and why (CPE-1881).
///
/// Before this existed, [`download_tree`] returned a bare `usize` — files written — and the CPE-1857
/// hard-link leaf guard (and its pre-existing-symlink-leaf neighbour) reported a skip with `eprintln!`
/// and nothing else: the caller saw the delivered count silently one lower, with no `undelivered` entry,
/// no reason, no count. Measured by the independent Security Auditor on PR #1016: the line went to
/// stderr and `n == 0` was the only signal reaching the caller at all.
///
/// **This is deliberately NOT [`RestoreReport`](crate::revert_engine::RestoreReport)'s shape.** A
/// grouped write refusal earns a shared paragraph there because the *same* checkpoint tree can produce
/// thousands of them from one `rsync --link-dest`-style hard-linked source. A transfer's hard-link skip
/// is a per-entry **policy** decision by a *gate that never wrote anything* (see `download_tree`'s doc),
/// so the plainer [`ArchiveReport`](crate::archive::ArchiveReport)-style "count implied by `skipped.len()`
/// plus one reason string per entry" shape this ticket also asks for is the right fit — and it is never
/// truncated: every skip is a real one, and dropping any of them silently would be the exact defect this
/// ticket exists to close, worn as a different hat.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DownloadReport {
    /// Files actually written to `local_dir`.
    pub files: usize,
    /// Entries a per-entry guard refused to write — a pre-existing hard link or symlink at the local leaf
    /// name — each as `"{remote path}: {reason}"`. Neither delivered nor a delivery failure: **not**
    /// writing is the correct, safe outcome (see [`download_tree`]'s doc on why these do not go through
    /// `undelivered`), but until CPE-1881 the only trace was a line on stderr. Never truncated.
    pub skipped: Vec<String>,
}

/// One entry yielded by [`walk`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkEntry {
    /// Full path within the provider.
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// Maximum directory depth [`walk`] will descend into (CPE-1462). A hostile server can advertise an
/// infinitely deep tree — one fresh child directory per `readdir` — which would grow the DFS stack (and,
/// for [`download_tree`], the accumulated work) without bound. A legitimate remote tree is only ever a few
/// dozen levels deep, so 100 sits far above anything real while firmly bounding recursion. Reaching it
/// stops descent into *deeper* directories (surfaced as a stderr notice) rather than failing the whole
/// transfer, matching the repo's skip-on-error ethos for enumeration.
pub const MAX_WALK_DEPTH: usize = 100;

/// Maximum total entries [`walk`] will visit (CPE-1462). A hostile server can advertise millions of
/// entries — by breadth (one directory with millions of children) or depth — to exhaust memory/time on an
/// unattended transfer. Hundreds of thousands comfortably covers any legitimate large tree; exceeding it
/// aborts the walk with a surfaced error, because a bounded, failed transfer is vastly preferable to an
/// OOM or an indefinite hang.
pub const MAX_WALK_ENTRIES: usize = 500_000;

/// Whether a provider-supplied entry **name** is a safe single path segment (CPE-1461, source-side
/// defense). A name is a leaf, never a path: it must be non-empty, not `.`/`..`, contain neither a `/`
/// nor `\` separator nor a NUL, and be a single *normal* path component (rejecting a bare drive like
/// `C:`, a root, or any other prefix). Remote providers (SFTP `READDIR` filenames, WebDAV `href`
/// segments) call this to drop a hostile name at the SOURCE, before it can ever reach the local-write
/// sink in [`download_tree`]. A directory entry with an unsafe name is skipped entirely.
pub fn is_safe_name(name: &str) -> bool {
    if name.is_empty() || name == "." || name == ".." {
        return false;
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        return false;
    }
    // Reject a Windows NTFS alternate-data-stream / drive selector anywhere in the leaf (`x:y`,
    // `..::$DATA`, `file:stream`) — a `:` in a name is meaningless on Unix and dangerous on Windows,
    // so fail closed. Also reject a leaf that *begins* with `..` (`..stream`, `..:$DATA`), which the
    // single-component check below would otherwise accept as `Normal` (CPE-1461 hardening).
    if name.contains(':') || name.starts_with("..") {
        return false;
    }
    // Exactly one normal component: rejects a bare drive (`C:`), a root, or any prefix, which
    // `Path::components` classifies as non-`Normal`.
    let mut comps = Path::new(name).components();
    matches!((comps.next(), comps.next()), (Some(Component::Normal(_)), None))
}

/// Characters a Windows path cannot carry in a leaf name (CPE-1709). `/` and `\` are absent on purpose —
/// [`guarded_join`] has already split the path on both, so they can never appear inside a segment.
///
/// Measured behaviour of each, written through the real sink (`download_tree` → `guarded_join` →
/// `fs::write`) on Windows 11 Pro 26200, NTFS:
/// - `:` — **the reported bug.** The write *succeeds* and the bytes go into an NTFS alternate data
///   stream: `colon:name.txt` leaves a **0-byte** file named `colon` plus a hidden stream
///   `colon:name.txt:$DATA` holding the content (`dir /r` shows both). Silent total loss to the user.
/// - `< > " | ? *` and control characters `U+0000`–`U+001F` — `CreateFileW` refuses them outright (os
///   error 123). The entry was already skipped before this fix, but only *accidentally* and with a
///   misleading notice: the CPE-1696 leaf `symlink_metadata` probe was the call that failed, so the
///   transfer reported "could not be inspected for a pre-existing symlink". Encoding them turns a
///   wrongly-diagnosed skip into a correct download.
const WINDOWS_UNSAFE_CHARS: &[char] = &['<', '>', ':', '"', '|', '?', '*'];

/// Unicode bidi/format control characters (CPE-1712) — **deliberately absent** from
/// [`WINDOWS_UNSAFE_CHARS`] and from every pass of [`windows_safe_segment`]. Listed here, and covered
/// by `cpe_1712_bidi_format_chars_are_never_rewritten_on_disk`, so the omission is a recorded decision
/// rather than an accident waiting to be "fixed" by someone assuming it is another CPE-1709 gap.
///
/// `U+202E RIGHT-TO-LEFT OVERRIDE` is the reported case: a remote leaf `\u{202E}gnp.txt` downloads
/// **byte-intact** and Windows Explorer renders it as `txt.png`, because RLO tells the bidi renderer to
/// draw everything after it right-to-left. It is Unicode category **Cf** (format), not **Cc**
/// (control), so `c.is_control()` — the exact predicate every pass here uses — never sees it. The FULL
/// `Bidi_Control=Yes` set, all **twelve** code points, enumerated rather than stopping at the reported
/// character (this is CPE-1709's own lesson, stated explicitly in the ticket that opened this constant
/// — and the exact trap round 2 of the CPE-1712 review caught here: the first cut had 11 of 12, missing
/// `U+061C ARABIC LETTER MARK`): the five embeddings/overrides `U+202A`–`U+202E`, the four isolates
/// `U+2066`–`U+2069`, the two directional marks `U+200E`/`U+200F`, and `U+061C`.
///
/// **The decision, and why it differs from CPE-1709's:** CPE-1709 rewrote `:`, control characters, a
/// trailing dot/space, and the reserved device names because the local filesystem or an ordinary Win32
/// application **could not otherwise hold or open the file** — the transform was *compelled* by a real
/// write failure or an unreachable name. None of that is true here: every one of these code points is
/// legal on NTFS, ext4, and APFS; `download_tree` writes them without incident and the file opens fine
/// by its real name. Nothing forces a rewrite.
///
/// Rewriting anyway would trade a display bug for a **data-mangling** one: a real Hebrew or Arabic
/// filename can legitimately carry `U+200E`/`U+200F` (commonly right before a Latin extension, to keep
/// it drawing left-to-right inside an otherwise right-to-left name) or the other marks at a
/// mixed-script boundary — exactly the case this ticket's own text warns against casually mangling.
/// Escaping them on disk would alter those users' filenames to "fix" a spoof they were never exposed
/// to, on every platform, forever, for a problem that is really about *rendering* in one specific
/// application (Windows Explorer) this codebase does not own.
///
/// **So the fix lives in the other lever this ticket names: our own rendering**
/// (`src/lib/filename.ts`'s `displaySafeName`, wired into the listing components), not the bytes on
/// disk or the remote name. Rendering is a presentation choice, re-evaluated on every redraw and
/// touching no one's data — the more surgical of the two, and reversible if it ever turns out wrong.
/// Explorer's own rendering of the on-disk name stays spoofed; that is Explorer's bug, not this app's
/// sink's, per the ticket's own scoping ("Explorer's behaviour is not ours to fix").
#[cfg(test)]
const BIDI_FORMAT_CHARS: &[char] = &[
    '\u{061C}', // Arabic Letter Mark — the twelfth Bidi_Control code point, missed in round 1
    '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}', // embeddings + overrides + pop
    '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}', // isolates
    '\u{200E}', '\u{200F}', // directional marks
];

/// The DOS device names Windows still reserves in every directory (CPE-1709), matched
/// case-insensitively against the name's stem — Windows reserves `CON.txt` exactly as it reserves `CON`.
///
/// Measured on Windows 11 Pro 26200 (scoped claim — this is version-dependent, which is precisely why
/// the fix does not rely on it): with the download root canonicalized to a `\\?\` verbatim path, as
/// [`download_tree`] does, every one of these was *created* as a real 5-byte file. Reading them back by
/// ordinary Win32 name (`cmd /c type`) then returned the real contents for `CON`, `PRN`, `AUX`,
/// `CON.txt`, `nul.txt` and `COM1`–`COM9` / `LPT1`–`LPT9` — but **`NUL` returned nothing at all**: the
/// name still resolves to the null device, so a user-visible 5-byte file reads as empty in every
/// application. That is the same silent-loss shape as the colon, reached by a different mechanism.
///
/// `NUL` was the only one that lost data *on this build*, but device-name resolution has moved between
/// Windows releases and is not a property we control, so all of them are escaped rather than only the
/// one that happened to misbehave here.
/// The list is every name Microsoft documents as reserved in "Naming Files, Paths, and Namespaces",
/// including the superscript `COM¹`/`LPT²` forms and `CONIN$`/`CONOUT$` (CPE-1709 round 2, F5). Those
/// were missing from the first cut. **Scoped measurement:** on Windows 11 Pro 26200 the superscript
/// forms and `CONIN$`/`CONOUT$` all downloaded and read back correctly, so none of them loses data
/// *here* — but the whole reason every name is escaped rather than only `NUL` (the one that misbehaved
/// on this build) is that device resolution has moved between Windows releases and is not ours to
/// depend on. That argument covers these identically, so leaving them out was inconsistent.
const WINDOWS_DEVICE_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", //
    "COM0", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "COM¹", "COM²",
    "COM³", //
    "LPT0", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9", "LPT¹", "LPT²",
    "LPT³",
];

/// The longest single path component every filesystem this app targets accepts. **Measured** (PR #894
/// round-2 UAT, Windows 11 Pro 26200 / NTFS): a 255-character component writes `Ok`, a 256-character one
/// fails with os error 123. Linux (ext4) and macOS (APFS) impose the same 255 limit, in bytes.
///
/// This is used **only to explain** a failure the OS has already reported — never to pre-reject a name.
/// Pre-rejecting would mean guessing at a limit whose unit differs per platform (UTF-16 code units on
/// Windows, bytes elsewhere) and would risk refusing a name the filesystem would actually have taken,
/// which is its own new bug. The OS makes the decision; this constant only turns "os error 123" into a
/// sentence naming the real cause.
pub const MAX_LOCAL_COMPONENT: usize = 255;

/// Windows' classic `MAX_PATH`. A path at or past this length is still *created* by
/// [`download_tree`] — its root is canonicalized to a `\\?\` verbatim path, which exempts it — but
/// applications that call the ordinary Win32 API without long-path support cannot open it afterwards
/// (CPE-1709 round 2, F4). Surfaced as a notice, never a failure: the file really is delivered and
/// really is readable by long-path-aware software, so calling it a failure would be its own wrong answer.
pub const MAX_WINDOWS_PATH: usize = 260;

/// Percent-escape one character as `%XX` (upper-case hex). Only ever called for code points ≤ `U+00FF`.
fn push_escaped(out: &mut String, c: char) {
    out.push('%');
    let b = c as u32;
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out.push(HEX[((b >> 4) & 0xF) as usize] as char);
    out.push(HEX[(b & 0xF) as usize] as char);
}

/// Whether `code` is a value [`windows_safe_segment`] can actually emit as a `%XX` escape (CPE-1709
/// round 2, F2).
///
/// The first cut escaped **every** `%HH`, which silently rewrote legal remote keys the encoder could
/// never have produced. `%2f` is the clearest case: `guarded_join` splits the path on `/` and `\`
/// *before* the transform runs, so nothing in this codebase can emit `%2F` — yet `report%2ffinal.txt`
/// came out as `report%252ffinal.txt`, and `city=A%2FB` (an ordinary Hive/Athena partition value) as
/// `city=A%252FB`. Both are perfectly storable local names, so that was a gratuitous rename of a key
/// that already worked — and, combined with the name-length ceiling, it turned working downloads into
/// lost ones.
///
/// Narrowing to the emittable set keeps injectivity intact. The proof only ever needed "the output
/// contains no `%HH` sequence that did not come from an escape this function emitted", and both the
/// encoder and [`decode_windows_safe_segment`] now read that phrase the same, narrower way.
fn is_encoder_emitted_code(code: u32) -> bool {
    let Some(c) = char::from_u32(code) else { return false };
    // Pass 2 (unholdable characters + control characters), pass 3 (the trailing `.`/space run), and the
    // escape character itself, which pass 1 emits as `%25`.
    if c.is_control() || WINDOWS_UNSAFE_CHARS.contains(&c) || c == '.' || c == ' ' || c == '%' {
        return true;
    }
    // Pass 4 escapes the FIRST character of a reserved device name, in whichever case it arrived.
    WINDOWS_DEVICE_NAMES.iter().filter_map(|d| d.chars().next()).any(|first| first.eq_ignore_ascii_case(&c))
}

/// The code point `chars[i..]` carries if it begins a `%HH` escape **this encoder could have emitted**.
/// `None` for anything else, including a well-formed `%HH` whose code is outside that set — those are
/// ordinary characters and are left exactly as they are.
fn escape_code_at(chars: &[char], i: usize) -> Option<u32> {
    if chars[i] != '%' || i + 2 >= chars.len() {
        return None;
    }
    let hi = chars[i + 1].to_digit(16)?;
    let lo = chars[i + 2].to_digit(16)?;
    let code = hi * 16 + lo;
    is_encoder_emitted_code(code).then_some(code)
}

/// Rewrite one path segment into a name a **Windows** filesystem can actually hold and an ordinary
/// Windows application can actually open (CPE-1709). Pure and platform-independent so the whole rule is
/// unit-testable on all three CI OSes; [`local_safe_segment`] decides *when* to apply it.
///
/// Four passes, in order:
/// 1. A `%` beginning an escape **this encoder could itself have emitted** ([`escape_code_at`]) becomes
///    `%25`. This is the escape-the-escape step, and it is the entire reason the mapping is safe (see the
///    injectivity note below). Every other `%` is left alone, so `50% off.txt`, `report%2ffinal.txt` and
///    `city=A%2FB` all pass through untouched.
/// 2. Every [`WINDOWS_UNSAFE_CHARS`] character and every control character becomes `%XX`.
/// 3. A trailing run of `.` or space becomes `%2E` / `%20`. Measured: with a verbatim root such a file
///    *is* created — `download_tree` reported `Ok(1)` and `read_dir` showed `trailingdot.` at 5 bytes —
///    but nothing can open it by name afterwards, because every non-verbatim Win32 path strips the
///    trailing run first (`cmd /c type` → "The system cannot find the file specified", Rust's
///    `fs::read` → os error 2). Full bytes on disk, unreachable: the colon bug's twin.
/// 4. If the resulting stem (up to the first `.`) is a [`WINDOWS_DEVICE_NAMES`] entry, its first
///    character is escaped — `CON` → `%43ON`, `COM1.txt` → `%43OM1.txt`.
///
/// **Two distinct names can never collide onto one local file**, which is the property that stops this
/// fix from replacing a silent-loss bug with a different silent-loss bug. The mapping is *injective*
/// because [`decode_windows_safe_segment`] inverts it exactly: the output never contains an
/// encoder-emittable `%HH` sequence that did not come from an escape this function emitted. Pass 1
/// removes every pre-existing one, and no new one can form afterwards, since an escape a later pass
/// emits starts with `%` (never a hex digit) and a literal `%` that survived pass 1 provably has no
/// *emittable* hex pair after it in the output. `decode(encode(x)) == x` for every `x` therefore forces
/// `encode(x) == encode(y) → x == y`. `cpe_1709_windows_name_mapping_is_injective` asserts both halves;
/// the round-2 UAT additionally brute-forced 66,429 adversarial names for **0 collisions and 0
/// round-trip breaks**.
///
/// (Two remote names that differ only in **case** — `A.txt` and `a.txt` — still land on one file on a
/// case-insensitive NTFS volume. That is a pre-existing property of the platform, unchanged by and
/// independent of this mapping, which is case-preserving; it is recorded here rather than fixed, since
/// no leaf-name rewriting can address it. Confirmed by the same round-2 brute force, which searched
/// specifically for a pair whose *case-folded* inputs differ but whose case-folded encodings match and
/// found none — so this mapping introduces no case collision of its own.)
///
/// This rewrites the **name**, and cannot rewrite away a name that is simply too *long*: encoding grows
/// a name by up to 3× per escaped character, and a component past [`MAX_LOCAL_COMPONENT`] is refused by
/// the filesystem. [`download_tree`] treats that as a reported delivery failure — see F1 there.
pub fn windows_safe_segment(name: &str) -> Cow<'_, str> {
    // Cheap pre-scan. `%` is deliberately over-broad here (most `%` names are NOT rewritten any more);
    // it only costs an allocation that then returns an identical string, never a wrong answer.
    let needs = name.chars().any(|c| WINDOWS_UNSAFE_CHARS.contains(&c) || c.is_control() || c == '%')
        || name.ends_with('.')
        || name.ends_with(' ')
        || is_windows_device_name(name);
    if !needs {
        return Cow::Borrowed(name);
    }

    let chars: Vec<char> = name.chars().collect();
    // Index at which the trailing `.`/space run starts (pass 3).
    let keep = chars.len() - chars.iter().rev().take_while(|c| **c == '.' || **c == ' ').count();

    let mut out = String::with_capacity(name.len() + 8);
    for (i, &c) in chars.iter().enumerate() {
        if escape_code_at(&chars, i).is_some() {
            out.push_str("%25"); // pass 1
        } else if WINDOWS_UNSAFE_CHARS.contains(&c) || c.is_control() || i >= keep {
            push_escaped(&mut out, c); // passes 2 + 3
        } else {
            out.push(c);
        }
    }

    // Pass 4, evaluated on the ALREADY-encoded name so an escape this function just emitted can never be
    // re-flagged: a literal `%43ON` encodes to `%2543ON`, whose stem is not a device name.
    if is_windows_device_name(&out) {
        let mut first = out.chars();
        let c = first.next().expect("a device name is never empty");
        let mut escaped = String::with_capacity(out.len() + 2);
        push_escaped(&mut escaped, c);
        escaped.push_str(first.as_str());
        out = escaped;
    }
    Cow::Owned(out)
}

/// Whether `name`'s stem (up to the first `.`) is a reserved DOS device name, case-insensitively.
///
/// `pub(crate)` since CPE-1823: [`crate::snapshot_capture::restore`] needs exactly this question about a
/// manifest-supplied path component (a `sub/NUL` entry "restored" into the null device and left nothing
/// on disk while returning `Ok`). One predicate, two callers — not a second list of device names.
pub(crate) fn is_windows_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("");
    WINDOWS_DEVICE_NAMES.iter().any(|d| d.eq_ignore_ascii_case(stem))
}

/// The exact inverse of [`windows_safe_segment`] — recovers the remote name from the local one
/// (CPE-1709). Decodes exactly the escapes the encoder can emit ([`escape_code_at`]) and leaves every
/// other `%HH` alone, so `report%2ffinal.txt` decodes to itself.
///
/// Shipped, not test-only: it is what makes the mapping *demonstrably* reversible rather than merely
/// claimed to be, and it is the proof obligation behind the injectivity argument, so it must live next
/// to the encoder and be exercised by the same table.
///
/// **It is deliberately NOT wired into [`upload_tree`]** (CPE-1709 round 2, F6), which means a
/// download-then-upload round trip does **not** restore the original remote name: an object downloaded
/// as `colon:name.txt` → `colon%3Aname.txt` uploads back as the key `colon%3Aname.txt`. That asymmetry
/// is a choice, recorded here rather than left to be discovered:
///
/// - Encoding is **compelled**. The local filesystem cannot hold the original name, so the transform is
///   forced and provably necessary at the moment it happens.
/// - Decoding would be a **guess about provenance**. `report%3Afinal.txt` is a perfectly storable local
///   name that a user may simply have typed, and we do not track which local files came from a
///   download. Decoding on upload would silently rename such a file's remote key to `report:final.txt`
///   — an unrequested rename with no forcing function behind it and no way to opt out, which is exactly
///   the class of surprise this ticket exists to remove.
///
/// So the asymmetry is the conservative direction: a re-upload preserves the bytes and the name you can
/// actually see on disk. The user-facing note is in `src/docs/31-network.md`.
pub fn decode_windows_safe_segment(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len());
    let mut i = 0;
    while i < chars.len() {
        if let Some(code) = escape_code_at(&chars, i) {
            if let Some(c) = char::from_u32(code) {
                out.push(c);
            }
            i += 3;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// Apply the local platform's leaf-name rule to one segment (CPE-1709). On Windows that is
/// [`windows_safe_segment`]; everywhere else the name is passed through untouched, because `:`, `*`,
/// `?`, a trailing dot and `CON` are all perfectly ordinary bytes in a Unix filename and rewriting them
/// would mangle legitimate names for no reason. This is why the sink — not any provider guard — is the
/// right place for the rule: the rule belongs to the local filesystem, and every provider that can
/// legally produce such a name arrives here.
///
/// `cfg!` rather than `#[cfg]` deliberately: both arms compile on every OS, so the encoder is never
/// dead code on Unix and its tests run on all three CI legs.
pub fn local_safe_segment(name: &str) -> Cow<'_, str> {
    if cfg!(windows) {
        windows_safe_segment(name)
    } else {
        Cow::Borrowed(name)
    }
}

/// Join an untrusted, provider-supplied relative path `rel` onto `base`, guaranteeing the result stays
/// inside `base` (CPE-1461, sink-side defense). The path is rebuilt segment-by-segment — splitting on
/// BOTH `/` and `\` so a Windows-style separator is neutralized on every OS — keeping only plain `Normal`
/// components. Any `..` segment, or any segment that is itself a root/drive/UNC prefix, makes the whole
/// entry unsafe and yields `None` (the caller skips it, and must NOT create parent directories for it).
/// Because only `Normal` segments are ever appended to `base`, the returned path is always lexically
/// contained in `base`. Callers additionally verify the on-disk parent canonicalizes back under `base`,
/// as a defense against a pre-existing symlink inside the download root.
pub fn guarded_join(base: &Path, rel: &str) -> Option<PathBuf> {
    let mut out = base.to_path_buf();
    let mut pushed = false;
    for seg in rel.split(['/', '\\']) {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." {
            return None; // parent-dir escape
        }
        // CPE-1709: rewrite the segment into something the LOCAL filesystem can actually hold before it
        // is checked or pushed. Without this a `:` sailed through as a `Normal` component and
        // `fs::write` diverted the bytes into an NTFS alternate data stream, leaving the user a 0-byte
        // file. Applied here, at the sink, because the rule belongs to the local platform, not to any
        // one provider — and after the `..` check above, which stays on the raw segment.
        //
        // Containment is unaffected: the transform only ever replaces a character with its own `%XX`
        // escape, never introduces a `/`, `\` or `..`, and the `Normal`-component check below still
        // gates what is pushed. It can only *widen* what is accepted — a segment like `C:` that used to
        // reject the whole entry (Windows parses it as a drive prefix) now becomes the ordinary
        // contained directory name `C%3A`.
        let seg = local_safe_segment(seg);
        // Each surviving segment must itself be a single normal component (rejects `C:` / rooted segments
        // on the platforms where they parse as prefixes/roots).
        let mut comps = Path::new(seg.as_ref()).components();
        match (comps.next(), comps.next()) {
            (Some(Component::Normal(s)), None) => {
                out.push(s);
                pushed = true;
            }
            _ => return None,
        }
    }
    if pushed {
        Some(out)
    } else {
        None
    }
}

// **`existing_ancestor` / `AncestorProbe` / `LeafProbe` / `classify_leaf_probe` lived here and are
// gone (CPE-1913).** All four were by-PATH probes standing in front of the write: canonicalise the
// deepest existing ancestor and compare it to the root (CPE-1461/1696), `lstat` the leaf for a
// pre-existing symlink (CPE-1461/1696), then `batch_media::name_links` for a hard link (CPE-1857).
// `download_tree` now opens the destination through `open_beneath::create_beneath` against a root
// handle held for the whole transfer and asks `fsutil::claim_destination_handle` the same three
// questions **of the handle it opened**, where no rename can change the answer afterwards.
//
// They were deleted rather than kept as belt-and-braces on purpose. A path question standing in front
// of a handle question is not redundancy, it is a shadow: it answers first, so the handle guard can
// be disabled with the whole suite still green and no fixture can be built that reaches it. That is
// CPE-1929, generalised from CPE-1896 round 4, where exactly this shape hid a live guard behind an
// `is_symlink` check for four review rounds. The reporting they fed is unchanged — a policy refusal
// is still a `DownloadReport::skipped` entry and an I/O refusal still lands in `undelivered` and
// fails the transfer — but the verdict now comes from `open_beneath::Refusal::policy`, set by the
// guard that fired, instead of from which probe happened to run.

/// The length of `path` as an ordinary Win32 application would spell it — without the `\\?\` verbatim
/// prefix that [`download_tree`]'s canonicalized root carries on Windows. That prefix is what exempts
/// our own writes from `MAX_PATH`; it is not part of the path anything else will use, so counting it
/// would overstate the length by four and misjudge the [`MAX_WINDOWS_PATH`] notice.
fn win32_visible_len(path: &Path) -> usize {
    let s = path.to_string_lossy();
    s.strip_prefix(r"\\?\").unwrap_or(&s).chars().count()
}

/// Explain, in the user's terms, why an entry could not be written — naming the **real** cause rather
/// than whichever syscall happened to fail first (CPE-1709 round 2, F1).
///
/// The failure this exists for: an encoded name past [`MAX_LOCAL_COMPONENT`] is refused by
/// `CreateFileW` with os error 123, and the call that hit it first was the CPE-1696 leaf
/// `symlink_metadata` probe — so the transfer announced *"could not be inspected for a pre-existing
/// symlink"* about a name that has nothing to do with symlinks and everything to do with length. A
/// confident wrong answer is worse than an honest one (CPE-1673/1678), so the length is measured and
/// stated when it is the plausible cause. The OS still makes the decision; this only describes it.
///
/// **Scoped limitation, recorded rather than left to be rediscovered (PR #894 UAT fold-in):** on an
/// **astral-plane** name — emoji and other characters outside the Basic Multilingual Plane, where Rust's
/// `char`/`chars().count()` and Windows' UTF-16-code-unit accounting diverge 1:2 — the length branch
/// above can fail to fire even though the length limit is the real cause, and the message then degrades
/// to the raw OS error instead of naming the length. That is an **absent** explanation, not a **wrong**
/// one: the two properties that actually matter still hold in every case — the transfer still correctly
/// ends `Err` (never a silent `Ok` over a dropped file), and the message never blames a symlink probe for
/// an astral-plane name either.
fn describe_undeliverable(remote: &str, local: &Path, cause: &str) -> String {
    let leaf_len =
        local.file_name().map(|n| n.to_string_lossy().chars().count()).unwrap_or_default();
    let mut why = format!("{remote}: could not be written to {} ({cause})", local.display());
    if leaf_len > MAX_LOCAL_COMPONENT {
        why.push_str(&format!(
            " — the local name it needs is {leaf_len} characters once encoded for this filesystem, \
             past the {MAX_LOCAL_COMPONENT}-character limit for a single path component"
        ));
    } else if win32_visible_len(local) > MAX_WINDOWS_PATH {
        why.push_str(&format!(
            " — the full local path is {} characters, past the {MAX_WINDOWS_PATH}-character \
             MAX_PATH limit",
            win32_visible_len(local)
        ));
    }
    why
}

/// Join a directory + child name. An empty `dir` (the provider root) yields the bare name — so a
/// remote root of `/` produces `/name` while a `FakeProvider`/relative root of `` produces `name`.
fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{}/{}", dir.trim_end_matches('/'), name)
    }
}

/// Recursively walk the tree under `root` (depth-first), invoking `on_entry` for every file and directory.
/// `cancel` is checked before each directory listing and each entry. A directory that can't be listed is
/// skipped rather than aborting the walk. Returns the number of entries visited.
///
/// Bounded against a hostile/runaway server (CPE-1462): descent stops at [`MAX_WALK_DEPTH`] (a surfaced
/// notice, the rest of the walk continues) and the whole walk aborts with an `Err` past
/// [`MAX_WALK_ENTRIES`] total entries. (`ProviderEntry` carries no symlink signal, so a symlink loop
/// advertised by the server as an ordinary directory is bounded by these same caps rather than by
/// real-path tracking.)
pub fn walk(
    provider: &dyn FileSystemProvider,
    root: &str,
    cancel: &AtomicBool,
    mut on_entry: impl FnMut(WalkEntry),
) -> Result<usize, String> {
    // Each stack item carries its depth so descent past MAX_WALK_DEPTH can be capped (CPE-1462).
    let mut stack = vec![(root.to_string(), 0usize)];
    let mut visited = 0usize;
    while let Some((dir, depth)) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(entries) = provider.list(&dir) else { continue };
        for entry in entries {
            if cancel.load(Ordering::Relaxed) {
                return Ok(visited);
            }
            let path = join(&dir, &entry.name);
            visited += 1;
            if visited > MAX_WALK_ENTRIES {
                return Err(format!(
                    "transfer: aborted — tree exceeds the {MAX_WALK_ENTRIES}-entry safety cap \
                     (possible hostile or runaway remote server)"
                ));
            }
            let is_dir = entry.is_dir;
            on_entry(WalkEntry { path: path.clone(), name: entry.name, is_dir, size: entry.size });
            if is_dir {
                if depth < MAX_WALK_DEPTH {
                    stack.push((path, depth + 1));
                } else {
                    // Bound infinite depth: don't descend further, but keep the rest of the walk going.
                    eprintln!("transfer: not descending past depth {MAX_WALK_DEPTH} at {path} (depth safety cap)");
                }
            }
        }
    }
    Ok(visited)
}

/// Download the tree under `remote_root` into `local_dir`, recreating the directory structure. Returns a
/// [`DownloadReport`] (files written, plus what a per-entry guard skipped and why — CPE-1881). Cancellable.
///
/// Hardened against a hostile remote server (CPE-1461/CPE-1462):
/// - Each entry is **streamed straight to disk as it is walked** rather than collecting the whole tree
///   into a `Vec` first, so accumulation is bounded even for an enormous remote tree.
/// - Every server-named path is run through [`guarded_join`] so a traversal name (`..`, an absolute/drive/
///   UNC path, a `\`-separated segment) can never write outside `local_dir`. An entry that would escape is
///   **skipped with a surfaced notice** (skip-on-error — one hostile entry does not fail the whole
///   transfer), and its parent directories are NOT created.
/// - The download root is canonicalized once up front. **Before** creating any directory, the longest
///   already-existing ancestor of the target is canonicalized and verified to still live under the root,
///   so a pre-existing symlink inside `local_dir` pointing outward is caught *before* any `mkdir` can
///   follow it. A file whose leaf path is itself a pre-existing symlink is skipped (never followed on
///   write). Both are defenses against a symlink planted by some other channel — the remote can't create
///   one, but defense-in-depth is the point.
///
/// **Delivery is reported honestly (CPE-1709).** Two different things can stop an entry being written,
/// and they end differently:
/// - A **security refusal** — a traversal name, a pre-existing symlink, an uninspectable ancestor — is
///   skipped with a notice and the transfer still ends `Ok`, because not writing it is the correct
///   outcome.
/// - A **delivery failure** — an entry we intended to write that the local filesystem refused (an
///   encoded name past [`MAX_LOCAL_COMPONENT`] being the reachable case) — makes the transfer end
///   `Err`, naming how many were lost and why. Everything deliverable is still delivered first. This
///   used to be a silent skip that returned `Ok`, which meant a batch could report success while
///   quietly dropping files.
///
/// **A third category, CPE-1881: a per-entry policy skip that IS worth telling the caller about, even
/// though it must not fail the transfer.** The CPE-1857 hard-link leaf guard, and its pre-existing-symlink
/// neighbour right next to it, used to report through `eprintln!` alone — invisible to any caller, with
/// the delivered count silently one lower and no way to learn what was skipped or why. `undelivered` was
/// considered and rejected for this (it fails the WHOLE transfer, which is wrong for a policy skip that
/// legitimately recurs across a hard-linked source tree — see the guard's own comment below); the
/// alternative actually taken is [`DownloadReport::skipped`], a counted list the transfer still ends `Ok`
/// with, the same shape [`crate::archive::ArchiveReport::skipped`] already established for the archive
/// extractor's per-entry refusals.
pub fn download_tree(
    provider: &dyn FileSystemProvider,
    remote_root: &str,
    local_dir: &Path,
    cancel: &AtomicBool,
) -> Result<DownloadReport, String> {
    let base = remote_root.trim_end_matches('/').to_string();
    std::fs::create_dir_all(local_dir).map_err(|e| format!("{}: {e}", local_dir.display()))?;
    // Canonicalize the download root ONCE; every written path is verified to stay under this.
    let canonical_root =
        std::fs::canonicalize(local_dir).map_err(|e| format!("{}: {e}", local_dir.display()))?;

    let mut files = 0usize;
    // A callback can't use `?`; capture the first hard I/O error and stop doing work once it's set.
    let mut hard_err: Option<String> = None;
    // CPE-1709 (F1): entries we INTENDED to deliver and could not. Distinct from the security refusals
    // below (traversal names, pre-existing symlinks), where not writing IS the correct outcome and `Ok`
    // is honest. These are files the user asked for and did not get, so the transfer must not end `Ok`.
    let mut undelivered: Vec<String> = Vec::new();
    // CPE-1881: per-entry policy skips (hard link / symlink at the local leaf name) — reported, but never
    // fail the transfer. See `DownloadReport::skipped`'s doc for why this is a distinct third bucket from
    // both `undelivered` above and the plain `eprintln!`-only security refusals (traversal names,
    // uninspectable ancestors) that stay exactly as they were: those are decided before any name is even
    // resolved to a leaf, are not CPE-1857's shape, and are unchanged here — out of this ticket's scope.
    let mut skipped: Vec<String> = Vec::new();

    // CPE-1913: hold the resolved download root **open** for the whole transfer, once — the same
    // anchor `backup::apply_backup_plan_walk` opens. Every write below is resolved
    // component-by-component against this handle instead of by re-parsing a path, which is what
    // makes each entry's containment atomic with its own open. A root that resolves but will not
    // open fails the whole transfer, for the same reason an unresolvable one does: with no anchor
    // there is no containment question that can be answered.
    let root_handle = crate::open_beneath::open_root(&canonical_root, "download folder")
        .map_err(|e| format!("{}: the download folder could not be opened ({e}), so nothing can be written into it in a way that can be checked", canonical_root.display()))?;

    walk(provider, remote_root, cancel, |entry| {
        if hard_err.is_some() {
            return;
        }
        let rel = entry.path.strip_prefix(&base).unwrap_or(&entry.path).trim_start_matches('/');
        // Reject/skip any entry whose server-supplied path would escape the download root.
        let Some(local) = guarded_join(&canonical_root, rel) else {
            eprintln!("transfer: skipped unsafe entry name from remote (path traversal): {}", entry.path);
            return;
        };
        // **Everything from here is answered against the HELD ROOT HANDLE, never against a path**
        // (CPE-1913). `rel_local` is what `guarded_join` just built, minus the root — always a plain
        // relative path of `Component::Normal` parts, because that is the only thing `guarded_join`
        // can produce, and it has already applied `local_safe_segment` to every one of them (which is
        // the Windows name-normalisation obligation `open_beneath::create_beneath` records for a new
        // caller: a name the NT layer and the Win32 layer disagree about never gets this far).
        let Ok(rel_local) = local.strip_prefix(&canonical_root) else {
            // Unreachable: `guarded_join` built `local` by pushing onto `canonical_root`. Refused
            // rather than unwrapped, because an unreachable case that silently writes is worse than
            // one that silently skips.
            eprintln!("transfer: skipped entry whose local path left the download root: {}", entry.path);
            return;
        };
        let rel_local = rel_local.to_path_buf();

        // What a refusal means for the report. The two buckets were already here (CPE-1709 F1 and
        // CPE-1881); what changed is where the answer comes from — `Refusal::policy` is set by the
        // guard that fired rather than inferred from its wording. `policy` = "not writing is the
        // correct outcome" (a link, a hard link, a directory in the way) and stays a per-entry skip;
        // anything else is a file the user asked for and did not get, so it fails the transfer.
        macro_rules! record {
            ($r:expr) => {{
                let r = $r;
                if r.policy {
                    let why = format!("{}: {}", entry.path, r.why);
                    eprintln!("transfer: skipped entry — {why}");
                    skipped.push(why);
                } else {
                    let why = describe_undeliverable(&entry.path, &local, &r.why);
                    eprintln!("transfer: FAILED to deliver {why}");
                    undelivered.push(why);
                }
            }};
        }

        if entry.is_dir {
            // Was `create_dir_all(dir_to_make)` behind a `canonicalize` of the deepest existing
            // ancestor. `create_dir_all` walks a junction like any other directory, so the check and
            // the creation were two different questions about a path that could change in between —
            // and the check could not see a junction pointing *inside* the root at all (CPE-1912).
            // `create_dir_beneath` opens each level relative to the level above it and refuses a link
            // at every one.
            if let Err(r) = crate::open_beneath::create_dir_beneath(&root_handle, &rel_local) {
                record!(r);
            }
            return;
        }

        // **The remote body is fetched BEFORE the destination is claimed, and the order is the point.**
        // It used to be the other way round: every local check ran, then `provider.read` fetched an
        // entire file over the network, and only then did `fs::write` resolve the local path again.
        // That put a whole network transfer inside the check-to-write window — the widest of the five
        // windows CPE-1913 was filed for. Fetching first means the claim below is the *last* thing
        // before the bytes go in, and it also keeps the pre-existing property that a failed fetch
        // leaves whatever was already at the local name untouched (the claim truncates).
        let data = match provider.read(&entry.path) {
            Ok(d) => d,
            Err(e) => {
                hard_err = Some(e);
                return;
            }
        };

        // ONE gate, shared with the backup and restore legs (`fsutil::claim_destination_handle`).
        // It opens the destination one component at a time against the root handle, then asks the
        // handle itself — never the path again — whether the object is a link, a directory, or a
        // second name for a file that may live anywhere.
        //
        // **The three by-path probes that used to stand here are gone, not kept as belt-and-braces.**
        // `existing_ancestor` + `canonicalize` (containment), the leaf `symlink_metadata` probe, and
        // `batch_media::name_links` all asked, by path, questions this gate answers by handle. Left in
        // front they would have made the handle guards unreachable for every refusal — the exact shape
        // CPE-1929 names, and the reason CPE-1896 ended by *reordering* rather than adding: a guard
        // that can be deleted with the suite still green is not a guard.
        let mut claimed = match crate::fsutil::claim_destination_handle(
            &local,
            crate::fsutil::LinkGuardWording::DOWNLOAD,
            crate::fsutil::DestinationSite::Beneath { root: &root_handle, rel: &rel_local },
        ) {
            Ok(c) => c,
            Err(r) => {
                record!(r);
                return;
            }
        };
        if let Err(e) = std::io::Write::write_all(&mut claimed.file, &data) {
            hard_err = Some(format!("{}: {e}", local.display()));
            return;
        }
        // CPE-1961: the bytes are in a staging sibling until this line renames it over `local`. A
        // `return` above drops the claim, which removes the staged file — so a download that fails
        // mid-write no longer leaves a truncated local file where a complete one used to be.
        //
        // **`record!`, not `hard_err` — CPE-1961 round 4, and it is the same finding as the archive
        // leg's Blocker 1 arriving here.** Rounds 1–3 wrote `hard_err = Some(r.why)`, on the reasoning
        // that a local write failure has always set `hard_err` (`write_all` above still does). That
        // reasoning is about the *bucket*, and the thing that changed is the *input*: this ticket adds
        // a failure point the loop did not have — `sync_all`, then a rename the filesystem can refuse
        // — and the commonest way to reach it is a destination another program is holding open. On
        // `main` that download succeeded, because writing through an already-open handle is not
        // something a sharing mode blocks; here it took the whole tree down at the first such file,
        // and `hard_err` also short-circuits every entry after it (`if hard_err.is_some()` at the top
        // of this closure).
        //
        // **Which bucket `record!` picks, and the sentence round 4 got wrong (Reviewer Major 1).**
        // Round 4 wrote *"`record!` with a `policy: false` refusal — which is what `commit` returns"*.
        // The parenthetical is **false**. It is true of `DestinationSite::ByPath`, which does
        // `commit_replacement(...).map_err(Refusal::failure)`; it is false of the **`Beneath`** arm
        // this leg uses, which returns `open_beneath::rename_beneath`'s `Refusal` unchanged — and that
        // function's `descend(root, Act::Commit, dirs)` calls `refuse_link` on a directory component
        // that has become a link since the claim, which is `policy: true`. Executed, red-proofed, on a
        // real planted link: `fsutil::tests::
        // cpe_1961_a_link_planted_at_an_interior_component_makes_commit_refuse_with_policy_true`.
        //
        // So `record!` here forks, and both arms are live:
        //
        // ```text
        // commit refuses with…  bucket        the walk   this call returns
        // policy: false         undelivered   continues  Err, naming this file  (a lock, ENOSPC, a
        //                                                                        dropped share)
        // policy: true          skipped       continues  Ok(DownloadReport)     (a component swapped
        //                                                                        for a link mid-write)
        // ```
        //
        // **The second row has to be said out loud rather than left to be inferred: a link planted on a
        // directory component between this claim and this commit produces a file the user asked for,
        // did not get, and that `download_tree` reports as `Ok`.** The reason it is not delivered is
        // named — it goes into `DownloadReport::skipped` with its wording, and onto stderr — but the
        // call's verdict is success. That is not something this ticket introduced and not something it
        // changes: it is CPE-1709/CPE-1881's standing contract for this leg, which classifies a **link
        // verdict** as neither delivered nor a delivery failure ("not writing is the correct, safe
        // outcome" — `DownloadReport::skipped`'s own doc), and which has produced exactly this
        // `skipped` + `Ok` for a link found at *claim* time since long before CPE-1961 — verified
        // against `git show 8c9ddb60:crates/server/src/transfer.rs`, a **pre-branch** revision on
        // `main` (*"sprint: 'use the shape from the other leg' is not a mechanical transform"*), where
        // it is byte-identical. Round 5 called `8c9ddb60` "this branch's merge base" and it is **not**
        // one — it is an *ancestor* of the merge base, which sat fifteen commits later. The
        // substantive claim is unaffected, and it is stated in the rebase-proof form that should have
        // been used first: `8c9ddb60` is an ancestor of this branch's merge base, and
        // `git diff 8c9ddb60 $(git merge-base origin/main HEAD) -- crates/server/src/transfer.rs` is
        // **empty**, so the file is unchanged all the way to the branch point and the silent `Ok`
        // genuinely predates this branch. Naming the merge base by sha is what went stale: the sha
        // written at round 7 was falsified by round 7's own rebase, minutes later. Say what the
        // revision *is* and derive the rest. Saying "merge base" of a revision that is not one is the
        // CPE-1933 shape — a sentence that reads as verified while naming a different object.
        // Whether that contract is right is a real question, and it is **CPE-1980**;
        // what this round fixes is that the two moments now answer it the same way, so the outcome no
        // longer depends on which microsecond the link was planted in.
        //
        // **A ticket rather than a note** (round 6, Reviewer). Round 5 wrote "a separate ticket's" with
        // no ticket behind it, and an unnamed follow-up is a follow-up nobody can find: the next reader
        // sees a sentence that sounds resolved. CPE-1980 carries the decision — skip, failure, or a
        // report shape a caller cannot ignore by accident — along with the measurement that it predates
        // this branch, so the argument does not have to be reconstructed.
        //
        // **The other leg, and why the two now agree.** `archive::extract_zip_archive_stream` had the
        // identical refusal going to `report.fail` unconditionally — the opposite bucket from this one,
        // with *"clear that and extract again"* attached, which cannot work for a planted link. Round 5
        // gave it the same `policy` fork its own claim site has had since CPE-1935. Both legs now
        // classify a commit refusal by exactly the rule they already used for a claim refusal. They
        // still differ in **consequence** — `archive` returns `Ok(report)` with counted skips either
        // way, this leg returns `Ok` only when nothing reached `undelivered` — and that difference is
        // the two legs' own pre-existing report shapes, identical for claim-time refusals, not
        // something either site decides.
        //
        // The rest of round 4's reasoning stands and is why `record!` replaced `hard_err` at all: it
        // is what CPE-1709's own sentence forty lines below asks for — *"Everything deliverable IS
        // still delivered first (the walk runs to completion, matching the skip-on-error ethos); only
        // the final verdict changes, and it names what was lost."*
        //
        // `write_all`'s `hard_err` above is left alone deliberately: it predates this ticket, it is a
        // different question (a failure writing into a file nothing else has a name for), and moving
        // it is a behaviour change this ticket did not measure.
        if let Err(r) = claimed.commit() {
            record!(r);
            return;
        }
        files += 1;
        // CPE-1709 (F4): the file IS delivered — the verbatim root exempts our own write from
        // MAX_PATH — but an application without long-path support cannot open it afterwards.
        // Encoding grows a name by up to 3x, so this transfer makes a pre-existing hazard materially
        // likelier. A notice, not a failure: calling a delivered, long-path-readable file a failure
        // would be its own wrong answer.
        if cfg!(windows) && win32_visible_len(&local) > MAX_WINDOWS_PATH {
            eprintln!(
                "transfer: delivered {} at a {}-character path, past Windows' \
                 {MAX_WINDOWS_PATH}-character MAX_PATH — applications without long-path support \
                 will not be able to open it",
                entry.path,
                win32_visible_len(&local)
            );
        }
    })?;

    if let Some(e) = hard_err {
        return Err(e);
    }
    // CPE-1709 (F1): never report success for a tree we did not fully deliver. In a batch the old
    // silent skip vanished completely — three keys in, `Ok(2)`, two files out — which is precisely the
    // "the transfer said it worked and the file is not there" failure this ticket exists to eliminate.
    // Everything deliverable IS still delivered first (the walk runs to completion, matching the
    // skip-on-error ethos); only the final verdict changes, and it names what was lost.
    if !undelivered.is_empty() {
        const SHOWN: usize = 5;
        let more = undelivered.len().saturating_sub(SHOWN);
        return Err(format!(
            "transfer: delivered {files} file(s), but {} could not be written to this filesystem: {}{}",
            undelivered.len(),
            undelivered.iter().take(SHOWN).cloned().collect::<Vec<_>>().join("; "),
            if more > 0 { format!(" (+{more} more)") } else { String::new() }
        ));
    }
    Ok(DownloadReport { files, skipped })
}

/// Ensure a directory exists at `path`, creating it (and any missing ancestors) if it does not — CPE-1741.
///
/// `mkdir` on every real backend is `mkdir(2)`: it creates exactly one directory, fails `EEXIST`/
/// `SSH_FX_FAILURE`/550 when `path` is already there, and fails `ENOENT` when its parent is not. Neither
/// failure means "ensure it exists" failed — but a client can't safely tell them apart from `mkdir`'s own
/// error text: measured against the `cpe-sftp` test rig, `create_dir`'s `ErrorKind::AlreadyExists` maps to
/// the exact same `StatusCode::Failure` (rendered `"Failure: Failure"`) that every *other* unclassified
/// `mkdir` error also maps to (`crates/sftp/src/lib.rs`'s `io_err`, matching real OpenSSH's `sftp-server.c`,
/// which likewise returns a bare `SSH_FX_FAILURE` for `EEXIST` with no reason code to distinguish it). The
/// `cpe-ftp` rig is the same shape one layer up the stack: both `EEXIST` and `ENOENT` on its `MKD` arm reply
/// `550 Create failed` — RFC 959 gives `550` no finer-grained sub-code at all. So this function never parses
/// `mkdir`'s error text; it asks `stat` instead, which every provider already reports honestly.
///
/// - `path` already exists as a directory → nothing to do, no `mkdir` call is made at all.
/// - `path` exists but is a **file**, not a directory → a clear error, not swallowed.
/// - `path` doesn't exist → its parent is ensured first (recursively — `mkdir` cannot create a chain, so
///   this function does, one level at a time), then `path` itself is created.
/// - if `mkdir` still fails (a race with another writer, or a genuine failure — permission denied, a
///   broken remote) — `stat` is rechecked once: if a directory is there now, the race resolved in our
///   favour and this returns `Ok`; otherwise the original `mkdir` error is returned unchanged. A real
///   failure always surfaces; only "it turned out to already be there" is swallowed, and only once
///   confirmed by `stat`.
fn ensure_dir(provider: &mut dyn FileSystemProvider, path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Ok(()); // the provider root — always "exists".
    }
    match provider.stat(path) {
        Ok(entry) if entry.is_dir => return Ok(()),
        Ok(_) => return Err(format!("{path}: already exists and is not a directory")),
        Err(_) => {} // not found (or unresolvable) — fall through and try to create it.
    }
    if let Some(i) = path.rfind('/') {
        if i > 0 {
            ensure_dir(provider, &path[..i])?;
        } // else: parent is the root itself ("/a" → ""), which always exists.
    }
    match provider.mkdir(path) {
        Ok(()) => Ok(()),
        Err(e) => match provider.stat(path) {
            Ok(entry) if entry.is_dir => Ok(()), // it's there now — good enough, whatever `e` said.
            _ => Err(e),                         // still not there: a real failure, surface it.
        },
    }
}

/// Upload the local tree under `local_dir` into `remote_root`, recreating the structure — the symmetric
/// counterpart to [`download_tree`]. Returns the number of files written. Cancellable. Local `\` are mapped
/// to `/` so a Windows source produces provider-native paths.
pub fn upload_tree(
    provider: &mut dyn FileSystemProvider,
    local_dir: &Path,
    remote_root: &str,
    cancel: &AtomicBool,
) -> Result<usize, String> {
    let base = remote_root.trim_end_matches('/').to_string();
    // CPE-1741 fix: `mkdir` is `mkdir(2)` on every real backend (`SSH_FXP_MKDIR`, FTP `MKD`) — it is
    // NOT idempotent, and has no way to create a missing parent chain either. A bare `provider.mkdir(&base)?`
    // therefore aborted the whole upload both when `base` already existed (`EEXIST`/`SSH_FX_FAILURE`/550)
    // and when its parents did not exist yet (`ENOENT`). `ensure_dir` (below) replaces it: it `stat`s
    // before it `mkdir`s, so "already there" is answered from `stat`, never from parsing `mkdir`'s error
    // text — see `ensure_dir`'s doc for why that distinction can't be trusted on the wire.
    ensure_dir(provider, &base)?;
    let mut stack = vec![local_dir.to_path_buf()];
    let mut files = 0usize;
    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(read_dir) = std::fs::read_dir(&dir) else { continue };
        for entry in read_dir.flatten() {
            if cancel.load(Ordering::Relaxed) {
                return Ok(files);
            }
            let local = entry.path();
            let Ok(rel) = local.strip_prefix(local_dir) else { continue };
            let remote = join(&base, &rel.to_string_lossy().replace('\\', "/"));
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                // Same CPE-1741 shape as the base directory above, one level down: any directory in the
                // tree that already exists remotely (a partial re-upload) must not abort the transfer.
                // But unlike the base, `stack` pushed this directory's *parent* onto the walk before we
                // ever get here, so for a fresh in-tree directory the parent is already known to exist and
                // a bare `mkdir` succeeds first try. Measured: `ensure_dir` here unconditionally costs
                // stat(self) miss + stat(parent) hit + mkdir(self) = 3 round trips per fresh directory —
                // 2.9x the round trips of the old (unsafe) code, not the 2x anticipated. On FTP it's worse
                // than "a round trip": `FtpProvider::stat` (crates/ftp/src/lib.rs:300) is `list(parent)`, a
                // full PASV + data-connection LIST, so n sibling subdirectories cost O(n^2) bytes. Try
                // `mkdir` first and only fall back to `ensure_dir` on failure — "already there, or missing
                // parent, or a genuine failure" all still get handled correctly by `ensure_dir`, just off
                // the fast path instead of on it.
                if provider.mkdir(&remote).is_err() {
                    ensure_dir(provider, &remote)?; // already there, or a genuine failure — `ensure_dir` decides
                }
                stack.push(local);
            } else {
                let data = std::fs::read(&local).map_err(|e| format!("{}: {e}", local.display()))?;
                provider.write(&remote, &data)?;
                files += 1;
            }
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{FakeProvider, ProviderCapabilities, ProviderEntry};

    /// A `FakeProvider` seeded with `a.txt` + `sub/b.txt` (an empty `""` root, no leading slash).
    fn seeded() -> FakeProvider {
        let mut fs = FakeProvider::new();
        fs.write("a.txt", b"alpha").unwrap();
        fs.write("sub/b.txt", b"bravo").unwrap();
        fs
    }

    #[test]
    fn walk_recurses_every_file_and_dir() {
        let fs = seeded();
        let cancel = AtomicBool::new(false);
        let mut paths: Vec<_> = Vec::new();
        let n = walk(&fs, "", &cancel, |e| paths.push((e.path, e.is_dir))).unwrap();
        paths.sort();
        assert_eq!(n, 3, "a.txt + sub + sub/b.txt; got {paths:?}");
        assert!(paths.contains(&("a.txt".to_string(), false)));
        assert!(paths.contains(&("sub".to_string(), true)));
        assert!(paths.contains(&("sub/b.txt".to_string(), false)));
    }

    #[test]
    fn walk_stops_when_cancelled() {
        let fs = seeded();
        let cancel = AtomicBool::new(false);
        let mut count = 0;
        let visited = walk(&fs, "", &cancel, |_| {
            count += 1;
            cancel.store(true, Ordering::Relaxed);
        })
        .unwrap();
        assert_eq!((visited, count), (1, 1));
    }

    #[test]
    fn download_tree_writes_the_provider_files_locally() {
        let fs = seeded();
        let out = std::env::temp_dir().join(format!("cpe-xfer-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let cancel = AtomicBool::new(false);
        let files = download_tree(&fs, "", &out, &cancel).unwrap().files;
        assert_eq!(files, 2);
        assert_eq!(std::fs::read(out.join("a.txt")).unwrap(), b"alpha");
        assert_eq!(std::fs::read(out.join("sub").join("b.txt")).unwrap(), b"bravo");
        let _ = std::fs::remove_dir_all(&out);
    }

    /// **CPE-1961 round 4: one local file another program holds open costs THAT file, not the rest of
    /// the download.**
    ///
    /// The archive leg's Blocker 1, arriving here. This ticket adds a failure point `download_tree` did
    /// not have — `sync_all`, then a rename the filesystem can refuse — and rounds 1–3 routed it into
    /// `hard_err`, which both ends the transfer `Err` **and** short-circuits every entry after it. On
    /// `main` the same download succeeded outright: writing through an already-open handle is not
    /// something a sharing mode blocks, so nothing about a held-open destination could fail a write.
    ///
    /// The transfer still ends `Err` — CPE-1709 requires that, and the message names the file — but the
    /// walk runs to completion and everything deliverable is delivered, which is that ticket's own
    /// stated ethos. So the assertion that matters is on the **filesystem**, not the `Result`: both
    /// are `Err` either way, and only `z_after.txt` tells the two apart.
    ///
    /// **Red-proof, run** (`record!(r)` back to `hard_err = Some(r.why)`, `Compiling cpe-server` seen):
    ///
    /// ```text
    /// cpe_1961_a_local_file_held_open_costs_that_file_not_the_rest_of_the_download ... FAILED
    ///   THE POINT: the entry AFTER the held-open one must still be delivered … left: None
    /// ```
    ///
    /// Windows-only for the same reason the archive leg's twin is: `rename(2)` over an open file always
    /// succeeds on Linux, so the fixture does not exist there. The code path is shared.
    #[test]
    #[cfg(windows)]
    fn cpe_1961_a_local_file_held_open_costs_that_file_not_the_rest_of_the_download() {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 0x0000_0001;
        const FILE_SHARE_WRITE: u32 = 0x0000_0002;

        let mut fs = FakeProvider::new();
        fs.write("a_before.txt", b"BEFORE").unwrap();
        fs.write("m_victim.txt", b"REPLACEMENT").unwrap();
        fs.write("z_after.txt", b"AFTER").unwrap();

        let out = std::env::temp_dir().join(format!("cpe-xfer-held-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out).unwrap();
        let victim = out.join("m_victim.txt");
        std::fs::write(&victim, b"ORIGINAL").unwrap();
        // No FILE_SHARE_DELETE — the share mode a program not using Rust's `std` picks by default.
        let hold = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(&victim)
            .expect("holding the victim open for reading must succeed");

        let cancel = AtomicBool::new(false);
        let outcome = download_tree(&fs, "", &out, &cancel);

        assert_eq!(
            std::fs::read(out.join("a_before.txt")).ok().as_deref(),
            Some(&b"BEFORE"[..]),
            "the entry before the held-open one must be delivered: {outcome:?}"
        );
        assert_eq!(
            std::fs::read(out.join("z_after.txt")).ok().as_deref(),
            Some(&b"AFTER"[..]),
            "THE POINT: the entry AFTER the held-open one must still be delivered. Missing means the \
             commit failure set `hard_err`, which short-circuits the rest of the walk: {outcome:?}"
        );
        assert_eq!(
            std::fs::read(&victim).ok().as_deref(),
            Some(&b"ORIGINAL"[..]),
            "and the file that could not be replaced must be exactly as it was: {outcome:?}"
        );
        let residue: Vec<_> = std::fs::read_dir(&out)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".cpe-tmp"))
            .collect();
        assert!(residue.is_empty(), "a refused commit takes its staging sibling with it: {residue:?}");

        let why = outcome.expect_err(
            "a tree that was not fully delivered must not report success — CPE-1709 F1",
        );
        assert!(
            why.contains("m_victim.txt"),
            "and the verdict must name the file that was lost: {why}"
        );
        drop(hold);
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn upload_tree_writes_local_files_into_the_provider() {
        let src = std::env::temp_dir().join(format!("cpe-xfer-up-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&src);
        std::fs::create_dir_all(src.join("inner")).unwrap();
        std::fs::write(src.join("x.txt"), b"ex").unwrap();
        std::fs::write(src.join("inner").join("y.txt"), b"why").unwrap();

        let mut fs = FakeProvider::new();
        let cancel = AtomicBool::new(false);
        let files = upload_tree(&mut fs, &src, "dest", &cancel).unwrap();
        assert_eq!(files, 2);
        assert_eq!(fs.read("dest/x.txt").unwrap(), b"ex");
        assert_eq!(fs.read("dest/inner/y.txt").unwrap(), b"why");
        let _ = std::fs::remove_dir_all(&src);
    }

    // ---------------------------------------------------------------------------------------------
    // CPE-1741 — `upload_tree` against a `mkdir` that has REAL `mkdir(2)` semantics: refuses an
    // existing name, refuses a missing parent. `FakeProvider::mkdir` is `mkdir -p` and always
    // idempotent, which is exactly the "the test double is more forgiving than the wire" shape
    // CPE-1731 named — so these tests wrap it in `StrictMkdirProvider`, a thin `mkdir` override that
    // gives the same two refusals a real SFTP/FTP server gives, with the same error TEXT a real
    // provider produces (indistinguishable-from-generic-failure for "already exists", per the
    // `ensure_dir` doc above) so a fix that cheated by parsing `mkdir`'s message could not pass here.
    // Everything else delegates straight to the inner `FakeProvider`.
    // ---------------------------------------------------------------------------------------------

    /// Wraps a [`FakeProvider`] but gives `mkdir` real, non-idempotent `mkdir(2)` semantics: `EEXIST`
    /// for a name already there, `ENOENT` for a missing parent — the two symptoms CPE-1741 is about.
    /// The error text on both branches deliberately mirrors what a real backend's message looks like
    /// once run through this crate's own `{path}: {e}` formatting (SFTP's `"Failure: Failure"` for
    /// EEXIST — the same text a generic failure gets — and `"No such file: No such file"` for ENOENT),
    /// so nothing in `ensure_dir` could pass by matching a convenient fake string that a real backend
    /// would never actually send.
    struct StrictMkdirProvider(FakeProvider);

    impl FileSystemProvider for StrictMkdirProvider {
        fn list(&self, path: &str) -> Result<Vec<ProviderEntry>, String> {
            self.0.list(path)
        }
        fn stat(&self, path: &str) -> Result<ProviderEntry, String> {
            self.0.stat(path)
        }
        fn read(&self, path: &str) -> Result<Vec<u8>, String> {
            self.0.read(path)
        }
        fn write(&mut self, path: &str, data: &[u8]) -> Result<(), String> {
            self.0.write(path, data)
        }
        fn delete(&mut self, path: &str) -> Result<(), String> {
            self.0.delete(path)
        }
        fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
            self.0.rename(from, to)
        }
        fn mkdir(&mut self, path: &str) -> Result<(), String> {
            if self.0.stat(path).is_ok() {
                return Err(format!("{path}: Failure: Failure")); // EEXIST, indistinguishable from a generic failure
            }
            let parent = path.rfind('/').map(|i| &path[..i]).unwrap_or("");
            if !parent.is_empty() {
                match self.0.stat(parent) {
                    Err(_) => return Err(format!("{path}: No such file: No such file")), // ENOENT — no recursive create
                    Ok(e) if !e.is_dir => return Err(format!("{path}: Failure: Failure")), // ENOTDIR — parent is a file
                    Ok(_) => {}
                }
            }
            self.0.mkdir(path) // parent verified present, name verified absent: a real single-level mkdir
        }
        fn capabilities(&self) -> ProviderCapabilities {
            self.0.capabilities()
        }
    }

    /// A provider whose `mkdir` always fails for a reason that is NOT "already exists" (a stand-in for
    /// permission-denied or a genuinely broken remote) — proves `ensure_dir`'s already-exists tolerance
    /// does not swallow a real failure. `stat` still delegates honestly, so `ensure_dir`'s post-`mkdir`
    /// recheck correctly reports "still not there" and the original error surfaces.
    struct AlwaysDeniedMkdir(FakeProvider);

    impl FileSystemProvider for AlwaysDeniedMkdir {
        fn list(&self, path: &str) -> Result<Vec<ProviderEntry>, String> {
            self.0.list(path)
        }
        fn stat(&self, path: &str) -> Result<ProviderEntry, String> {
            self.0.stat(path)
        }
        fn read(&self, path: &str) -> Result<Vec<u8>, String> {
            self.0.read(path)
        }
        fn write(&mut self, path: &str, data: &[u8]) -> Result<(), String> {
            self.0.write(path, data)
        }
        fn delete(&mut self, path: &str) -> Result<(), String> {
            self.0.delete(path)
        }
        fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
            self.0.rename(from, to)
        }
        fn mkdir(&mut self, path: &str) -> Result<(), String> {
            Err(format!("{path}: Permission denied"))
        }
        fn capabilities(&self) -> ProviderCapabilities {
            self.0.capabilities()
        }
    }

    /// A `src` scratch dir cleaned up via `Drop`, armed before any assertion runs (per this shift's
    /// leaked-scratch-dir finding) rather than on the last line of the test, where an early `panic!`
    /// from a failed assertion would skip it.
    struct ScratchDirGuard(std::path::PathBuf);
    impl Drop for ScratchDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn one_file_src(tag: &str) -> (std::path::PathBuf, ScratchDirGuard) {
        let src = std::env::temp_dir().join(format!("cpe-xfer-1741-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&src);
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("z.txt"), b"zed").unwrap();
        // An empty subdirectory: `FakeProvider::write` invents ancestors via `ensure_ancestors`, so a
        // directory holding a file proves nothing about whether `mkdir` itself ran. Only a successful
        // `mkdir` can produce a directory with nothing inside it — CPE-1741 F4.
        std::fs::create_dir(src.join("empty")).unwrap();
        (src.clone(), ScratchDirGuard(src))
    }

    #[test]
    fn upload_tree_succeeds_when_the_remote_base_directory_already_exists() {
        let src = std::env::temp_dir().join(format!("cpe-xfer-1741-exists-{}", std::process::id()));
        let _guard = ScratchDirGuard(src.clone()); // armed before any assertion
        let _ = std::fs::remove_dir_all(&src);
        std::fs::create_dir_all(src.join("inner")).unwrap();
        std::fs::write(src.join("x.txt"), b"ex").unwrap();
        std::fs::write(src.join("inner").join("y.txt"), b"why").unwrap();

        let mut fs = StrictMkdirProvider(FakeProvider::new());
        fs.mkdir("dest").unwrap(); // seed: the remote base ALREADY EXISTS before we upload into it
        let cancel = AtomicBool::new(false);
        let files = upload_tree(&mut fs, &src, "dest", &cancel)
            .expect("uploading into an existing remote base must succeed (CPE-1741)");
        assert_eq!(files, 2);
        assert_eq!(fs.read("dest/x.txt").unwrap(), b"ex");
        assert_eq!(fs.read("dest/inner/y.txt").unwrap(), b"why");
    }

    #[test]
    fn upload_tree_succeeds_for_a_multi_level_remote_root_whose_parents_do_not_exist() {
        let (src, _guard) = one_file_src("multilevel"); // guard armed before any assertion

        let mut fs = StrictMkdirProvider(FakeProvider::new()); // neither "a" nor "a/b" exists
        let cancel = AtomicBool::new(false);
        let files = upload_tree(&mut fs, &src, "a/b", &cancel)
            .expect("a multi-level remote_root with missing parents must succeed (CPE-1741)");
        assert_eq!(files, 1);
        assert_eq!(fs.read("a/b/z.txt").unwrap(), b"zed");
        assert!(
            fs.stat("a/b/empty").unwrap().is_dir,
            "the mkdir chain, not write's ensure_ancestors, must have made this"
        );
    }

    #[test]
    fn upload_tree_succeeds_when_a_nested_directory_already_exists_remotely() {
        // The partial-re-upload case (transfer.rs:761's own CPE-1741 shape): a directory INSIDE the
        // tree, not just the base, is already there remotely.
        let src = std::env::temp_dir().join(format!("cpe-xfer-1741-partial-{}", std::process::id()));
        let _guard = ScratchDirGuard(src.clone()); // armed before any assertion
        let _ = std::fs::remove_dir_all(&src);
        std::fs::create_dir_all(src.join("inner")).unwrap();
        std::fs::write(src.join("inner").join("y.txt"), b"why").unwrap();

        let mut fs = StrictMkdirProvider(FakeProvider::new());
        fs.mkdir("dest").unwrap();
        fs.mkdir("dest/inner").unwrap(); // seed: "inner" already exists remotely (a prior partial upload)
        let cancel = AtomicBool::new(false);
        let files = upload_tree(&mut fs, &src, "dest", &cancel)
            .expect("re-uploading over an already-existing nested dir must succeed (CPE-1741)");
        assert_eq!(files, 1);
        assert_eq!(fs.read("dest/inner/y.txt").unwrap(), b"why");
    }

    #[test]
    fn upload_tree_reports_a_clear_error_when_the_remote_base_is_a_file_not_a_directory() {
        let (src, _guard) = one_file_src("basefile"); // guard armed before any assertion

        let mut fs = StrictMkdirProvider(FakeProvider::new());
        fs.write("dest", b"i am a file, not a directory").unwrap(); // seed: "dest" exists as a FILE
        let cancel = AtomicBool::new(false);
        let err = upload_tree(&mut fs, &src, "dest", &cancel)
            .expect_err("uploading into a remote path that is a file must fail, not silently succeed");
        assert!(err.contains("not a directory"), "expected a clear diagnosis, got {err:?}");
    }

    #[test]
    fn upload_tree_surfaces_a_genuine_mkdir_failure_rather_than_swallowing_it() {
        // CPE-1741's already-exists tolerance must not blur into "ignore every mkdir error": a
        // permission-denied (or otherwise genuinely broken) remote must still fail the transfer.
        let (src, _guard) = one_file_src("denied"); // guard armed before any assertion

        let mut fs = AlwaysDeniedMkdir(FakeProvider::new());
        let cancel = AtomicBool::new(false);
        let err = upload_tree(&mut fs, &src, "dest", &cancel)
            .expect_err("a real mkdir failure must surface, not be swallowed by the already-exists tolerance");
        assert!(err.contains("Permission denied"), "expected the real mkdir error, got {err:?}");
    }

    // ---------------------------------------------------------------------------------------------
    // CPE-1461 (path traversal) + CPE-1462 (unbounded walk/accumulation DoS) hardening battery.
    // ---------------------------------------------------------------------------------------------

    /// The canonical set of hostile entry names a remote server could return (the ticket's list). Every
    /// one must be neutralized: either rejected outright, or contained strictly inside the download root.
    const TRAVERSAL_INPUTS: &[&str] = &[
        "../../../../../home/x/.bashrc",                                                              // unix relative escape
        r"C:\Users\x\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup\evil.bat",          // windows drive-absolute
        r"\\host\share\x",                                                                             // UNC
        r"\x",                                                                                         // rooted
        r"x\..\..\y",                                                                                  // backslash-separated `..`
        "a/../../b",                                                                                   // mixed `..`
        "%2e%2e",                                                                                      // percent-encoded (literal at the sink)
    ];

    #[test]
    fn guarded_join_never_escapes_the_base() {
        let base = std::env::temp_dir().join("cpe-gj-base-dir");
        // The security invariant: for EVERY hostile input, the join is either rejected or stays under base.
        for inp in TRAVERSAL_INPUTS {
            if let Some(p) = guarded_join(&base, inp) {
                assert!(p.starts_with(&base), "guarded_join({inp:?}) escaped base: {p:?}");
            }
        }
        // The clearly-escaping ones must be rejected outright on EVERY platform (we split on `\` too, so a
        // backslash-separated `..` is caught even on Unix, where `\` is otherwise a legal filename char).
        assert!(guarded_join(&base, "../../../../../home/x/.bashrc").is_none());
        assert!(guarded_join(&base, "a/../../b").is_none());
        assert!(guarded_join(&base, r"x\..\..\y").is_none());
        assert!(guarded_join(&base, "..").is_none());
        // A legit nested path is preserved exactly (no over-rejection).
        let ok = guarded_join(&base, "normal/nested/file.txt").expect("legit nested must join");
        assert_eq!(ok, base.join("normal").join("nested").join("file.txt"));
    }

    #[test]
    fn is_safe_name_accepts_leaves_and_rejects_paths() {
        for good in ["readme.txt", "my file (1).txt", "résumé.pdf", ".hidden"] {
            assert!(is_safe_name(good), "{good:?} should be safe");
        }
        for bad in [
            "", ".", "..", "a/b", r"a\b", "/etc", r"\x", "a\0b", r"C:\x", "sub/",
            // Windows ADS / drive-selector + `..`-prefixed leaves (CPE-1461 hardening):
            "x:y", "..:stream", "..::$DATA", "file:$DATA", "..evil", "C:",
        ] {
            assert!(!is_safe_name(bad), "{bad:?} should be unsafe");
        }
    }

    /// A provider whose root listing returns exactly the hostile names it was handed (as regular files),
    /// and nothing for any other directory — so `download_tree` tries to write each name straight into the
    /// download root. `read` returns a payload that would be the planted file's contents.
    struct HostileNames {
        names: Vec<String>,
    }
    impl FileSystemProvider for HostileNames {
        fn list(&self, path: &str) -> Result<Vec<ProviderEntry>, String> {
            if path.is_empty() {
                Ok(self.names.iter().map(|n| ProviderEntry { name: n.clone(), is_dir: false, size: 3 }).collect())
            } else {
                Ok(vec![])
            }
        }
        fn read(&self, _path: &str) -> Result<Vec<u8>, String> {
            Ok(b"pwn".to_vec())
        }
        fn stat(&self, _: &str) -> Result<ProviderEntry, String> {
            Err("unsupported".into())
        }
        fn write(&mut self, _: &str, _: &[u8]) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn mkdir(&mut self, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn delete(&mut self, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn rename(&mut self, _: &str, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
    }

    #[test]
    fn download_tree_neutralizes_every_traversal_input() {
        let base = std::env::temp_dir().join(format!("cpe-xfer-trav-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        // Sentinel targets that MUST NOT be created outside the download root.
        let parent = base.parent().unwrap().to_path_buf();
        let sentinel_up = parent.join("cpe-PWNED-marker.txt");
        let _ = std::fs::remove_file(&sentinel_up);

        // Hostile names: the full traversal set, plus concrete single-level escapes aimed at a sentinel.
        let mut names: Vec<String> = TRAVERSAL_INPUTS.iter().map(|s| s.to_string()).collect();
        names.push("../cpe-PWNED-marker.txt".into());
        names.push(r"..\cpe-PWNED-marker.txt".into());
        names.push("sub/../../cpe-PWNED-marker.txt".into());

        let provider = HostileNames { names };
        let cancel = AtomicBool::new(false);
        let n = download_tree(&provider, "", &base, &cancel).expect("hostile transfer must not error, just skip").files;

        // The escaping sentinel must not exist anywhere outside the root.
        assert!(!sentinel_up.exists(), "path traversal escaped the download root: {sentinel_up:?}");

        // Whatever WAS written (a contained input like `%2e%2e`, or a UNC/drive path contained on Unix)
        // must live strictly inside the download root — nothing escaped.
        let mut stack = vec![base.clone()];
        let mut written = 0usize;
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                assert!(p.starts_with(&base), "a written path escaped the root: {p:?}");
                if p.is_dir() {
                    stack.push(p);
                } else {
                    written += 1;
                }
            }
        }
        assert_eq!(written, n, "reported file count must match what actually landed under the root");
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_file(&sentinel_up);
    }

    #[test]
    fn download_tree_still_downloads_a_legit_nested_tree() {
        // Regression guard: the hardening must not over-reject an ordinary tree.
        let fs = seeded();
        let out = std::env::temp_dir().join(format!("cpe-xfer-legit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let cancel = AtomicBool::new(false);
        let files = download_tree(&fs, "", &out, &cancel).unwrap().files;
        assert_eq!(files, 2);
        assert_eq!(std::fs::read(out.join("a.txt")).unwrap(), b"alpha");
        assert_eq!(std::fs::read(out.join("sub").join("b.txt")).unwrap(), b"bravo");
        let _ = std::fs::remove_dir_all(&out);
    }

    // Symlink-escape defenses (CPE-1461 review follow-up). Gated to Unix: creating a symlink on Windows
    // needs admin/developer-mode, and the fix (validate-before-mutate + no-follow leaf) is cross-platform
    // — these tests just need a real symlink to exercise it.

    /// A provider that reports one directory `d` (holding a file `d/inner.txt`), so `download_tree` will
    /// want to `mkdir local_dir/d` and then write into it.
    struct DirThenFile;
    impl FileSystemProvider for DirThenFile {
        fn list(&self, path: &str) -> Result<Vec<ProviderEntry>, String> {
            if path.is_empty() {
                Ok(vec![ProviderEntry { name: "d".into(), is_dir: true, size: 0 }])
            } else if path == "d" {
                Ok(vec![ProviderEntry { name: "inner.txt".into(), is_dir: false, size: 3 }])
            } else {
                Ok(vec![])
            }
        }
        fn read(&self, _: &str) -> Result<Vec<u8>, String> {
            Ok(b"pwn".to_vec())
        }
        fn stat(&self, _: &str) -> Result<ProviderEntry, String> {
            Err("unsupported".into())
        }
        fn write(&mut self, _: &str, _: &[u8]) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn mkdir(&mut self, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn delete(&mut self, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn rename(&mut self, _: &str, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
    }

    #[cfg(unix)]
    #[test]
    fn download_tree_does_not_create_a_child_through_a_preexisting_symlinked_dir() {
        use std::os::unix::fs::symlink;
        let pid = std::process::id();
        let root = std::env::temp_dir().join(format!("cpe-symdir-root-{pid}"));
        let evil = std::env::temp_dir().join(format!("cpe-symdir-evil-{pid}"));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&evil);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&evil).unwrap();
        // Plant `root/d` as a symlink to the outside `evil` directory (as some other channel might).
        symlink(&evil, root.join("d")).unwrap();

        let cancel = AtomicBool::new(false);
        // Must NOT error the whole transfer, and must NOT create anything inside `evil`.
        let _ = download_tree(&DirThenFile, "", &root, &cancel).expect("must skip, not fail");
        assert!(
            std::fs::read_dir(&evil).unwrap().next().is_none(),
            "a child was created outside the root by following a symlinked directory"
        );
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&evil);
    }

    /// A provider that reports a single top-level file `target.txt`.
    struct OneFile;
    impl FileSystemProvider for OneFile {
        fn list(&self, path: &str) -> Result<Vec<ProviderEntry>, String> {
            if path.is_empty() {
                Ok(vec![ProviderEntry { name: "target.txt".into(), is_dir: false, size: 3 }])
            } else {
                Ok(vec![])
            }
        }
        fn read(&self, _: &str) -> Result<Vec<u8>, String> {
            Ok(b"pwn".to_vec())
        }
        fn stat(&self, _: &str) -> Result<ProviderEntry, String> {
            Err("unsupported".into())
        }
        fn write(&mut self, _: &str, _: &[u8]) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn mkdir(&mut self, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn delete(&mut self, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn rename(&mut self, _: &str, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
    }

    /// **CPE-1913 round 2, the Reviewer's finding A: a destination handle that cannot be described must
    /// REFUSE, and the transfer must not report success for it.**
    ///
    /// Round 1 moved this leg's hard-link and directoriness questions off `batch_media::name_links` (a
    /// path probe) and onto the write handle, which was right — and dropped the *"could not tell"*
    /// answer with them, which was not. Every one of those questions sits inside an
    /// `if let Some(facts) = handle_facts(&w)`, and round 1 let a `None` fall through to the write. The
    /// code it replaced refused: `NameLinks::Unknown` went into `undelivered` and failed the transfer,
    /// with a doc that said in as many words *"a gate that cannot tell must REFUSE"*.
    ///
    /// This pins the restored arm at the leg where the harm is visible. The fixture is a real hard link
    /// to a file outside the download folder, so a fall-through does not merely write something
    /// unchecked — it overwrites a bystander, which is CPE-1857's measured harm arriving silently.
    ///
    /// `ProbeInjection::HandleUndescribable` is the seam, for the same reason its two siblings exist:
    /// `GetFileInformationByHandle` on a handle the OS just returned does not fail on any filesystem a
    /// test can reach, and `File::metadata` on a live fd is close to unfailable. A fail-open no test can
    /// reach is one that comes back.
    #[test]
    fn cpe_1913_a_destination_whose_handle_cannot_be_described_is_never_written_through() {
        let d = crate::fsutil::scratch_dir("cpe1913-xfer-blind-handle");
        let root = d.join("root");
        let outside = d.join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let victim = outside.join("victim.txt");
        std::fs::write(&victim, b"OUTSIDE CONTENT").unwrap();
        if std::fs::hard_link(&victim, root.join("target.txt")).is_err() {
            crate::skip_notice!(
                "SKIPPING cpe_1913_a_destination_whose_handle_cannot_be_described_is_never_written_through: \
                 no hard-link support here — NOTHING on this run covered the undescribable-handle fail-open"
            );
            return;
        }
        // Liveness: the two names must really be one object, or a green here proves nothing.
        std::fs::write(&victim, b"OUTSIDE CONTENT").unwrap();
        assert_eq!(
            std::fs::read(root.join("target.txt")).ok().as_deref(),
            Some(&b"OUTSIDE CONTENT"[..]),
            "fixture is inert: the leaf and the outside file are not one object"
        );

        let cancel = AtomicBool::new(false);
        let outcome = {
            let _reset = crate::batch_media::ProbeReset::arm(
                crate::batch_media::ProbeInjection::HandleUndescribable,
            );
            download_tree(&OneFile, "", &root, &cancel)
        };

        // HARM FIRST, off the filesystem.
        assert_eq!(
            std::fs::read(&victim).ok().as_deref(),
            Some(&b"OUTSIDE CONTENT"[..]),
            "HARM: the gate could not describe the handle it was about to write through and wrote the \
             remote server's bytes anyway, into a file outside the download folder: {outcome:?}"
        );
        let err = outcome.expect_err(
            "a destination that cannot be described is a file the user asked for and did not get, so \
             the transfer must not end Ok — that is the route `NameLinks::Unknown` took before CPE-1913 \
             and the route CPE-1709 F1 exists to keep open",
        );
        assert!(
            err.contains("could not check how many names"),
            "and it must be THIS guard's wording, not an incidental failure from elsewhere — those are \
             the same red for opposite reasons and only the string tells them apart: {err}"
        );
    }

    /// A provider serving one file in a subdirectory the remote server names — `sub/target.txt`.
    /// Separate from [`OneFile`] because the whole point of CPE-1913's transfer harm test is that the
    /// **remote** chooses an intermediate path component, which is what a junction planted locally can
    /// then redirect.
    struct OneNestedFile;
    impl FileSystemProvider for OneNestedFile {
        fn list(&self, path: &str) -> Result<Vec<ProviderEntry>, String> {
            match path {
                "" => Ok(vec![ProviderEntry { name: "sub".into(), is_dir: true, size: 0 }]),
                // **`deeper` is not decoration — CPE-1913 round 2, the Reviewer's finding B.** With
                // `sub` as the only directory entry, the directory guard could not be red-proofed on
                // its own: `dl/sub` already exists (it IS the junction), so sabotaging
                // `create_dir_beneath` back to `create_dir_all` produced no observable debris and the
                // harm test stayed green. A directory entry *below* the junction is the one shape that
                // makes a by-path `create_dir_all` build something on the far side of it, which is what
                // `!elsewhere.join("deeper").exists()` then asserts on.
                "sub" => Ok(vec![
                    ProviderEntry { name: "deeper".into(), is_dir: true, size: 0 },
                    ProviderEntry { name: "target.txt".into(), is_dir: false, size: 11 },
                ]),
                _ => Ok(vec![]),
            }
        }
        fn read(&self, _: &str) -> Result<Vec<u8>, String> {
            Ok(b"REMOTE BYTES".to_vec())
        }
        fn stat(&self, _: &str) -> Result<ProviderEntry, String> {
            Err("unsupported".into())
        }
        fn write(&mut self, _: &str, _: &[u8]) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn mkdir(&mut self, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn delete(&mut self, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn rename(&mut self, _: &str, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
    }

    /// **CPE-1913's harm test for the transfer leg, in both directions a junction can point.**
    ///
    /// A directory junction is planted at `dl/sub` — the exact component the remote server names — and
    /// the transfer is run twice: once with the junction leading **outside** the download folder, once
    /// with it leading to another folder **inside** it.
    ///
    /// Both cases passed every guard this leg had before CPE-1913, for different reasons and with the
    /// same outcome (`Ok`, the file elsewhere, nothing said):
    ///
    /// - **Outside** — the containment check canonicalised the deepest *existing* ancestor. For a file
    ///   entry that ancestor is `dl/sub`, whose canonical form is the junction's target, so
    ///   `starts_with(root)` was false and the entry was skipped — with an `eprintln!` and an `Ok`
    ///   verdict, which is the silent-success shape this ticket is named for. The **directory** entry
    ///   that precedes it was skipped the same way, so the transfer reported success for a tree it had
    ///   not delivered.
    /// - **Inside** — `dl/other` is under the root, so containment was satisfied and the write went
    ///   through, `files += 1`, `Ok`. This is CPE-1912's shape at the transfer leg: no race, no thread,
    ///   nothing any path check can see.
    ///
    /// Now every component is opened relative to the one before it, so a junction at `sub` stops the
    /// entry wherever it points, and the refusal reaches the caller in `DownloadReport::skipped`
    /// instead of stderr.
    #[test]
    fn cpe_1913_a_junction_inside_the_download_folder_never_redirects_an_entry() {
        for point_outside in [true, false] {
            let d = crate::fsutil::scratch_dir("cpe1913-xfer-junction");
            let dl = d.join("dl");
            let elsewhere =
                if point_outside { d.join("outside") } else { dl.join("other") };
            std::fs::create_dir_all(&dl).unwrap();
            std::fs::create_dir_all(&elsewhere).unwrap();
            if !crate::fsutil::make_dir_link(&elsewhere, &dl.join("sub")) {
                crate::skip_notice!(
                    "SKIPPING cpe_1913_a_junction_inside_the_download_folder_never_redirects_an_entry: \
                     could not stage a directory link. NOTHING on this run covered the transfer leg's \
                     redirected-component hole"
                );
                return;
            }
            // Liveness: the fixture must really redirect, or the test certifies nothing.
            std::fs::write(dl.join("sub/liveness.txt"), b"through").unwrap();
            assert_eq!(
                std::fs::read(elsewhere.join("liveness.txt")).ok().as_deref(),
                Some(&b"through"[..]),
                "fixture is inert: the junction at dl/sub does not redirect (point_outside={point_outside})"
            );
            std::fs::remove_file(dl.join("sub/liveness.txt")).unwrap();

            let cancel = AtomicBool::new(false);
            let report = download_tree(&OneNestedFile, "", &dl, &cancel);

            // HARM FIRST, off the filesystem, before any verdict is inspected.
            assert!(
                !elsewhere.join("target.txt").exists(),
                "HARM: the download wrote the remote server's bytes through a junction at dl/sub into \
                 {}, which the user never named (point_outside={point_outside})",
                elsewhere.display()
            );
            // **The DIRECTORY leg's own harm, which the file assertion above cannot see** (CPE-1913
            // round 2, finding B). `create_dir_all` is not destructive, so a redirected directory entry
            // writes no bytes — it silently builds the remote tree's shape somewhere the user never
            // named, and the deeper the tree the more of it goes out there. This is the assertion that
            // reddens when `create_dir_beneath` alone is sabotaged; without it, the directory guard was
            // only ever proven by the file guard standing next to it.
            assert!(
                !elsewhere.join("deeper").exists(),
                "HARM: the download created the remote tree's `sub/deeper` directory through a junction \
                 at dl/sub, inside {}, which the user never named (point_outside={point_outside})",
                elsewhere.display()
            );
            // And the refusal must REACH THE CALLER. `eprintln!` + `Ok` is the shape CPE-1913 exists
            // to remove: a transfer that says it worked and did not.
            let skipped: Vec<String> = match &report {
                Ok(r) => r.skipped.clone(),
                Err(e) => vec![e.clone()],
            };
            assert!(
                skipped.iter().any(|m| m.contains("target.txt") && m.contains("is a link")),
                "the refusal must reach the caller and name the redirecting component as a link \
                 (point_outside={point_outside}): {report:?}"
            );
            assert_eq!(
                report.as_ref().map(|r| r.files).unwrap_or(0),
                0,
                "nothing was delivered, so nothing may be counted as delivered: {report:?}"
            );
        }
    }

    /// **CPE-1857, the transfer half — and unlike its symlink sibling below this one runs everywhere**,
    /// because a hard link needs no privilege on any of the three platforms CI builds on.
    ///
    /// The remote server chooses the entry path, so it chooses which local name the bytes land on. A
    /// pre-existing hard link at that name is a second name for a file that may live anywhere: `lstat`
    /// reports an ordinary regular file (which is exactly what it is), `guarded_join` and the
    /// containment walk both pass it, and all of them are *right* — a hard link resolves to itself, so
    /// the name really is inside the download root. `fs::write` then lands the remote bytes in the
    /// **inode**, and they come out at the other name too.
    ///
    /// **Reported exactly like its symlink neighbour — [`DownloadReport::skipped`], not an `undelivered`
    /// entry — and that was a decision, not a default.** `undelivered` makes `download_tree` return `Err`
    /// for the WHOLE transfer, which is right for its own class ("this filesystem refused this name") and
    /// wrong for this one: a hard-linked leaf is a per-entry *policy* skip, exactly like a symlinked
    /// leaf, and one collided name must not cost the user every other file in the tree. Sending a
    /// hard-linked leaf down the `undelivered` path while a symlinked leaf skips would also be an
    /// asymmetry with nothing behind it. Measured on the first cut of this test, which reddened with
    /// `must skip, not fail: "transfer: delivered 0 file(s), but 1 could not be written…"`.
    ///
    /// **CPE-1881 addition, pinned here rather than only in the wording change:** before this ticket, the
    /// skip above reached nowhere but stderr — `report.skipped` did not exist, so a caller had `n == 0`
    /// and nothing else. This test now also proves the reason reaches the return value.
    #[test]
    fn cpe_1857_download_tree_never_writes_through_a_preexisting_hard_linked_leaf() {
        let d = crate::fsutil::scratch_dir("cpe-transfer-1857");
        let root = d.join("root");
        let outside = d.join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let victim = outside.join("victim.txt");
        std::fs::write(&victim, b"placeholder").unwrap();
        if std::fs::hard_link(&victim, root.join("target.txt")).is_err() {
            crate::skip_notice!(
                "SKIPPING cpe_1857_download_tree_never_writes_through_a_preexisting_hard_linked_leaf: \
                 no hard-link support on this filesystem — NOTHING on this run covered the hard-link hole"
            );
            return;
        }
        // Liveness, the only way a hard link can be proved live: write through the OUTSIDE name and read
        // it back through the IN-TREE one. `OneFile` serves exactly three bytes, so the victim's content
        // is a different length as well as different bytes.
        std::fs::write(&victim, b"original").unwrap();
        assert_eq!(
            std::fs::read(root.join("target.txt")).ok().as_deref(),
            Some(&b"original"[..]),
            "fixture is inert: the leaf and the outside file are not one object, so this run could not \
             have tested writing through a hard link at all"
        );

        let cancel = AtomicBool::new(false);
        let report = download_tree(&OneFile, "", &root, &cancel).expect("must skip, not fail");

        // HARM FIRST, on the filesystem, before any claim about what was reported.
        assert_eq!(
            std::fs::read(&victim).ok().as_deref(),
            Some(&b"original"[..]),
            "HARM: the download wrote the remote server's bytes through a hard link, into a file \
             outside the download root that the user never named"
        );
        assert_eq!(report.files, 0, "the hard-linked leaf must be skipped, not counted as delivered");
        // CPE-1881: before this ticket, this was the ONLY signal a caller had — an `eprintln!` on
        // stderr, nothing on the return value. The skip must now be visible AND name the leaf and why.
        assert_eq!(report.skipped.len(), 1, "the hard-linked skip must be reported, not silent: {report:?}");
        assert!(
            report.skipped[0].contains("target.txt") && report.skipped[0].contains("hard-linked"),
            "the reported skip must name the entry and the reason: {report:?}"
        );
    }

    /// **CPE-1857 Security-Auditor finding 1, at the transfer gate — re-aimed by CPE-1913.**
    ///
    /// The finding was that the transfer's hard-link gate asked `batch_media::name_links`, a **path**
    /// probe, and folded two of its answers wrongly: a degenerate identity (a network redirector's
    /// zero file index) discarded a perfectly readable link count, and a genuinely unreadable probe
    /// fell through to the write. Both were staged through `ProbeInjection`, a test seam on the path
    /// probe, because neither shape can be conjured on CI.
    ///
    /// **CPE-1913 removed the question rather than re-answering it.** The count now comes off the
    /// **write handle** (`fsutil::claim_destination_handle` → `batch_media::handle_facts`), which is
    /// the same object the bytes would enter, so there is no second path lookup to be blinded and no
    /// identity to be degenerate — `facts.links` is read directly. This test is therefore no longer a
    /// taxonomy check on the classifier; it is a **liveness check on the seam that used to matter**:
    /// arm each injection, and confirm the refusal happens anyway.
    ///
    /// That is a strictly stronger property than the one it replaces, and it is the reason the test is
    /// kept rather than deleted with the classifier: an injection that no longer changes the outcome is
    /// evidence the outcome no longer depends on the thing injected.
    #[test]
    fn cpe_1913_the_path_probe_injections_can_no_longer_blind_the_transfer_hard_link_gate() {
        let d = crate::fsutil::scratch_dir("cpe-transfer-1857-blind");
        let root = d.join("root");
        let outside = d.join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let victim = outside.join("victim.txt");
        std::fs::write(&victim, b"placeholder").unwrap();
        if std::fs::hard_link(&victim, root.join("target.txt")).is_err() {
            crate::skip_notice!(
                "SKIPPING cpe_1913_the_path_probe_injections_can_no_longer_blind_the_transfer_hard_link_gate: \
                 no hard-link support here — NOTHING on this run covered either fail-open"
            );
            return;
        }
        std::fs::write(&victim, b"original").unwrap();
        assert_eq!(
            std::fs::read(root.join("target.txt")).ok().as_deref(),
            Some(&b"original"[..]),
            "fixture is inert: the leaf and the outside file are not one object"
        );

        let cancel = AtomicBool::new(false);
        for injection in [
            crate::batch_media::ProbeInjection::DegenerateIdentity,
            crate::batch_media::ProbeInjection::Unreadable,
        ] {
            let report = {
                let _reset = crate::batch_media::ProbeReset::arm(injection);
                download_tree(&OneFile, "", &root, &cancel)
                    .expect("a hard-linked leaf is a per-entry skip, not a whole-transfer failure")
            };
            // HARM FIRST, off the filesystem.
            assert_eq!(
                std::fs::read(&victim).ok().as_deref(),
                Some(&b"original"[..]),
                "HARM: the download wrote the remote server's bytes through a hard link into a file \
                 outside the download root"
            );
            assert_eq!(report.files, 0, "the hard-linked leaf must never be counted as delivered");
            assert_eq!(
                report.skipped.len(),
                1,
                "the skip must be reported, not silent: {report:?}"
            );
            assert!(
                report.skipped[0].contains("target.txt")
                    && report.skipped[0].contains("hard-linked"),
                "the refusal must come from the HANDLE's link count, naming the entry and the reason \
                 — a message about an unreadable probe would mean the path probe is still in the \
                 decision: {report:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn download_tree_does_not_follow_a_preexisting_symlinked_leaf_on_write() {
        use std::os::unix::fs::symlink;
        let pid = std::process::id();
        let root = std::env::temp_dir().join(format!("cpe-symleaf-root-{pid}"));
        let outside = std::env::temp_dir().join(format!("cpe-symleaf-outside-{pid}.txt"));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&outside);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&outside, b"original").unwrap();
        // Plant `root/target.txt` as a symlink to the outside file.
        symlink(&outside, root.join("target.txt")).unwrap();

        let cancel = AtomicBool::new(false);
        let report = download_tree(&OneFile, "", &root, &cancel).expect("must skip, not fail");
        assert_eq!(report.files, 0, "the symlinked leaf must be skipped, not written");
        assert_eq!(
            std::fs::read(&outside).unwrap(),
            b"original",
            "the write followed a symlink and clobbered a file outside the root"
        );
        // CPE-1881 (item 3 — "the adjacent symlink arm has the same stderr-only shape"): the skip must
        // now reach the caller, not just stderr.
        assert_eq!(report.skipped.len(), 1, "the symlink skip must be reported, not silent: {report:?}");
        assert!(
            report.skipped[0].contains("target.txt") && report.skipped[0].contains("symlink"),
            "the reported skip must name the entry and the reason: {report:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&outside);
    }

    /// A provider that advertises an infinitely deep tree: every `list` returns one fresh child directory.
    struct InfiniteDepth;
    impl FileSystemProvider for InfiniteDepth {
        fn list(&self, _path: &str) -> Result<Vec<ProviderEntry>, String> {
            Ok(vec![ProviderEntry { name: "a".into(), is_dir: true, size: 0 }])
        }
        fn read(&self, _: &str) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
        fn stat(&self, _: &str) -> Result<ProviderEntry, String> {
            Err("unsupported".into())
        }
        fn write(&mut self, _: &str, _: &[u8]) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn mkdir(&mut self, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn delete(&mut self, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn rename(&mut self, _: &str, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
    }

    #[test]
    fn walk_depth_cap_terminates_an_infinitely_deep_tree() {
        let p = InfiniteDepth;
        let cancel = AtomicBool::new(false);
        let mut count = 0usize;
        // Without the depth cap this never returns; with it, it must terminate quickly and bounded.
        let visited = walk(&p, "", &cancel, |_| count += 1).unwrap();
        assert_eq!(visited, count);
        assert!(visited <= MAX_WALK_DEPTH + 1, "depth cap must bound the walk; got {visited}");
    }

    /// A provider that advertises a tree far larger than the entry cap: 1000 subdirs, each with 1000
    /// files (~1,001,000 entries), so the total-entries cap fires. Per-call listings stay small (1000
    /// entries), so the test's own memory is modest.
    struct HugeTree;
    impl FileSystemProvider for HugeTree {
        fn list(&self, path: &str) -> Result<Vec<ProviderEntry>, String> {
            if path.is_empty() {
                Ok((0..1000).map(|i| ProviderEntry { name: format!("d{i}"), is_dir: true, size: 0 }).collect())
            } else {
                Ok((0..1000).map(|i| ProviderEntry { name: format!("f{i}"), is_dir: false, size: 1 }).collect())
            }
        }
        fn read(&self, _: &str) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
        fn stat(&self, _: &str) -> Result<ProviderEntry, String> {
            Err("unsupported".into())
        }
        fn write(&mut self, _: &str, _: &[u8]) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn mkdir(&mut self, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn delete(&mut self, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
        fn rename(&mut self, _: &str, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
    }

    #[test]
    fn walk_entry_cap_aborts_a_huge_tree() {
        let p = HugeTree;
        let cancel = AtomicBool::new(false);
        let err = walk(&p, "", &cancel, |_| {}).unwrap_err();
        assert!(err.contains("safety cap"), "expected a safety-cap abort, got: {err}");
    }

    // **CPE-1696's four unit tests lived here and went with the code they pinned (CPE-1913).** They
    // asserted the taxonomies of `classify_ancestor_probe` and `classify_leaf_probe` — that an `lstat`
    // which failed for any reason other than `NotFound` must never be read as "nothing here". Both
    // classifiers are deleted: `download_tree` no longer `lstat`s anything before writing, because it
    // no longer writes by path. The property they existed to protect — a level that cannot be
    // inspected must not be stepped over — is now structural rather than classified: the walk in
    // `open_beneath` opens every component, and a component it cannot open stops the entry. There is
    // no "keep climbing" branch left to get wrong.
    //
    // What replaces them as coverage is a harm test, not a taxonomy test:
    // `cpe_1913_a_junction_inside_the_download_folder_never_redirects_an_entry` asserts on where the
    // bytes went.

    // ---------------------------------------------------------------------------------------------
    // CPE-1709: a name the LOCAL filesystem cannot hold must not silently lose the file's contents.
    //
    // The bug: `guarded_join` had no `:` rule. It never needed one while every remote listing was
    // pre-filtered by `is_safe_name` (which refuses `:`); CPE-1704 correctly relaxed that for S3, where
    // `:` is an ordinary legal key byte, and removed the accidental protection the sink was leaning on.
    // `fs::write("…/colon:name.txt", b"…")` then returned `Ok(())` while diverting the bytes into an
    // NTFS alternate data stream, leaving the user a 0-byte file named `colon`.
    //
    // Every test below therefore asserts on **the file the user would open**, never on the `Result` of
    // the write — the write was already returning success when the data was being lost.
    // ---------------------------------------------------------------------------------------------

    /// The enumerated table: `(remote leaf, expected local leaf on Windows, expected local leaf
    /// elsewhere)`. The expectations are written out as literals rather than computed, so a bug in the
    /// encoder cannot make the test agree with it.
    ///
    /// Measured behaviour of each remote leaf **before** the fix, through the real sink on Windows 11
    /// Pro 26200 / NTFS — they do not group, which is why each is listed rather than reasoned about:
    /// `:` wrote a 0-byte file + a hidden stream; `< > " | ? *` and controls were refused by
    /// `CreateFileW` (os error 123) and skipped under a misleading symlink-inspection notice; a trailing
    /// dot/space produced a full-size file that no Win32 path can reopen; `NUL` produced a full-size
    /// file that reads back empty; the other device names happened to behave on this build.
    const CPE_1709_NAMES: &[(&str, &str, &str)] = &[
        ("normal.txt", "normal.txt", "normal.txt"),
        ("colon:name.txt", "colon%3Aname.txt", "colon:name.txt"),
        ("file:$DATA", "file%3A$DATA", "file:$DATA"),
        ("less<name.txt", "less%3Cname.txt", "less<name.txt"),
        ("greater>name.txt", "greater%3Ename.txt", "greater>name.txt"),
        ("quote\"name.txt", "quote%22name.txt", "quote\"name.txt"),
        ("pipe|name.txt", "pipe%7Cname.txt", "pipe|name.txt"),
        ("question?name.txt", "question%3Fname.txt", "question?name.txt"),
        ("star*name.txt", "star%2Aname.txt", "star*name.txt"),
        ("ctrl\u{1}name.txt", "ctrl%01name.txt", "ctrl\u{1}name.txt"),
        ("trailingdot.", "trailingdot%2E", "trailingdot."),
        ("trailingspace ", "trailingspace%20", "trailingspace "),
        ("CON", "%43ON", "CON"),
        ("CON.txt", "%43ON.txt", "CON.txt"),
        ("nul.txt", "%6Eul.txt", "nul.txt"),
        ("NUL", "%4EUL", "NUL"),
        ("PRN", "%50RN", "PRN"),
        ("AUX", "%41UX", "AUX"),
        ("COM1", "%43OM1", "COM1"),
        ("LPT9", "%4CPT9", "LPT9"),
        // Left alone: a `%` that does NOT begin an escape is an ordinary character.
        ("50% off.txt", "50% off.txt", "50% off.txt"),
        // Escaped: a `%` that DOES begin an escape THIS ENCODER CAN EMIT — what keeps it injective.
        ("literal%3Aname", "literal%253Aname", "literal%3Aname"),
        // Left alone (CPE-1709 R2, F2): `%2f` is a well-formed escape the encoder can NEVER emit,
        // because `guarded_join` splits on `/` and `\` before the transform runs. Escaping it renamed
        // legal, working S3 keys for nothing.
        ("report%2ffinal.txt", "report%2ffinal.txt", "report%2ffinal.txt"),
        ("city=A%2FB", "city=A%2FB", "city=A%2FB"),
        ("hash%5Cvalue", "hash%5Cvalue", "hash%5Cvalue"),
        ("pct%FFtail", "pct%FFtail", "pct%FFtail"),
        // ...and the round-2 device-name additions (F5).
        ("COM¹", "%43OM¹", "COM¹"),
        ("CONIN$", "%43ONIN$", "CONIN$"),
    ];

    /// Names that must survive a download **byte-identical**, because the local filesystem can hold
    /// them perfectly well. Round 2 (F2) found the encoder was rewriting the `%2f` family, which both
    /// renamed working keys and — by growing them — pushed some past the length ceiling into total loss.
    const CPE_1709_MUST_NOT_REWRITE: &[&str] = &[
        "50% off.txt",
        "report%2ffinal.txt",
        "city=A%2FB",
        "hash%5Cvalue",
        "pct%FFtail",
        "100%.txt",
        "a%zzb.txt",
        "ordinary.txt",
        "2026-08-13-report.json",
    ];

    /// The local leaf `CPE_1709_NAMES` expects on the OS the test is running on.
    fn expected_local<'a>(row: &(&'a str, &'a str, &'a str)) -> &'a str {
        if cfg!(windows) {
            row.1
        } else {
            row.2
        }
    }

    #[test]
    fn cpe_1709_every_enumerated_windows_hostile_name_maps_to_its_literal_expectation() {
        for row in CPE_1709_NAMES {
            assert_eq!(
                windows_safe_segment(row.0).as_ref(),
                row.1,
                "windows_safe_segment({:?}) — the enumerated Windows rule",
                row.0
            );
        }
    }

    /// The property that stops this fix from replacing one silent-loss bug with another: **no two
    /// distinct remote names can land on the same local file.** Proved by exhibiting the inverse —
    /// `decode(encode(x)) == x` forces injectivity — and by checking the outputs really are all
    /// distinct, including the adversarial pairs where one input is the *encoding* of another.
    #[test]
    fn cpe_1709_windows_name_mapping_is_injective() {
        let mut inputs: Vec<String> = CPE_1709_NAMES.iter().map(|r| r.0.to_string()).collect();
        // Adversarial pairs: a raw name and the exact text its encoding produces. A naive scheme that
        // only escaped `:` would collide these two onto one file and silently lose one of them.
        inputs.extend(["a:b".into(), "a%3Ab".into(), "a%253Ab".into(), "CON".into(), "%43ON".into()]);

        // Collisions are checked over the WHOLE table first, in their own pass: a collision is the
        // failure this test exists to name, and a round-trip assertion tripping on an earlier row would
        // otherwise mask it behind a less legible message.
        let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for inp in &inputs {
            let enc = windows_safe_segment(inp).into_owned();
            if let Some(other) = seen.insert(enc.clone(), inp.clone()) {
                assert_eq!(
                    other, *inp,
                    "COLLISION: two distinct remote names both map to the local file {enc:?} — one of \
                     the two would be silently overwritten by the other"
                );
            }
        }
        // And the inverse really is an inverse, which is what makes the absence of collisions a
        // property of the mapping rather than of this particular table.
        for inp in &inputs {
            let enc = windows_safe_segment(inp).into_owned();
            assert_eq!(
                decode_windows_safe_segment(&enc),
                *inp,
                "decode(encode({inp:?})) must return the original exactly — reversibility is what \
                 makes the mapping injective"
            );
        }
    }

    /// Over-rejection guard: an ordinary name must come out byte-identical, on every OS.
    #[test]
    fn cpe_1709_ordinary_names_are_left_untouched() {
        for good in ["readme.txt", "my file (1).txt", "résumé.pdf", ".hidden", "50% off.txt", "a.b.c"] {
            assert_eq!(windows_safe_segment(good).as_ref(), good, "{good:?} needs no rewriting");
            assert_eq!(local_safe_segment(good).as_ref(), good);
        }
    }

    /// **CPE-1709 round 2, F2.** A `%HH` the encoder can never emit is an ordinary character sequence
    /// and must survive untouched. `%2f` is the case that mattered: `guarded_join` splits on `/` and
    /// `\` *before* the transform runs, so nothing in this codebase can produce `%2F` — yet the first
    /// cut rewrote `report%2ffinal.txt` to `report%252ffinal.txt`, renaming a legal, already-working
    /// S3 key (and `city=A%2FB`, an ordinary Hive/Athena partition value).
    #[test]
    fn cpe_1709_a_percent_escape_the_encoder_cannot_emit_is_left_alone() {
        for name in CPE_1709_MUST_NOT_REWRITE {
            assert_eq!(
                windows_safe_segment(name).as_ref(),
                *name,
                "{name:?} contains no character this filesystem refuses, so it must not be rewritten"
            );
        }
        // The complement, so this test cannot pass by the encoder simply doing nothing: a `%HH` the
        // encoder CAN emit is still escaped, which is what keeps the mapping injective.
        assert_eq!(windows_safe_segment("literal%3Aname").as_ref(), "literal%253Aname");
        assert_eq!(windows_safe_segment("dev%43ON").as_ref(), "dev%2543ON");
    }

    /// The same names, end to end through the real sink: they must land under their **original** names
    /// and be readable there. This is the half that proves the rewriting regression was real data loss
    /// and not just cosmetic — a renamed file is a file the user cannot find.
    #[test]
    fn cpe_1709_names_needing_no_rewrite_arrive_under_their_original_names() {
        let out = std::env::temp_dir().join(format!("cpe-1709-norewrite-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let provider =
            HostileNames { names: CPE_1709_MUST_NOT_REWRITE.iter().map(|s| s.to_string()).collect() };
        let cancel = AtomicBool::new(false);
        let n = download_tree(&provider, "", &out, &cancel).expect("all of these are storable names").files;
        assert_eq!(n, CPE_1709_MUST_NOT_REWRITE.len());
        for name in CPE_1709_MUST_NOT_REWRITE {
            assert_eq!(
                std::fs::read(out.join(name)).unwrap_or_else(|e| panic!("{name:?}: {e}")),
                b"pwn",
                "{name:?} must land under its own name, unrewritten"
            );
        }
        let _ = std::fs::remove_dir_all(&out);
    }

    /// A remote name long enough that its **encoded** form exceeds `MAX_LOCAL_COMPONENT`. Built from the
    /// ticket's own motivating example — an ISO-8601 S3 key — so the test exercises the shape that
    /// actually reaches this code rather than an artificial one. Each `:` costs two extra characters.
    fn overlong_iso_key() -> String {
        let stamp = "2026-08-13T10:00:00Z"; // 2 colons -> +4 characters once encoded
        let padding = "x".repeat(MAX_LOCAL_COMPONENT - stamp.len() + 2);
        format!("{stamp}{padding}.json")
    }

    /// **CPE-1709 round 2, F1 — the blocker.** A file whose encoded local name the filesystem refuses
    /// must be a **reported failure**, never `Ok`. The old code skipped it silently, so a transfer
    /// announced success for a tree it had not delivered — this ticket's own bug, one layer up.
    ///
    /// Ungated: the 255-character component limit is not a Windows peculiarity (ext4 and APFS impose the
    /// same limit in bytes), so all three CI legs exercise the reporting path. The *reason* differs per
    /// platform; the contract — never `Ok` for an undelivered tree — does not.
    #[test]
    fn cpe_1709_a_name_the_filesystem_refuses_is_a_reported_failure_not_a_silent_skip() {
        let out = std::env::temp_dir().join(format!("cpe-1709-toolong-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let key = overlong_iso_key();
        let provider = HostileNames { names: vec![key.clone()] };
        let cancel = AtomicBool::new(false);

        let err = download_tree(&provider, "", &out, &cancel)
            .expect_err("an undelivered file must NOT be reported as success");
        assert!(
            err.contains("could not be written"),
            "the error must say the file was not written; got: {err}"
        );
        assert!(
            err.contains(&MAX_LOCAL_COMPONENT.to_string()),
            "the error must name the real cause (the component-length limit), not blame a symlink \
             probe; got: {err}"
        );
        assert!(
            !err.contains("symlink"),
            "the old message blamed a pre-existing-symlink probe for a length problem; got: {err}"
        );
        // And the file the user would open genuinely is not there — the failure is real, not cosmetic.
        assert!(std::fs::read_dir(&out).unwrap().flatten().next().is_none(), "nothing should have landed");
        let _ = std::fs::remove_dir_all(&out);
    }

    /// The batch shape, which is how this actually hides: two deliverable keys and one that cannot be
    /// written. The two must still arrive — the walk runs to completion — but the transfer as a whole
    /// must **not** report success, because it did not deliver everything it was asked for.
    #[test]
    fn cpe_1709_a_batch_containing_one_undeliverable_file_does_not_report_success() {
        let out = std::env::temp_dir().join(format!("cpe-1709-batch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let provider = HostileNames {
            names: vec!["first.txt".into(), overlong_iso_key(), "second.txt".into()],
        };
        let cancel = AtomicBool::new(false);

        let err = download_tree(&provider, "", &out, &cancel)
            .expect_err("2-of-3 delivered is not success — the old code returned Ok(2) here");
        assert!(err.contains("delivered 2 file(s)"), "the verdict must say what DID land; got: {err}");
        assert!(err.contains("but 1 could not"), "and how many did not; got: {err}");
        // The deliverable siblings are still delivered — a bad name must not abort the whole transfer.
        for good in ["first.txt", "second.txt"] {
            assert_eq!(
                std::fs::read(out.join(good)).unwrap_or_else(|e| panic!("{good}: {e}")),
                b"pwn",
                "{good} must still arrive despite an undeliverable sibling"
            );
        }
        let _ = std::fs::remove_dir_all(&out);
    }

    /// A security refusal is **not** a delivery failure: not writing a traversal name or a pre-existing
    /// symlink is the correct outcome, so those must still end `Ok`. Without this the F1 change would
    /// quietly turn every hostile-name transfer into an error and mask real problems behind noise.
    ///
    /// **This exercises only the traversal branch of the three security refusals** (PR #894 UAT,
    /// CPE-1712 fold-in): traversal names, a pre-existing symlink, and an uninspectable ancestor. It is
    /// not a happy-path assertion — it mixes a refused entry with a deliverable one and pins `n == 1`, so
    /// it does distinguish the two categories — but a future change that moved the pre-existing-symlink
    /// refusal into `undelivered` would still pass THIS test alone, because it never exercises that
    /// branch. [`cpe_1712_a_preexisting_symlink_refusal_still_reports_ok`] below closes that gap
    /// (unix-gated: creating a symlink on Windows needs admin/developer-mode).
    ///
    /// **The third category no longer exists as a category** (CPE-1913). "An uninspectable ancestor"
    /// was `classify_ancestor_probe`'s verdict, and that classifier is gone with the by-path ancestor
    /// walk it served: `download_tree` no longer inspects an ancestor before writing, because it no
    /// longer writes by path. A component it cannot open now stops the entry structurally, inside
    /// `open_beneath`'s walk, and there is no "keep climbing" branch left to classify.
    #[test]
    fn cpe_1709_a_security_refusal_still_reports_ok() {
        let base = std::env::temp_dir().join(format!("cpe-1709-refusal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let names = vec!["../escape.txt".into(), r"..\escape.txt".into(), "legit.txt".into()];
        let cancel = AtomicBool::new(false);
        let n = download_tree(&HostileNames { names }, "", &base, &cancel)
            .expect("refusing a traversal name is correct behaviour, not a delivery failure").files;
        assert_eq!(n, 1, "only the legitimate entry is delivered");
        assert_eq!(std::fs::read(base.join("legit.txt")).unwrap(), b"pwn");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The second of the three security-refusal categories (PR #894 UAT fold-in, closed while touching
    /// this file for CPE-1712): a pre-existing leaf symlink must be SKIPPED, not delivered and not
    /// treated as an undelivered failure — `n == 1` (only the legitimate file) with the transfer still
    /// `Ok` is what proves the pre-existing-symlink refusal stayed out of `undelivered` — a property
    /// that outlived the guard that used to carry it: since CPE-1913 the refusal comes from the write
    /// handle (`open_beneath`'s `O_NOFOLLOW` leaf, classified by `link_at`) rather than from a
    /// `LeafProbe`, and this test is what pins that the *bucket* did not change with it. Unix-gated for
    /// the same reason `download_tree_does_not_follow_a_preexisting_symlinked_leaf_on_write` is: creating
    /// a symlink on Windows needs admin/developer-mode privilege this CI runner does not have.
    #[cfg(unix)]
    #[test]
    fn cpe_1712_a_preexisting_symlink_refusal_still_reports_ok() {
        use std::os::unix::fs::symlink;
        let base = std::env::temp_dir().join(format!("cpe-1712-symrefusal-{}", std::process::id()));
        let outside = std::env::temp_dir().join(format!("cpe-1712-symrefusal-outside-{}.txt", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_file(&outside);
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(&outside, b"original").unwrap();
        // Plant `base/target.txt` as a pre-existing symlink BEFORE the transfer runs — this is the
        // OneFile provider's only leaf name, so the write attempt hits exactly this guard.
        symlink(&outside, base.join("target.txt")).unwrap();

        let cancel = AtomicBool::new(false);
        let n = download_tree(&OneFile, "", &base, &cancel)
            .expect("skipping a pre-existing symlinked leaf is correct behaviour, not a delivery failure").files;
        assert_eq!(n, 0, "the symlinked leaf must be skipped, counted as neither delivered nor failed");
        assert_eq!(
            std::fs::read(&outside).unwrap(),
            b"original",
            "the write must not have followed the symlink and clobbered the outside file"
        );
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_file(&outside);
    }

    /// **The headline test, ungated so all three CI legs assert something real.** Downloads a key
    /// containing `:` through the REAL sink and reads the bytes back from the path the user would open —
    /// `colon%3Aname.txt` on Windows, `colon:name.txt` (where `:` is an ordinary byte) elsewhere.
    ///
    /// Before the fix this failed on Windows with a 0-byte file named `colon`; the `download_tree`
    /// return value was `Ok(1)` both before and after, which is exactly why nothing is asserted about it.
    #[test]
    fn cpe_1709_download_tree_delivers_the_bytes_of_a_colon_bearing_key() {
        let out = std::env::temp_dir().join(format!("cpe-1709-colon-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let provider = HostileNames { names: vec!["colon:name.txt".into()] };
        let cancel = AtomicBool::new(false);
        download_tree(&provider, "", &out, &cancel).expect("the transfer must not fail");

        let expected = if cfg!(windows) { "colon%3Aname.txt" } else { "colon:name.txt" };
        assert_eq!(
            std::fs::read(out.join(expected)).unwrap_or_else(|e| panic!("{expected}: {e}")),
            b"pwn",
            "the FILE THE USER OPENS must hold the full contents"
        );
        let _ = std::fs::remove_dir_all(&out);
    }

    /// The Windows-only half: prove the alternate-data-stream outcome is gone. `:` is the only character
    /// with this behaviour and it has no analogue on Unix, so the assertion cannot be made there — the
    /// ungated sibling above covers the Linux and macOS legs.
    #[cfg(windows)]
    #[test]
    fn cpe_1709_windows_leaves_no_zero_byte_stub_and_no_alternate_data_stream() {
        let out = std::env::temp_dir().join(format!("cpe-1709-ads-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let provider = HostileNames { names: vec!["colon:name.txt".into()] };
        let cancel = AtomicBool::new(false);
        download_tree(&provider, "", &out, &cancel).expect("the transfer must not fail");

        let entries: Vec<(String, u64)> = std::fs::read_dir(&out)
            .unwrap()
            .flatten()
            .map(|e| (e.file_name().to_string_lossy().into_owned(), e.metadata().map(|m| m.len()).unwrap_or(0)))
            .collect();
        assert_eq!(
            entries,
            vec![("colon%3Aname.txt".to_string(), 3u64)],
            "the download root must hold exactly the one full-size file — the bug left a 0-byte \
             \"colon\" here with the bytes hidden in a colon:name.txt:$DATA stream"
        );
        assert!(!out.join("colon").exists(), "the 0-byte alternate-data-stream stub is back");
        let _ = std::fs::remove_dir_all(&out);
    }

    #[cfg(not(windows))]
    #[test]
    fn cpe_1709_windows_leaves_no_zero_byte_stub_and_no_alternate_data_stream() {
        use std::io::Write;
        let _ = writeln!(
            std::io::stderr(),
            "SKIP cpe_1709_windows_leaves_no_zero_byte_stub_and_no_alternate_data_stream: NTFS \
             alternate data streams do not exist on {}; the ungated sibling \
             cpe_1709_download_tree_delivers_the_bytes_of_a_colon_bearing_key asserts the real \
             behaviour on this platform",
            std::env::consts::OS
        );
    }

    /// Every enumerated name, end to end through the real sink: the bytes must be readable back from the
    /// local name this platform is supposed to produce, and the transfer must report exactly as many
    /// files as landed. This is the test that would have caught the trailing-dot and `NUL` variants,
    /// which lose data by mechanisms entirely unlike the colon's.
    #[test]
    fn cpe_1709_every_enumerated_name_arrives_intact_through_download_tree() {
        let out = std::env::temp_dir().join(format!("cpe-1709-all-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let provider =
            HostileNames { names: CPE_1709_NAMES.iter().map(|r| r.0.to_string()).collect() };
        let cancel = AtomicBool::new(false);
        let n = download_tree(&provider, "", &out, &cancel).expect("the transfer must not fail").files;
        assert_eq!(n, CPE_1709_NAMES.len(), "every enumerated name must be written, none skipped");

        for row in CPE_1709_NAMES {
            let leaf = expected_local(row);
            assert_eq!(
                std::fs::read(out.join(leaf)).unwrap_or_else(|e| panic!("{:?} -> {leaf:?}: {e}", row.0)),
                b"pwn",
                "remote {:?} must be readable at {leaf:?} with its full contents",
                row.0
            );
        }
        let _ = std::fs::remove_dir_all(&out);
    }

    /// **PR #890's traversal measurements, re-run (CPE-1709 acceptance criterion).** The sink now
    /// rewrites `:` instead of letting `CreateFileW` refuse it, so the containment property is asserted
    /// directly rather than resting on the OS happening to reject a name — a strictly stronger
    /// guarantee. Nothing may land outside the download root.
    #[test]
    fn cpe_1709_traversal_property_is_unchanged_by_the_name_rewriting() {
        let base = std::env::temp_dir().join(format!("cpe-1709-trav-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let parent = base.parent().unwrap().to_path_buf();
        let sentinel = parent.join("cpe-1709-PWNED-marker.txt");
        let _ = std::fs::remove_file(&sentinel);

        // PR #890's two inputs, plus the full CPE-1461 battery and concrete single-level escapes.
        let mut names: Vec<String> = vec!["..::$DATA".into(), "..:stream".into()];
        names.extend(TRAVERSAL_INPUTS.iter().map(|s| s.to_string()));
        names.push("../cpe-1709-PWNED-marker.txt".into());
        names.push(r"..\cpe-1709-PWNED-marker.txt".into());

        // The join is either refused outright or lexically contained — never an escape.
        for n in &names {
            if let Some(p) = guarded_join(&base, n) {
                assert!(p.starts_with(&base), "guarded_join({n:?}) escaped the base: {p:?}");
            }
        }
        assert!(guarded_join(&base, "..").is_none(), "a bare `..` is still refused");
        assert!(guarded_join(&base, "../x").is_none(), "a `..` segment still refuses the whole entry");

        let cancel = AtomicBool::new(false);
        let n = download_tree(&HostileNames { names }, "", &base, &cancel)
            .expect("a hostile transfer must skip, not fail").files;
        assert!(!sentinel.exists(), "path traversal escaped the download root: {sentinel:?}");

        let mut stack = vec![base.clone()];
        let mut written = 0usize;
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                assert!(p.starts_with(&base), "a written path escaped the root: {p:?}");
                if p.is_dir() {
                    stack.push(p);
                } else {
                    written += 1;
                }
            }
        }
        assert_eq!(written, n, "the reported count must match what actually landed under the root");
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_file(&sentinel);
    }

    // ---------------------------------------------------------------------------------------------
    // CPE-1712: `char::is_control()` only matches Unicode category Cc; `U+202E RIGHT-TO-LEFT OVERRIDE`
    // is category Cf (format), so it passed every guard above untouched. Measured: a remote leaf
    // `\u{202E}gnp.txt` downloads byte-intact and Windows Explorer renders it as `txt.png` — the bytes
    // are honest, only the rendering lies.
    //
    // Decision recorded next to `BIDI_FORMAT_CHARS`: the on-disk/remote name is left UNTOUCHED (unlike
    // CPE-1709's compelled rewrite), because nothing forces a rewrite here — every one of these code
    // points (all twelve of the `Bidi_Control=Yes` set, including `U+061C ARABIC LETTER MARK`) is legal
    // on every target filesystem — and rewriting would mangle a legitimate Arabic/Hebrew filename that
    // never exposed anyone to the spoof. The fix lives in this app's own rendering
    // (`src/lib/filename.ts`'s `displaySafeName`/`displaySafePath`), not in this sink.
    // ---------------------------------------------------------------------------------------------

    /// The core decision, asserted directly: not one of the enumerated bidi/format characters is ever
    /// rewritten by `windows_safe_segment`. Ungated — the function is platform-pure (see its own doc
    /// comment), so this needs no `#[cfg]` to mean something on every CI leg.
    #[test]
    fn cpe_1712_bidi_format_chars_are_never_rewritten_on_disk() {
        for &c in BIDI_FORMAT_CHARS {
            let name = format!("a{c}b.txt");
            assert_eq!(
                windows_safe_segment(&name).as_ref(),
                name,
                "U+{:04X} must NOT be rewritten on disk — it is legal on every target filesystem, so \
                 nothing compels a transform, and rewriting it would mangle a real RTL filename that \
                 legitimately carries it",
                c as u32
            );
        }
    }

    /// **"Do not casually mangle real RTL filenames"** — the ticket's own instruction. Real Arabic and
    /// Hebrew names, with and without an explicit directional mark right before the extension (a common
    /// real-world shape: an RLM there keeps a Latin extension drawing left-to-right inside otherwise
    /// right-to-left text), must survive the Windows encoder completely unchanged.
    #[test]
    fn cpe_1712_real_arabic_and_hebrew_names_survive_the_windows_encoder_untouched() {
        for name in [
            "مستند.pdf",            // Arabic: "document.pdf" — no bidi control char needed at all
            "تقرير الميزانية.xlsx", // Arabic: "budget report.xlsx"
            "מסמך.txt",             // Hebrew: "document.txt"
            "דוח כספי.docx",        // Hebrew: "financial report.docx"
            "דוח\u{200F}.pdf",      // Hebrew name with an explicit RLM right before the extension
            "تقرير\u{061C}.pdf",    // Arabic name with an explicit ALM right before the extension
        ] {
            assert_eq!(
                windows_safe_segment(name).as_ref(),
                name,
                "a legitimate RTL filename must not be altered: {name:?}"
            );
            assert_eq!(local_safe_segment(name).as_ref(), name);
        }
    }

    /// End to end through the REAL sink: the ticket's own repro. The write succeeds and the bytes are
    /// honest — this is a rendering bug, not a data-loss one, and this test pins that distinction: the
    /// file really is there, byte-intact, under its real (unrewritten) name. Ungated: `:` is a Windows
    /// peculiarity, but a bidi/format character is legal path text on every target OS, so this is real
    /// coverage on all three CI legs, not just Windows.
    #[test]
    fn cpe_1712_the_reported_spoof_writes_byte_intact_through_the_real_sink() {
        let out = std::env::temp_dir().join(format!("cpe-1712-rlo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let leaf = "\u{202E}gnp.txt".to_string(); // the ticket's own repro
        let provider = HostileNames { names: vec![leaf.clone()] };
        let cancel = AtomicBool::new(false);
        let n = download_tree(&provider, "", &out, &cancel).expect("the transfer must not fail").files;
        assert_eq!(n, 1, "the file must be delivered, not skipped — this is a display bug, not security");
        assert_eq!(
            std::fs::read(out.join(&leaf)).unwrap_or_else(|e| panic!("{leaf:?}: {e}")),
            b"pwn",
            "the bytes must arrive intact under the UNREWRITTEN name — proves this is a rendering bug, \
             not a data-loss one"
        );
        let _ = std::fs::remove_dir_all(&out);
    }

    /// And the guard still lets a legitimate nested download through — `download_tree`'s happy path is
    /// already covered by `download_tree_still_downloads_a_legit_nested_tree`, so this pins the narrower
    /// claim that the new `Err` arm did not turn the common case into a skip.
    #[test]
    fn cpe_1696_a_normal_download_is_not_skipped_by_the_hardened_ancestor_walk() {
        let fs = seeded();
        let out = std::env::temp_dir().join(format!("cpe-xfer-anc-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let cancel = AtomicBool::new(false);
        assert_eq!(download_tree(&fs, "", &out, &cancel).unwrap().files, 2);
        assert_eq!(std::fs::read(out.join("sub").join("b.txt")).unwrap(), b"bravo");
        let _ = std::fs::remove_dir_all(&out);
    }
}
