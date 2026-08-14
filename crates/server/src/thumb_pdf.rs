//! PDF first-page thumbnail extraction (CPE-1256, epic CPE-718): render page 0 of a `.pdf` file to
//! an `image::DynamicImage` at (approximately) `max_edge` pixels on its longest side, so the
//! thumbnail grid shows the document's actual first page instead of a generic icon. Integrates into
//! the same [`crate::thumb_source::decode_thumb_image`] dispatch the SVG/PSD/font paths use — see
//! that module for the shared orient+downscale+encode pipeline this plugs into.
//!
//! Feature-gated OFF by default (`pdf-thumb`; dependency approach decided in the research-library
//! entry `thumbnail-native-deps-pdf-video-2026-08-02.md`, Foreman decide-and-log 2026-08-02): PDF
//! rendering needs a real page-layout engine, which pure-Rust crates (`lopdf`/`pdf-render`) can't
//! provide with acceptable fidelity, and the only license-clean option that can is `pdfium-render`
//! (MIT/Apache) driving a **dynamically loaded** pdfium prebuilt (BSD-3-Clause) — Chrome's own PDF
//! engine. `mupdf` (AGPL) is rejected outright: it would attach a copyleft license to the whole
//! signed binary. Because the dependency + native lib are only pulled in when this feature is on, the
//! plain build compiles zero PDF code (the "small when off" rule) and the pdfium binary ships as a
//! bundle resource (CPE-1258), never linked into the exe.
//!
//! **Runtime lib resolution:** `pdfium-render`'s `Pdfium::new()` binds a *process-global* static
//! exactly once — a second call panics (`assert!`) — so [`pdfium`] lazily binds behind a
//! [`OnceLock`] and every call reuses the same instance (or the same cached bind failure).
//! Resolution order: (1) the injected native-dep dir ([`set_native_dep_dir`]) if the app adapter has
//! set one, (2) a `pdfium` dynamic library sitting next to the running executable (a dev-build
//! fallback — see the note below on why it's not sufficient alone), then (3) the OS's
//! system-installed pdfium, if any. If none are present, the bind attempt returns a clear `Err` and
//! every render call fails the same way — never a panic; the thumbnail pipeline's existing "no
//! thumbnail → generic type icon" fallback handles it. The underlying C library isn't documented as
//! safe for concurrent calls across different documents, so actual pdfium calls (document load +
//! render) additionally serialize behind [`PDFIUM_CALL_LOCK`] — cheap, since a thumbnail render is
//! already I/O/CPU-bound, not contended, and the thumbnail pipeline fans requests out across
//! `spawn_blocking` threads.
//!
//! **Why "next to the executable" alone isn't enough (CPE-1258 fix):** Tauri's `bundle.resources`
//! (where ship-time packaging stages pdfium) land in a *different* directory than the running
//! executable on 2 of the 3 shipped platforms — macOS (`AppName.app/Contents/MacOS/<exe>` vs
//! `AppName.app/Contents/Resources/`) and Linux (`usr/bin/<exe>` vs `usr/lib/<exe>/`); only Windows
//! NSIS stages resources next to the exe. This crate is Tauri-free and can't call
//! `app.path().resource_dir()` itself, so the app adapter resolves it and calls
//! [`set_native_dep_dir`] once at startup (mirroring how the sidecar host resolves
//! `resource_dir()` for its own bundled binaries in `src-tauri/src/lib.rs`) — that's candidate (1)
//! above, tried before the `current_exe().parent()` guess.
//!
//! Bomb-guard (mirrors `thumb_svg`'s `MAX_SVG_DIMENSION`): a PDF page's declared `MediaBox` size (in
//! points) is clamped before any render target is allocated, so a crafted absurd page size can't
//! force a giant bitmap. Encrypted (no/incorrect password), malformed, empty, or zero-page PDFs all
//! return `Err` — pdfium's own document/page-open calls already fail cleanly for these; this module
//! never unwraps/panics on their result.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use image::DynamicImage;
use pdfium_render::prelude::*;

