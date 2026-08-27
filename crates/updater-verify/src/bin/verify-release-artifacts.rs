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
    let version = match conf_json.get("version").and_then(|v| v.as_str()) {
        Some(v) => v.to_string(),
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
    let product_name = conf_json.get("productName").and_then(|v| v.as_str()).unwrap_or("");
    let (expected_channel, channel_source) = match expect_channel {
        Some(c) => (c, "--expect-channel"),
        None => (cpe_updater_verify::expected_channel_from_product_name(product_name), "conf productName"),
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

    // CPE-1894: the guard this ticket adds. `release.yml`'s `v*` tag trigger used to match
    // `-sidecar` tags too, so the PLAIN workflow fired on a sidecar tag push and merged its plain
    // installers into the SAME draft release the sidecar workflow was populating -- one manifest
    // naming assets from two different products. That is now impossible for a NEW manifest to
    // reach this check (the tag triggers are disjoint), but this asserts on the manifest itself,
    // not the workflow YAML that used to cause it -- a test that only read the tag pattern would
    // have agreed with the very pattern that was wrong. Unconditional (not gated behind a flag,
    // unlike `--expect-url-prefix`): it costs nothing and needs no external GitHub context, only
    // this checkout's own `tauri.conf.json` (already being read above) and the manifest.
    let channel_offenders = cpe_updater_verify::platforms_with_mismatched_channel(&manifest, expected_channel);
    if !channel_offenders.is_empty() {
        let detail = channel_offenders
            .iter()
            .map(|(name, channel)| format!("{name} -> {channel}"))
            .collect::<Vec<_>>()
            .join(", ");
        return fail(&format!(
            "manifest mixes release channels (CPE-1894/CPE-1908) -- expected channel '{expected_channel}' \
             (source: {channel_source}; {} productName is '{product_name}'), so every platform's asset \
             must be from that channel, but the following platform(s) are not: {detail}. This is the \
             exact shape of the CPE-1894 defect (a workflow's \
             tag trigger firing on the wrong channel's tag and merging its installers into this release) \
             -- do not publish this manifest.",
            conf.display(),
        ));
    }

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
        Ok(()) => {
            // CPE-1872 (security-audit finding 1): as of the lib.rs fix, `Ok(())` is only possible when
            // EVERY platform the manifest names was fetched and cryptographically verified -- a platform
            // this runner couldn't fetch is now a hard `ArtifactUnavailable` failure, not a skip, so `n`
            // is always the full platform count on this path. `served == 0` is therefore unreachable in
            // practice (an empty `platforms` is already `NoPlatforms`, which is `Err`) -- kept as a
            // belt-and-suspenders guard in case that invariant is ever loosened again, so a future
            // regression fails loud here too rather than silently printing a misleading "OK".
            let n = served.get();
            let total = cpe_updater_verify::manifest_platform_count(&manifest).unwrap_or(n);
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
