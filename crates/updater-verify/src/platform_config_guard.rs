//! CPE-1903 — Tauri's **automatic** per-platform config files, *derived* rather than enumerated.
//!
//! # Why this module exists at all
//!
//! `tauri-utils::config::parse::read_from` reads `src-tauri/tauri.conf.json` and then, with **no
//! `--config` flag and no workflow involvement**, looks next to it for a *per-platform* config file
//! and merges it into the build via RFC 7396 (JSON Merge Patch). Anything that file sets wins —
//! `plugins.updater.pubkey` and `plugins.updater.endpoints` included. That is the app's entire root of
//! trust for auto-updates, decided by one file that never appears in any `--config` chain.
//!
//! CPE-1873 closed that path three times, and each time it closed the *reproduction* it was handed
//! rather than the *mechanism*:
//!
//! | round | what got pinned | how it was bypassed |
//! |---|---|---|
//! | 1 | the base `tauri.conf.json` | the `--config` overlay chain |
//! | 2 | the `--config` overlay chain | `tauri.<os>.conf.json`, merged automatically |
//! | 3 | those three `.json` filenames | `.json5` and `Tauri.<os>.toml` |
//!
//! Round 3's list was three names. Tauri's real surface is **fifteen**:
//! `ConfigFormat::into_platform_file_name` crosses three formats (`tauri.<t>.conf.json`,
//! `tauri.<t>.conf.json5`, `Tauri.<t>.toml`) with five targets (`macos`, `windows`, `linux`,
//! `android`, `ios`), `does_supported_file_name_exist` returns true if **any enabled format's**
//! platform file exists, and `do_parse` falls through `json → json5 → toml`. Both non-`.json` formats
//! were demonstrated live against this repo's own installed `@tauri-apps/cli` while every guard
//! reported green (see CPE-1903 for the measurements).
//!
//! # The derivation
//!
//! Extending a hardcoded list from 3 names to 15 is the same mistake with a bigger number, and it has
//! now failed three times on this exact surface. So this module does not enumerate filenames. It
//! **reads the directory** and classifies what is actually there:
//!
//! 1. [`scan_for_platform_config_updater_overrides`] calls `read_dir` on the config root. `read_dir`
//!    reports the *on-disk spelling* of every entry, so nothing here ever performs a name lookup —
//!    which is what makes this check identical on a case-insensitive NTFS build runner and on a
//!    byte-exact `ubuntu-latest` gate. Round 3's check did
//!    `read_to_string(dir.join("tauri.windows.conf.json"))`, i.e. a lookup, so a
//!    `Tauri.Windows.Conf.json` was caught on NTFS and *invisible* on Linux while the Windows build
//!    runner merged it regardless.
//! 2. [`is_auto_merged_platform_config_name`] ASCII-lowercases each entry name and matches the
//!    *shape* Tauri derives its names from — `tauri` `.` `<platform token>` `.` `…` — rather than any
//!    particular spelling of the tail. All fifteen of today's names match; so does a sixteenth format
//!    Tauri may add tomorrow, and so does any casing NTFS would fold.
//! 3. [`platform_config_updater_refusal`] decides whether that file *sets* `plugins.updater`.
//!
//! Deliberately NOT matched: `tauri.conf.json` / `tauri.conf.json5` / `Tauri.toml` (the base config —
//! pinned by value in [`crate::pinned_pubkey`]) and `tauri.sidecar.*.conf.json` (the explicit
//! `--config` overlays — covered by `src/lib/sidecarBundleResources.test.ts`'s `CONFIG_CHAIN`). Their
//! second dot-segment is not a platform token, which is exactly the property Tauri itself keys on.
//!
//! # The semantics, which CPE-1873 round 3 got right and this round preserves
//!
//! A per-platform file is refused for **carrying a `plugins.updater` key**, never for merely existing
//! and never by comparing its value to the pin:
//!
//! - a per-platform file carrying only, say, `plugins.cli` is legitimate and stays allowed;
//! - `{"plugins":{"updater":null}}` is refused — under RFC 7396 a `null` *deletes* the base config's
//!   updater block, which is every bit as much a change to the root of trust as replacing it;
//! - the pinned pubkey/endpoints *values* are not consulted here at all, because a file that sets the
//!   right value today can set the wrong one in the next commit, and nothing in this repo would
//!   re-review a file class that is not supposed to exist.
//!
//! # Where it runs
//!
//! Both places, on purpose — that is CPE-1873 round 1's lesson, which round 3 never carried over to
//! this particular check:
//!
//! - `verify-release-artifacts` (the binary) calls it on the directory holding whatever `--conf` it
//!   was given. `release.yml`'s tag-triggered `verify-published-manifest` job runs that binary, so the
//!   plain channel's **tag path** is covered — a `#[test]` never reaches it (`ci.yml` has no `tags:`
//!   trigger).
//! - `tests/pinned_pubkey_guard.rs` calls it on the real `src-tauri/`, as the fast PR-time signal on
//!   every push/PR to `main`, alongside a mirrored check in `src/lib/sidecarBundleResources.test.ts`
//!   that gates `release-sidecar.yml` via its `verify-updater-pin` job.
//!
//! # What this does NOT prove
//!
//! The same ceiling as every other guard in this crate, restated because it keeps mattering: these
//! checks read files from the **same commit** and consult nothing outside it. A rotation that changes
//! `tauri.conf.json` and every pin in one commit is perfectly self-consistent and passes all of them.
//! See [`crate::pinned_pubkey`]'s "What none of this proves" section.

