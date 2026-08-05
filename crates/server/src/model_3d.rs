//! 3D-model geometry/stats reader (CPE-1333, epic CPE-118 "Preview/edit support for 3D models
//! (STL/OBJ/GLTF)"). CPE-118 itself is parked in `Ticketing/Tickets/Blocked/` — an interactive WebGL
//! viewer needs a large frontend dependency (three.js) that can't be verified headlessly — but the
//! metadata-pane fallback the epic's acceptance criteria call for ("graceful handling … fall back to the
//! metadata pane") doesn't need a renderer at all: triangle/vertex counts and a bounding box are enough
//! for a Properties-style summary. This module reads that summary with pure Rust, std-only, zero new
//! dependencies.
//!
//! Supports binary STL, ASCII STL, Wavefront OBJ, glTF (JSON, `.gltf`), and glTF Binary (`.glb`). Pure
//! over an in-memory byte slice — no filesystem I/O; the Tauri command reads the bytes.
//!
//! glTF/GLB support (CPE-1335) is deliberately partial and says so honestly: full triangle/vertex counts
//! need per-primitive accessor dereferencing (resolving each mesh primitive's `POSITION`/`indices`
//! accessor, decoding buffer views, honoring primitive `mode`) that this module doesn't implement. Rather
//! than fabricate a number, glTF/GLB reports the mesh count (directly available from the document) and
//! leaves triangle/vertex counts at an honest `0`. The bounding box is filled in when accessors carry the
//! `min`/`max` extrema glTF requires on a rendered `POSITION` accessor.

use serde::{Deserialize, Serialize};

/// Which of the supported 3D formats [`read_model_info`] recognised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ModelFormat {
    Stl,
    Obj,
    /// Covers both a standalone `.gltf` JSON document and the JSON chunk of a binary `.glb` container;
    /// [`ModelInfo::ascii`] distinguishes the two.
    Gltf,
}

/// Geometry summary for a 3D-model file, good enough for a metadata-pane fallback (triangle/vertex
/// counts + bounding box) without ever needing to actually render the mesh.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ModelInfo {
    pub format: ModelFormat,
    /// STL: the facet count. OBJ: the `f` (face) line count — OBJ faces are not necessarily triangles
    /// (they may be quads/n-gons), so this is a face count, not a guaranteed-triangle count. glTF/GLB:
    /// always `0` — see the module doc comment for why this isn't computed.
    pub triangle_count: u32,
    /// glTF/GLB: always `0` — see the module doc comment for why this isn't computed.
    pub vertex_count: u32,
    /// `[min_x, min_y, min_z, max_x, max_y, max_z]`. All zero when no vertices were read (an empty mesh,
    /// or — for glTF/GLB — no `POSITION` accessor carried `min`/`max`).
    pub bounding_box: [f32; 6],
    /// True for ASCII STL, OBJ, and a standalone `.gltf` JSON file (all plain text); false for binary STL
    /// and `.glb` (the JSON chunk is text, but the container itself is a binary format).
    pub ascii: bool,
    /// glTF/GLB only: the document's top-level `meshes` array length — the one geometry count directly
    /// available without accessor dereferencing. Always `0` for STL/OBJ (the concept doesn't apply).
    pub mesh_count: u32,
}

/// Read `bytes` as a 3D-model file and return its geometry stats, or `None` if it matches none of the
/// supported formats (or is too corrupt/truncated to parse at all).
///
/// Detection order: binary STL first (a precise structural check — see [`parse_binary_stl`]'s doc comment
/// for why this must run *before* any text-based sniff), then ASCII STL, then OBJ, then binary GLB (a
/// precise magic-number + structural check), then glTF JSON last (the most permissive text format, so it
/// must lose to every more specific check). A file that fails every check (e.g. random binary garbage, or
/// a truncated/corrupt STL/GLB) yields `None` rather than panicking or guessing — callers fall back to
/// whatever generic metadata they already show.
pub fn read_model_info(bytes: &[u8]) -> Option<ModelInfo> {
    parse_binary_stl(bytes)
        .or_else(|| parse_ascii_stl(bytes))
        .or_else(|| parse_obj(bytes))
        .or_else(|| parse_glb(bytes))
        .or_else(|| parse_gltf_json(bytes))
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
        mesh_count: 0,
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
        mesh_count: 0,
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
        mesh_count: 0,
    })
}