/// Same spirit as `thumb_svg::MAX_SVG_DIMENSION` / `thumb_source::MAX_IMAGE_DIMENSION` — a PDF page's
/// *declared* MediaBox size (in points) is clamped to this before we ever ask pdfium to allocate a
/// render target. No real PDF page is anywhere near this big.
const MAX_PDF_DIMENSION: u32 = 20_000;

/// Serializes actual calls into the pdfium C library (document load + render). `pdfium-render` marks
/// its `Pdfium` handle `Send + Sync` (the `thread_safe` default feature), but that covers the Rust
/// wrapper crossing threads safely, not a guarantee that the underlying C library tolerates concurrent
/// calls across different documents — the thumbnail pipeline already fans render requests out across
/// `spawn_blocking` threads, so this serializes rather than assumes.
static PDFIUM_CALL_LOCK: Mutex<()> = Mutex::new(());

/// The process-wide pdfium binding, resolved (and cached — success or failure) on first use.
static PDFIUM: OnceLock<Result<Pdfium, String>> = OnceLock::new();

/// The bundle-resource directory injected by the (Tauri-aware) app adapter — see the module doc's
/// "CPE-1258 fix" note. `None` until [`set_native_dep_dir`] is called (dev builds / not yet wired),
/// in which case resolution falls through to the `current_exe().parent()` guess unchanged.
static NATIVE_DEP_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Sets the directory the bundled pdfium library was staged into, so [`resolve_bindings`] can find it
/// there first — call once at app startup with `app.path().resource_dir()` (or wherever the
/// `bundle.resources` pdfium entry actually lands), **before** the first thumbnail request touches
/// this module (the binding itself is resolved lazily and cached on first use, so any call after that
/// point is too late). This crate is Tauri-free and can't resolve that path itself, hence the
/// injection seam. A second call is a silent no-op — startup only ever calls this once, so that's not
/// expected to matter in practice.
pub fn set_native_dep_dir(dir: PathBuf) {
    let _ = NATIVE_DEP_DIR.set(dir);
}

/// Returns the shared, lazily-bound [`Pdfium`] instance, or the cached error from the first (and
/// only) bind attempt. Never panics even if pdfium can't be found — a missing/unloadable library is a
/// normal, expected outcome (pdfium isn't installed on most systems) reported as an `Err`.
fn pdfium() -> Result<&'static Pdfium, String> {
    PDFIUM
        .get_or_init(|| {
            let bindings = resolve_bindings().map_err(|e| e.to_string())?;
            Ok(Pdfium::new(bindings))
        })
        .as_ref()
        .map_err(|e| e.clone())
}

/// Resolves the pdfium dynamic library, trying each candidate path in order (see the module doc's
/// resolution-order note), then finally the OS's system-installed pdfium.
fn resolve_bindings() -> Result<Box<dyn PdfiumLibraryBindings>, PdfiumError> {
    for candidate in bundled_library_path_candidates() {
        if candidate.exists() {
            if let Ok(bindings) = Pdfium::bind_to_library(&candidate) {
                return Ok(bindings);
            }
        }
    }
    Pdfium::bind_to_system_library()
}

/// Every place a bundled pdfium library might sit, in resolution order: the injected native-dep dir
/// first (real installs, once [`set_native_dep_dir`] has run — this is the CPE-1258 fix, since that's
/// the ONLY correct location on macOS/Linux), then next to the running executable (already correct on
/// Windows; a dev-build fallback everywhere else, e.g. `cargo run` with a hand-copied library).
fn bundled_library_path_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::with_capacity(2);
    if let Some(dir) = NATIVE_DEP_DIR.get() {
        candidates.push(Pdfium::pdfium_platform_library_name_at_path(dir));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(Pdfium::pdfium_platform_library_name_at_path(dir));
        }
    }
    candidates
}

