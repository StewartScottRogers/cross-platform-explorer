---
id: CPE-1434
title: ".eml structured email preview (headers + MIME parts + attachments)"
type: Feature
status: Backlog
priority: High
component: Full-stack
tags: [ready]
epic: CPE-1433
created: 2026-08-07
---
## Scope
Structured preview for `.eml` (RFC 822 / MIME) files, following the crypto-viewer template exactly
(`crates/server/src/jwt_preview.rs` → `jwt_preview` command → `src/lib/components/JwtPreview.svelte` + jsdom
test, wired in `PreviewPane.svelte`, provider entry before `text`).

**Backend** — new `crates/server/src/email_preview.rs`: `email_preview(bytes: &[u8]) -> EmailPreview` (specta::Type)
with: From, To, Cc, Subject, Date (humanized), the MIME-part tree (content-type per part), an attachment list
(filename + size + content-type), and a **sanitized plain-text body** (prefer text/plain; if only text/html,
strip to text — NEVER return raw HTML/scripts and never load remote resources). Decode common transfer-encodings
(base64, quoted-printable) and MIME-encoded-word headers (`=?utf-8?...?=`). Malformed input → a graceful partial
result or `Err`, never a panic. Use a small pure-Rust MIME parse — hand-rolled, or the MIT `mailparse` crate IF
justified (flag the new dep in the PR for the Dependency Steward; prefer zero-dep if tractable).

**Command** — thin `#[tauri::command] email_preview` dispatcher in `src-tauri/src/lib.rs` into
`cpe_server::email_preview`, `spawn_blocking`, capped by the existing preview size guard (`ensure_previewable_size`).
Register in `generate_handler!` + `collect_commands!`. Regenerate bindings:
`cargo run --bin export_bindings --features "specta-bindings sidecar-platform"` (CI typed-bindings drift guard).

**Frontend** — add an `email` kind to `src/lib/preview/provider.ts` for `.eml` (+ `.mht`? no — just `.eml`),
ordered BEFORE the generic text/code provider (mirror jwt/cert). A loader in `src/lib/preview/loaders.ts`.
`EmailPreview.svelte`: header card (From/To/Cc/Subject/Date), an attachments pill row (reflow: flex-wrap + nowrap
pills, size), the sanitized body in a scroll region, a "remote content not loaded / body shown as text" note.
Light theme, dialog/card conventions.

**Tests** — `email_preview` cargo tests (a hand-built multipart message with an attachment + a QP/base64 body +
an encoded-word subject → asserts parsed fields; a malformed message → graceful). Provider-selection test (`.eml`
→ email kind, before text). jsdom `EmailPreview.test.ts` (renders headers, attachment pills, body; empty/malformed
state). Non-hollow.

**Docs (CPE-579)** — `src/docs/30-structured-previews.md` (covers the epic; extended by 1435/1436) + a
`"structured-previews"` entry in `src/lib/sectionDocs.ts` (guard must pass).

**Samples** — add a small `.eml` (with an attachment) under `samples/` so it can be exercised.

## Acceptance
- Opening a `.eml` shows a structured header/attachment/body card; malformed `.eml` degrades to text/hex (no panic).
- Backend pure + cargo-tested; provider + render specs pass; bindings regenerated; `npm run check` + `cargo test`
  + `npx vitest run` green; docs + sectionDocs added (guard passes); no raw HTML/remote-resource execution.

## Notes
First child of epic CPE-1433 — establishes the shared wiring (provider ordering, PreviewPane import block,
handler list) that CPE-1435/1436 build on, so those two follow after this merges.
