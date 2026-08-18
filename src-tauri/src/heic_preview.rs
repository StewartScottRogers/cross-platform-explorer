//! HEIC/HEIF preview via per-OS platform APIs (CPE-1351, epic CPE-097).
//!
//! `.heic`/`.heif`/`.hif` files carry HEVC-compressed image data the webview can't render and no
//! pure-Rust decoder handles cleanly. Rather than bundle libheif (an LGPL C library + its HEVC
//! codec), we decode through the platform's own image stack — Windows Imaging Component (WIC) on
//! Windows, ImageIO/CoreGraphics on macOS — and hand the raw RGBA off to the shared PNG encoder in
//! `cpe-server` (`image_preview::encode_rgba_to_png_data_url`). Only the FFI lives here; the
//! PNG-encode + base64 wrap stays in the tested domain crate.
//!
//! Dispatch is by plain platform `cfg` (no Cargo feature). Every platform call maps its error to an
//! `Err(String)` — never a panic/unwrap — so the frontend can fall back to the metadata view. On
//! Windows the HEIF codec is an optional Store component ("HEIF Image Extensions"); when it isn't
//! installed the decoder-open fails with the `WINCODEC_ERR_COMPONENTNOTFOUND` family and we return a
//! clear, distinct hint. macOS decodes HEIC natively (since 10.13) with nothing to install.

/// Decode a HEIC/HEIF file to a PNG `data:image/png;base64,...` URL, dispatching to the platform
/// image stack. `Err` (never a panic) on any failure — corrupt file, missing codec, unsupported
/// platform — so the caller can fall back to a metadata preview.
pub fn decode_heic_preview(path: &str) -> Result<String, String> {
    #[cfg(windows)]
    {
        decode_heic_windows(path)
    }
    #[cfg(target_os = "macos")]
    {
        decode_heic_macos(path)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = path;
        Err("HEIC preview is not supported on this platform".into())
    }
}

// ---------------------------------------------------------------------------------------------
// Windows — Windows Imaging Component (WIC)
// ---------------------------------------------------------------------------------------------

#[cfg(windows)]
fn decode_heic_windows(path: &str) -> Result<String, String> {
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

    // COM must be initialised on this (spawn_blocking) thread before creating the WIC factory.
    // S_OK/S_FALSE both mean "we initialised it and must pair a CoUninitialize"; RPC_E_CHANGED_MODE
    // means COM is already up in another mode — we proceed but must NOT balance it. `is_ok()` is
    // true for S_OK and S_FALSE and false for the RPC_E_CHANGED_MODE failure, which is exactly the
    // "did we initialise it?" question.
    let did_init = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();

    // Run the actual decode with COM up, then ALWAYS balance our own init (even on error).
    let result = decode_heic_windows_inner(path);

    if did_init {
        unsafe { CoUninitialize() };
    }
    result
}