use std::path::Path;

/// The platform tokens Tauri's `Target` enum maps onto a per-platform config file name — the second
/// dot-segment of every name `ConfigFormat::into_platform_file_name` can produce, in all three
/// formats. This is the one list genuinely closed by Tauri's own type (a `Target` variant, not a
/// filename), so it is the only thing worth hardcoding: adding a *format* cannot grow it, and Tauri
/// adding a *target* is a visible upstream API change rather than a filename nobody thought of.
pub const TAURI_PLATFORM_TOKENS: &[&str] = &["macos", "windows", "linux", "android", "ios"];

/// One per-platform config file that must not be in the tree in its current form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformConfigOverride {
    /// The entry's name exactly as the filesystem spelled it (never normalised — the message has to
    /// name the file a human will go and look for).
    pub file_name: String,
    /// Why it was refused, phrased to complete the sentence "`<file>` …".
    pub reason: String,
}

/// Does `file_name` name a config file Tauri would merge **automatically**, with no `--config` flag?
///
/// Matched by shape, not by spelling: ASCII-lowercased, the name must be `tauri`, then a
/// [`TAURI_PLATFORM_TOKENS`] entry, then at least one more segment (the format tail — `conf.json`,
/// `conf.json5`, `toml`, or whatever Tauri adds next).
///
/// ASCII case folding is the right normalisation for both hosts that matter: NTFS folds ASCII case
/// when Tauri's `.exists()` looks the name up, and on a case-sensitive filesystem a mixed-case file is
/// invisible to Tauri — so over-matching there is fail-safe, and under-matching never is.
pub fn is_auto_merged_platform_config_name(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    let mut segments = lower.split('.');
    if segments.next() != Some("tauri") {
        return false;
    }
    let Some(token) = segments.next() else {
        return false;
    };
    if !TAURI_PLATFORM_TOKENS.contains(&token) {
        return false;
    }
    // A bare `tauri.windows` with no format tail is not something Tauri reads.
    segments.next().is_some()
}