/// Binary glTF (`.glb`): a 12-byte header — magic `b"glTF"`, `u32` LE version, `u32` LE total declared
/// file length — followed by one or more chunks (`u32` LE chunk length + 4-byte chunk type + chunk data).
/// The first `JSON`-typed chunk holds the glTF document itself; any `BIN`-typed chunk (raw buffer bytes)
/// is skipped over — this reader only needs the document metadata, never the buffer contents. Every
/// offset, the declared total length, and every chunk's bounds are validated against `bytes.len()`
/// *before* any slice is taken, so a bad magic, a lying/truncated length, or a chunk that claims to run
/// past the end of the file all yield `None` instead of panicking.
fn parse_glb(bytes: &[u8]) -> Option<ModelInfo> {
    if bytes.len() < 12 || &bytes[0..4] != b"glTF" {
        return None;
    }
    let _version = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
    let declared_len = u32::from_le_bytes(bytes[8..12].try_into().ok()?) as usize;
    if declared_len < 12 || declared_len > bytes.len() {
        return None;
    }

    let mut offset = 12usize;
    while offset.checked_add(8)? <= declared_len {
        let chunk_len = u32::from_le_bytes(bytes[offset..offset + 4].try_into().ok()?) as usize;
        let chunk_type = &bytes[offset + 4..offset + 8];
        let data_start = offset + 8;
        let data_end = data_start.checked_add(chunk_len)?;
        if data_end > declared_len || data_end > bytes.len() {
            // A chunk claiming a length that would run past the declared/actual file end — corrupt or
            // adversarial input; decline rather than slice out of bounds.
            return None;
        }
        if chunk_type == b"JSON" {
            let doc: serde_json::Value = serde_json::from_slice(&bytes[data_start..data_end]).ok()?;
            return model_info_from_gltf_document(&doc, false);
        }
        offset = data_end;
    }
    // Ran out of chunks (or hit the declared length) without finding a JSON chunk — not a usable glTF
    // document.
    None
}

/// glTF (`.gltf`): the entire file is the JSON document — no container framing to validate first.
/// `serde_json::from_slice(...).ok()?` means malformed JSON (truncated, invalid syntax, wrong top-level
/// shape) falls through to `None` rather than panicking.
fn parse_gltf_json(bytes: &[u8]) -> Option<ModelInfo> {
    let doc: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    model_info_from_gltf_document(&doc, true)
}

/// Build a [`ModelInfo`] from an already-parsed glTF JSON document (shared by [`parse_glb`] and
/// [`parse_gltf_json`]). `ascii` records which container it came from — a standalone `.gltf` text file
/// (`true`) or the JSON chunk pulled out of a binary `.glb` (`false`).
///
/// Requires a top-level `asset` object, which the glTF spec mandates on every conforming document — its
/// absence means this is just some unrelated JSON file, not a mis-parsed model, so it's rejected with
/// `None` rather than reported as an empty model.
fn model_info_from_gltf_document(doc: &serde_json::Value, ascii: bool) -> Option<ModelInfo> {
    let obj = doc.as_object()?;
    obj.get("asset")?.as_object()?;

    let mesh_count = obj
        .get("meshes")
        .and_then(|m| m.as_array())
        .map(|meshes| meshes.len() as u32)
        .unwrap_or(0);

    let bounding_box = obj
        .get("accessors")
        .and_then(|a| a.as_array())
        .map(|accessors| bbox_from_position_accessors(obj, accessors))
        .unwrap_or([0.0; 6]);

    Some(ModelInfo {
        format: ModelFormat::Gltf,
        // Honest 0s, not fabricated numbers — see the module doc comment for why these aren't computed.
        triangle_count: 0,
        vertex_count: 0,
        bounding_box,
        ascii,
        mesh_count,
    })
}