#[cfg(windows)]
fn decode_heic_windows_inner(path: &str) -> Result<String, String> {
    use std::os::windows::ffi::OsStrExt;

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::GENERIC_READ;
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, IWICImagingFactory, WICBitmapDitherTypeNone,
        WICBitmapPaletteTypeMedianCut, WICDecodeMetadataCacheOnDemand,
        GUID_WICPixelFormat32bppRGBA,
    };
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};

    // `WINCODEC_ERR_COMPONENTNOTFOUND` = 0x88982F50 — the whole family lives around here; the exact
    // code when the HEIF codec is absent is COMPONENTNOTFOUND. Treat it as "no codec installed".
    const WINCODEC_ERR_COMPONENTNOTFOUND: u32 = 0x8898_2F50;

    // Null-terminated wide (UTF-16) path for the Win32 W API.
    let wide: Vec<u16> = std::ffi::OsStr::new(path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("WIC imaging factory unavailable: {e}"))?;

        let decoder = factory
            .CreateDecoderFromFilename(
                PCWSTR(wide.as_ptr()),
                None,
                GENERIC_READ,
                WICDecodeMetadataCacheOnDemand,
            )
            .map_err(|e| {
                if (e.code().0 as u32) == WINCODEC_ERR_COMPONENTNOTFOUND {
                    "HEIC decode unavailable: install the HEIF Image Extensions from the Microsoft Store"
                        .to_string()
                } else {
                    format!("HEIC decode failed to open file: {e}")
                }
            })?;

        let frame = decoder
            .GetFrame(0)
            .map_err(|e| format!("WIC GetFrame(0) failed: {e}"))?;

        let converter = factory
            .CreateFormatConverter()
            .map_err(|e| format!("WIC CreateFormatConverter failed: {e}"))?;
        converter
            .Initialize(
                &frame,
                &GUID_WICPixelFormat32bppRGBA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeMedianCut,
            )
            .map_err(|e| format!("WIC format-converter Initialize failed: {e}"))?;

        let mut w: u32 = 0;
        let mut h: u32 = 0;
        converter
            .GetSize(&mut w, &mut h)
            .map_err(|e| format!("WIC GetSize failed: {e}"))?;
        if w == 0 || h == 0 {
            return Err("HEIC decode produced empty dimensions".into());
        }
        // A malformed/hostile HEIC can *declare* enormous dimensions independent of its file size (the
        // upstream `ensure_previewable_size` cap bounds bytes, not pixels). Refuse absurd dimensions
        // before allocating: this keeps the RGBA buffer well under `u32::MAX` bytes — both avoiding a
        // multi-gigabyte transient allocation and sidestepping the windows-rs `CopyPixels` wrapper's
        // internal `len().try_into::<u32>().unwrap()` panic on an over-4GiB buffer. (CPE-1351 review.)
        const MAX_HEIC_DIM: u32 = 20_000;
        if w > MAX_HEIC_DIM || h > MAX_HEIC_DIM {
            return Err(format!(
                "HEIC dimensions {w}x{h} exceed the {MAX_HEIC_DIM}px preview limit"
            ));
        }

        let stride = w
            .checked_mul(4)
            .ok_or("HEIC width overflows a 4-byte-per-pixel stride")?;
        let buf_len = (stride as usize)
            .checked_mul(h as usize)
            .ok_or("HEIC dimensions overflow the pixel-buffer size")?;
        let mut buf = vec![0u8; buf_len];
        // `prc = null` copies the whole frame (windows 0.56 types this param as a raw `*const
        // WICRect`, not an `Option`).
        converter
            .CopyPixels(std::ptr::null(), stride, &mut buf)
            .map_err(|e| format!("WIC CopyPixels failed: {e}"))?;

        cpe_server::image_preview::encode_rgba_to_png_data_url(w, h, buf)
    }
}

// ---------------------------------------------------------------------------------------------
// macOS — ImageIO / CoreGraphics
// ---------------------------------------------------------------------------------------------
//
// NOTE: This path cannot be compiled or run on the Windows dev box; it is written per the vetted
// Library plan and is compile-checked by CI's macOS runner. Visual correctness is an attended check
// on a real Mac. Every fallible platform call maps `None`/error to `Err(String)`.

