//! DICOM medical-image reading (CPE-1345, gated-format-reader lane): a curated tag list for the
//! Properties dialog + pixel-data → PNG `data:` URL transcode, via the pure-Rust `dicom-rs` crates
//! (`dicom-object` parses the file meta + data set, `dicom-pixeldata` decodes pixel data to a
//! `DynamicImage`). Feature-gated behind `dicom-thumb` (off by default) so the plain build pulls in
//! none of this. Mirrors `image_preview::read_image_data_url`'s `data:image/png;base64,...` shape.
//!
//! Native compressed transfer syntaxes (JPEG2000, JPEG-LS, vendor codecs) are deliberately NOT
//! supported — `dicom-pixeldata`'s `openjp2`/`charls`/`gdcm` features all require a C toolchain and
//! are left off the build (see `Cargo.toml`). Opening a file that needs one of those cleanly errors
//! (`ts.can_decode_all()` is false for an unregistered codec) instead of decoding — never a panic.

use dicom_dictionary_std::tags;
use dicom_object::Tag;
use dicom_pixeldata::PixelDecoder;

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

/// Decode a DICOM file's first pixel-data frame to a PNG `data:` URL the `<img>` tag can show.
/// Applies the default `dicom-pixeldata` processing pipeline (Modality LUT, then the first VOI
/// LUT/window-center-width transform found in the object, if any) before encoding to PNG via the
/// `image` crate. Errors — never panics — on a corrupt file, an unreadable pixel-data attribute set,
/// or a transfer syntax needing a native codec this build doesn't carry (JPEG2000/JPEG-LS/vendor).
pub fn read_dicom_image_data_url(path: &str) -> Result<String, String> {
    use base64::Engine;
    use std::io::Cursor;

    let obj = dicom_object::open_file(path).map_err(|e| e.to_string())?;
    let pixel_data = obj.decode_pixel_data().map_err(|e| e.to_string())?;
    let dynamic_image = pixel_data.to_dynamic_image(0).map_err(|e| e.to_string())?;

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
