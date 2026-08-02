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
//! Resolution order: (1) a `pdfium` dynamic library sitting next to the running executable (where
//! ship-time bundling, CPE-1258, drops it), then (2) the OS's system-installed pdfium, if any. If
//! neither is present, the bind attempt returns a clear `Err` and every render call fails the same
//! way — never a panic; the thumbnail pipeline's existing "no thumbnail → generic type icon"
//! fallback handles it. The underlying C library isn't documented as safe for concurrent calls across
//! different documents, so actual pdfium calls (document load + render) additionally serialize behind
//! [`PDFIUM_CALL_LOCK`] — cheap, since a thumbnail render is already I/O/CPU-bound, not contended, and
//! the thumbnail pipeline fans requests out across `spawn_blocking` threads.
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

/// Resolves the pdfium dynamic library: first a copy bundled next to the running executable (where
/// ship-time packaging, CPE-1258, places it), then the OS's system-installed pdfium as a fallback
/// (useful for local dev on a platform that packages pdfium, e.g. some Linux distros).
fn resolve_bindings() -> Result<Box<dyn PdfiumLibraryBindings>, PdfiumError> {
    if let Some(bundled) = bundled_library_path() {
        if bundled.exists() {
            if let Ok(bindings) = Pdfium::bind_to_library(&bundled) {
                return Ok(bindings);
            }
        }
    }
    Pdfium::bind_to_system_library()
}

/// The platform-specific pdfium library filename (e.g. `pdfium.dll` / `libpdfium.so` /
/// `libpdfium.dylib`) resolved next to the current executable, or `None` if the executable's own
/// path can't be determined.
fn bundled_library_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    Some(Pdfium::pdfium_platform_library_name_at_path(dir))
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
            eprintln!(
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
            eprintln!(
                "skipping render_first_page zero-max-edge test: no pdfium library available in this \
                 environment (expected until CPE-1258 provisions one for CI/dev)"
            );
            return;
        }

        let img = render_first_page(&minimal_one_page_pdf(), 0)
            .expect("pdfium is available; rendering at max_edge=0 must still succeed (clamped)");
        assert_eq!((img.width(), img.height()), (1, 1));
    }
}