#[cfg(target_os = "macos")]
fn decode_heic_macos(path: &str) -> Result<String, String> {
    use std::ffi::c_void;

    // objc2 0.3 deprecated the free-function forms (`CGImageSourceCreateWithURL`, …) in favour of
    // associated-method forms on the types; CI's macOS `clippy -D warnings` promotes those
    // deprecation notes to errors, so we call the method forms here. The parameter/return types are
    // identical to the free functions — only the call path moved onto the type (CPE-1351 CI fix).
    use objc2_core_foundation::{CFURL, CGPoint, CGRect, CGSize};
    use objc2_core_graphics::{
        CGColorSpace, CGContext, CGImage, CGImageAlphaInfo,
    };
    use objc2_image_io::CGImageSource;

    // On macOS the file-system representation of a path is its UTF-8 bytes.
    let bytes = path.as_bytes();
    unsafe {
        let url = CFURL::from_file_system_representation(
            None,
            bytes.as_ptr(),
            bytes.len() as isize,
            false,
        )
        .ok_or("failed to build CFURL for the HEIC path")?;

        let source = CGImageSource::with_url(&url, None)
            .ok_or("ImageIO could not open the file as an image source")?;

        let image = CGImageSource::image_at_index(&source, 0, None)
            .ok_or("ImageIO could not decode the first HEIC image")?;

        let w = CGImage::width(Some(&image));
        let h = CGImage::height(Some(&image));
        if w == 0 || h == 0 {
            return Err("HEIC decode produced empty dimensions".into());
        }

        let color_space =
            CGColorSpace::new_device_rgb().ok_or("failed to create a device RGB color space")?;

        let stride = w.checked_mul(4).ok_or("HEIC width overflows the row stride")?;
        let buf_len = stride
            .checked_mul(h)
            .ok_or("HEIC dimensions overflow the pixel-buffer size")?;
        let mut buf = vec![0u8; buf_len];

        // 8 bits/component, alpha-last (RGBA) — draw the decoded image into our own buffer.
        // NOTE: the bitmap-context constructor's renamed method form couldn't be confirmed from the
        // objc2 0.3 docs (unlike the other calls here), and this box can't compile macOS to check —
        // so this ONE known-compiling free-function call is kept under a scoped `#[allow(deprecated)]`
        // rather than risk a wrong rename that would break the macOS build. Behaviour is unchanged.
        #[allow(deprecated)]
        let ctx = objc2_core_graphics::CGBitmapContextCreate(
            buf.as_mut_ptr() as *mut c_void,
            w,
            h,
            8,
            stride,
            Some(&color_space),
            CGImageAlphaInfo::PremultipliedLast.0,
        )
        .ok_or("failed to create an RGBA bitmap context")?;

        let rect = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(w as f64, h as f64));
        CGContext::draw_image(Some(&ctx), rect, Some(&image));

        cpe_server::image_preview::encode_rgba_to_png_data_url(w as u32, h as u32, buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> cpe_server::fsutil::ScratchDir {
        cpe_server::fsutil::scratch_dir(&format!("cpe-heic-{tag}"))
    }

    /// A corrupt/truncated `.heic` must return `Err` (no panic) on every platform, whether or not a
    /// codec is present. Note the test asserts "no panic; graceful Err", never decode success — CI
    /// machines (and Windows boxes without the HEIF Image Extensions) legitimately can't decode.
    #[test]
    fn corrupt_heic_errors_without_panicking() {
        let d = scratch("bad");
        let f = d.join("bad.heic");
        std::fs::write(&f, b"not a real heic file, just some bytes").unwrap();
        let r = decode_heic_preview(&f.to_string_lossy());
        assert!(r.is_err(), "a corrupt HEIC must yield Err, not Ok/panic");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// If a real decode happens to succeed on this machine (this dev box HAS the HEIF Image
    /// Extensions installed), the result must be a well-formed PNG data URL whose payload decodes to
    /// non-zero dimensions. Environment-independent: a missing codec / missing fixture simply skips
    /// the success assertions rather than failing. Set `CPE_HEIC_FIXTURE` to a real `.heic` path to
    /// exercise the Ok branch locally.
    #[test]
    fn real_heic_if_decodable_is_a_well_formed_png_url() {
        let Ok(fixture) = std::env::var("CPE_HEIC_FIXTURE") else {
            return; // no fixture provided — nothing to assert, and we never assert success
        };
        if !std::path::Path::new(&fixture).is_file() {
            return;
        }
        // Only assert on success; a codec-absent / undecodable environment yields Err, which is fine.
        if let Ok(url) = decode_heic_preview(&fixture) {
            assert!(
                url.starts_with("data:image/png;base64,"),
                "a successful decode must be a PNG data URL"
            );
            use base64::Engine;
            let b64 = url.strip_prefix("data:image/png;base64,").unwrap();
            let raw = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
            let img = image::load_from_memory(&raw).unwrap();
            assert!(img.width() > 0 && img.height() > 0, "decoded image has real dimensions");
        }
    }
}
