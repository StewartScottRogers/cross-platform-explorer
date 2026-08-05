//! 3D-model geometry/stats reader (CPE-1333, epic CPE-118 "Preview/edit support for 3D models
//! (STL/OBJ/GLTF)"). CPE-118 itself is parked in `Ticketing/Tickets/Blocked/` — an interactive WebGL
//! viewer needs a large frontend dependency (three.js) that can't be verified headlessly — but the
//! metadata-pane fallback the epic's acceptance criteria call for ("graceful handling … fall back to the
//! metadata pane") doesn't need a renderer at all: triangle/vertex counts and a bounding box are enough
//! for a Properties-style summary. This module reads that summary with pure Rust, std-only, zero new
//! dependencies.
//!
//! Supports binary STL, ASCII STL, and Wavefront OBJ. Pure over an in-memory byte slice — no filesystem
//! I/O; the Tauri command reads the bytes.

use serde::{Deserialize, Serialize};

/// Which of the supported 3D formats [`read_model_info`] recognised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ModelFormat {
    Stl,
    Obj,
}

/// Geometry summary for a 3D-model file, good enough for a metadata-pane fallback (triangle/vertex
/// counts + bounding box) without ever needing to actually render the mesh.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ModelInfo {
    pub format: ModelFormat,
    /// STL: the facet count. OBJ: the `f` (face) line count — OBJ faces are not necessarily triangles
    /// (they may be quads/n-gons), so this is a face count, not a guaranteed-triangle count.
    pub triangle_count: u32,
    pub vertex_count: u32,
    /// `[min_x, min_y, min_z, max_x, max_y, max_z]`. All zero when no vertices were read (an empty mesh).
    pub bounding_box: [f32; 6],
    /// True for ASCII STL and OBJ (both plain text); false for binary STL.
    pub ascii: bool,
}

/// Read `bytes` as a 3D-model file and return its geometry stats, or `None` if it matches none of the
/// supported formats (or is too corrupt/truncated to parse at all).
///
/// Detection order: binary STL first (a precise structural check — see [`parse_binary_stl`]'s doc comment
/// for why this must run *before* any text-based sniff), then ASCII STL, then OBJ. A file that fails every
/// check (e.g. random binary garbage, or a truncated/corrupt STL) yields `None` rather than panicking or
/// guessing — callers fall back to whatever generic metadata they already show.
pub fn read_model_info(bytes: &[u8]) -> Option<ModelInfo> {
    parse_binary_stl(bytes)
        .or_else(|| parse_ascii_stl(bytes))
        .or_else(|| parse_obj(bytes))
}

/// Binary STL: an 80-byte header (arbitrary bytes — **not** a reliable text signature) followed by a
/// little-endian `u32` triangle count, then 50 bytes per triangle (12-byte normal + 3×12-byte vertices +
/// 2-byte attribute count).
///
/// **Why this can't be told apart from ASCII STL by looking at the header text:** some binary STL writers
/// put a human-readable string in the 80-byte header — and that string sometimes literally starts with (or
/// contains) the word "solid", which is exactly the keyword that opens an ASCII STL. Sniffing on the
/// `solid` prefix alone would therefore misclassify such a binary file as ASCII and then fail to parse it
/// as text-flavoured floats. The robust test instead validates the file's *total length* against the
/// exact size a binary STL with the header-declared triangle count must have:
/// `80 + 4 + 50 * triangle_count == bytes.len()`. That structural identity is what an ASCII STL (whose
/// size has no such fixed relationship to a triangle count) essentially never satisfies by chance, so this
/// check runs first and is trusted over any text sniff.
fn parse_binary_stl(bytes: &[u8]) -> Option<ModelInfo> {
    if bytes.len() < 84 {
        return None;
    }
    let triangle_count = u32::from_le_bytes(bytes[80..84].try_into().ok()?);
    let body_len = (triangle_count as usize).checked_mul(50)?;
    let expected_len = 84usize.checked_add(body_len)?;
    if expected_len != bytes.len() {
        return None;
    }

    let mut bbox = empty_bbox();
    let mut vertex_count: u32 = 0;
    for i in 0..triangle_count as usize {
        let facet = 84 + i * 50;
        // Bytes 0..12 of the facet are the normal (skipped); 12..48 are 3 vertices of 12 bytes each;
        // 48..50 is the attribute byte count (skipped).
        for v in 0..3 {
            let off = facet + 12 + v * 12;
            let x = f32::from_le_bytes(bytes[off..off + 4].try_into().ok()?);
            let y = f32::from_le_bytes(bytes[off + 4..off + 8].try_into().ok()?);
            let z = f32::from_le_bytes(bytes[off + 8..off + 12].try_into().ok()?);
            grow_bbox(&mut bbox, x, y, z);
            vertex_count += 1;
        }
    }
    Some(ModelInfo {
        format: ModelFormat::Stl,
        triangle_count,
        vertex_count,
        bounding_box: normalize_bbox(bbox),
        ascii: false,
    })
}

