---
id: CPE-1350
title: "Wire DICOM preview provider + ENABLE dicom-thumb in the shipped build (user-approved)"
type: Feature
status: Done
priority: Medium
component: Multiple
tags: [ready]
epic: CPE-219
created: 2026-08-05
closed: 2026-08-05
---

## Goal

Make `.dcm` files show image + key tags in the preview pane, and **ship it** — the user approved enabling
the `dicom-thumb` feature in the released build (moderate binary-size cost accepted). Backend reader exists
(CPE-1345, feature-gated `dicom-thumb`, `cpe_server::dicom::{read_dicom_tags, read_dicom_image_data_url}`).

## Changes (mirror the CPE-1349 raw-image provider wiring)

1. **Enable the feature in the app build** — turn on `cpe-server/dicom-thumb` for the app (src-tauri):
   add it to the default/enabled feature set the release build compiles with (find how features flow from
   `src-tauri/Cargo.toml` into `cpe-server`; mirror how any always-on cpe-server feature is enabled, or add a
   `dicom` passthrough feature that's in the default set). Confirm the shipped build (the `Release
   (sidecar-enabled)` config) compiles it in. Document the size delta in the PR.
2. **Command(s)** — thin `#[tauri::command]` dispatchers in `src-tauri/src/lib.rs`:
   `read_dicom_image_data_url(path) -> Result<String,String>` and `read_dicom_tags(path) ->
   Result<Vec<(String,String)>,String>` → `spawn_blocking` into `cpe_server::dicom::*`. These call into
   feature-gated code, so the commands are only compiled when `dicom-thumb` is on (gate the `#[tauri::command]`
   fns with `#[cfg(feature=...)]` as appropriate, matching how the app conditionally exposes feature-gated
   commands elsewhere — grep for an existing `#[cfg(feature` command pattern). Register in `generate_handler!`.
3. **Bindings** — regen `bindings.gen.ts` via the `export_bindings` bin (as CPE-1349 did) so the new command
   wrappers exist; drift guard must pass.
4. **Frontend provider** — `provider.ts`: add a `dicom` kind (or reuse `decoded-image`/`raw-image` shape) +
   `DICOM_EXT = new Set(["dcm"])`, placed appropriately. The DICOM view shows the decoded image AND the tag
   list — decide the cleanest rendering (image via data-URL like raw-image; tags in the metadata/details area,
   or a small tag table). Keep it read-only.
5. **Loader/render** — route `.dcm` to `read_dicom_image_data_url` (+ `read_dicom_tags` for the tag list);
   on `Err` (unsupported transfer syntax / corrupt) fall back to metadata. Cancel-on-selection-change.
6. **Tests** — provider vitest (dcm → dicom provider); loader/PreviewPane test (routing + Err→fallback,
   mocked invoke). Backend command compiles under the feature.

## Acceptance criteria

- Released build compiles with `dicom-thumb` on; selecting a `.dcm` requests the DICOM image + tags and
  renders them; unsupported/corrupt → metadata fallback (no broken pane).
- `cargo clippy --all-targets` green in the app's shipped feature config AND default; bindings regenerated
  (drift guard passes). `npm run check` clean; JS suite green.
- PR notes the binary/installer size delta from enabling dicom-rs (Performance-Guard concern).

## Notes

Sequence AFTER CPE-1349 (shares `src-tauri/lib.rs`, `bindings.gen.ts`, `provider.ts`, `loaders.ts`,
`PreviewPane.svelte`). Final on-screen visual (does the DICOM image render + tags read) is an attended/
Visual-Critic check post-merge. Ship-enablement mirrors CPE-1258 (pdf-thumb) pattern.

## Work Log
- 2026-08-05 (workshift): PR #645 merged. DICOM shipped: dicom-thumb enabled in app build; read_dicom_image_data_url + read_dicom_tags commands; dcm provider renders image + independent tag list (tags show even if image decode Errs); Err->metadata fallback. Reviewer(+supply-chain) APPROVE + UAT PASS (mutation-tested). Size +2.81 MiB (real crates: 7 dicom + exr/pnm via image-feature unification + CJK charset tables; AVIF/JXL are UNCOMPILED lock rows, 0 bytes). Trim follow-up = CPE-1352. Supply-chain minor: encoding-index-* unmaintained (transitive from dicom-rs). On-screen render = attended.