/// Decide whether a per-platform config file's `text` sets `plugins.updater`, returning the refusal
/// reason if it does — or if this guard cannot honestly rule it out.
///
/// Strict JSON is parsed and inspected structurally: full fidelity, `null` included. Anything else
/// (JSON5, TOML, or a `.json`-named file holding JSON5, which Tauri's `do_parse` accepts) gets a
/// conservative textual scan instead, because this crate deliberately carries **no** JSON5 or TOML
/// parser and a guard on the root of trust must not silently pass a format it cannot read. So a
/// non-JSON per-platform config is allowed only when it demonstrably cannot be naming that key: no
/// `updater` token anywhere in it, and no backslash escape that could spell one (`"\u0075pdater"`). A
/// file that trips either arm fails loud, and the message says how to make it inspectable.
pub fn platform_config_updater_refusal(text: &str) -> Option<String> {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(text) {
        return json.pointer("/plugins/updater").map(|_| {
            "exists and sets a `plugins.updater` key. Tauri merges this file into the build \
             automatically via RFC 7396, so that key decides the shipped updater's root of trust — and \
             a `null` there DELETES the pinned block just as effectively as a value replaces it"
                .to_string()
        });
    }

    let lower = text.to_ascii_lowercase();
    if lower.contains("updater") {
        return Some(
            "exists, is not strict JSON (so this guard cannot parse it structurally — no JSON5/TOML \
             parser is carried here on purpose), and mentions `updater`. Tauri merges this file into \
             the build automatically via RFC 7396, so it is refused rather than guessed at"
                .to_string(),
        );
    }
    if text.contains('\\') {
        return Some(
            "exists, is not strict JSON (so this guard cannot parse it structurally), and contains \
             backslash escape sequences that could spell a `plugins.updater` key without the literal \
             token ever appearing. Refused rather than guessed at"
                .to_string(),
        );
    }
    None
}

/// Scan `dir` for per-platform config files that set `plugins.updater`.
///
/// `read_dir` — never a name lookup — so the result does not depend on the host filesystem's case
/// behaviour. Directories are skipped (Tauri reads files). A file this guard cannot read as text is a
/// refusal, not a skip: failing closed is the only safe default on this surface.
///
/// Returns `Err` only when `dir` itself cannot be listed, which callers must treat as a failure rather
/// than as "nothing found".
pub fn scan_for_platform_config_updater_overrides(
    dir: &Path,
) -> std::io::Result<Vec<PlatformConfigOverride>> {
    let mut hits = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if !is_auto_merged_platform_config_name(&file_name) {
            continue;
        }
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let reason = match std::fs::read_to_string(entry.path()) {
            Ok(text) => platform_config_updater_refusal(&text),
            Err(e) => Some(format!(
                "exists but could not be read as text ({e}), so this guard cannot rule out a \
                 `plugins.updater` key in a file Tauri merges automatically. Refused rather than \
                 skipped"
            )),
        };
        if let Some(reason) = reason {
            hits.push(PlatformConfigOverride { file_name, reason });
        }
    }
    hits.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    Ok(hits)
}

