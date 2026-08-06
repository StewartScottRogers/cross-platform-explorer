//! DICOM medical-image reading (CPE-1345, gated-format-reader lane): a curated tag list for the
//! Properties dialog + pixel-data → PNG `data:` URL transcode, via the pure-Rust `dicom-rs` crates
//! (`dicom-object` parses the file meta + data set, `dicom-pixeldata` decodes pixel data). Feature-gated
//! behind `dicom-thumb` (off by default) so the plain build pulls in none of this. Mirrors
//! `image_preview::read_image_data_url`'s `data:image/png;base64,...` shape.
//!
//! Native compressed transfer syntaxes (JPEG2000, JPEG-LS, vendor codecs) are deliberately NOT
//! supported — `dicom-pixeldata`'s `openjp2`/`charls`/`gdcm` features all require a C toolchain and
//! are left off the build (see `Cargo.toml`). Opening a file that needs one of those cleanly errors
//! (`ts.can_decode_all()` is false for an unregistered codec) instead of decoding — never a panic.
//!
//! `read_dicom_image_data_url` deliberately does NOT use `dicom-pixeldata`'s `to_dynamic_image` /
//! `image` feature (CPE-1352, trimming the +2.81 MiB DICOM ship-cost from CPE-1350): that feature pulls
//! in `dicom-pixeldata`'s OWN `image` dependency, which via Cargo feature-unification expands the app's
//! curated `image` dep (`tiff/png/jpeg/gif/webp/bmp`) to also compile the `exr` + `pnm` decoders DICOM
//! never needs. Instead it reads windowed pixel bytes via the un-feature-gated
//! `DecodedPixelData::to_vec_frame_with_options` + `rows()`/`columns()`/`samples_per_pixel()`/
//! `photometric_interpretation()`/`bits_allocated()` accessors, and builds the `image::DynamicImage`
//! from the app's own `image` dependency.

use dicom_dictionary_std::tags;
use dicom_object::Tag;
use dicom_pixeldata::{ConvertOptions, PhotometricInterpretation, PixelDecoder, VoiLutOption};

/// The curated tag set surfaced for the Properties dialog: identity + a handful of the imaging
/// attributes that describe the pixel data itself. Order here is the display order.
const CURATED_TAGS: &[(&str, Tag)] = &[
    ("PatientName", tags::PATIENT_NAME),
    ("PatientID", tags::PATIENT_ID),
    ("StudyDate", tags::STUDY_DATE),
    ("Modality", tags::MODALITY),
    ("SeriesDescription", tags::SERIES_DESCRIPTION),
    ("Rows", tags::ROWS),
    ("Columns", tags::COLUMNS),
    ("BitsAllocated", tags::BITS_ALLOCATED),
];

/// Read a curated set of DICOM tags (patient/study identity + basic imaging attributes) for the
/// Properties dialog. A tag that is absent from the file is skipped rather than erroring — only a
/// file that can't be opened as DICOM at all (corrupt/truncated/wrong format) yields `Err`.
pub fn read_dicom_tags(path: &str) -> Result<Vec<(String, String)>, String> {
    let obj = dicom_object::open_file(path).map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for (name, tag) in CURATED_TAGS {
        if let Ok(elem) = obj.element(*tag) {
            if let Ok(s) = elem.to_str() {
                let s = s.trim().to_string();
                if !s.is_empty() {
                    out.push((name.to_string(), s));
                }
            }
        }
    }
    Ok(out)
}

/// Convert a `YBR_FULL`/`YBR_FULL_422` pixel buffer (interleaved Y, Cb, Cr samples, one triple per
/// pixel) to RGB in place. Ported verbatim (same matrix, same `+0.5`-then-`floor` rounding, same
/// clamp) from `dicom-pixeldata`'s own (feature-gated-behind-`image`, hence unavailable to us now)
/// `convert_colorspace_u8` — CPE-1352 needs the identical output for parity, since this is the one
/// piece of real color-space math the old `to_dynamic_image` path did that a raw byte copy does not.
/// `data.len()` must be a multiple of 3; a short trailing partial pixel (shouldn't happen for a
/// well-formed buffer already validated to be `rows * columns * 3` bytes) is left untouched by
/// `chunks_exact_mut`.
fn convert_ybr_full_to_rgb_u8(data: &mut [u8]) {
    for pixel in data.chunks_exact_mut(3) {
        let y = pixel[0] as f32;
        let cb = pixel[1] as f32 - 128.0;
        let cr = pixel[2] as f32 - 128.0;

        let r = (y + 1.402 * cr) + 0.5;
        let g = (y + (0.114 * 1.772 / 0.587) * cb + (-0.299 * 1.402 / 0.587) * cr) + 0.5;
        let b = (y + 1.772 * cb) + 0.5;

        pixel[0] = r.floor().clamp(0.0, u8::MAX as f32) as u8;
        pixel[1] = g.floor().clamp(0.0, u8::MAX as f32) as u8;
        pixel[2] = b.floor().clamp(0.0, u8::MAX as f32) as u8;
    }
}

