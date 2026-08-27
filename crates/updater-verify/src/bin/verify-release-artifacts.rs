//! Slice B — the real-release guard (CPE-1058).
//!
//! Runs *once, after the whole release matrix completes* (CPE-1872 round 2), against the manifest as
//! actually published on the draft release, plus every asset it references, downloaded fresh into
//! `--search`. Reads the `pubkey` + `version` straight from `tauri.conf.json` and runs
//! [`cpe_updater_verify::verify_update_manifest`] over the real bytes — the same signature check the
//! runtime plugin performs. Exits non-zero on a manifest that wouldn't verify, so a botched signing /
//! version bump / manifest fails the release before it ships.
//!
//! Requires EVERY platform the manifest names to be fetched and verified (CPE-1872 round 2) — a
//! platform this run can't fetch is a hard failure, never a skip, because the manifest is the union of
//! the whole release matrix and no partial view of it is trustworthy. If `--expect-url-prefix` is given
//! (CPE-1872 round 3), every platform's `url` must additionally start with that prefix — the crypto
//! check alone only proves the artifact BYTES are genuine, not that the `url` a real updater will fetch
//! points at this repo's own release rather than a foreign host or the wrong tag serving an
//! identically-named asset.
//!
//! Also asserts (CPE-1894, unconditional) that every platform's asset is from the SAME release
//! channel as `--conf`'s own `productName` — the guard for "release.yml's tag trigger fired on the
//! wrong channel's tag and merged its installers into this draft release", the live bug that shipped
//! `v0.57.69`'s manifest with two platforms pointing at sidecar assets and two at plain ones. See
//! [`cpe_updater_verify::platforms_with_mismatched_channel`].
//!
//! CPE-1923 added three more unconditional checks after an independent Security Auditor built
//! hostile manifests that passed this binary at **EXIT 0 with genuine signatures**:
//!
//! 1. **artifact ↔ release binding** (the serious one) — every platform's artifact must be one OF
//!    the release being cut. A signature proves the bytes are ones we once signed; it says nothing
//!    about which release they came from, so an actor with release-asset write and no signing key
//!    could upload the old, vulnerable installer plus its genuine old signature to the new tag and
//!    downgrade every auto-updating user.
//! 2. **platform key → payload kind** — a `darwin-*` entry must not serve a Windows installer.
//! 3. **release channel** — anchored to the real `productName` rather than testing for the free
//!    substring "sidecar", which anyone who can name a release asset could add or omit.
//!
//! **All three are decided from the artifact's SIGNED name, not the name it was uploaded under.**
//! That distinction is the whole of the SEC-1/SEC-9 round: the uploaded name is attacker-chosen in
//! this guard's own threat model, so the first versions of checks 1 and 2 were defeated by simply
//! renaming the upload. `tauri-bundler` writes the original filename into the minisign trusted
//! comment, and `minisign::verify` authenticates that comment against the global signature, so it
//! is the one name here an asset-write attacker cannot pick.
//!
//! Check 1 therefore lives inside [`cpe_updater_verify::verify_update_manifest`] — a trusted comment
//! is only trustworthy once its signature has verified — and is implemented by
//! [`cpe_updater_verify::bind_signed_artifact`], which also carries the narrow macOS exception:
//! Tauri signs the macOS artifact as `<productName>.app.tar.gz` with no version in the *signed*
//! name either, so that one artifact kind has nothing to bind against (CPE-1942). Checks 2 and 3
//! run twice: once over the uploaded basenames before download (cheap, and it gates what gets
//! fetched) and again over the signed names afterwards, which is the pass that actually holds.
//!
//! `productName` is required in `--conf`; a config without one is refused rather than silently
//! disarming check 3.
//!
//! Usage:
//! ```text
//! verify-release-artifacts [--conf <tauri.conf.json>] [--search <dir>]... [--manifest <latest.json>] [--expect-url-prefix <prefix>] [--expect-channel <plain|sidecar>]
//! ```
//! Defaults: `--conf src-tauri/tauri.conf.json`, `--search src-tauri/target`, and the newest `latest.json`
//! found under the search dirs; `--expect-url-prefix` is unset (no URL-binding check) by default so ad
//! hoc/test invocations without a real GitHub release context keep working. Skipping when signing
//! secrets are absent is handled in `release.yml`/`release-sidecar.yml` (the step only runs when
//! `TAURI_SIGNING_PRIVATE_KEY` is set), so this binary always expects real artifacts when it runs at all.
//!
//! `--expect-channel <plain|sidecar>` (CPE-1908) overrides the channel this run expects the manifest to
//! be PURE to, instead of deriving it from `--conf`'s own `productName`. Needed because `--conf` always
//! reads pubkey/version/the CPE-1873 pin from the base `src-tauri/tauri.conf.json` (correct for BOTH
//! channels -- the sidecar overlay never touches those fields), but that file's `productName` always
//! says "Cross-Platform Explorer" (plain) even when this run is checking the SIDECAR channel's published
//! manifest. `release.yml` passes `--expect-channel plain`; `release-sidecar.yml` passes
//! `--expect-channel sidecar` -- both now explicit rather than one of them being implicit, so
//! `channelPurityCoverage.test.ts` can assert every `Channel` variant has a workflow actually invoking
//! this check for it (CPE-1908's own "impossible to silently lose again" requirement).