/// Renders page 0 of `bytes` (the contents of a `.pdf` file) to an RGBA image whose longest edge is
/// at most `max_edge` pixels, preserving the page's aspect ratio. Never panics: a missing/unloadable
/// pdfium library, an encrypted/malformed/empty/zero-page PDF, an implausible declared page size, or a
/// render-target allocation failure all return `Err` (the caller's existing contract: no thumbnail,
/// fall back to the generic type icon).
pub fn render_first_page(bytes: &[u8], max_edge: u32) -> Result<DynamicImage, String> {
    let pdfium = pdfium()?;

    // Serialize the actual FFI-touching work (see the module doc on PDFIUM_CALL_LOCK). Recover from
    // lock poisoning rather than propagate a panic from an unrelated earlier caller.
    let _guard = PDFIUM_CALL_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let document = pdfium
        .load_pdf_from_byte_slice(bytes, None)
        .map_err(|e| e.to_string())?;
    let page = document.pages().get(0).map_err(|e| e.to_string())?;

    let src_w = page.width().value;
    let src_h = page.height().value;
    if !src_w.is_finite() || !src_h.is_finite() || src_w <= 0.0 || src_h <= 0.0 {
        return Err(format!(
            "PDF page has an implausible declared size {src_w}x{src_h}"
        ));
    }
    if src_w > MAX_PDF_DIMENSION as f32 || src_h > MAX_PDF_DIMENSION as f32 {
        return Err(format!(
            "PDF page size {src_w}x{src_h}pt exceeds the {MAX_PDF_DIMENSION}px bomb-guard limit"
        ));
    }

    let edge = max_edge.max(1) as f32;
    let scale = (edge / src_w).min(edge / src_h);
    let out_w = (src_w * scale).round().max(1.0) as u32;
    let out_h = (src_h * scale).round().max(1.0) as u32;
    if out_w > MAX_PDF_DIMENSION || out_h > MAX_PDF_DIMENSION {
        return Err("PDF scaled render target exceeds the bomb-guard limit".to_string());
    }

    let config = PdfRenderConfig::new().set_target_size(out_w as Pixels, out_h as Pixels);
    let bitmap = page.render_with_config(&config).map_err(|e| e.to_string())?;
    bitmap.as_image().map_err(|e| e.to_string())
}