/// The one failure message every caller prints, so the binary and the `#[test]` say the same thing.
pub fn platform_config_override_message(dir: &Path, hits: &[PlatformConfigOverride]) -> String {
    let mut s = String::from(
        "\n\nSECURITY (CPE-1903): a per-platform Tauri config file in the tree can override the \
         updater's root of trust.\n",
    );
    s.push_str(&format!("Scanned directory: {}\n\n", dir.display()));
    for hit in hits {
        s.push_str(&format!("  - `{}` {}\n", hit.file_name, hit.reason));
    }
    s.push_str(
        "\nTauri picks these files up with NO `--config` flag and no workflow involvement: \
         `tauri-utils::config::parse::read_from` reads `tauri.conf.json`, then merges \
         `tauri.<platform>.conf.json` / `.json5` / `Tauri.<platform>.toml` next to it via RFC 7396 on \
         every build for that platform. Such a file is therefore invisible to the base-config pin, \
         invisible to the `--config` overlay-chain guard, and ships on the plain channel and the \
         sidecar channel alike.\n\
         \n\
         If this is a DELIBERATE, authorized change: it must not set `plugins.updater` at all. Put any \
         real key/endpoint change through `src-tauri/tauri.conf.json` (or a `--config` overlay already \
         listed in `CONFIG_CHAIN`) so the existing pins actually see it, update \
         `crates/updater-verify/src/pinned_pubkey.rs`'s constants in the same commit, and record why in \
         the ticket that authorized it -- see that file's rotation procedure.\n\
         \n\
         If you did NOT intend to add this file: STOP. Its `plugins.updater` block is not trustworthy, \
         and neither is any build made from this commit (CPE-1873 / CPE-1903).\n",
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every filename `ConfigFormat::into_platform_file_name` can produce in tauri-utils 2.x: three
    /// formats x five `Target` variants. Written out ONLY as test data — the guard itself never sees
    /// this list, which is the whole point of CPE-1903.
    fn every_tauri_platform_file_name() -> Vec<String> {
        let mut names = Vec::new();
        for t in TAURI_PLATFORM_TOKENS {
            names.push(format!("tauri.{t}.conf.json"));
            names.push(format!("tauri.{t}.conf.json5"));
            // Capital `T`, lowercase target — the exact spelling `ConfigFormat::Toml` produces.
            names.push(format!("Tauri.{t}.toml"));
        }
        names
    }

    #[test]
    fn matches_every_format_and_target_tauri_can_auto_merge() {
        let names = every_tauri_platform_file_name();
        for name in &names {
            assert!(
                is_auto_merged_platform_config_name(name),
                "{name} is auto-merged by Tauri but the guard did not match it"
            );
        }
        // Exactly the fifteen names tauri-utils 2.9.3 enumerates.
        assert_eq!(names.len(), 15);
    }

    #[test]
    fn matches_the_real_toml_spellings_in_any_case() {
        for name in ["Tauri.windows.toml", "tauri.windows.toml", "TAURI.WINDOWS.TOML"] {
            assert!(is_auto_merged_platform_config_name(name), "{name}");
        }
    }

    /// The case leg CPE-1903 calls out: round 3's check did a name LOOKUP, so a mixed-case file was
    /// caught on NTFS and invisible on `ubuntu-latest` — while the Windows build runner merged it.
    /// This matcher only ever sees names `read_dir` already handed back, so casing cannot hide one.
    #[test]
    fn matching_is_case_insensitive_in_both_directions() {
        for name in [
            "Tauri.Windows.Conf.json",
            "TAURI.WINDOWS.CONF.JSON",
            "tauri.WINDOWS.conf.JSON5",
            "TaUrI.MaCoS.CoNf.JsOn",
            "Tauri.LINUX.Toml",
            "TAURI.iOS.conf.json",
            "tauri.Android.conf.json5",
        ] {
            assert!(is_auto_merged_platform_config_name(name), "{name}");
        }
    }

    #[test]
    fn does_not_match_the_base_config_or_the_explicit_config_overlays() {
        for name in [
            "tauri.conf.json",
            "tauri.conf.json5",
            "Tauri.toml",
            "tauri.sidecar.conf.json",
            "tauri.sidecar.windows.conf.json",
            "tauri.sidecar.unix.conf.json",
            "tauri.sidecar.pdfium.windows.conf.json",
            "Cargo.toml",
            "tauri.windows",
            "notauri.windows.conf.json",
            "windows.conf.json",
        ] {
            assert!(
                !is_auto_merged_platform_config_name(name),
                "{name} is not auto-merged by Tauri; matching it would misdirect the reader"
            );
        }
    }

    /// A format Tauri has not shipped yet: the derivation covers it, an enumeration could not.
    #[test]
    fn matches_a_format_tail_that_does_not_exist_yet() {
        assert!(is_auto_merged_platform_config_name("tauri.windows.conf.yaml"));
        assert!(is_auto_merged_platform_config_name("Tauri.macos.jsonc"));
    }

    #[test]
    fn refuses_a_plugins_updater_key_in_strict_json() {
        assert!(platform_config_updater_refusal(r#"{"plugins":{"updater":{"pubkey":"x"}}}"#).is_some());
    }

    /// RFC 7396: `null` DELETES the base config's updater block. Judged a deliberate refusal in
    /// CPE-1873 round 3; preserved here verbatim.
    #[test]
    fn refuses_an_rfc_7396_delete_of_the_updater_block() {
        assert!(platform_config_updater_refusal(r#"{"plugins":{"updater":null}}"#).is_some());
    }

    /// Equally deliberate: a per-platform file that does not touch the updater is legitimate.
    #[test]
    fn allows_a_per_platform_file_that_does_not_set_the_updater() {
        assert!(platform_config_updater_refusal(r#"{"plugins":{"cli":{"args":[]}}}"#).is_none());
        assert!(platform_config_updater_refusal(r#"{"bundle":{"targets":["msi"]}}"#).is_none());
        // ...including in TOML, which this guard cannot parse but can still clear.
        assert!(platform_config_updater_refusal("[plugins.cli]\ndescription = \"hi\"\n").is_none());
    }

    #[test]
    fn refuses_the_updater_key_in_formats_it_cannot_parse() {
        // JSON5: comments + unquoted keys, so serde_json cannot read it.
        assert!(
            platform_config_updater_refusal("// hi\n{ plugins: { updater: { pubkey: 'x' } } }").is_some()
        );
        // TOML table header.
        assert!(platform_config_updater_refusal("[plugins.updater]\npubkey = \"x\"\n").is_some());
        // TOML dotted key.
        assert!(platform_config_updater_refusal("plugins.updater.pubkey = \"x\"\n").is_some());
        // Escaped so the literal token never appears in the file at all.
        assert!(platform_config_updater_refusal("{ plugins: { \"\\u0075pdater\": {} } }").is_some());
    }

    #[test]
    fn scan_finds_every_planted_variant_and_ignores_the_innocent_ones() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::write(root.join("tauri.conf.json"), r#"{"plugins":{"updater":{"pubkey":"real"}}}"#)
            .unwrap();
        std::fs::write(
            root.join("tauri.sidecar.windows.conf.json"),
            r#"{"plugins":{"updater":{"pubkey":"overlay"}}}"#,
        )
        .unwrap();
        std::fs::write(root.join("tauri.linux.conf.json"), r#"{"plugins":{"cli":{}}}"#).unwrap();
        for name in every_tauri_platform_file_name() {
            if name == "tauri.linux.conf.json" {
                continue;
            }
            std::fs::write(root.join(&name), "[plugins.updater]\npubkey = \"attacker\"\n").unwrap();
        }
        let hits = scan_for_platform_config_updater_overrides(root).expect("scan");
        assert_eq!(
            hits.len(),
            14,
            "expected 14 refusals (15 platform names minus the innocent linux one); got {hits:?}"
        );
        assert!(
            !hits
                .iter()
                .any(|h| h.file_name.eq_ignore_ascii_case("tauri.linux.conf.json")),
            "a per-platform file that only sets plugins.cli must stay allowed"
        );
        assert!(
            !hits
                .iter()
                .any(|h| h.file_name.contains("sidecar") || h.file_name == "tauri.conf.json"),
            "the base config and the --config overlays are other guards' business"
        );
    }

    /// The directory leg of the case check. On `ubuntu-latest` — where `verify-updater-pin` runs and
    /// where round 3's lookup-based check went silently blind — this executes against a genuinely
    /// case-sensitive filesystem with real files on disk.
    #[test]
    fn scan_catches_a_mixed_case_spelling_on_any_filesystem() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("Tauri.Windows.Conf.JSON"),
            r#"{"plugins":{"updater":{"pubkey":"attacker"}}}"#,
        )
        .unwrap();
        let hits = scan_for_platform_config_updater_overrides(dir.path()).expect("scan");
        assert_eq!(hits.len(), 1, "{hits:?}");
    }

    #[test]
    fn an_unlistable_directory_is_an_error_not_an_empty_result() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(scan_for_platform_config_updater_overrides(&dir.path().join("nope")).is_err());
    }
}