/// Decode a DICOM file's first pixel-data frame to a PNG `data:` URL the `<img>` tag can show.
/// Applies the same processing pipeline `dicom-pixeldata`'s own `to_dynamic_image` default applies
/// (Modality LUT, then the first VOI LUT/window-center-width transform found in the object, if any;
/// MONOCHROME1 is inverted so the minimum sample still displays as white) — but via the un-feature-gated
/// `to_vec_frame_with_options` raw-bytes path (CPE-1352) instead of `dicom-pixeldata`'s `image`
/// integration, so the app's own curated `image` dependency does the PNG encode. Handles the common
/// photometric interpretations: MONOCHROME1/MONOCHROME2 (1 sample per pixel → 8-bit grayscale), RGB (3
/// samples per pixel → 8-bit RGB, copied through unwindowed like `dicom-pixeldata` does for
/// non-monochrome images), and YBR_FULL/YBR_FULL_422 (3 samples per pixel, the standard photometric
/// interpretation for color ultrasound/Doppler DICOM — converted to RGB via `convert_ybr_full_to_rgb_u8`,
/// since the raw-bytes path does none of `dicom-pixeldata`'s own colorspace conversion). Any other
/// photometric interpretation / sample-per-pixel count is a clean `Err`. Errors — never panics — on a
/// corrupt file, an unreadable pixel-data attribute set, or a transfer syntax needing a native codec
/// this build doesn't carry (JPEG2000/JPEG-LS/vendor).
pub fn read_dicom_image_data_url(path: &str) -> Result<String, String> {
    use base64::Engine;
    use image::{DynamicImage, ImageBuffer, Luma, Rgb};
    use std::io::Cursor;

    let obj = dicom_object::open_file(path).map_err(|e| e.to_string())?;
    let pixel_data = obj.decode_pixel_data().map_err(|e| e.to_string())?;

    let rows = pixel_data.rows();
    let columns = pixel_data.columns();
    let samples_per_pixel = pixel_data.samples_per_pixel();
    let photometric_interpretation = pixel_data.photometric_interpretation();
    let bits_allocated = pixel_data.bits_allocated();

    // `VoiLutOption::First` is the raw-vec-path equivalent of `to_dynamic_image`'s default: apply the
    // first VOI LUT/window-level found in the object (its own `Default` only auto-applies the window
    // "when converting to an image" per the `dicom-pixeldata` docs; the bare-vec conversion methods
    // need `First` spelled out to get the same windowing). Note: unlike `to_dynamic_image`'s default,
    // this does not consult an explicit VOI LUT SEQUENCE (only WindowCenter/WindowWidth) — a narrow gap
    // accepted here since a plain window/level is the overwhelmingly common case.
    let options = ConvertOptions::new().with_voi_lut(VoiLutOption::First);

    let dynamic_image: DynamicImage = match (samples_per_pixel, photometric_interpretation) {
        (1, pi) if pi.is_monochrome() => {
            // `to_vec_frame_with_options`'s output range tracks the INPUT bit depth (bits_stored
            // rounded up to a power of two), not the requested output type — asking for `u8` when
            // bits_allocated > 8 would try to cast values up to 65535 into a `u8` LUT table and error.
            // So: decode at native width, then narrow to 8-bit ourselves for bits_allocated > 8,
            // mirroring `dicom-pixeldata`'s own `mono_image_with_narrow`'s `(x >> 8) as u8`. Narrowing
            // straight to 8-bit here (rather than preserving 16-bit precision) is an intentional
            // simplification: the `<img>` tag this feeds is an 8-bit-per-channel PNG either way, so
            // there is no display-visible precision to preserve past 8 bits.
            let mut gray: Vec<u8> = if bits_allocated <= 8 {
                pixel_data
                    .to_vec_frame_with_options::<u8>(0, &options)
                    .map_err(|e| e.to_string())?
            } else {
                pixel_data
                    .to_vec_frame_with_options::<u16>(0, &options)
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .map(|v| (v >> 8) as u8)
                    .collect()
            };

            // `to_vec_frame_with_options` ignores photometric interpretation entirely (per its own
            // docs), unlike `to_dynamic_image`'s default `PhotometricInterpretationOption::InvertMonochrome1`
            // — so MONOCHROME1 (minimum sample = white) needs the same inversion applied by hand here.
            if matches!(photometric_interpretation, PhotometricInterpretation::Monochrome1) {
                for px in gray.iter_mut() {
                    *px = 255 - *px;
                }
            }

            let buffer: ImageBuffer<Luma<u8>, Vec<u8>> = ImageBuffer::from_raw(columns, rows, gray)
                .ok_or_else(|| "DICOM pixel buffer size does not match rows x columns".to_string())?;
            DynamicImage::ImageLuma8(buffer)
        }
        (3, pi @ (PhotometricInterpretation::Rgb
        | PhotometricInterpretation::YbrFull
        | PhotometricInterpretation::YbrFull422)) => {
            // Non-monochrome pixel data gets no Modality/VOI LUT from `dicom-pixeldata` either (that
            // only applies when the photometric interpretation is monochrome) — but unlike the
            // monochrome case, the SAMPLE VALUES themselves still need narrowing to 8-bit for a
            // bits_allocated > 8 buffer (same reasoning/simplification as the monochrome path above:
            // the output PNG is 8-bit-per-channel regardless).
            let mut samples: Vec<u8> = if bits_allocated <= 8 {
                pixel_data
                    .to_vec_frame_with_options::<u8>(0, &options)
                    .map_err(|e| e.to_string())?
            } else {
                pixel_data
                    .to_vec_frame_with_options::<u16>(0, &options)
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .map(|v| (v >> 8) as u8)
                    .collect()
            };

            // RGB is already what the `ImageBuffer` below expects. YBR_FULL/YBR_FULL_422 (the standard
            // photometric interpretation for color ultrasound/Doppler DICOM) is Y/Cb/Cr triples and
            // needs the real colorspace conversion — `to_vec_frame_with_options` is a raw byte
            // passthrough with no colorspace awareness, unlike `dicom-pixeldata`'s own `to_dynamic_image`
            // (which special-cases exactly these two enum variants via `convert_colorspace_u8`).
            if matches!(
                pi,
                PhotometricInterpretation::YbrFull | PhotometricInterpretation::YbrFull422
            ) {
                convert_ybr_full_to_rgb_u8(&mut samples);
            }

            let buffer: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_raw(columns, rows, samples)
                .ok_or_else(|| "DICOM pixel buffer size does not match rows x columns".to_string())?;
            DynamicImage::ImageRgb8(buffer)
        }
        (spp, pi) => {
            return Err(format!(
                "unsupported DICOM pixel layout: {spp} samples/pixel, {} photometric interpretation",
                pi.as_str()
            ));
        }
    };

    let mut out = Cursor::new(Vec::new());
    dynamic_image
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(out.into_inner());
    Ok(format!("data:image/png;base64,{b64}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dicom_core::{DataElement, PrimitiveValue, VR, dicom_value};
    use dicom_dictionary_std::uids;
    use dicom_object::{FileMetaTableBuilder, InMemDicomObject};

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-dicom-{}-{}-{}", tag, std::process::id(), n));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Build a minimal, valid, **uncompressed** DICOM file (Explicit VR Little Endian) with a
    /// handful of identity tags plus a `width` x `height` 16-bit MONOCHROME2 pixel array, and write
    /// it to `path`. Mirrors the round-trip recipe in `dicom-object`'s own `mem.rs` tests
    /// (`inmem_write_to_file_with_meta`): build an `InMemDicomObject` from scratch with `put`,
    /// attach a `FileMetaTableBuilder`-built meta table naming the transfer syntax, then
    /// `write_to_file`.
    fn write_minimal_dicom(path: &std::path::Path, width: u16, height: u16) {
        let mut obj = InMemDicomObject::new_empty();

        obj.put(DataElement::new(tags::PATIENT_NAME, VR::PN, dicom_value!(Strs, ["Doe^Jane"])));
        obj.put(DataElement::new(tags::PATIENT_ID, VR::LO, dicom_value!(Strs, ["12345"])));
        obj.put(DataElement::new(tags::STUDY_DATE, VR::DA, dicom_value!(Strs, ["20240102"])));
        obj.put(DataElement::new(tags::MODALITY, VR::CS, dicom_value!(Strs, ["OT"])));
        obj.put(DataElement::new(
            tags::SERIES_DESCRIPTION,
            VR::LO,
            dicom_value!(Strs, ["Test series"]),
        ));

        obj.put(DataElement::new(tags::ROWS, VR::US, dicom_value!(U16, [height])));
        obj.put(DataElement::new(tags::COLUMNS, VR::US, dicom_value!(U16, [width])));
        obj.put(DataElement::new(tags::SAMPLES_PER_PIXEL, VR::US, dicom_value!(U16, [1])));
        obj.put(DataElement::new(
            tags::PHOTOMETRIC_INTERPRETATION,
            VR::CS,
            dicom_value!(Strs, ["MONOCHROME2"]),
        ));
        obj.put(DataElement::new(tags::BITS_ALLOCATED, VR::US, dicom_value!(U16, [16])));
        obj.put(DataElement::new(tags::BITS_STORED, VR::US, dicom_value!(U16, [16])));
        obj.put(DataElement::new(tags::HIGH_BIT, VR::US, dicom_value!(U16, [15])));
        obj.put(DataElement::new(tags::PIXEL_REPRESENTATION, VR::US, dicom_value!(U16, [0])));

        let pixels: Vec<u16> = (0..(width as u32 * height as u32))
            .map(|i| ((i * 4000) % u16::MAX as u32) as u16)
            .collect();
        obj.put(DataElement::new(tags::PIXEL_DATA, VR::OW, PrimitiveValue::U16(pixels.into())));

        let file_obj = obj
            .with_meta(
                FileMetaTableBuilder::new()
                    .transfer_syntax(uids::EXPLICIT_VR_LITTLE_ENDIAN)
                    // Secondary Capture Image Storage — a generic, non-specific SOP class is fine
                    // for a synthetic test fixture.
                    .media_storage_sop_class_uid("1.2.840.10008.5.1.4.1.1.7"),
            )
            .unwrap();
        file_obj.write_to_file(path).unwrap();
    }

    #[test]
    fn read_dicom_tags_returns_the_curated_set() {
        let d = scratch("tags");
        let f = d.join("a.dcm");
        write_minimal_dicom(&f, 4, 4);

        let tags = read_dicom_tags(&f.to_string_lossy()).unwrap();
        let map: std::collections::HashMap<_, _> = tags.into_iter().collect();

        assert_eq!(map.get("PatientName").map(String::as_str), Some("Doe^Jane"));
        assert_eq!(map.get("PatientID").map(String::as_str), Some("12345"));
        assert_eq!(map.get("StudyDate").map(String::as_str), Some("20240102"));
        assert_eq!(map.get("Modality").map(String::as_str), Some("OT"));
        assert_eq!(map.get("SeriesDescription").map(String::as_str), Some("Test series"));
        assert_eq!(map.get("Rows").map(String::as_str), Some("4"));
        assert_eq!(map.get("Columns").map(String::as_str), Some("4"));
        assert_eq!(map.get("BitsAllocated").map(String::as_str), Some("16"));

        let _ = std::fs::remove_dir_all(&d);
    }

    /// A tag not present in the file (e.g. no `SeriesDescription` set) is skipped, not an error —
    /// `read_dicom_tags` still succeeds and simply omits it.
    #[test]
    fn read_dicom_tags_skips_an_absent_tag_instead_of_erroring() {
        let d = scratch("tags-partial");
        let f = d.join("a.dcm");

        let mut obj = InMemDicomObject::new_empty();
        obj.put(DataElement::new(tags::PATIENT_NAME, VR::PN, dicom_value!(Strs, ["Only^Name"])));
        let file_obj = obj
            .with_meta(
                FileMetaTableBuilder::new()
                    .transfer_syntax(uids::EXPLICIT_VR_LITTLE_ENDIAN)
                    .media_storage_sop_class_uid("1.2.840.10008.5.1.4.1.1.7"),
            )
            .unwrap();
        file_obj.write_to_file(&f).unwrap();

        let tags = read_dicom_tags(&f.to_string_lossy()).unwrap();
        let map: std::collections::HashMap<_, _> = tags.into_iter().collect();
        assert_eq!(map.get("PatientName").map(String::as_str), Some("Only^Name"));
        assert!(!map.contains_key("Modality"));
        assert!(!map.contains_key("Rows"));

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn read_dicom_image_data_url_decodes_the_pixel_data_to_a_png() {
        let d = scratch("image");
        let f = d.join("a.dcm");
        write_minimal_dicom(&f, 4, 4);

        let url = read_dicom_image_data_url(&f.to_string_lossy()).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));

        // Decode the PNG back out and check the dimensions round-tripped.
        let b64 = url.strip_prefix("data:image/png;base64,").unwrap();
        let png_bytes = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.decode(b64).unwrap()
        };
        let decoded = image::load_from_memory(&png_bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (4, 4));

        let _ = std::fs::remove_dir_all(&d);
    }

    /// Decode a `data:image/png;base64,...` URL (as produced by `read_dicom_image_data_url`) back to
    /// an `image::DynamicImage` for pixel-level assertions in the tests below.
    fn decode_png_data_url(url: &str) -> image::DynamicImage {
        use base64::Engine;
        let b64 = url.strip_prefix("data:image/png;base64,").unwrap();
        let png_bytes = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
        image::load_from_memory(&png_bytes).unwrap()
    }

    /// A 3-samples-per-pixel RGB DICOM (no windowing applies to non-monochrome pixel data, mirroring
    /// `dicom-pixeldata` itself) decodes to an 8-bit RGB PNG with the exact input colors preserved.
    #[test]
    fn read_dicom_image_data_url_decodes_an_rgb_image() {
        let d = scratch("rgb");
        let f = d.join("a.dcm");

        let mut obj = InMemDicomObject::new_empty();
        obj.put(DataElement::new(tags::ROWS, VR::US, dicom_value!(U16, [2])));
        obj.put(DataElement::new(tags::COLUMNS, VR::US, dicom_value!(U16, [2])));
        obj.put(DataElement::new(tags::SAMPLES_PER_PIXEL, VR::US, dicom_value!(U16, [3])));
        obj.put(DataElement::new(
            tags::PHOTOMETRIC_INTERPRETATION,
            VR::CS,
            dicom_value!(Strs, ["RGB"]),
        ));
        obj.put(DataElement::new(tags::PLANAR_CONFIGURATION, VR::US, dicom_value!(U16, [0])));
        obj.put(DataElement::new(tags::BITS_ALLOCATED, VR::US, dicom_value!(U16, [8])));
        obj.put(DataElement::new(tags::BITS_STORED, VR::US, dicom_value!(U16, [8])));
        obj.put(DataElement::new(tags::HIGH_BIT, VR::US, dicom_value!(U16, [7])));
        obj.put(DataElement::new(tags::PIXEL_REPRESENTATION, VR::US, dicom_value!(U16, [0])));

        // 2x2 pixels, interleaved RGB: red, green, blue, white.
        #[rustfmt::skip]
        let pixels: Vec<u8> = vec![
            255, 0, 0,    0, 255, 0,
            0, 0, 255,    255, 255, 255,
        ];
        obj.put(DataElement::new(tags::PIXEL_DATA, VR::OB, PrimitiveValue::U8(pixels.into())));

        let file_obj = obj
            .with_meta(
                FileMetaTableBuilder::new()
                    .transfer_syntax(uids::EXPLICIT_VR_LITTLE_ENDIAN)
                    .media_storage_sop_class_uid("1.2.840.10008.5.1.4.1.1.7"),
            )
            .unwrap();
        file_obj.write_to_file(&f).unwrap();

        let url = read_dicom_image_data_url(&f.to_string_lossy()).unwrap();
        let decoded = decode_png_data_url(&url).into_rgb8();
        assert_eq!((decoded.width(), decoded.height()), (2, 2));
        assert_eq!(decoded.get_pixel(0, 0).0, [255, 0, 0]);
        assert_eq!(decoded.get_pixel(1, 0).0, [0, 255, 0]);
        assert_eq!(decoded.get_pixel(0, 1).0, [0, 0, 255]);
        assert_eq!(decoded.get_pixel(1, 1).0, [255, 255, 255]);

        let _ = std::fs::remove_dir_all(&d);
    }

    /// A YBR_FULL (YCbCr) DICOM — the standard photometric interpretation for color
    /// ultrasound/Doppler DICOM, not an exotic case — is converted to RGB rather than passed through
    /// as raw bytes. `[Y=76, Cb=85, Cr=255]` is the standard full-range JFIF YCbCr encoding of pure
    /// red; decoding it as raw RGB bytes (the bug this test guards against) would wrongly produce a
    /// dull olive `[76, 85, 255]` instead.
    #[test]
    fn read_dicom_image_data_url_converts_ybr_full_to_rgb() {
        let d = scratch("ybr");
        let f = d.join("a.dcm");

        let mut obj = InMemDicomObject::new_empty();
        obj.put(DataElement::new(tags::ROWS, VR::US, dicom_value!(U16, [1])));
        obj.put(DataElement::new(tags::COLUMNS, VR::US, dicom_value!(U16, [1])));
        obj.put(DataElement::new(tags::SAMPLES_PER_PIXEL, VR::US, dicom_value!(U16, [3])));
        obj.put(DataElement::new(
            tags::PHOTOMETRIC_INTERPRETATION,
            VR::CS,
            dicom_value!(Strs, ["YBR_FULL"]),
        ));
        obj.put(DataElement::new(tags::PLANAR_CONFIGURATION, VR::US, dicom_value!(U16, [0])));
        obj.put(DataElement::new(tags::BITS_ALLOCATED, VR::US, dicom_value!(U16, [8])));
        obj.put(DataElement::new(tags::BITS_STORED, VR::US, dicom_value!(U16, [8])));
        obj.put(DataElement::new(tags::HIGH_BIT, VR::US, dicom_value!(U16, [7])));
        obj.put(DataElement::new(tags::PIXEL_REPRESENTATION, VR::US, dicom_value!(U16, [0])));

        // A single pixel: Y=76, Cb=85, Cr=255 — pure red in full-range YCbCr.
        let pixels: Vec<u8> = vec![76, 85, 255];
        obj.put(DataElement::new(tags::PIXEL_DATA, VR::OB, PrimitiveValue::U8(pixels.into())));

        let file_obj = obj
            .with_meta(
                FileMetaTableBuilder::new()
                    .transfer_syntax(uids::EXPLICIT_VR_LITTLE_ENDIAN)
                    .media_storage_sop_class_uid("1.2.840.10008.5.1.4.1.1.7"),
            )
            .unwrap();
        file_obj.write_to_file(&f).unwrap();

        let url = read_dicom_image_data_url(&f.to_string_lossy()).unwrap();
        let decoded = decode_png_data_url(&url).into_rgb8();
        assert_eq!((decoded.width(), decoded.height()), (1, 1));
        let [r, g, b] = decoded.get_pixel(0, 0).0;
        assert!(r >= 254, "expected R ~= 255 (pure red), got {r}");
        assert!(g <= 1, "expected G ~= 0 (pure red), got {g}");
        assert!(b <= 1, "expected B ~= 0 (pure red), got {b}");

        let _ = std::fs::remove_dir_all(&d);
    }

    /// MONOCHROME1 (minimum sample value = white) is inverted so it displays the same as an
    /// equivalent MONOCHROME2 image would — mirroring `dicom-pixeldata`'s own `to_dynamic_image`
    /// default (`PhotometricInterpretationOption::InvertMonochrome1`), which the raw-vec conversion
    /// path used here does not apply automatically.
    #[test]
    fn read_dicom_image_data_url_inverts_monochrome1() {
        let d = scratch("mono1");
        let f = d.join("a.dcm");

        let mut obj = InMemDicomObject::new_empty();
        obj.put(DataElement::new(tags::ROWS, VR::US, dicom_value!(U16, [2])));
        obj.put(DataElement::new(tags::COLUMNS, VR::US, dicom_value!(U16, [2])));
        obj.put(DataElement::new(tags::SAMPLES_PER_PIXEL, VR::US, dicom_value!(U16, [1])));
        obj.put(DataElement::new(
            tags::PHOTOMETRIC_INTERPRETATION,
            VR::CS,
            dicom_value!(Strs, ["MONOCHROME1"]),
        ));
        obj.put(DataElement::new(tags::BITS_ALLOCATED, VR::US, dicom_value!(U16, [8])));
        obj.put(DataElement::new(tags::BITS_STORED, VR::US, dicom_value!(U16, [8])));
        obj.put(DataElement::new(tags::HIGH_BIT, VR::US, dicom_value!(U16, [7])));
        obj.put(DataElement::new(tags::PIXEL_REPRESENTATION, VR::US, dicom_value!(U16, [0])));

        let pixels: Vec<u8> = vec![0, 50, 200, 255];
        obj.put(DataElement::new(tags::PIXEL_DATA, VR::OB, PrimitiveValue::U8(pixels.into())));

        let file_obj = obj
            .with_meta(
                FileMetaTableBuilder::new()
                    .transfer_syntax(uids::EXPLICIT_VR_LITTLE_ENDIAN)
                    .media_storage_sop_class_uid("1.2.840.10008.5.1.4.1.1.7"),
            )
            .unwrap();
        file_obj.write_to_file(&f).unwrap();

        let url = read_dicom_image_data_url(&f.to_string_lossy()).unwrap();
        let decoded = decode_png_data_url(&url).into_luma8();
        assert_eq!((decoded.width(), decoded.height()), (2, 2));
        // Input samples [0, 50, 200, 255] inverted (255 - x) => [255, 205, 55, 0].
        assert_eq!(decoded.get_pixel(0, 0).0, [255]);
        assert_eq!(decoded.get_pixel(1, 0).0, [205]);
        assert_eq!(decoded.get_pixel(0, 1).0, [55]);
        assert_eq!(decoded.get_pixel(1, 1).0, [0]);

        let _ = std::fs::remove_dir_all(&d);
    }

    /// A file with no recognizable magic bytes / structure at all errors, never panics.
    #[test]
    fn read_dicom_tags_and_image_error_on_a_corrupt_file() {
        let d = scratch("corrupt");
        let f = d.join("bad.dcm");
        std::fs::write(&f, b"this is not a dicom file, just some short garbage bytes").unwrap();

        assert!(read_dicom_tags(&f.to_string_lossy()).is_err());
        assert!(read_dicom_image_data_url(&f.to_string_lossy()).is_err());

        let _ = std::fs::remove_dir_all(&d);
    }

    /// A transfer syntax that would need a native (C) codec this build doesn't carry — JPEG 2000,
    /// gated behind the `openjp2` feature we deliberately leave off — errors cleanly rather than
    /// attempting to decode or panicking.
    #[test]
    fn read_dicom_image_data_url_errors_on_an_unsupported_native_codec_transfer_syntax() {
        let d = scratch("jp2k");
        let f = d.join("a.dcm");

        let mut obj = InMemDicomObject::new_empty();
        obj.put(DataElement::new(tags::ROWS, VR::US, dicom_value!(U16, [2])));
        obj.put(DataElement::new(tags::COLUMNS, VR::US, dicom_value!(U16, [2])));
        obj.put(DataElement::new(tags::SAMPLES_PER_PIXEL, VR::US, dicom_value!(U16, [1])));
        obj.put(DataElement::new(
            tags::PHOTOMETRIC_INTERPRETATION,
            VR::CS,
            dicom_value!(Strs, ["MONOCHROME2"]),
        ));
        obj.put(DataElement::new(tags::BITS_ALLOCATED, VR::US, dicom_value!(U16, [8])));
        obj.put(DataElement::new(tags::BITS_STORED, VR::US, dicom_value!(U16, [8])));
        obj.put(DataElement::new(tags::HIGH_BIT, VR::US, dicom_value!(U16, [7])));
        obj.put(DataElement::new(tags::PIXEL_REPRESENTATION, VR::US, dicom_value!(U16, [0])));
        // A handful of bytes standing in for a JPEG2000 codestream we never actually decode — the
        // transfer syntax is rejected before the pixel data content is even inspected.
        obj.put(DataElement::new(tags::PIXEL_DATA, VR::OB, dicom_value!(U8, [0u8, 1, 2, 3])));

        let file_obj = obj
            .with_meta(
                FileMetaTableBuilder::new()
                    .transfer_syntax(uids::JPEG2000)
                    .media_storage_sop_class_uid("1.2.840.10008.5.1.4.1.1.7"),
            )
            .unwrap();
        file_obj.write_to_file(&f).unwrap();

        let err = read_dicom_image_data_url(&f.to_string_lossy());
        assert!(err.is_err(), "an unsupported native-codec transfer syntax must error, not decode/panic");

        let _ = std::fs::remove_dir_all(&d);
    }
}