/// Walk every mesh primitive's `POSITION` attribute, resolve that index into `accessors`, and grow a
/// bounding box from each accessor's `min`/`max` (the glTF spec requires both on a `POSITION` accessor
/// used for rendering, so a conforming exporter always provides them). A primitive with no `POSITION`
/// attribute, an index that doesn't resolve to an accessor, or an accessor missing `min`/`max` is simply
/// skipped rather than aborting the whole bbox — a partial box from whatever accessors do carry the
/// extrema beats reporting nothing.
fn bbox_from_position_accessors(
    doc: &serde_json::Map<String, serde_json::Value>,
    accessors: &[serde_json::Value],
) -> [f32; 6] {
    let mut bbox = empty_bbox();
    let Some(meshes) = doc.get("meshes").and_then(|m| m.as_array()) else {
        return normalize_bbox(bbox);
    };
    for mesh in meshes {
        let Some(primitives) = mesh.get("primitives").and_then(|p| p.as_array()) else {
            continue;
        };
        for prim in primitives {
            let Some(index) = prim
                .get("attributes")
                .and_then(|a| a.get("POSITION"))
                .and_then(|v| v.as_u64())
            else {
                continue;
            };
            let Some(accessor) = accessors.get(index as usize) else {
                continue;
            };
            let Some(min) = accessor.get("min").and_then(read_vec3) else {
                continue;
            };
            let Some(max) = accessor.get("max").and_then(read_vec3) else {
                continue;
            };
            grow_bbox(&mut bbox, min[0], min[1], min[2]);
            grow_bbox(&mut bbox, max[0], max[1], max[2]);
        }
    }
    normalize_bbox(bbox)
}