use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use cpe_updater_verify::verify_update_manifest;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut conf = PathBuf::from("src-tauri/tauri.conf.json");
    let mut manifest_path: Option<PathBuf> = None;
    let mut search_dirs: Vec<PathBuf> = Vec::new();
    let mut expect_url_prefix: Option<String> = None;
    let mut expect_channel: Option<cpe_updater_verify::Channel> = None;
    // CPE-1873: opt-OUT of the pubkey/endpoints pin check (default is to run it). Exists ONLY for
    // this crate's own test fixtures, which scaffold a fresh, throwaway keypair per test unrelated
    // to the repo's real pinned value -- they test manifest/signature logic, not the pin itself. A
    // real invocation (release.yml, or anyone running this by hand) must never pass this.
    let mut skip_pin_check = false;

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--conf" => match it.next() {
                Some(v) => conf = PathBuf::from(v),
                None => return fail("--conf needs a path"),
            },
            "--manifest" => match it.next() {
                Some(v) => manifest_path = Some(PathBuf::from(v)),
                None => return fail("--manifest needs a path"),
            },
            "--search" => match it.next() {
                Some(v) => search_dirs.push(PathBuf::from(v)),
                None => return fail("--search needs a path"),
            },
            "--expect-url-prefix" => match it.next() {
                Some(v) => expect_url_prefix = Some(v.clone()),
                None => return fail("--expect-url-prefix needs a value"),
            },
            "--expect-channel" => match it.next() {
                Some(v) => match v.parse::<cpe_updater_verify::Channel>() {
                    Ok(c) => expect_channel = Some(c),
                    Err(e) => return fail(&format!("--expect-channel: {e}")),
                },
                None => return fail("--expect-channel needs a value (plain or sidecar)"),
            },
            "--skip-pin-check" => skip_pin_check = true,
            "-h" | "--help" => {
                println!(
                    "verify-release-artifacts [--conf <tauri.conf.json>] [--search <dir>]... [--manifest <latest.json>] [--expect-url-prefix <prefix>] [--expect-channel <plain|sidecar>] [--skip-pin-check]"
                );
                return ExitCode::SUCCESS;
            }
            other => return fail(&format!("unknown argument: {other}")),
        }
    }
    if search_dirs.is_empty() {
        search_dirs.push(PathBuf::from("src-tauri/target"));
    }

    // --- Read pubkey + version from tauri.conf.json ---
    let conf_text = match std::fs::read_to_string(&conf) {
        Ok(t) => t,
        Err(e) => return fail(&format!("cannot read {}: {e}", conf.display())),
    };
    let conf_json: serde_json::Value = match serde_json::from_str(&conf_text) {
        Ok(v) => v,
        Err(e) => return fail(&format!("{} is not valid JSON: {e}", conf.display())),
    };
    let pubkey = match conf_json
        .pointer("/plugins/updater/pubkey")
        .and_then(|v| v.as_str())
    {
        Some(p) if !p.is_empty() => p.to_string(),
        _ => return fail("tauri.conf.json has no plugins.updater.pubkey"),
    };
    // CPE-1923 (Reviewer): `Some(v) => v.to_string()` used to accept `"version": ""`, while the
    // pubkey and productName reads on either side of it both check for emptiness. An empty version
    // fails closed for every non-darwin platform, but a darwin-only manifest would have passed --
    // and `artifact_binding`'s doc asserted "the binary refuses before reaching here", which was
    // simply not true. Enforce the property the doc claims.
    let version = match conf_json.get("version").and_then(|v| v.as_str()) {
        Some(v) if !v.trim().is_empty() => v.to_string(),
        Some(_) => return fail(&format!(
            "{} has an EMPTY top-level `version`. A release must declare the version it is shipping: it is what every artifact's signed name is bound against (CPE-1923              finding 1), and with nothing to bind to that check cannot do its job.",
            conf.display()
        )),
        None => return fail("tauri.conf.json has no top-level version"),
    };
    // CPE-1894: which channel THIS conf builds — release.yml always passes the plain
    // `src-tauri/tauri.conf.json` here, so this resolves to `Channel::Plain` in real runs; derived
    // from `productName` by default so an ad hoc/test invocation pointed at a SELF-CONTAINED conf
    // (productName + pubkey + version all in one file) needs no extra flag.
    //
    // CPE-1908: `--expect-channel`, when given, overrides this derivation outright. Real workflow
    // invocations now ALWAYS pass it explicitly (release.yml: `plain`; release-sidecar.yml: `sidecar`)
    // rather than relying on `--conf`'s productName -- load-bearing for the sidecar job, whose `--conf`
    // is still the base `tauri.conf.json` (for pubkey/version/the CPE-1873 pin, which the sidecar
    // overlay never touches) even though the manifest it's checking must be pure SIDECAR.
    //
    // CPE-1923 finding 3: `productName` is no longer optional, and its role changed. The channel
    // check used to be an unanchored `basename.contains("sidecar")` substring test that needed no
    // product name at all; it is now ANCHORED to this product's name, so a config without one
    // leaves nothing to anchor against and would silently turn the strongest of the three checks
    // into a no-op -- the exact failure mode this ticket is about.
    //
    // Note what `productName` is used for AFTER CPE-1908, because the two tickets interact: it no
    // longer decides the channel when `--expect-channel` is passed (that would be wrong -- the
    // sidecar job deliberately passes the PLAIN conf), it supplies the *base product identity* the
    // asset names are anchored against. `expected_channel` decides which of that base name's two
    // forms an asset must match. See `cpe_updater_verify::platforms_with_mismatched_channel`.
    let product_name = match conf_json.get("productName").and_then(|v| v.as_str()) {
        Some(p) if !p.trim().is_empty() => p.to_string(),
        _ => {
            return fail(&format!(
                "{} has no non-empty top-level `productName`. It is required: the channel check (CPE-1894/CPE-1923) anchors every asset basename to this product's own name, and with nothing to anchor against that check would pass everything.",
                conf.display()
            ))
        }
    };
    // The channel-free base identity, and the sidecar form of it. Computed once here so the
    // pre-download check (over uploaded basenames) and the post-verification check (over signed
    // names) cannot drift apart into two different notions of what this product is called.
    let base_token = cpe_updater_verify::base_product_token(&product_name);
    let sidecar_token = cpe_updater_verify::channel_product_token(&base_token, cpe_updater_verify::Channel::Sidecar);
    let (expected_channel, channel_source) = match expect_channel {
        Some(c) => (c, "--expect-channel"),
        None => (cpe_updater_verify::expected_channel_from_product_name(&product_name), "conf productName"),
    };

    // CPE-1873 round 2 (independent reviewer, attempt 1's rejection): the `#[test]` guard in
    // `tests/pinned_pubkey_guard.rs` only runs where `cargo test -p cpe-updater-verify` runs --
    // `ci.yml`'s push/PR-to-main path. Neither release workflow runs that test, so a tag pointed at a
    // commit that never touched `main` (or one CI never evaluated) sailed straight past it. THIS binary
    // is what both `release.yml`'s `verify-published-manifest` job actually invokes on every tag push,
    // so the pin is enforced HERE too -- the same invocation that already checks manifest signatures now
    // also refuses to run against a `tauri.conf.json` whose pubkey/endpoints don't match the pinned
    // copies, before it ever gets to the (comparatively weaker) internal-consistency check below. See
    // `pinned_pubkey.rs` for what this proves and the rotation procedure; a deliberate rotation updates
    // BOTH `tauri.conf.json` and the pinned constants in the same commit, so this passes for an
    // authorized rotation exactly like the `#[test]` does. `--skip-pin-check` exists only for this
    // crate's own fixtures (see its use sites in tests/release_guard.rs) -- never pass it for real.
    if !skip_pin_check {
        if pubkey != cpe_updater_verify::EXPECTED_TAURI_UPDATER_PUBKEY {
            return fail(&format!(
                "SECURITY (CPE-1873): the updater root of trust changed. {}'s plugins.updater.pubkey does \
                 not match the pinned copy in crates/updater-verify/src/pinned_pubkey.rs::EXPECTED_TAURI_UPDATER_PUBKEY. \
                 If this is a deliberate, authorized key rotation, update EXPECTED_TAURI_UPDATER_PUBKEY to the \
                 same new value in the same commit as the tauri.conf.json change (see that file's module doc \
                 for the full procedure) -- do not sign or publish a release until it does. If this was not an \
                 intentional rotation: STOP, this tag/build is not trustworthy.\n  configured: {pubkey}\n  pinned:      {}",
                conf.display(),
                cpe_updater_verify::EXPECTED_TAURI_UPDATER_PUBKEY,
            ));
        }

        // Same pin, same reasoning, for `endpoints` (CPE-1873 attempt 2): a signature check alone
        // doesn't stop a downgrade -- repointing where the app fetches `latest.json` from can serve an
        // older, genuinely-signed, vulnerable build forever, even with the pubkey pin fully intact.
        let endpoints: Vec<String> = conf_json
            .pointer("/plugins/updater/endpoints")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        let expected_endpoints: Vec<String> = cpe_updater_verify::EXPECTED_TAURI_UPDATER_ENDPOINTS
            .iter()
            .map(|s| s.to_string())
            .collect();
        if endpoints != expected_endpoints {
            return fail(&format!(
                "SECURITY (CPE-1873): the updater's manifest endpoint(s) changed. {}'s plugins.updater.endpoints \
                 does not match the pinned copy in crates/updater-verify/src/pinned_pubkey.rs::EXPECTED_TAURI_UPDATER_ENDPOINTS. \
                 If this is a deliberate, authorized change, update EXPECTED_TAURI_UPDATER_ENDPOINTS to match in the \
                 same commit -- do not sign or publish a release until it does. If this was not intentional: STOP, \
                 this tag/build is not trustworthy.\n  configured: {endpoints:?}\n  pinned:      {expected_endpoints:?}",
                conf.display(),
            ));
        }
    }

    // CPE-1903: the pin above compares the BASE config's values against compiled-in constants. It
    // cannot see a file Tauri merges on top of that config AUTOMATICALLY -- no `--config` flag, no
    // workflow involvement: `tauri-utils::config::parse::read_from` reads `tauri.conf.json` and then
    // merges `tauri.<platform>.conf.json` / `.json5` / `Tauri.<platform>.toml` from the same directory
    // via RFC 7396, on every build for that platform. CPE-1873 round 3 closed that with a `#[test]`
    // over three hardcoded `.json` filenames; `.json5` and `Tauri.<os>.toml` walked straight past it,
    // and a `#[test]` never reaches THIS path -- the one `release.yml`'s tag-triggered
    // `verify-published-manifest` job actually runs. So the check lives here too, and it derives the
    // filenames by scanning the directory instead of listing them (see
    // `cpe_updater_verify::platform_config_guard` for why enumeration kept failing).
    //
    // Deliberately OUTSIDE the `--skip-pin-check` block. That flag exists because this crate's own
    // fixtures scaffold throwaway keypairs unrelated to the pinned VALUES; this check compares no
    // values and reads no constants, so that rationale does not reach it, and the tag path keeps this
    // leg even if the flag is ever passed.
    let conf_dir = match conf.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    match cpe_updater_verify::scan_for_platform_config_updater_overrides(&conf_dir) {
        Ok(hits) if !hits.is_empty() => {
            return fail(&cpe_updater_verify::platform_config_override_message(&conf_dir, &hits));
        }
        Ok(_) => {}
        Err(e) => {
            return fail(&format!(
                "SECURITY (CPE-1903): cannot list {} to check for per-platform Tauri config files that \
                 would override the updater pin ({e}). Refusing to proceed -- an unreadable config \
                 directory is not the same as a clean one.",
                conf_dir.display()
            ));
        }
    }

    // --- Build a basename -> path index of every file under the search dirs, and find latest.json ---
    //
    // CPE-1872 (security-audit finding 2): this used to be `index.entry(name).or_insert_with(...)` --
    // first-wins, in WHATEVER order the OS's directory walk happens to visit files in. `--search
    // src-tauri/target` is a build dir a cache restore can leave stale/duplicate-named files in, and a
    // basename collision anywhere in the search tree means the file actually verified is decided by
    // filesystem enumeration order, not by which one is the genuine build output -- demonstrated by the
    // auditor's `basename_decoy` fixture (a same-named file elsewhere in the tree, containing the bytes
    // a signature actually verifies against, shadowed the real bundle output and passed EXIT=0 while the
    // real file was never read). Every basename must now be UNIQUE across the entire search tree, or this
    // refuses to guess and fails loud instead of silently keeping whichever one it saw first.
    let mut index: HashMap<String, PathBuf> = HashMap::new();
    let mut duplicate_basenames: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut discovered_manifest: Option<PathBuf> = None;
    for dir in &search_dirs {
        walk(dir, &mut |p| {
            if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                if name == "latest.json" {
                    // Prefer the most recently modified latest.json if several exist.
                    if is_newer(p, discovered_manifest.as_deref()) {
                        discovered_manifest = Some(p.to_path_buf());
                    }
                }
                if index.contains_key(name) {
                    duplicate_basenames.insert(name.to_string());
                } else {
                    index.insert(name.to_string(), p.to_path_buf());
                }
            }
        });
    }
    if !duplicate_basenames.is_empty() {
        return fail(&format!(
            "ambiguous artifact basename(s) found more than once under the search dirs -- refusing to guess which one is real: {}",
            duplicate_basenames.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }

    let manifest_path = match manifest_path.or(discovered_manifest) {
        Some(p) => p,
        None => {
            return fail(&format!(
                "no latest.json found under {} (searched: {})",
                search_dirs
                    .iter()
                    .map(|d| d.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                search_dirs.len()
            ));
        }
    };
    let manifest = match std::fs::read_to_string(&manifest_path) {
        Ok(t) => t,
        Err(e) => return fail(&format!("cannot read {}: {e}", manifest_path.display())),
    };
    // How many platforms the manifest itself names. Used both for the CPE-1923 exemption tally
    // below and for the final "verified N of M" partial-check guard, so both read the same number.
    let manifest_platform_total = cpe_updater_verify::manifest_platform_count(&manifest).unwrap_or(0);

    // CPE-1894: the guard this ticket adds. `release.yml`'s `v*` tag trigger used to match
    // `-sidecar` tags too, so the PLAIN workflow fired on a sidecar tag push and merged its plain
    // installers into the SAME draft release the sidecar workflow was populating -- one manifest
    // naming assets from two different products. That is now impossible for a NEW manifest to
    // reach this check (the tag triggers are disjoint), but this asserts on the manifest itself,
    // not the workflow YAML that used to cause it -- a test that only read the tag pattern would
    // have agreed with the very pattern that was wrong. Unconditional (not gated behind a flag,
    // unlike `--expect-url-prefix`): it costs nothing and needs no external GitHub context, only
    // this checkout's own `tauri.conf.json` (already being read above) and the manifest.
    // CPE-1923 finding 3 re-founded this on an ANCHORED comparison against `productName`: the old
    // rule was a free `basename.contains("sidecar")` substring test, which the auditor flipped in
    // both directions with nothing but release-asset write (a plain installer uploaded as
    // `…_x64-setup.nsis.zip.sidecar` passed a sidecar-channel run at EXIT 0). See
    // `cpe_updater_verify::platforms_with_mismatched_channel`.
    let channel_offenders =
        cpe_updater_verify::platforms_with_mismatched_channel(&manifest, expected_channel, &product_name);
    if !channel_offenders.is_empty() {
        let detail = channel_offenders
            .iter()
            .map(|(name, fault)| format!("{name}: {fault}"))
            .collect::<Vec<_>>()
            .join("; ");
        return fail(&format!(
            "PROPERTY FAILED -- release channel (CPE-1894/CPE-1908/CPE-1923 finding 3): expected channel '{expected_channel}' (source: {channel_source}; {} declares productName              '{product_name}'), so every platform's asset basename must ANCHOR to that product's name in its '{expected_channel}' form. These do not: {detail}. This is the shape of the CPE-1894 defect (a workflow's tag trigger firing on the wrong channel's tag and merging its installers into this release), and of CPE-1923 finding 3 (an asset renamed to claim a channel it did not come from) -- do not publish this manifest.",
            conf.display(),
        ));
    }

    // CPE-1923 finding 2 -- the platform-key -> payload-kind binding. The auditor's fixture served
    // the Windows installer under `darwin-aarch64` and the macOS `.app.tar.gz` under
    // `windows-x86_64`, EACH WITH ITS OWN GENUINE SIGNATURE: channel purity, url prefix and every
    // signature passed (`verified 2 of 2 platform signature(s)`, EXIT 0) because nothing anywhere
    // related a platform key to the kind of file behind it. Unconditional, like the channel check:
    // it needs no external context beyond the manifest itself.
    let extension_offenders = cpe_updater_verify::platforms_with_wrong_extension_for_key(&manifest);
    if !extension_offenders.is_empty() {
        let detail = extension_offenders
            .iter()
            .map(|(name, fault)| format!("{name}: {fault}"))
            .collect::<Vec<_>>()
            .join("; ");
        return fail(&format!(
            "PROPERTY FAILED -- platform/asset mapping (CPE-1923 finding 2): a platform key must \
             serve a payload its own OS's bundler produces. These do not: {detail}. Every signature \
             in such a manifest can still be genuine while every client downloads something it \
             cannot run -- do not publish this manifest."
        ));
    }

    // CPE-1923 finding 1 -- THE ANTI-ROLLBACK DECISION -- deliberately does NOT live here.
    //
    // It used to: this is where the manifest's asset basenames were checked for the version being
    // shipped. SEC-1 showed why that was worthless -- the uploaded asset's name is chosen by the
    // attacker in this guard's own threat model (release-asset write, no signing key), so the old
    // 0.1.0 installer, byte-identical and with its genuine signature, simply had to be uploaded
    // under a 0.57.70 name to pass. The only name an asset-write attacker cannot choose is the one
    // inside the minisign trusted comment, which the global signature covers -- and that is not
    // trustworthy until the signature has verified.
    //
    // So the decision moved INTO `verify_update_manifest`, immediately after each artifact's
    // `minisign::verify` succeeds, and it is reported as
    // `ManifestProblem::ArtifactNotBoundToRelease`. There is exactly one copy of the rule
    // (`cpe_updater_verify::artifact_binding::bind_signed_artifact`), reading exactly one name.

    // CPE-1872 round 3 (security-audit finding B): the crypto check below only ever proves the ARTIFACT
    // BYTES behind a `url` are genuine -- it never looks at the url's host or path, because the loader
    // matches purely by basename. A manifest can carry a perfectly-signed artifact under a `url` that
    // points at a foreign host, or at the right repo but the wrong release tag, and pass clean while
    // shipping real updater clients infrastructure this release never checked (demonstrated:
    // `n1_foreign_host_same_basename`, `n2_wrong_tag_same_basename`, both EXIT=0 before this check).
    // Enforced HERE, at the same site as the crypto check, rather than as a separate shell-level grep
    // over the manifest in release.yml: this way the binding can never be silently dropped by a future
    // refactor of the download step, and it is covered by this crate's own test suite like every other
    // manifest invariant. Opt-in via `--expect-url-prefix` (unset by default) so ad hoc/test invocations
    // that don't have a real GitHub release context keep working unchanged.
    if let Some(prefix) = &expect_url_prefix {
        let offenders = cpe_updater_verify::platforms_with_url_outside_prefix(&manifest, prefix);
        if !offenders.is_empty() {
            let detail = offenders
                .iter()
                .map(|(name, url)| format!("{name} -> {url}"))
                .collect::<Vec<_>>()
                .join(", ");
            return fail(&format!(
                "manifest platform url(s) do not start with the expected prefix '{prefix}' -- refusing \
                 to trust a manifest that could point real updater clients at unexpected infrastructure \
                 even though the artifact bytes/signatures may check out: {detail}"
            ));
        }
    }

    println!("verify-release-artifacts (CPE-1058)");
    println!("  config     : {}", conf.display());
    println!(
        "  platform cfgs: no auto-merged per-platform config in {} sets plugins.updater (CPE-1903)",
        conf_dir.display()
    );
    println!("  version    : {version}");
    println!("  channel    : {expected_channel} (source: {channel_source}; conf product name: '{product_name}')");
    println!("  manifest   : {}", manifest_path.display());
    println!("  search dirs: {}", search_dirs.len());
    if let Some(prefix) = &expect_url_prefix {
        println!("  url prefix : {prefix} (enforced)");
    }

    // --- Verify. The loader resolves a platform `url` to a local file by basename; it counts how many
    //     artifacts it actually served so we can reject a run that verified nothing. ---
    let served = Cell::new(0usize);
    let result = verify_update_manifest(&manifest, &pubkey, &version, |url| {
        let name = basename(url)?;
        let path = index.get(name)?;
        match std::fs::read(path) {
            Ok(bytes) => {
                served.set(served.get() + 1);
                println!("  verifying  : {name}");
                Some(bytes)
            }
            Err(e) => {
                eprintln!("  warn: cannot read {}: {e}", path.display());
                None
            }
        }
    });

    match result {
        Ok(verified) => {
            // CPE-1872 (security-audit finding 1): as of the lib.rs fix, `Ok(())` is only possible when
            // EVERY platform the manifest names was fetched and cryptographically verified -- a platform
            // this runner couldn't fetch is now a hard `ArtifactUnavailable` failure, not a skip, so `n`
            // is always the full platform count on this path. `served == 0` is therefore unreachable in
            // practice (an empty `platforms` is already `NoPlatforms`, which is `Err`) -- kept as a
            // belt-and-suspenders guard in case that invariant is ever loosened again, so a future
            // regression fails loud here too rather than silently printing a misleading "OK".
            // CPE-1923: every artifact admitted WITHOUT the version in its signed name is named
            // here rather than passing silently, so a run cannot consist entirely of exemptions
            // without that being visible in the log. These are the SIGNED names -- the ones an
            // asset-write attacker cannot choose -- not the uploaded basenames.
            println!(
                "  version bind: {} of {} artifact(s) were SIGNED as version '{version}'; {} exempt (macOS .app.tar.gz, which Tauri signs without a version -- CPE-1942)",
                verified.signed_files.len() - verified.versionless_exemptions.len(),
                verified.signed_files.len(),
                verified.versionless_exemptions.len()
            );
            for (platform, signed_file) in &verified.versionless_exemptions {
                println!("    exempt   : {platform} -> signed as `{signed_file}`");
            }

            // CPE-1923 finding 3, second pass -- over the SIGNED names this time. The check before
            // download reads the uploaded basename, which an asset-write attacker chooses; this one
            // reads the trusted comment, which they cannot. It is also strictly better evidence:
            // the signed name carries the raw, unsanitised product name
            // (`Cross-Platform Explorer (Sidecar)_...`) that the uploaded asset name never has.
            let signed_channel_offenders: Vec<String> = verified
                .signed_files
                .iter()
                .filter(|(_, signed_file)| {
                    let token = cpe_updater_verify::product_token(signed_file);
                    let actual = if token.starts_with(&sidecar_token) {
                        cpe_updater_verify::Channel::Sidecar
                    } else {
                        cpe_updater_verify::Channel::Plain
                    };
                    !token.starts_with(&base_token) || actual != expected_channel
                })
                .map(|(platform, signed_file)| format!("{platform}: signed as `{signed_file}`"))
                .collect();
            if !signed_channel_offenders.is_empty() {
                return fail(&format!(
                    "PROPERTY FAILED -- release channel, signed name (CPE-1923 finding 3): the \
                     signature's own trusted comment says these artifact(s) are not from the \
                     '{expected_channel}' channel: {}. The uploaded asset names claimed otherwise, \
                     so the upload was renamed -- do not publish this manifest.",
                    signed_channel_offenders.join("; ")
                ));
            }

            // CPE-1923 finding 2, second pass -- SEC-9. The pre-download mapping check reads the
            // uploaded basename, which is attacker-chosen, so the identical rename that defeated
            // SEC-1 defeated this too: the current release's genuine, correctly-versioned,
            // correctly-channelled Linux `.deb`, uploaded under `windows-x86_64` as
            // `..._x64-setup.exe`, passed at EXIT 0. The version and channel passes above do not
            // catch it -- both are satisfied, because the artifact really IS this release's build
            // of this channel. It is simply the wrong OS's payload for the key serving it, and the
            // users get denial-of-update: macOS clients downloading a Windows `.exe`, Windows
            // clients downloading a Linux `.deb`.
            //
            // Same data and same shape as the channel pass above: ask the SIGNED name whether it is
            // a payload this platform key's OS actually produces. An unrecognised key is skipped
            // here rather than reported, because `bind_signed_artifact` has already failed the run
            // for it (`UnknownPlatformKey`) -- reaching this line at all means every key resolved.
            let signed_mapping_offenders: Vec<String> = verified
                .signed_files
                .iter()
                .filter_map(|(platform, signed_file)| {
                    let os = cpe_updater_verify::platform_os_of_key(platform)?;
                    let lower = signed_file.to_ascii_lowercase();
                    let allowed = os.allowed_extensions();
                    if allowed.iter().any(|ext| lower.ends_with(ext)) {
                        None
                    } else {
                        Some(format!(
                            "{platform}: signed as `{signed_file}`, which is not a {os} updater payload (expected one of: {})",
                            allowed.join(", ")
                        ))
                    }
                })
                .collect();
            if !signed_mapping_offenders.is_empty() {
                return fail(&format!(
                    "PROPERTY FAILED -- platform/asset mapping, signed name (CPE-1923 finding 2): the signature's own trusted comment says these platform key(s) serve another OS's payload: {}. Every signature here can be genuine and current while every client downloads something it cannot run -- do not publish this manifest.",
                    signed_mapping_offenders.join("; ")
                ));
            }
            let n = served.get();
            let total = if manifest_platform_total == 0 { n } else { manifest_platform_total };
            if n == 0 || n != total {
                return fail(&format!(
                    "manifest claims {total} platform(s) but only {n} were actually, cryptographically verified -- refusing to report success on a partial check (this should be unreachable; verify_update_manifest is supposed to fail before returning Ok in this case)",
                ));
            }
            // CPE-1873 (round 2 wording fix): say plainly what this proved, and — as importantly — what
            // it did NOT. This checks that the manifest's signatures are internally CONSISTENT with the
            // pubkey baked into `tauri.conf.json` in *this* checkout, AND (via the pin check above) that
            // this checkout's own pubkey/endpoints agree with the pinned copies in pinned_pubkey.rs. That
            // is agreement between files read from the SAME commit/checkout -- it does not, and cannot,
            // consult anything outside it (no repo secret, no org variable, no previously published
            // release). It does not prove authenticity against the key users already trust; it proves
            // this commit is self-consistent, and that a lone edit to the pubkey/endpoints without the
            // matching pin update would have failed loud instead.
            println!(
                "OK: verified {n} of {total} platform signature(s) are internally consistent with the \
                 pubkey configured in this checkout's tauri.conf.json, AND that pubkey/endpoints match the \
                 second in-repo pin -- so a lone tauri.conf.json edit would have failed this run. This does \
                 NOT prove authenticity against a value outside this commit; see \
                 crates/updater-verify/src/pinned_pubkey.rs (CPE-1873) for exactly what is and isn't proven."
            );
            ExitCode::SUCCESS
        }
        Err(problems) => {
            eprintln!("FAILED: the updater manifest would not verify:");
            for p in &problems {
                eprintln!("  - {p}");
            }
            ExitCode::FAILURE
        }
    }
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("verify-release-artifacts: {msg}");
    ExitCode::FAILURE
}

/// The last path segment of an updater `url` (a GitHub release download URL), used to match it to a
/// locally-built artifact of the same filename.
fn basename(url: &str) -> Option<&str> {
    url.rsplit(['/', '\\']).next().filter(|s| !s.is_empty())
}

fn is_newer(candidate: &Path, current: Option<&Path>) -> bool {
    let Some(current) = current else { return true };
    let mtime = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    match (mtime(candidate), mtime(current)) {
        (Some(a), Some(b)) => a >= b,
        _ => true,
    }
}

/// Recursively visit every file under `dir`, calling `f` on each. Silently ignores unreadable dirs
/// (matches the app's "skip what you can't read" filesystem convention) so a permission hiccup on some
/// unrelated subtree never fails the guard.
fn walk(dir: &Path, f: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(ft) if ft.is_dir() => walk(&path, f),
            Ok(ft) if ft.is_file() => f(&path),
            _ => {}
        }
    }
}