/// ASCII STL: `solid [name]` / repeated `facet normal … / outer loop / vertex x y z (×3) / endloop /
/// endfacet` blocks / `endsolid`. Only reached once [`parse_binary_stl`] has already rejected the bytes on
/// structural grounds, so a binary STL whose header happens to contain "solid" never gets here.
fn parse_ascii_stl(bytes: &[u8]) -> Option<ModelInfo> {
    let text = std::str::from_utf8(bytes).ok()?;
    if !text.trim_start().to_ascii_lowercase().starts_with("solid") {
        return None;
    }

    let mut bbox = empty_bbox();
    let mut triangle_count: u32 = 0;
    let mut vertex_count: u32 = 0;
    let mut in_facet = false;
    for raw_line in text.lines() {
        let line = raw_line.trim();
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("facet") {
            in_facet = true;
        } else if lower.starts_with("endfacet") {
            if in_facet {
                triangle_count += 1;
            }
            in_facet = false;
        } else if lower.starts_with("vertex") {
            if let Some((x, y, z)) = parse_three_floats(line, "vertex") {
                grow_bbox(&mut bbox, x, y, z);
                vertex_count += 1;
            }
        }
    }
    if triangle_count == 0 && vertex_count == 0 {
        // Text that merely starts with the word "solid" but has no facet/vertex content at all isn't a
        // real ASCII STL (e.g. an unrelated document that opens with "Solid state drives…") — decline
        // rather than report an empty model.
        return None;
    }
    Some(ModelInfo {
        format: ModelFormat::Stl,
        triangle_count,
        vertex_count,
        bounding_box: normalize_bbox(bbox),
        ascii: true,
    })
}

/// Wavefront OBJ: count `v` (vertex) and `f` (face) lines; bounding box from the `v` coordinates.
/// Ignores every other line kind (`vt`/`vn`/`vp`/`o`/`g`/`s`/`usemtl`/`mtllib`/`#` comments/…).
fn parse_obj(bytes: &[u8]) -> Option<ModelInfo> {
    let text = std::str::from_utf8(bytes).ok()?;

    let mut bbox = empty_bbox();
    let mut vertex_count: u32 = 0;
    let mut face_count: u32 = 0;
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.starts_with("v ") || line == "v" {
            if let Some((x, y, z)) = parse_three_floats(line, "v") {
                grow_bbox(&mut bbox, x, y, z);
                vertex_count += 1;
            }
        } else if line.starts_with("f ") || line == "f" {
            face_count += 1;
        }
    }
    if vertex_count == 0 && face_count == 0 {
        return None;
    }
    Some(ModelInfo {
        format: ModelFormat::Obj,
        triangle_count: face_count,
        vertex_count,
        bounding_box: normalize_bbox(bbox),
        ascii: true,
    })
}

/// Parse the 3 whitespace-separated floats that follow `keyword` at the start of `line` (e.g. `"vertex 1.0
/// 2.0 3.0"` with `keyword = "vertex"`, or `"v 1.0 2.0 3.0"` with `keyword = "v"`). Returns `None` — rather
/// than panicking or aborting the whole parse — when the line is short or has a non-numeric token, so one
/// malformed line just doesn't contribute a vertex instead of failing the entire file.
fn parse_three_floats(line: &str, keyword: &str) -> Option<(f32, f32, f32)> {
    let rest = line.strip_prefix(keyword)?;
    let mut it = rest.split_whitespace();
    let x: f32 = it.next()?.parse().ok()?;
    let y: f32 = it.next()?.parse().ok()?;
    let z: f32 = it.next()?.parse().ok()?;
    Some((x, y, z))
}