/// Read a JSON array's first 3 elements as `f32`s (a glTF accessor `min`/`max` is `[x, y, z]` for a
/// `VEC3` POSITION accessor). `None` for anything shorter or non-numeric, rather than panicking on
/// out-of-bounds indexing.
fn read_vec3(v: &serde_json::Value) -> Option<[f32; 3]> {
    let arr = v.as_array()?;
    if arr.len() < 3 {
        return None;
    }
    let x = arr[0].as_f64()? as f32;
    let y = arr[1].as_f64()? as f32;
    let z = arr[2].as_f64()? as f32;
    Some([x, y, z])
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

    /// A minimal-but-valid glTF JSON document: one mesh with one primitive whose `POSITION` accessor
    /// carries `min`/`max`. Used both as a standalone `.gltf` fixture and as the JSON chunk of a `.glb`.
    const MINIMAL_GLTF_JSON: &str = r#"{
        "asset": {"version": "2.0"},
        "meshes": [
            {"primitives": [{"attributes": {"POSITION": 0}}]}
        ],
        "accessors": [
            {"componentType": 5126, "count": 3, "type": "VEC3", "min": [0.0, -1.0, 0.0], "max": [2.0, 1.0, 3.0]}
        ]
    }"#;

    /// Build a well-formed `.glb`: 12-byte header (magic/version/declared total length) followed by a
    /// single `JSON`-typed chunk holding `json`, space-padded to a 4-byte boundary per the glTF spec.
    fn glb(json: &[u8]) -> Vec<u8> {
        let mut padded = json.to_vec();
        while padded.len() % 4 != 0 {
            padded.push(b' ');
        }
        let total_len = 12 + 8 + padded.len();
        let mut out = Vec::with_capacity(total_len);
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes()); // version
        out.extend_from_slice(&(total_len as u32).to_le_bytes());
        out.extend_from_slice(&(padded.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&padded);
        out
    }

    #[test]
    fn gltf_json_reports_mesh_count_and_bbox_from_accessor_min_max() {
        let info = read_model_info(MINIMAL_GLTF_JSON.as_bytes()).expect("valid glTF JSON");
        assert_eq!(info.format, ModelFormat::Gltf);
        assert!(info.ascii, "a standalone .gltf JSON file is text");
        assert_eq!(info.mesh_count, 1);
        assert_eq!(
            info.triangle_count, 0,
            "honest 0 — accessor dereferencing for a real triangle count isn't implemented"
        );
        assert_eq!(
            info.vertex_count, 0,
            "honest 0 — accessor dereferencing for a real vertex count isn't implemented"
        );
        assert_eq!(info.bounding_box, [0.0, -1.0, 0.0, 2.0, 1.0, 3.0]);
    }

    #[test]
    fn gltf_json_without_accessor_min_max_has_zeroed_bbox() {
        let text = r#"{
            "asset": {"version": "2.0"},
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
            "accessors": [{"componentType": 5126, "count": 3, "type": "VEC3"}]
        }"#;
        let info = read_model_info(text.as_bytes()).expect("valid glTF JSON, just no accessor extrema");
        assert_eq!(info.format, ModelFormat::Gltf);
        assert_eq!(info.mesh_count, 1);
        assert_eq!(info.bounding_box, [0.0; 6]);
    }

    #[test]
    fn gltf_json_with_no_meshes_has_zero_mesh_count() {
        let text = r#"{"asset": {"version": "2.0"}}"#;
        let info = read_model_info(text.as_bytes()).expect("bare asset-only document is still valid glTF");
        assert_eq!(info.format, ModelFormat::Gltf);
        assert_eq!(info.mesh_count, 0);
        assert_eq!(info.bounding_box, [0.0; 6]);
    }

    #[test]
    fn arbitrary_json_without_an_asset_object_is_not_mistaken_for_gltf() {
        let text = r#"{"hello": "world", "meshes": [1, 2, 3]}"#;
        assert_eq!(read_model_info(text.as_bytes()), None);
    }

    #[test]
    fn malformed_json_yields_none_not_a_panic() {
        assert_eq!(read_model_info(b"{ this is not valid json"), None);
    }

    #[test]
    fn glb_is_read_via_the_json_chunk_with_mesh_count_and_bbox() {
        let bytes = glb(MINIMAL_GLTF_JSON.as_bytes());
        let info = read_model_info(&bytes).expect("well-formed glb");
        assert_eq!(info.format, ModelFormat::Gltf);
        assert!(!info.ascii, "a .glb container is binary even though its JSON chunk is text");
        assert_eq!(info.mesh_count, 1);
        assert_eq!(info.triangle_count, 0);
        assert_eq!(info.vertex_count, 0);
        assert_eq!(info.bounding_box, [0.0, -1.0, 0.0, 2.0, 1.0, 3.0]);
    }

    #[test]
    fn glb_with_bad_magic_yields_none() {
        let mut bytes = glb(MINIMAL_GLTF_JSON.as_bytes());
        bytes[0..4].copy_from_slice(b"NOPE");
        assert_eq!(read_model_info(&bytes), None);
    }

    #[test]
    fn glb_truncated_before_the_header_completes_yields_none() {
        assert_eq!(read_model_info(b"glTF\x02\x00\x00\x00"), None, "only 8 of the 12 header bytes present");
    }

    #[test]
    fn glb_with_declared_length_longer_than_the_actual_file_yields_none() {
        let mut bytes = glb(MINIMAL_GLTF_JSON.as_bytes());
        let lying_len = (bytes.len() as u32) + 10_000;
        bytes[8..12].copy_from_slice(&lying_len.to_le_bytes());
        assert_eq!(read_model_info(&bytes), None, "declared total length must not exceed the real byte count");
    }

    #[test]
    fn glb_with_chunk_length_longer_than_the_declared_file_yields_none() {
        let mut bytes = glb(MINIMAL_GLTF_JSON.as_bytes());
        // Chunk length field is bytes[12..16] — inflate it so the chunk claims to run past the file end.
        let lying_chunk_len = 1_000_000u32;
        bytes[12..16].copy_from_slice(&lying_chunk_len.to_le_bytes());
        assert_eq!(read_model_info(&bytes), None, "a chunk that overruns the file must not be sliced");
    }

    #[test]
    fn glb_with_malformed_json_chunk_yields_none() {
        let bytes = glb(b"{ not valid json at all");
        assert_eq!(read_model_info(&bytes), None);
    }

    #[test]
    fn glb_too_short_to_contain_even_a_header_yields_none() {
        assert_eq!(read_model_info(b"glT"), None);
    }
}
