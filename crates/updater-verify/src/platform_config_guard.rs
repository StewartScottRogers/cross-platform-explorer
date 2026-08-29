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
//! `--config` overlays — covered by the chain `src/lib/tauriConfigChain.ts` derives from the release
//! workflows and `src/lib/sidecarBundleResources.test.ts` merges). Their
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
//! - and so is `{"plugins":null}`, and a non-object root (CPE-1903 finding 9). Deleting the parent
//!   deletes the child: a `null` at `plugins` removes the whole plugins block, updater included, and
//!   a non-object root replaces the entire config. The harm class there is update *suppression* rather
//!   than root-of-trust replacement — every install frozen on the build it already has, security fixes
//!   silently stopping — which is exactly what the `endpoints` pin exists to prevent, so it is refused
//!   the same way;
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

/// Decide whether a per-platform config file's `text` changes the shipped updater configuration,
/// returning the refusal reason if it does — or if this guard cannot honestly rule it out.
///
/// "Changes it" is applied at every level RFC 7396 can reach, not only at `plugins.updater`: a
/// non-object root replaces the whole config, a non-object (or `null`) `plugins` replaces/deletes the
/// whole plugins block, and a `plugins.updater` key of any value replaces or deletes the updater block
/// itself. See CPE-1903 finding 9 — the first version checked only the innermost of the three.
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
        // CPE-1903 finding 9: the refusal has to be applied at every level a `null` can delete from,
        // not just at `plugins.updater`. Under RFC 7396 a `null` (or any non-object) AT a key replaces
        // whatever the base config had there -- so `{"plugins":null}` deletes the entire plugins block,
        // updater included, and a non-object ROOT replaces the entire config. Both are changes to the
        // shipped updater configuration made by a file no pin can see; the first version of this guard
        // checked only `/plugins/updater` and let both through. Deleting the parent deletes the child.
        if !json.is_object() {
            return Some(
                "exists and its top-level value is not a JSON object. As an RFC 7396 merge patch that \
                 REPLACES the entire base config -- `plugins.updater` and all -- rather than adding to \
                 it, so it changes the shipped updater configuration without any pin being able to see \
                 it"
                    .to_string(),
            );
        }
        if let Some(plugins) = json.pointer("/plugins") {
            if !plugins.is_object() {
                return Some(
                    "exists and sets `plugins` to something other than an object. Under RFC 7396 that \
                     REPLACES the base config's whole `plugins` block -- and a `null` there DELETES it \
                     -- so the shipped app can end up with no updater configuration at all: update \
                     suppression, which freezes every install on the build it already has and stops \
                     security fixes reaching it (CPE-1903 finding 9)"
                        .to_string(),
                );
            }
            if plugins.get("updater").is_some() {
                return Some(
                    "exists and sets a `plugins.updater` key. Tauri merges this file into the build \
                     automatically via RFC 7396, so that key decides the shipped updater's root of \
                     trust — and a `null` there DELETES the pinned block just as effectively as a \
                     value replaces it"
                        .to_string(),
                );
            }
        }
        return None;
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
/// behaviour. Every matching entry is then **read through**, with no file-type probe of any kind: a
/// symlink is followed exactly as Tauri follows it, and a directory, junction or otherwise unreadable
/// entry becomes a refusal rather than a skip. Failing closed is the only safe default on this
/// surface, and CPE-1903 finding 8 is what happens when a fail-open probe sits in front of it.
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
        // CPE-1903 finding 8: there is deliberately NO file-type probe here any more. The first
        // version had `if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) { continue; }`,
        // which is lstat-shaped -- `DirEntry::file_type` does not traverse, so a SYMLINK named
        // `tauri.linux.conf.json` pointing at an innocuous-looking committed file was dropped before
        // it was ever read, while Tauri's own `read_platform` (`path.exists()` + `read_to_string`,
        // both of which FOLLOW links) merged the payload. Demonstrated with the real CLI: `npx tauri
        // info` reported the attacker's config while every guard was green. Git stores such a link as
        // mode 120000, so it materialises as a real symlink on the ubuntu and macOS runners -- and
        // `verify-updater-pin`, the job that gates the sidecar build/sign/publish matrix, is
        // `runs-on: ubuntu-latest`, exactly where it is real and therefore was invisible.
        //
        // Worse, that probe was fail-OPEN: `unwrap_or(false)` skipped the entry on an lstat error too,
        // one line above a `read_to_string` whose error branch is correctly fail-CLOSED. Removing it
        // is both the fix and the simplification: `read_to_string` follows links exactly as Tauri
        // does, and a directory / junction / unreadable entry now produces a refusal through the
        // already-correct fail-closed branch instead of a silent skip.
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
         real key/endpoint change through `src-tauri/tauri.conf.json` (or a `--config` overlay the \
         release workflow already passes) so the existing pins actually see it, update \
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
    use std::path::Path;

    /// CPE-1950 — the shared oracle for this guard's TypeScript twin,
    /// `src/lib/sidecarBundleResources.test.ts`.
    ///
    /// The two copies used to be held together by the sentence "keep the two derivations in
    /// lockstep" on the TS side: a provenance claim, untested by construction, sitting on the app's
    /// updater root of trust. Both now run `src/lib/platformConfigGuard.cases.json` — this test and
    /// that file's `the platform-config guard is DERIVED from its Rust twin` block — so a change on
    /// one side that the other does not follow reds on the side that made it.
    ///
    /// **What this cannot catch: shared blindness.** Agreement is not correctness. A filename or
    /// merge-patch shape neither implementation considered is absent from the case file, both sides
    /// answer it the same wrong way, and this passes. Measured on PR #1060: a `<<` inside a quoted
    /// string opened a phantom heredoc in both the TS and Rust shell scanners while their shared
    /// case file agreed with itself. The leg that does NOT depend on anyone having thought of a case
    /// is the TS side reading [`TAURI_PLATFORM_TOKENS`] out of this file at run time; the rest of the
    /// cover is each side's own tests. If you touch either implementation, add the case to the
    /// SHARED file, never to one side's own test.
    ///
    /// **Red-proofed, not assumed.** Requiring one extra segment in
    /// [`is_auto_merged_platform_config_name`] (`segments.next().is_some() && segments.next().is_some()`,
    /// which makes this side stop matching `Tauri.<t>.toml`) fails this test on shared case
    /// `"TOML tail, ConfigFormat::Toml's exact spelling"`. Reverted.
    fn shared_cases() -> serde_json::Value {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("src")
            .join("lib")
            .join("platformConfigGuard.cases.json");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        serde_json::from_str(&text).expect("platformConfigGuard.cases.json")
    }

    #[test]
    fn both_implementations_agree_on_every_shared_case() {
        let cases = shared_cases();

        let names = cases["names"].as_array().expect("names array");
        let refusals = cases["refusals"].as_array().expect("refusals array");
        // CPE-1932: an empty or truncated fixture would let this guard agree with its twin
        // vacuously. Both sides assert the same floor.
        assert!(names.len() >= 20, "only {} shared name cases", names.len());
        assert!(refusals.len() >= 18, "only {} shared refusal cases", refusals.len());

        for case in names {
            let label = case["name"].as_str().expect("case name");
            let file_name = case["fileName"].as_str().expect("case fileName");
            let expected = case["autoMerged"].as_bool().expect("case autoMerged");
            assert_eq!(
                is_auto_merged_platform_config_name(file_name),
                expected,
                "SECURITY (CPE-1903/CPE-1950): this guard disagrees with its TypeScript twin \
                 (src/lib/sidecarBundleResources.test.ts) on shared case {label:?} \
                 ({file_name:?}). One of the two now refuses a file Tauri auto-merges that the other \
                 lets through. Fix the implementation, and put any new case in \
                 src/lib/platformConfigGuard.cases.json rather than in one side's own test."
            );
        }

        for case in refusals {
            let label = case["name"].as_str().expect("case name");
            let text = case["text"].as_str().expect("case text");
            let expected = case["refused"].as_bool().expect("case refused");
            // The oracle pins the DECISION, never the message: the two sides word their refusals
            // differently on purpose (each names its own remediation path).
            assert_eq!(
                platform_config_updater_refusal(text).is_some(),
                expected,
                "SECURITY (CPE-1903/CPE-1950): this guard disagrees with its TypeScript twin on \
                 shared refusal case {label:?}."
            );
        }
    }

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

    /// CPE-1903 finding 9. `{"plugins":null}` is 17 bytes and, under RFC 7396, deletes the base
    /// config's entire `plugins` block — updater included. The first version of this guard checked
    /// `/plugins/updater`, which on a null `plugins` cannot index and returned `None`. Deleting the
    /// parent deletes the child; the refusal has to apply at every level a `null` can reach.
    #[test]
    fn refuses_a_null_or_non_object_plugins_block() {
        for body in [
            r#"{"plugins":null}"#,
            r#"{"plugins":[]}"#,
            r#"{"plugins":"gone"}"#,
            r#"{"plugins":42}"#,
        ] {
            assert!(platform_config_updater_refusal(body).is_some(), "{body}");
        }
    }

    /// The same principle one level further up: a non-object ROOT is an RFC 7396 patch that replaces
    /// the whole config rather than merging into it.
    #[test]
    fn refuses_a_non_object_root() {
        for body in ["null", "[]", r#""gone""#, "0"] {
            assert!(platform_config_updater_refusal(body).is_some(), "{body}");
        }
    }

    /// CPE-1903 finding 8, the directory half: a directory named like a per-platform config used to be
    /// skipped by the `is_file()` probe. With the probe gone it becomes a refusal through
    /// `read_to_string`'s already-correct fail-closed branch.
    #[test]
    fn scan_refuses_a_directory_named_like_a_platform_config() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join("tauri.linux.conf.json")).unwrap();
        let hits = scan_for_platform_config_updater_overrides(dir.path()).expect("scan");
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert!(hits[0].reason.contains("could not be read as text"), "{hits:?}");
    }

    /// CPE-1903 finding 8, the symlink half — the one that was DEMONSTRATED against the real CLI. A
    /// symlink named `tauri.linux.conf.json` pointing at an innocuous committed file was dropped by
    /// the old lstat-shaped `entry.file_type().is_file()` probe, while Tauri's `read_platform`
    /// (`exists()` + `read_to_string`, both of which follow links) merged the payload. Git stores it
    /// as mode 120000, so it is a real symlink on the ubuntu and macOS runners — including
    /// `verify-updater-pin`'s `ubuntu-latest`, which gates the sidecar build/sign/publish matrix.
    ///
    /// Creating a symlink needs no privilege on Unix; on Windows it needs Developer Mode or elevation,
    /// so there this test reports that it could not construct the fixture rather than passing
    /// vacuously. The legs that matter run it for real.
    #[test]
    fn scan_follows_a_symlink_named_like_a_platform_config() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("innocent-looking-resource.json");
        std::fs::write(&target, r#"{"plugins":{"updater":{"pubkey":"attacker"}}}"#).unwrap();
        let link = dir.path().join("tauri.linux.conf.json");

        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&target, &link).is_ok();
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(&target, &link).is_ok();
        #[cfg(not(any(unix, windows)))]
        let made = false;

        if !made {
            eprintln!(
                "CPE-1903 finding 8: could not create a symlink on this host (Windows needs Developer \
                 Mode or elevation) -- the fixture, not the guard, is what is unavailable. This test \
                 runs for real on the Linux and macOS legs, which are the ones where git materialises \
                 mode 120000 as a real symlink."
            );
            return;
        }

        let hits = scan_for_platform_config_updater_overrides(dir.path()).expect("scan");
        assert_eq!(
            hits.len(),
            1,
            "a symlink named like a per-platform config must be READ THROUGH, exactly as Tauri reads \
             it -- never skipped on a file-type probe; got {hits:?}"
        );
        assert_eq!(hits[0].file_name, "tauri.linux.conf.json");
    }

    #[test]
    fn an_unlistable_directory_is_an_error_not_an_empty_result() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(scan_for_platform_config_updater_overrides(&dir.path().join("nope")).is_err());
    }
}