/// A bbox accumulator seeded so the first real point always wins both the min and max side.
fn empty_bbox() -> [f32; 6] {
    [f32::MAX, f32::MAX, f32::MAX, f32::MIN, f32::MIN, f32::MIN]
}

fn grow_bbox(bbox: &mut [f32; 6], x: f32, y: f32, z: f32) {
    bbox[0] = bbox[0].min(x);
    bbox[1] = bbox[1].min(y);
    bbox[2] = bbox[2].min(z);
    bbox[3] = bbox[3].max(x);
    bbox[4] = bbox[4].max(y);
    bbox[5] = bbox[5].max(z);
}

/// If no point was ever accumulated (the seeded MAX/MIN sentinels are still in place), report an
/// all-zero box instead of leaking `f32::MAX`/`f32::MIN` to callers.
fn normalize_bbox(bbox: [f32; 6]) -> [f32; 6] {
    if bbox[0] > bbox[3] {
        [0.0; 6]
    } else {
        bbox
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal binary STL: `header` (padded/truncated to exactly 80 bytes), then the triangle
    /// count, then `triangles` — each `(normal, [v0, v1, v2])` — laid out per the spec.
    fn binary_stl(header: &[u8], triangles: &[([f32; 3], [[f32; 3]; 3])]) -> Vec<u8> {
        let mut out = vec![0u8; 80];
        let n = header.len().min(80);
        out[..n].copy_from_slice(&header[..n]);
        out.extend_from_slice(&(triangles.len() as u32).to_le_bytes());
        for (normal, verts) in triangles {
            for f in normal {
                out.extend_from_slice(&f.to_le_bytes());
            }
            for v in verts {
                for f in v {
                    out.extend_from_slice(&f.to_le_bytes());
                }
            }
            out.extend_from_slice(&[0u8, 0u8]); // attribute byte count
        }
        out
    }

    fn one_triangle() -> ([f32; 3], [[f32; 3]; 3]) {
        ([0.0, 0.0, 1.0], [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]])
    }

    #[test]
    fn binary_stl_is_read_by_structural_length_not_header_text() {
        let bytes = binary_stl(b"an ordinary binary stl header", &[one_triangle(), one_triangle()]);
        let info = read_model_info(&bytes).expect("valid binary STL");
        assert_eq!(info.format, ModelFormat::Stl);
        assert!(!info.ascii);
        assert_eq!(info.triangle_count, 2);
        assert_eq!(info.vertex_count, 6);
        assert_eq!(info.bounding_box, [0.0, 0.0, 0.0, 1.0, 1.0, 0.0]);
    }

    /// CRITICAL regression case: a binary STL whose 80-byte header happens to *contain* the word "solid"
    /// (some binary STL writers do this) must still be recognised as binary via the structural length
    /// check, not misdetected as ASCII because of the header text.
    #[test]
    fn binary_stl_with_solid_in_header_is_still_binary() {
        let header = b"solid binary stl written by some exporter that fibs in the header\0\0\0\0\0\0\0\0\0";
        let bytes = binary_stl(header, &[one_triangle()]);
        assert!(bytes.starts_with(b"solid"), "test fixture sanity: header must start with the trap word");
        let info = read_model_info(&bytes).expect("must still parse as binary STL");
        assert_eq!(info.format, ModelFormat::Stl);
        assert!(!info.ascii, "a header containing 'solid' must not fool binary detection into ASCII mode");
        assert_eq!(info.triangle_count, 1);
        assert_eq!(info.vertex_count, 3);
    }

    #[test]
    fn binary_stl_with_zero_triangles_has_zeroed_bbox() {
        let bytes = binary_stl(b"empty", &[]);
        let info = read_model_info(&bytes).expect("empty-but-well-formed binary STL");
        assert_eq!(info.triangle_count, 0);
        assert_eq!(info.vertex_count, 0);
        assert_eq!(info.bounding_box, [0.0; 6]);
    }

    #[test]
    fn ascii_stl_is_parsed_with_facet_and_vertex_counts_and_bbox() {
        let text = "solid cube_face\n\
            facet normal 0 0 1\n\
              outer loop\n\
                vertex 0 0 0\n\
                vertex 2 0 0\n\
                vertex 0 2 0\n\
              endloop\n\
            endfacet\n\
            facet normal 0 0 -1\n\
              outer loop\n\
                vertex -1 -1 -1\n\
                vertex 1 -1 -1\n\
                vertex -1 1 -1\n\
              endloop\n\
            endfacet\n\
            endsolid cube_face\n";
        let info = read_model_info(text.as_bytes()).expect("valid ASCII STL");
        assert_eq!(info.format, ModelFormat::Stl);
        assert!(info.ascii);
        assert_eq!(info.triangle_count, 2);
        assert_eq!(info.vertex_count, 6);
        assert_eq!(info.bounding_box, [-1.0, -1.0, -1.0, 2.0, 2.0, 0.0]);
    }

    #[test]
    fn ascii_stl_tolerates_a_malformed_vertex_line_by_skipping_it() {
        let text = "solid s\n\
            facet normal 0 0 1\n\
              outer loop\n\
                vertex 0 0 0\n\
                vertex not-a-number 0 0\n\
                vertex 1 1 1\n\
              endloop\n\
            endfacet\n\
            endsolid s\n";
        let info = read_model_info(text.as_bytes()).expect("still parses despite one bad line");
        assert_eq!(info.triangle_count, 1);
        assert_eq!(info.vertex_count, 2, "the malformed vertex line is skipped, not counted");
    }

    #[test]
    fn plain_text_merely_starting_with_solid_is_not_mistaken_for_stl() {
        let text = "solid state drives are a common storage medium found in most modern computers.\n\
            they have no facets, loops, or vertices at all.\n";
        assert_eq!(read_model_info(text.as_bytes()), None);
    }

    #[test]
    fn obj_counts_vertices_and_faces_with_bbox_from_v_lines() {
        let text = "# a simple triangle\n\
            o Triangle\n\
            v 0.0 0.0 0.0\n\
            v 1.0 0.0 0.0\n\
            v 0.0 1.0 0.0\n\
            vn 0.0 0.0 1.0\n\
            vt 0.0 0.0\n\
            f 1 2 3\n";
        let info = read_model_info(text.as_bytes()).expect("valid OBJ");
        assert_eq!(info.format, ModelFormat::Obj);
        assert!(info.ascii);
        assert_eq!(info.vertex_count, 3);
        assert_eq!(info.triangle_count, 1, "one f line -> face/triangle_count 1");
        assert_eq!(info.bounding_box, [0.0, 0.0, 0.0, 1.0, 1.0, 0.0]);
    }

    #[test]
    fn obj_with_only_vertices_and_no_faces_still_parses() {
        let text = "v 0 0 0\nv 5 5 5\n";
        let info = read_model_info(text.as_bytes()).expect("vertices alone are enough to recognise OBJ");
        assert_eq!(info.vertex_count, 2);
        assert_eq!(info.triangle_count, 0);
        assert_eq!(info.bounding_box, [0.0, 0.0, 0.0, 5.0, 5.0, 5.0]);
    }

    #[test]
    fn empty_bytes_yield_none() {
        assert_eq!(read_model_info(&[]), None);
    }

    #[test]
    fn random_binary_garbage_yields_none() {
        let bytes: Vec<u8> = (0..200u32).map(|i| (i % 251) as u8).collect();
        assert_eq!(read_model_info(&bytes), None);
    }

    #[test]
    fn truncated_binary_stl_declared_count_does_not_match_length_falls_through_gracefully() {
        // Declares 10 triangles in the header but the body is far too short for that — must not panic,
        // and (being non-UTF8 binary bytes) also fails every text-based fallback, yielding None.
        let mut bytes = vec![0xAAu8; 80];
        bytes.extend_from_slice(&10u32.to_le_bytes());
        bytes.extend_from_slice(&[0xFFu8; 20]); // far short of 10 * 50 bytes
        assert_eq!(read_model_info(&bytes), None);
    }

    #[test]
    fn corrupt_ascii_stl_with_no_geometry_at_all_yields_none() {
        assert_eq!(read_model_info(b"solid\nendsolid\n"), None);
    }
}