/// Extracts every page's text from `bytes` (the contents of a `.pdf` file), concatenated in page order
/// with a blank line between pages — the CPE-1274 text-extraction sibling of [`render_first_page`],
/// reusing the same lazily-bound [`pdfium()`] instance and [`PDFIUM_CALL_LOCK`] serialization (see the
/// module doc for why FFI calls are serialized). `max_chars` bounds accumulation *while iterating*
/// (rather than extracting everything then truncating) so an implausibly text-heavy PDF can't balloon
/// memory before the [`crate::content_text`] caller's own cap would have trimmed it anyway.
///
/// Never panics: a missing/unloadable pdfium library, or an encrypted/malformed/empty/zero-page PDF,
/// returns `Err` — the caller ([`crate::content_text::content_text_of`]) treats that as "skip this
/// file", the same discipline every other unreadable document already gets in the content index.
pub fn extract_text(bytes: &[u8], max_chars: usize) -> Result<String, String> {
    let pdfium = pdfium()?;

    // Serialize the actual FFI-touching work — same reasoning as render_first_page.
    let _guard = PDFIUM_CALL_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let document = pdfium
        .load_pdf_from_byte_slice(bytes, None)
        .map_err(|e| e.to_string())?;

    let mut out = String::new();
    for page in document.pages().iter() {
        if out.chars().count() >= max_chars {
            break;
        }
        // A single page failing to yield a text object (rare — e.g. a scanned-image-only page with no
        // text layer) shouldn't abort extraction of the rest of the document.
        if let Ok(page_text) = page.text() {
            let s = page_text.all();
            if !s.trim().is_empty() {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&s);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal but well-formed single-page PDF, built by hand with byte offsets computed
    /// programmatically (same discipline as `thumb_source`'s hand-built `minimal_psd`/`bomb_png`
    /// fixtures): a Catalog → Pages → one empty Page (200×300pt MediaBox, zero-length content
    /// stream), with a correct xref table and trailer. Real PDF readers (pdfium included) can open
    /// this even though the page draws nothing.
    fn minimal_one_page_pdf() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"%PDF-1.4\n");

        let objects: [&[u8]; 4] = [
            b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
            b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n",
            b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Resources << >> /Contents 4 0 R >>\nendobj\n",
            b"4 0 obj\n<< /Length 0 >>\nstream\nendstream\nendobj\n",
        ];

        let mut offsets = Vec::with_capacity(objects.len());
        for obj in objects.iter() {
            offsets.push(buf.len());
            buf.extend_from_slice(obj);
        }

        let xref_offset = buf.len();
        buf.extend_from_slice(b"xref\n");
        buf.extend_from_slice(format!("0 {}\n", objects.len() + 1).as_bytes());
        buf.extend_from_slice(b"0000000000 65535 f \n");
        for off in &offsets {
            buf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        buf.extend_from_slice(b"trailer\n");
        buf.extend_from_slice(format!("<< /Size {} /Root 1 0 R >>\n", objects.len() + 1).as_bytes());
        buf.extend_from_slice(b"startxref\n");
        buf.extend_from_slice(format!("{xref_offset}\n").as_bytes());
        buf.extend_from_slice(b"%%EOF");
        buf
    }

    /// Unconditional — no pdfium install needed: empty bytes must fail cleanly at the load/bind step,
    /// never panic. If pdfium isn't available in this environment, `pdfium()` itself already returns
    /// `Err` before touching the bytes at all, which still satisfies "malformed input -> Err".
    #[test]
    fn render_first_page_rejects_empty_bytes_without_panicking() {
        let err = render_first_page(&[], 64);
        assert!(err.is_err(), "empty bytes must be rejected, not panic");
    }

    /// Unconditional, same reasoning as above: garbage (non-PDF) bytes must fail cleanly.
    #[test]
    fn render_first_page_rejects_garbage_bytes_without_panicking() {
        let err = render_first_page(b"not a pdf at all, just some garbage bytes", 64);
        assert!(err.is_err(), "garbage bytes must be rejected, not panic");
    }

    /// Real-render smoke test, gated on pdfium actually being loadable in this environment. pdfium is
    /// NOT installed/bundled in the sandbox this ticket (CPE-1256) was built in — ship-time
    /// acquisition + CI provisioning is CPE-1258 — so this test attempts the bind first and SKIPS
    /// (eprintln + early return, no panic/fail) if it can't get a real pdfium library, rather than
    /// failing the suite. It only asserts a real, non-degenerate render once a real pdfium lib is
    /// present (e.g. a later CI job that provisions one, or a dev machine with the system lib
    /// installed).
    #[test]
    fn render_first_page_renders_a_real_minimal_pdf_when_pdfium_is_available() {
        if pdfium().is_err() {
            crate::skip_notice!(
                "skipping render_first_page real-render test: no pdfium library available in this \
                 environment (expected until CPE-1258 provisions one for CI/dev)"
            );
            return;
        }

        let img = render_first_page(&minimal_one_page_pdf(), 64)
            .expect("pdfium is available; rendering a well-formed minimal PDF must succeed");
        assert!(
            img.width() > 0 && img.height() > 0,
            "rendered image must be non-degenerate, got {}x{}",
            img.width(),
            img.height()
        );
        assert!(
            img.width() <= 64 && img.height() <= 64,
            "longest edge should be scaled to ~max_edge, got {}x{}",
            img.width(),
            img.height()
        );
        // 200x300pt page -> portrait, so height (the longer source edge) should hit max_edge.
        assert_eq!(img.height(), 64, "longest edge (height) scaled to max_edge");
    }

    /// Unconditional: a max_edge of 0 must clamp to at least 1px, never divide by zero / attempt a
    /// 0x0 render target — mirrors `thumb_svg`'s equivalent guard. Gated the same way as the
    /// real-render test since it still needs pdfium to actually render.
    #[test]
    fn render_first_page_handles_a_zero_max_edge_without_panicking() {
        if pdfium().is_err() {
            crate::skip_notice!(
                "skipping render_first_page zero-max-edge test: no pdfium library available in this \
                 environment (expected until CPE-1258 provisions one for CI/dev)"
            );
            return;
        }

        let img = render_first_page(&minimal_one_page_pdf(), 0)
            .expect("pdfium is available; rendering at max_edge=0 must still succeed (clamped)");
        assert_eq!((img.width(), img.height()), (1, 1));
    }

    /// A minimal single-page PDF whose content stream actually draws text ("Hello World") using a
    /// standard base-14 font (Helvetica, needs no embedded font program) — [`minimal_one_page_pdf`]
    /// draws nothing, so text extraction needs its own fixture. Built by hand with the same
    /// byte-offset/xref discipline (CPE-1274).
    fn minimal_text_pdf() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"%PDF-1.4\n");

        let content = b"BT /F1 24 Tf 72 200 Td (Hello World) Tj ET";
        let content_obj = format!(
            "4 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n",
            content.len(),
            std::str::from_utf8(content).unwrap()
        );

        let objects: Vec<String> = vec![
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".to_string(),
            "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n".to_string(),
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] \
             /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>\nendobj\n"
                .to_string(),
            content_obj,
            "5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n".to_string(),
        ];

        let mut offsets = Vec::with_capacity(objects.len());
        for obj in &objects {
            offsets.push(buf.len());
            buf.extend_from_slice(obj.as_bytes());
        }

        let xref_offset = buf.len();
        buf.extend_from_slice(b"xref\n");
        buf.extend_from_slice(format!("0 {}\n", objects.len() + 1).as_bytes());
        buf.extend_from_slice(b"0000000000 65535 f \n");
        for off in &offsets {
            buf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        buf.extend_from_slice(b"trailer\n");
        buf.extend_from_slice(format!("<< /Size {} /Root 1 0 R >>\n", objects.len() + 1).as_bytes());
        buf.extend_from_slice(b"startxref\n");
        buf.extend_from_slice(format!("{xref_offset}\n").as_bytes());
        buf.extend_from_slice(b"%%EOF");
        buf
    }

    /// Unconditional — no pdfium install needed: empty/garbage bytes must fail cleanly, never panic
    /// (mirrors `render_first_page`'s equivalent unconditional tests).
    #[test]
    fn extract_text_rejects_empty_and_garbage_bytes_without_panicking() {
        assert!(extract_text(&[], 1000).is_err(), "empty bytes must be rejected, not panic");
        assert!(
            extract_text(b"not a pdf at all", 1000).is_err(),
            "garbage bytes must be rejected, not panic"
        );
    }

    /// Real-extraction smoke test, gated on pdfium being loadable (same discipline as the render tests
    /// above — pdfium isn't bundled in this sandbox). Confirms the actual drawn text round-trips out.
    #[test]
    fn extract_text_returns_real_text_when_pdfium_is_available() {
        if pdfium().is_err() {
            crate::skip_notice!(
                "skipping extract_text real-extraction test: no pdfium library available in this \
                 environment (expected until CPE-1258 provisions one for CI/dev)"
            );
            return;
        }

        let text = extract_text(&minimal_text_pdf(), 10_000)
            .expect("pdfium is available; extracting text from a well-formed minimal PDF must succeed");
        assert!(text.contains("Hello World"), "drawn text should round-trip out: {text:?}");
    }

    /// Gated the same way: `max_chars` must actually bound accumulation rather than being ignored.
    #[test]
    fn extract_text_respects_max_chars() {
        if pdfium().is_err() {
            crate::skip_notice!(
                "skipping extract_text max_chars test: no pdfium library available in this environment \
                 (expected until CPE-1258 provisions one for CI/dev)"
            );
            return;
        }

        // A cap of 0 means the very first page already trips the break-before-append check.
        let text = extract_text(&minimal_text_pdf(), 0)
            .expect("pdfium is available; extraction with a zero cap must still succeed (empty result)");
        assert!(text.is_empty(), "a zero max_chars cap must yield no accumulated text: {text:?}");
    }
}
