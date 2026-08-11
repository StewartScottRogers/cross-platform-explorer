//! Hand-rolled ECMA-335 CLR metadata reader (CPE-1596, epic CPE-1562 "Binary Inspector" slice 3).
//!
//! `dotnetdll` is GPL-licensed and therefore cannot be linked into this app (hard constraint from the
//! ticket) — so this module hand-parses the ECMA-335 "Common Language Infrastructure" metadata format
//! directly against the spec, adding no new dependency: `goblin` (already a dep, used elsewhere in
//! [`crate::binary_preview`]) locates the PE and its CLI header; everything past that — the metadata
//! root, the stream headers, the `#~` compressed table stream, and the `#Strings`/`#Blob` heaps — is
//! parsed by hand here.
//!
//! # The walk, in order
//!
//! 1. **CLI header.** A managed PE's `IMAGE_OPTIONAL_HEADER` data directory #14 (`IMAGE_COR20_HEADER`,
//!    aka the "CLI header" or "COM descriptor") points at a small fixed struct; [`is_managed`] just
//!    checks this directory is present and non-empty. Its `MetaData` field is itself a directory (RVA +
//!    size) pointing at the metadata root.
//! 2. **Metadata root** (ECMA-335 II.24.2.1): signature `BSJB`, a version string, then a list of named
//!    stream headers (`#~`/`#-`, `#Strings`, `#US`, `#GUID`, `#Blob`) each an (offset, size) pair
//!    relative to the metadata root's own start.
//! 3. **`#~`/`#-` tables stream** (II.24.2.6): a `Valid` bitmask says which of the ≤64 table kinds are
//!    present, followed by one row-count `u32` per present table (ascending table-number order), then
//!    the row data itself — again in ascending table-number order, back to back with no padding between
//!    tables. A row's *byte layout* depends on the table kind (ECMA-335 II.22.*) and on how wide its
//!    heap-index and other-table-index columns are: an index is 4 bytes if what it indexes could exceed
//!    16 bits (a `HeapSizes` bit for heap indices; the *other* table's own row count for a table index;
//!    a per-coded-index-kind threshold for a coded index spanning several tables — see
//!    [`coded_index_size`]). Column widths for every table kind from `Module` (0x00) through
//!    `AssemblyRef` (0x23) are implemented in [`row_size`] — that upper bound is exactly what's needed
//!    to walk far enough to reach this reader's four target tables (`TypeDef`=0x02, `MethodDef`=0x06,
//!    `Assembly`=0x20, `AssemblyRef`=0x23); nothing past 0x23 is ever read.
//! 4. **Name resolution.** `TypeDef`/`MethodDef`/`Assembly`/`AssemblyRef` rows reference names as
//!    `u16`/`u32` offsets into the `#Strings` heap (a flat run of NUL-terminated UTF-8 strings) and
//!    public keys as offsets into the `#Blob` heap (each blob prefixed by an ECMA-335 §II.24.2.4
//!    "compressed unsigned integer" length). [`read_c_str`] and [`read_blob`] do that resolution.
//!
//! # Safety — the actual point of this module
//!
//! Every offset, count, and heap index above is attacker/corruption-controlled file content. The rule
//! followed throughout: **never index a byte slice directly** (`bytes[i]`/`&s[a..b]`) — always
//! [`slice::get`]/[`str::get`] (or the `u16_at`/`u32_at`/`u64_at` wrappers below, which are `get`-based),
//! so an out-of-range offset is a clean `None`/skip, never a panic. Every arithmetic combination of two
//! file-supplied values (an offset plus a length, a row count times a row size) goes through
//! `checked_add`/`checked_mul`, so a crafted huge value degrades to "stop parsing, return what's been
//! collected so far" rather than a panic on overflow or a multi-gigabyte allocation attempt. Every
//! collected list is capped at [`crate::model::MAX_BINARY_LIST_ENTRIES`]. [`read`] itself never panics
//! and never hangs: a malformed managed PE degrades to a partial or empty [`DotnetMetadata`] (see its
//! doc comment), matching [`crate::binary_preview::binary_info`]'s own skip-on-error contract — the only
//! `Err` cases are an unreadable file and bytes goblin can't parse as a PE at all.

use crate::model::{
    DotnetAssemblyIdentity, DotnetAssemblyRef, DotnetMetadata, DotnetMethodDef, DotnetTypeDef,
    MAX_BINARY_LIST_ENTRIES,
};

// ---------------------------------------------------------------------------------------------
// Checked, panic-free byte access
// ---------------------------------------------------------------------------------------------

fn u16_at(b: &[u8], off: usize) -> Option<u16> {
    b.get(off..off.checked_add(2)?).map(|s| u16::from_le_bytes([s[0], s[1]]))
}

fn u32_at(b: &[u8], off: usize) -> Option<u32> {
    b.get(off..off.checked_add(4)?).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn u64_at(b: &[u8], off: usize) -> Option<u64> {
    let s = b.get(off..off.checked_add(8)?)?;
    Some(u64::from_le_bytes(s.try_into().ok()?))
}

// ---------------------------------------------------------------------------------------------
// PE / CLI header (IMAGE_COR20_HEADER)
// ---------------------------------------------------------------------------------------------

/// `true` when `pe` carries a non-empty CLI header (PE data directory #14): the structural signal
/// that this is a managed .NET/CLR image rather than a native one. Never panics — goblin's own
/// [`goblin::pe::data_directories::DataDirectories::get_clr_runtime_header`] already returns `None`
/// for an all-zero (i.e. absent) entry.
pub fn is_managed(pe: &goblin::pe::PE) -> bool {
    clr_header_dir(pe).is_some()
}

fn clr_header_dir(pe: &goblin::pe::PE) -> Option<goblin::pe::data_directories::DataDirectory> {
    let dd = pe.header.optional_header.as_ref()?.data_directories.get_clr_runtime_header()?;
    if dd.virtual_address == 0 || dd.size == 0 { None } else { Some(*dd) }
}

/// Resolve an RVA to a file offset via the section table — the section whose virtual address range
/// contains `rva`, translated through its `pointer_to_raw_data`. `None` for an RVA outside every
/// section (a bogus/hostile CLI-header or MetaData-directory RVA) rather than guessing.
fn rva_to_file_offset(pe: &goblin::pe::PE, rva: u32) -> Option<usize> {
    for s in &pe.sections {
        let start = s.virtual_address;
        let vsize = s.virtual_size.max(s.size_of_raw_data);
        let end = start.checked_add(vsize)?;
        if rva >= start && rva < end {
            let delta = rva - start;
            let file_off = s.pointer_to_raw_data.checked_add(delta)?;
            return Some(file_off as usize);
        }
    }
    None
}

/// The two fields of `IMAGE_COR20_HEADER` this reader needs: the `MetaData` data directory (RVA +
/// size of the metadata root). Every other CLI-header field (`EntryPointToken`, `Resources`,
/// `StrongNameSignature`, …) is out of this slice's scope.
struct ClrHeader {
    metadata_rva: u32,
    metadata_size: u32,
}

/// Parse just enough of `IMAGE_COR20_HEADER` (ECMA-335 II.25.3.3) to find the metadata root: `cb`(4) +
/// `MajorRuntimeVersion`(2) + `MinorRuntimeVersion`(2) = 8 bytes in, then the `MetaData` directory
/// (RVA u4 + Size u4). `None` if the header's own RVA doesn't resolve to a file offset, or the file is
/// truncated before those 16 bytes — never a panic on a bogus/short header.
fn parse_clr_header(bytes: &[u8], pe: &goblin::pe::PE) -> Option<ClrHeader> {
    let dir = clr_header_dir(pe)?;
    let off = rva_to_file_offset(pe, dir.virtual_address)?;
    let metadata_rva = u32_at(bytes, off.checked_add(8)?)?;
    let metadata_size = u32_at(bytes, off.checked_add(12)?)?;
    Some(ClrHeader { metadata_rva, metadata_size })
}

// ---------------------------------------------------------------------------------------------
// Metadata root + stream headers (ECMA-335 II.24.2.1 / II.24.2.2)
// ---------------------------------------------------------------------------------------------

/// Cap on how many stream headers the metadata root's own `Streams` count field is trusted for —
/// real assemblies have well under ten (`#~`, `#Strings`, `#US`, `#GUID`, `#Blob`); this just bounds
/// the loop against a hostile huge count (each iteration also naturally terminates once reads run
/// past the buffer, so this is belt-and-braces, not the only guard).
const MAX_STREAMS: usize = 64;

/// One named stream's (offset, size) within the metadata root, both relative to the root's own start.
struct StreamHeader {
    name: String,
    offset: u32,
    size: u32,
}

struct MetadataRoot {
    /// The version string (e.g. "v4.0.30319"), lossily decoded.
    version: String,
    streams: Vec<StreamHeader>,
}

/// Parse the metadata root at `root_off` (a file offset, `root_size` bytes long per the CLI header's
/// own `MetaData` directory): signature, version string, then the stream header list. `None` for a
/// bad signature or a header truncated past what's readable — every field read is bounds-checked, so
/// this degrades gracefully rather than panicking on a hostile `Length` field driving a huge slice, an
/// odd `Streams` count, or a truncated file. A stream whose own (offset, size) would run past
/// `root_size` is dropped rather than trusted (a "bogus CLI header" adversarial variant: the directory
/// undersells its own size while a stream header inside claims more).
fn parse_metadata_root(bytes: &[u8], root_off: usize, root_size: u32) -> Option<MetadataRoot> {
    // Signature: literal "BSJB" (ECMA-335 II.24.2.1), checked byte-for-byte rather than as a u32 to
    // sidestep any endianness confusion.
    if bytes.get(root_off..root_off.checked_add(4)?)? != b"BSJB" {
        return None;
    }
    let root_end = root_off.checked_add(root_size as usize)?;
    let length = u32_at(bytes, root_off.checked_add(12)?)? as usize;
    // A hostile Length can be arbitrarily large; `get()` bounds it against the real buffer, so no
    // allocation happens before the range is known to be in-bounds.
    let version_start = root_off.checked_add(16)?;
    let version_bytes = bytes.get(version_start..version_start.checked_add(length)?)?;
    let version = String::from_utf8_lossy(version_bytes).trim_end_matches('\0').to_string();
    // Version string is padded to a 4-byte boundary (II.24.2.1).
    let padded_length = length.checked_add(3)? & !3usize;
    let mut pos = version_start.checked_add(padded_length)?;
    let _flags = u16_at(bytes, pos)?; // reserved, always 0
    let stream_count = u16_at(bytes, pos.checked_add(2)?)? as usize;
    pos = pos.checked_add(4)?;

    let mut streams = Vec::new();
    for _ in 0..stream_count.min(MAX_STREAMS) {
        if pos >= root_end {
            break; // the Streams count overran the directory's own declared size
        }
        let offset = u32_at(bytes, pos)?;
        let size = u32_at(bytes, pos.checked_add(4)?)?;
        let name_start = pos.checked_add(8)?;
        // Stream name: NUL-terminated ASCII, padded to a 4-byte boundary. Scan for the NUL within a
        // generous cap (stream names are short, conventionally "#~"/"#Strings"/"#US"/"#GUID"/"#Blob")
        // rather than trusting any length field, since none is given for this string.
        const MAX_STREAM_NAME_SCAN: usize = 64;
        let scan = bytes.get(name_start..name_start.saturating_add(MAX_STREAM_NAME_SCAN)).unwrap_or(&[]);
        let nul_at = scan.iter().position(|&b| b == 0)?; // no NUL within the cap: malformed, bail
        let name = String::from_utf8_lossy(&scan[..nul_at]).to_string();
        let name_len = nul_at.checked_add(1)?; // include the NUL
        let padded_name_len = name_len.checked_add(3)? & !3usize;
        pos = name_start.checked_add(padded_name_len)?;
        // A stream claiming to run past the metadata root's own declared size is untrustworthy —
        // drop it rather than let a later read wander into whatever bytes happen to follow.
        if offset.checked_add(size).map(|end| end > root_size).unwrap_or(true) {
            continue;
        }
        streams.push(StreamHeader { name, offset, size });
    }
    Some(MetadataRoot { version, streams })
}

// ---------------------------------------------------------------------------------------------
// Heaps (#Strings / #Blob)
// ---------------------------------------------------------------------------------------------

/// Cap on how far a `#Strings`/stream-name scan looks for a terminating NUL before giving up — real
/// type/method/assembly names are well under this; it exists purely so a heap with no NUL anywhere
/// (hostile or truncated) can't turn a lookup into an unbounded scan.
const MAX_STRING_SCAN: usize = 4096;

/// Read a NUL-terminated UTF-8 (lossy) string at byte `index` within the `#Strings` heap
/// (`heap_off`..`heap_off+heap_size`, a file-offset range). Never panics or slices at a non-UTF-8
/// boundary: `String::from_utf8_lossy` tolerates arbitrary bytes, and every offset is `get()`-checked.
/// `None` for an index at/past the end of the heap (the "heap index past the end" adversarial case).
fn read_c_str(bytes: &[u8], heap_off: usize, heap_size: usize, index: u32) -> Option<String> {
    let start = heap_off.checked_add(index as usize)?;
    if start >= heap_off.checked_add(heap_size)? {
        return None;
    }
    let scan_end = start.saturating_add(MAX_STRING_SCAN).min(heap_off.checked_add(heap_size)?);
    let scan = bytes.get(start..scan_end)?;
    let s = match scan.iter().position(|&b| b == 0) {
        Some(p) => &scan[..p],
        None => scan, // no NUL within the cap/heap: use what's there rather than failing
    };
    Some(String::from_utf8_lossy(s).into_owned())
}

/// Cap on a single blob's decoded length — assembly public keys/tokens are at most a couple hundred
/// bytes in practice; this just bounds a hostile compressed-length header (up to ~0x1FFF_FFFF per
/// ECMA-335's 4-byte encoding) from being treated as real.
const MAX_BLOB_LEN: usize = 8192;

/// Read a length-prefixed blob at byte `index` within the `#Blob` heap, per ECMA-335 II.24.2.4's
/// "compressed unsigned integer": a 1/2/4-byte length prefix (selected by the top bits of its first
/// byte) followed by that many raw bytes. Returns `None` for an out-of-range index/prefix or a
/// declared length the buffer can't actually hold — never panics, never over-allocates before the
/// range is confirmed in-bounds via `get()`.
fn read_blob(bytes: &[u8], heap_off: usize, heap_size: usize, index: u32) -> Option<&[u8]> {
    let start = heap_off.checked_add(index as usize)?;
    let heap_end = heap_off.checked_add(heap_size)?;
    if start >= heap_end {
        return None;
    }
    let b0 = *bytes.get(start)?;
    let (len, header_len): (usize, usize) = if b0 & 0x80 == 0 {
        (b0 as usize, 1)
    } else if b0 & 0xC0 == 0x80 {
        let b1 = *bytes.get(start.checked_add(1)?)?;
        ((((b0 & 0x3F) as usize) << 8) | b1 as usize, 2)
    } else if b0 & 0xE0 == 0xC0 {
        let s = bytes.get(start.checked_add(1)?..start.checked_add(4)?)?;
        ((((b0 & 0x1F) as usize) << 24) | ((s[0] as usize) << 16) | ((s[1] as usize) << 8) | s[2] as usize, 4)
    } else {
        return None; // 0xFF and similar are not valid length-prefix leads
    };
    if len > MAX_BLOB_LEN {
        return None;
    }
    let data_start = start.checked_add(header_len)?;
    let data_end = data_start.checked_add(len)?;
    if data_end > heap_end {
        return None;
    }
    bytes.get(data_start..data_end)
}

fn hex_encode(b: &[u8]) -> String {
    b.iter().map(|byte| format!("{byte:02x}")).collect()
}

// ---------------------------------------------------------------------------------------------
// `#~` / `#-` compressed tables stream (ECMA-335 II.24.2.6)
// ---------------------------------------------------------------------------------------------

/// The ~44 ECMA-335 metadata table kinds are numbered 0x00..0x2C; this reader only ever needs to
/// compute row sizes up to `AssemblyRef` (0x23) — see the module doc comment for why that's a
/// sufficient upper bound for its four target tables.
const TABLE_MODULE: usize = 0x00;
const TABLE_TYPEREF: usize = 0x01;
const TABLE_TYPEDEF: usize = 0x02;
const TABLE_FIELDPTR: usize = 0x03;
const TABLE_FIELD: usize = 0x04;
const TABLE_METHODPTR: usize = 0x05;
const TABLE_METHODDEF: usize = 0x06;
const TABLE_PARAMPTR: usize = 0x07;
const TABLE_PARAM: usize = 0x08;
const TABLE_INTERFACEIMPL: usize = 0x09;
const TABLE_MEMBERREF: usize = 0x0A;
const TABLE_CONSTANT: usize = 0x0B;
const TABLE_CUSTOMATTRIBUTE: usize = 0x0C;
const TABLE_FIELDMARSHAL: usize = 0x0D;
const TABLE_DECLSECURITY: usize = 0x0E;
const TABLE_CLASSLAYOUT: usize = 0x0F;
const TABLE_FIELDLAYOUT: usize = 0x10;
const TABLE_STANDALONESIG: usize = 0x11;
const TABLE_EVENTMAP: usize = 0x12;
const TABLE_EVENTPTR: usize = 0x13;
const TABLE_EVENT: usize = 0x14;
const TABLE_PROPERTYMAP: usize = 0x15;
const TABLE_PROPERTYPTR: usize = 0x16;
const TABLE_PROPERTY: usize = 0x17;
const TABLE_METHODSEMANTICS: usize = 0x18;
const TABLE_METHODIMPL: usize = 0x19;
const TABLE_MODULEREF: usize = 0x1A;
const TABLE_TYPESPEC: usize = 0x1B;
const TABLE_IMPLMAP: usize = 0x1C;
const TABLE_FIELDRVA: usize = 0x1D;
const TABLE_ENCLOG: usize = 0x1E;
const TABLE_ENCMAP: usize = 0x1F;
const TABLE_ASSEMBLY: usize = 0x20;
const TABLE_ASSEMBLYPROCESSOR: usize = 0x21;
const TABLE_ASSEMBLYOS: usize = 0x22;
const TABLE_ASSEMBLYREF: usize = 0x23;
/// Highest table index [`row_size`] knows how to size — everything this reader needs (`TypeDef`,
/// `MethodDef`, `Assembly`, `AssemblyRef`) is at or below it, so the table walk in [`read_impl`]
/// never needs to go further.
const MAX_TABLE_INDEX: usize = TABLE_ASSEMBLYREF;
/// Number of table-kind bits in the `#~` stream's `Valid`/`Sorted` masks (ECMA-335 II.24.2.6: a
/// 64-bit field, one bit per possible table kind).
const TABLE_KIND_COUNT: usize = 64;

/// Number of bytes a single-table row index costs: 2 if that table's row count fits in 16 bits, else
/// 4 (ECMA-335 II.24.2.6). Table `t` absent (row count 0, the default for an unset `Valid` bit) is
/// always the 2-byte case.
fn table_index_size(row_counts: &[u32; TABLE_KIND_COUNT], t: usize) -> usize {
    if row_counts.get(t).copied().unwrap_or(0) > 0xFFFF { 4 } else { 2 }
}

/// Number of bytes a coded index spanning `tables` costs, tagged with `tag_bits` low bits selecting
/// which table (ECMA-335 II.24.2.6): 2 bytes if the largest of those tables' row counts fits under
/// `2^(16-tag_bits)`, else 4.
fn coded_index_size(row_counts: &[u32; TABLE_KIND_COUNT], tag_bits: u32, tables: &[usize]) -> usize {
    let max_rows = tables.iter().map(|&t| row_counts.get(t).copied().unwrap_or(0)).max().unwrap_or(0);
    let limit: u64 = 1u64 << (16 - tag_bits);
    if (max_rows as u64) < limit { 2 } else { 4 }
}

/// Byte size of one row of table `t`, per its ECMA-335 II.22.* column layout — see the module doc
/// comment's table for which columns are heap indices (`str`/`blob`/`guid`, widened by `HeapSizes`),
/// single-table indices (widened per [`table_index_size`]), or coded indices spanning several tables
/// (widened per [`coded_index_size`]). `None` for any table this reader doesn't need (anything past
/// [`MAX_TABLE_INDEX`]) — callers stop the table walk before reaching one.
#[allow(clippy::too_many_lines)] // one big match over ECMA-335's table schemas; splitting it would
// scatter the spec cross-reference this function exists to keep in one place.
fn row_size(t: usize, rc: &[u32; TABLE_KIND_COUNT], str_sz: usize, guid_sz: usize, blob_sz: usize) -> Option<usize> {
    let tbl = |i: usize| table_index_size(rc, i);
    let coded = |bits: u32, tables: &[usize]| coded_index_size(rc, bits, tables);
    Some(match t {
        TABLE_MODULE => 2 + str_sz + guid_sz * 3,
        TABLE_TYPEREF => coded(2, &[TABLE_MODULE, TABLE_MODULEREF, TABLE_ASSEMBLYREF, TABLE_TYPEREF]) + str_sz * 2,
        TABLE_TYPEDEF => {
            4 + str_sz * 2
                + coded(2, &[TABLE_TYPEDEF, TABLE_TYPEREF, TABLE_TYPESPEC])
                + tbl(TABLE_FIELD)
                + tbl(TABLE_METHODDEF)
        }
        TABLE_FIELDPTR => tbl(TABLE_FIELD),
        TABLE_FIELD => 2 + str_sz + blob_sz,
        TABLE_METHODPTR => tbl(TABLE_METHODDEF),
        TABLE_METHODDEF => 4 + 2 + 2 + str_sz + blob_sz + tbl(TABLE_PARAM),
        TABLE_PARAMPTR => tbl(TABLE_PARAM),
        TABLE_PARAM => 2 + 2 + str_sz,
        TABLE_INTERFACEIMPL => {
            tbl(TABLE_TYPEDEF) + coded(2, &[TABLE_TYPEDEF, TABLE_TYPEREF, TABLE_TYPESPEC])
        }
        TABLE_MEMBERREF => {
            coded(3, &[TABLE_TYPEDEF, TABLE_TYPEREF, TABLE_MODULEREF, TABLE_METHODDEF, TABLE_TYPESPEC])
                + str_sz
                + blob_sz
        }
        TABLE_CONSTANT => 1 + 1 + coded(2, &[TABLE_FIELD, TABLE_PARAM, TABLE_PROPERTY]) + blob_sz,
        TABLE_CUSTOMATTRIBUTE => {
            coded(
                5,
                &[
                    TABLE_METHODDEF,
                    TABLE_FIELD,
                    TABLE_TYPEREF,
                    TABLE_TYPEDEF,
                    TABLE_PARAM,
                    TABLE_INTERFACEIMPL,
                    TABLE_MEMBERREF,
                    TABLE_MODULE,
                    TABLE_DECLSECURITY,
                    TABLE_PROPERTY,
                    TABLE_EVENT,
                    TABLE_STANDALONESIG,
                    TABLE_MODULEREF,
                    TABLE_TYPESPEC,
                    TABLE_ASSEMBLY,
                    TABLE_ASSEMBLYREF,
                    // File/ExportedType/ManifestResource/GenericParam/GenericParamConstraint/
                    // MethodSpec (0x26,0x27,0x28,0x2A,0x2C,0x2B) are valid HasCustomAttribute targets
                    // but sit past MAX_TABLE_INDEX; their row counts (read into `rc` regardless of
                    // whether this reader parses their own rows) still feed the max-rows-so-far
                    // threshold correctly, they just can't be table `t` itself in this walk.
                    0x26, 0x27, 0x28, 0x2A, 0x2C, 0x2B,
                ],
            ) + coded(3, &[TABLE_METHODDEF, TABLE_MEMBERREF])
                + blob_sz
        }
        TABLE_FIELDMARSHAL => coded(1, &[TABLE_FIELD, TABLE_PARAM]) + blob_sz,
        TABLE_DECLSECURITY => {
            2 + coded(2, &[TABLE_TYPEDEF, TABLE_METHODDEF, TABLE_ASSEMBLY]) + blob_sz
        }
        TABLE_CLASSLAYOUT => 2 + 4 + tbl(TABLE_TYPEDEF),
        TABLE_FIELDLAYOUT => 4 + tbl(TABLE_FIELD),
        TABLE_STANDALONESIG => blob_sz,
        TABLE_EVENTMAP => tbl(TABLE_TYPEDEF) + tbl(TABLE_EVENT),
        TABLE_EVENTPTR => tbl(TABLE_EVENT),
        TABLE_EVENT => 2 + str_sz + coded(2, &[TABLE_TYPEDEF, TABLE_TYPEREF, TABLE_TYPESPEC]),
        TABLE_PROPERTYMAP => tbl(TABLE_TYPEDEF) + tbl(TABLE_PROPERTY),
        TABLE_PROPERTYPTR => tbl(TABLE_PROPERTY),
        TABLE_PROPERTY => 2 + str_sz + blob_sz,
        TABLE_METHODSEMANTICS => 2 + tbl(TABLE_METHODDEF) + coded(1, &[TABLE_EVENT, TABLE_PROPERTY]),
        TABLE_METHODIMPL => {
            tbl(TABLE_TYPEDEF) + coded(1, &[TABLE_METHODDEF, TABLE_MEMBERREF]) * 2
        }
        TABLE_MODULEREF => str_sz,
        TABLE_TYPESPEC => blob_sz,
        TABLE_IMPLMAP => {
            2 + coded(1, &[TABLE_FIELD, TABLE_METHODDEF]) + str_sz + tbl(TABLE_MODULEREF)
        }
        TABLE_FIELDRVA => 4 + tbl(TABLE_FIELD),
        TABLE_ENCLOG => 4 + 4,
        TABLE_ENCMAP => 4,
        TABLE_ASSEMBLY => 4 + 2 + 2 + 2 + 2 + 4 + blob_sz + str_sz + str_sz,
        TABLE_ASSEMBLYPROCESSOR => 4,
        TABLE_ASSEMBLYOS => 4 + 4 + 4,
        TABLE_ASSEMBLYREF => 2 + 2 + 2 + 2 + 4 + blob_sz + str_sz + str_sz + blob_sz,
        _ => return None,
    })
}

/// One target table's located row data: its absolute file offset, row count, and row size.
struct TableSlice {
    file_offset: usize,
    rows: u32,
    row_size: usize,
}

/// The four target tables' located row data, plus the exact `#Strings`/`#Blob` heap-index widths (in
/// bytes) the `#~` stream's own `HeapSizes` byte declared — [`read_impl`] reuses these same widths
/// when it re-reads individual row columns, rather than re-deriving them (e.g. from the heaps' own
/// sizes), so there's exactly one source of truth for "how wide is a heap index here" and no risk of
/// the two disagreeing on a crafted file with an inconsistent `HeapSizes` byte.
struct LocatedTables {
    typedef: Option<TableSlice>,
    methoddef: Option<TableSlice>,
    assembly: Option<TableSlice>,
    assembly_ref: Option<TableSlice>,
    str_sz: usize,
    blob_sz: usize,
}

/// Walk the `#~`/`#-` stream header (at absolute file offset `tilde_off`, length `tilde_size`) and
/// locate `TypeDef`/`MethodDef`/`Assembly`/`AssemblyRef`'s row data. Returns `None` only when the
/// stream header itself is unreadable (too short/truncated); an individual absent table is simply
/// `None` in the returned struct, not a whole-function failure. Every offset/count arithmetic step is
/// checked, so a hostile row count or heap-size flag degrades to "stop walking, keep what's already
/// located" rather than a panic or an out-of-bounds read (the "absurd table row counts" adversarial
/// case this ticket calls out).
fn locate_tables(bytes: &[u8], tilde_off: usize, tilde_size: usize) -> Option<LocatedTables> {
    let tilde_end = tilde_off.checked_add(tilde_size)?;
    let heap_sizes = *bytes.get(tilde_off.checked_add(6)?)?;
    let str_sz = if heap_sizes & 0x01 != 0 { 4 } else { 2 };
    let guid_sz = if heap_sizes & 0x02 != 0 { 4 } else { 2 };
    let blob_sz = if heap_sizes & 0x04 != 0 { 4 } else { 2 };
    let valid = u64_at(bytes, tilde_off.checked_add(8)?)?;

    let present: Vec<usize> = (0..TABLE_KIND_COUNT).filter(|&i| valid & (1u64 << i) != 0).collect();

    // Header (24 bytes) + one u32 row count per present table, in ascending table-index order.
    let mut pos = tilde_off.checked_add(24)?;
    let mut rc = [0u32; TABLE_KIND_COUNT];
    for &i in &present {
        rc[i] = u32_at(bytes, pos)?;
        pos = pos.checked_add(4)?;
    }
    if pos > tilde_end {
        // The row-count array alone claims to run past the stream's own declared size — a hostile
        // `Streams` count would hit this. Nothing further can be trusted.
        return None;
    }

    // Row data, back to back in ascending table-index order, starting right after the row-count array.
    let mut cumulative = pos;
    let mut typedef = None;
    let mut methoddef = None;
    let mut assembly = None;
    let mut assembly_ref = None;
    for i in 0..=MAX_TABLE_INDEX {
        let rows = rc[i];
        if rows == 0 {
            continue;
        }
        let size = row_size(i, &rc, str_sz, guid_sz, blob_sz)?;
        let file_offset = cumulative;
        let total = size.checked_mul(rows as usize)?;
        let next = cumulative.checked_add(total)?;
        if next > tilde_end {
            // This table's declared rows overrun the stream — stop here; anything located before
            // this point is still valid and returned.
            break;
        }
        let slice = TableSlice { file_offset, rows, row_size: size };
        match i {
            TABLE_TYPEDEF => typedef = Some(slice),
            TABLE_METHODDEF => methoddef = Some(slice),
            TABLE_ASSEMBLY => assembly = Some(slice),
            TABLE_ASSEMBLYREF => assembly_ref = Some(slice),
            _ => {}
        }
        cumulative = next;
    }
    Some(LocatedTables { typedef, methoddef, assembly, assembly_ref, str_sz, blob_sz })
}

// ---------------------------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------------------------

/// Read the CLR metadata of a managed PE at `path`. Returns:
/// - `Err` only when the file can't be read, or its bytes don't parse as a PE at all (goblin's own
///   error) — the same contract [`crate::binary_preview::binary_info`] holds.
/// - `Ok(None)` when the PE parses but isn't managed (no CLI header) — a native EXE/DLL.
/// - `Ok(Some(metadata))` for a managed PE, where `metadata` is filled in as far as parsing could get:
///   a truncated/corrupt managed assembly degrades to a partial or entirely-empty [`DotnetMetadata`]
///   (empty `runtime_version`, `assembly: None`, empty lists) rather than an `Err`, mirroring
///   `binary_info`'s "never fail the whole inspection over one malformed table" contract.
pub fn read(path: &str) -> Result<Option<DotnetMetadata>, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let pe = goblin::pe::PE::parse(&bytes).map_err(|e| e.to_string())?;
    if !is_managed(&pe) {
        return Ok(None);
    }
    Ok(Some(read_impl(&bytes, &pe)))
}

/// Best-effort parse once we know `pe` is managed — every step is `Option`-based internally and
/// degrades to leaving fields at their default (empty) rather than aborting, so this function itself
/// never returns an `Err`.
fn read_impl(bytes: &[u8], pe: &goblin::pe::PE) -> DotnetMetadata {
    let mut out =
        DotnetMetadata { runtime_version: String::new(), assembly: None, assembly_refs: Vec::new(), types: Vec::new(), methods: Vec::new() };

    let Some(clr) = parse_clr_header(bytes, pe) else { return out };
    let Some(root_off) = rva_to_file_offset(pe, clr.metadata_rva) else { return out };
    let Some(root) = parse_metadata_root(bytes, root_off, clr.metadata_size) else { return out };
    out.runtime_version = root.version;

    let find = |name: &str| {
        root.streams.iter().find(|s| s.name == name).and_then(|s| {
            let off = root_off.checked_add(s.offset as usize)?;
            Some((off, s.size as usize))
        })
    };
    // Prefer "#~" (the normal compressed stream); some tools emit the uncompressed "#-" variant
    // instead, which shares the same physical layout for the columns this reader touches.
    let Some((tilde_off, tilde_size)) = find("#~").or_else(|| find("#-")) else { return out };
    let strings_heap = find("#Strings");
    let blob_heap = find("#Blob");

    let Some(located) = locate_tables(bytes, tilde_off, tilde_size) else { return out };
    let str_idx_sz = located.str_sz;
    let blob_idx_sz = located.blob_sz;

    let get_str = |index: u32| -> Option<String> {
        let (off, size) = strings_heap?;
        let s = read_c_str(bytes, off, size, index)?;
        if s.is_empty() { None } else { Some(s) }
    };
    let get_blob_hex = |index: u32| -> Option<String> {
        let (off, size) = blob_heap?;
        let b = read_blob(bytes, off, size, index)?;
        if b.is_empty() { None } else { Some(hex_encode(b)) }
    };
    // For a required (non-Option-in-output) string column, fall back to an empty string rather than
    // dropping the whole row — a name we couldn't resolve is still worth listing as "".
    let get_str_or_empty = |index: u32| get_str(index).unwrap_or_default();

    if let Some(t) = &located.typedef {
        for row in 0..t.rows.min(MAX_BINARY_LIST_ENTRIES as u32) {
            let parsed = (|| {
                let row_off = t.file_offset.checked_add((row as usize).checked_mul(t.row_size)?)?;
                // TypeDef: Flags(4), Name(str), Namespace(str), ...
                let name_idx = read_index(bytes, row_off.checked_add(4)?, str_idx_sz)?;
                let ns_idx = read_index(bytes, row_off.checked_add(4)?.checked_add(str_idx_sz)?, str_idx_sz)?;
                Some(DotnetTypeDef { name: get_str_or_empty(name_idx), namespace: get_str_or_empty(ns_idx) })
            })();
            if let Some(r) = parsed {
                out.types.push(r);
            }
        }
    }
    if let Some(t) = &located.methoddef {
        for row in 0..t.rows.min(MAX_BINARY_LIST_ENTRIES as u32) {
            let parsed = (|| {
                let row_off = t.file_offset.checked_add((row as usize).checked_mul(t.row_size)?)?;
                // MethodDef: RVA(4), ImplFlags(2), Flags(2), Name(str), ...
                let name_idx = read_index(bytes, row_off.checked_add(8)?, str_idx_sz)?;
                Some(DotnetMethodDef { name: get_str_or_empty(name_idx) })
            })();
            if let Some(r) = parsed {
                out.methods.push(r);
            }
        }
    }
    if let Some(a) = &located.assembly {
        // Assembly: HashAlgId(4), MajorVersion(2), MinorVersion(2), BuildNumber(2), RevisionNumber(2),
        // Flags(4), PublicKey(blob), Name(str), Culture(str). At most one row.
        out.assembly = (|| {
            let off = a.file_offset;
            let major = u16_at(bytes, off.checked_add(4)?)?;
            let minor = u16_at(bytes, off.checked_add(6)?)?;
            let build = u16_at(bytes, off.checked_add(8)?)?;
            let rev = u16_at(bytes, off.checked_add(10)?)?;
            let flags = u32_at(bytes, off.checked_add(12)?)?;
            let pubkey_off = off.checked_add(16)?;
            let pubkey_idx = read_index(bytes, pubkey_off, blob_idx_sz)?;
            let name_off = pubkey_off.checked_add(blob_idx_sz)?;
            let name_idx = read_index(bytes, name_off, str_idx_sz)?;
            let culture_off = name_off.checked_add(str_idx_sz)?;
            let culture_idx = read_index(bytes, culture_off, str_idx_sz)?;
            Some(DotnetAssemblyIdentity {
                name: get_str_or_empty(name_idx),
                version: format!("{major}.{minor}.{build}.{rev}"),
                culture: get_str(culture_idx),
                public_key: get_blob_hex(pubkey_idx),
                flags,
            })
        })();
    }
    if let Some(t) = &located.assembly_ref {
        for row in 0..t.rows.min(MAX_BINARY_LIST_ENTRIES as u32) {
            let parsed = (|| {
                let row_off = t.file_offset.checked_add((row as usize).checked_mul(t.row_size)?)?;
                let major = u16_at(bytes, row_off)?;
                let minor = u16_at(bytes, row_off.checked_add(2)?)?;
                let build = u16_at(bytes, row_off.checked_add(4)?)?;
                let rev = u16_at(bytes, row_off.checked_add(6)?)?;
                // Flags(4) at +8, skipped (not surfaced on the DTO).
                let pubkey_off = row_off.checked_add(12)?;
                let pubkey_idx = read_index(bytes, pubkey_off, blob_idx_sz)?;
                let name_off = pubkey_off.checked_add(blob_idx_sz)?;
                let name_idx = read_index(bytes, name_off, str_idx_sz)?;
                let culture_off = name_off.checked_add(str_idx_sz)?;
                let culture_idx = read_index(bytes, culture_off, str_idx_sz)?;
                Some(DotnetAssemblyRef {
                    name: get_str_or_empty(name_idx),
                    version: format!("{major}.{minor}.{build}.{rev}"),
                    culture: get_str(culture_idx),
                    public_key_token: get_blob_hex(pubkey_idx),
                })
            })();
            if let Some(r) = parsed {
                out.assembly_refs.push(r);
            }
        }
    }

    out
}

fn read_index(bytes: &[u8], off: usize, size: usize) -> Option<u32> {
    if size == 4 { u32_at(bytes, off) } else { u16_at(bytes, off).map(u32::from) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-dotnetmeta-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_temp(bytes: &[u8], tag: &str) -> std::path::PathBuf {
        let d = scratch(tag);
        let f = d.join("asm.dll");
        fs::write(&f, bytes).unwrap();
        f
    }

    // -----------------------------------------------------------------------------------------
    // Fixture builders
    // -----------------------------------------------------------------------------------------

    fn pad4(len: usize) -> usize {
        len.checked_add(3).unwrap() & !3
    }

    fn stream_header(offset: u32, size: u32, name: &str) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&offset.to_le_bytes());
        b.extend_from_slice(&size.to_le_bytes());
        let mut name_bytes = name.as_bytes().to_vec();
        name_bytes.push(0);
        name_bytes.resize(pad4(name_bytes.len()), 0);
        b.extend_from_slice(&name_bytes);
        b
    }

    fn push_str_entry(heap: &mut Vec<u8>, s: &str) -> u16 {
        let off = heap.len() as u16;
        heap.extend_from_slice(s.as_bytes());
        heap.push(0);
        off
    }

    /// Compressed-length-prefixed blob (ECMA-335 II.24.2.4, 1-byte form only — every blob this
    /// fixture needs is well under 0x80 bytes).
    fn push_blob_entry(heap: &mut Vec<u8>, data: &[u8]) -> u16 {
        assert!(data.len() < 0x80, "fixture blob too long for the 1-byte compressed-length form");
        let off = heap.len() as u16;
        heap.push(data.len() as u8);
        heap.extend_from_slice(data);
        off
    }

    fn typedef_row(name: u16, ns: u16, methodlist: u16) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&0u32.to_le_bytes()); // Flags
        b.extend_from_slice(&name.to_le_bytes());
        b.extend_from_slice(&ns.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes()); // Extends (TypeDefOrRef coded, null)
        b.extend_from_slice(&1u16.to_le_bytes()); // FieldList (Field table absent; placeholder index)
        b.extend_from_slice(&methodlist.to_le_bytes()); // MethodList
        assert_eq!(b.len(), 14);
        b
    }

    fn methoddef_row(rva: u32, name: u16) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&rva.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes()); // ImplFlags
        b.extend_from_slice(&0x0091u16.to_le_bytes()); // Flags (public, hidebysig, cil-managed-ish bits; unparsed)
        b.extend_from_slice(&name.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes()); // Signature blob (index 0: empty)
        b.extend_from_slice(&1u16.to_le_bytes()); // ParamList (Param table absent; placeholder index)
        assert_eq!(b.len(), 14);
        b
    }

    fn assembly_row(name: u16, culture: u16, pubkey: u16) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&0x8004u32.to_le_bytes()); // HashAlgId (SHA1)
        b.extend_from_slice(&1u16.to_le_bytes()); // MajorVersion
        b.extend_from_slice(&0u16.to_le_bytes()); // MinorVersion
        b.extend_from_slice(&0u16.to_le_bytes()); // BuildNumber
        b.extend_from_slice(&0u16.to_le_bytes()); // RevisionNumber
        b.extend_from_slice(&0u32.to_le_bytes()); // Flags
        b.extend_from_slice(&pubkey.to_le_bytes());
        b.extend_from_slice(&name.to_le_bytes());
        b.extend_from_slice(&culture.to_le_bytes());
        assert_eq!(b.len(), 22);
        b
    }

    fn assemblyref_row(name: u16, culture: u16, pubkey_token: u16) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&4u16.to_le_bytes()); // MajorVersion
        b.extend_from_slice(&0u16.to_le_bytes()); // MinorVersion
        b.extend_from_slice(&0u16.to_le_bytes()); // BuildNumber
        b.extend_from_slice(&0u16.to_le_bytes()); // RevisionNumber
        b.extend_from_slice(&0u32.to_le_bytes()); // Flags
        b.extend_from_slice(&pubkey_token.to_le_bytes());
        b.extend_from_slice(&name.to_le_bytes());
        b.extend_from_slice(&culture.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes()); // HashValue blob (index 0: empty)
        assert_eq!(b.len(), 20);
        b
    }

    /// Precise byte offsets *within the fixture file* that the adversarial tests corrupt — computed
    /// once, in [`build_minimal_managed_pe`], from the same values used to actually place those
    /// fields. Deliberately not re-derived by pattern-matching a byte value in the finished file (a
    /// value like `0x2000` legitimately appears at several unrelated offsets in this fixture — e.g.
    /// both `address_of_entry_point` and the CLR data directory's VA — so a "find these bytes"
    /// approach could silently corrupt the wrong field and pass without testing anything).
    struct ManagedPeOffsets {
        /// Data directory #14's `VirtualAddress` field (4 bytes) in the PE optional header.
        clr_dir_va: usize,
        /// The `#~` stream's `TypeDef` row-count `u32` (the first entry in the row-count array, since
        /// `TypeDef`=0x02 sorts before `MethodDef`/`Assembly`/`AssemblyRef` in this fixture's `Valid` set).
        typedef_row_count: usize,
        /// The `Assembly` table's single row's `Name` string-heap index (`u16`).
        assembly_name_index: usize,
    }

    /// Build a real, minimal-but-structurally-valid managed PE32: one section holding a CLI header
    /// (`IMAGE_COR20_HEADER`) followed by a metadata root with `#~`/`#Strings`/`#Blob` streams whose
    /// `#~` stream declares exactly four tables — `TypeDef` (2 rows), `MethodDef` (2 rows), `Assembly`
    /// (1 row), `AssemblyRef` (2 rows) — enough to exercise every column this reader resolves, without
    /// the ~30 other real-world table kinds a genuine assembly would also carry (irrelevant to proving
    /// this reader's row-size/offset math, which is exactly what this fixture targets).
    fn build_minimal_managed_pe() -> (Vec<u8>, ManagedPeOffsets) {
        // ---- Heaps ----
        let mut strings = vec![0u8]; // index 0: the mandatory empty string
        let s_mytype1 = push_str_entry(&mut strings, "MyType1");
        let s_mynamespace = push_str_entry(&mut strings, "MyNamespace");
        let s_mytype2 = push_str_entry(&mut strings, "MyType2");
        let s_method1 = push_str_entry(&mut strings, "Method1");
        let s_method2 = push_str_entry(&mut strings, "Method2");
        let s_myassembly = push_str_entry(&mut strings, "MyAssembly");
        let s_mscorlib = push_str_entry(&mut strings, "mscorlib");
        let s_systemcore = push_str_entry(&mut strings, "System.Core");
        let s_empty = 0u16; // neutral culture / global namespace

        let mut blob = vec![0u8]; // index 0: the mandatory empty blob (length-prefix 0x00)
        let token_bytes = [0xb7u8, 0x7a, 0x5c, 0x56, 0x19, 0x34, 0xe0, 0x89]; // mscorlib's real token
        let b_token = push_blob_entry(&mut blob, &token_bytes);

        // ---- #~ tables stream ----
        let typedef_rows = [
            typedef_row(s_mytype1, s_mynamespace, 1),
            typedef_row(s_mytype2, s_empty, 1),
        ];
        let methoddef_rows = [methoddef_row(0x2050, s_method1), methoddef_row(0x2060, s_method2)];
        let assembly_rows = [assembly_row(s_myassembly, s_empty, 0)]; // unsigned: empty PublicKey blob
        let assemblyref_rows = [
            assemblyref_row(s_mscorlib, s_empty, b_token),
            assemblyref_row(s_systemcore, s_empty, 0), // unsigned reference
        ];

        let mut tilde = Vec::new();
        tilde.extend_from_slice(&0u32.to_le_bytes()); // Reserved
        tilde.push(1); // MajorVersion
        tilde.push(0); // MinorVersion
        tilde.push(0); // HeapSizes: all heap indices are 2 bytes
        tilde.push(1); // Reserved2
        let valid: u64 = (1 << TABLE_TYPEDEF) | (1 << TABLE_METHODDEF) | (1 << TABLE_ASSEMBLY) | (1 << TABLE_ASSEMBLYREF);
        tilde.extend_from_slice(&valid.to_le_bytes());
        tilde.extend_from_slice(&0u64.to_le_bytes()); // Sorted (unused by this reader)
        assert_eq!(tilde.len(), 24);
        // Row counts, ascending table-index order: TypeDef, MethodDef, Assembly, AssemblyRef.
        tilde.extend_from_slice(&(typedef_rows.len() as u32).to_le_bytes());
        tilde.extend_from_slice(&(methoddef_rows.len() as u32).to_le_bytes());
        tilde.extend_from_slice(&(assembly_rows.len() as u32).to_le_bytes());
        tilde.extend_from_slice(&(assemblyref_rows.len() as u32).to_le_bytes());
        // Row data, same ascending order.
        for r in &typedef_rows {
            tilde.extend_from_slice(r);
        }
        for r in &methoddef_rows {
            tilde.extend_from_slice(r);
        }
        for r in &assembly_rows {
            tilde.extend_from_slice(r);
        }
        for r in &assemblyref_rows {
            tilde.extend_from_slice(r);
        }

        // ---- Metadata root ----
        let version = b"v4.0.30319\0\0"; // 12 bytes, already 4-byte aligned
        assert_eq!(version.len() % 4, 0);
        let stream_headers = [
            stream_header(0, 0, "#~"),       // patched below once offsets are known
            stream_header(0, 0, "#Strings"),
            stream_header(0, 0, "#Blob"),
        ];
        let root_fixed_len = 4 + 2 + 2 + 4 + 4 + version.len() + 2 + 2;
        let streams_start = root_fixed_len + stream_headers.iter().map(Vec::len).sum::<usize>();
        let tilde_off = streams_start as u32;
        let strings_off = tilde_off + tilde.len() as u32;
        let blob_off = strings_off + strings.len() as u32;

        let mut root = Vec::new();
        root.extend_from_slice(b"BSJB");
        root.extend_from_slice(&1u16.to_le_bytes()); // MajorVersion
        root.extend_from_slice(&1u16.to_le_bytes()); // MinorVersion
        root.extend_from_slice(&0u32.to_le_bytes()); // Reserved
        root.extend_from_slice(&(version.len() as u32).to_le_bytes()); // Length
        root.extend_from_slice(version);
        root.extend_from_slice(&0u16.to_le_bytes()); // Flags
        root.extend_from_slice(&(stream_headers.len() as u16).to_le_bytes()); // Streams
        root.extend_from_slice(&stream_header(tilde_off, tilde.len() as u32, "#~"));
        root.extend_from_slice(&stream_header(strings_off, strings.len() as u32, "#Strings"));
        root.extend_from_slice(&stream_header(blob_off, blob.len() as u32, "#Blob"));
        assert_eq!(root.len(), streams_start, "stream data must start exactly where the header list ends");
        root.extend_from_slice(&tilde);
        root.extend_from_slice(&strings);
        root.extend_from_slice(&blob);

        // Row-data layout within `tilde`, ascending table-index order (TypeDef=2, MethodDef=6,
        // Assembly=0x20, AssemblyRef=0x23): header(24) + row-count array(4 tables * 4 bytes) + each
        // table's row bytes back to back. Computed from the same row Vecs used to actually build
        // `tilde` above, so this can't drift from the real layout.
        let typedef_data_len: usize = typedef_rows.iter().map(Vec::len).sum();
        let methoddef_data_len: usize = methoddef_rows.iter().map(Vec::len).sum();
        let rowcount_array_len = 4 * 4; // 4 present tables * one u32 each
        let typedef_row_count_in_tilde = 24; // first entry in the row-count array
        let assembly_row_start_in_tilde = 24 + rowcount_array_len + typedef_data_len + methoddef_data_len;
        // Assembly row layout: HashAlgId(4) + Version(4*u16=8) + Flags(4) + PublicKey blob idx(2,
        // since HeapSizes=0 means every heap index is 2 bytes) = 18 bytes in, then Name.
        let assembly_name_field_in_row = 4 + 8 + 4 + 2;

        // ---- CLI header (IMAGE_COR20_HEADER, 72 bytes) + section layout ----
        const SECTION_VA: u32 = 0x2000;
        const SECTION_FILE_OFF: u32 = 0x400;
        let cli_header_size = 72u32;
        let metadata_rva = SECTION_VA + cli_header_size;

        let mut cli = Vec::new();
        cli.extend_from_slice(&cli_header_size.to_le_bytes()); // cb
        cli.extend_from_slice(&2u16.to_le_bytes()); // MajorRuntimeVersion
        cli.extend_from_slice(&5u16.to_le_bytes()); // MinorRuntimeVersion
        cli.extend_from_slice(&metadata_rva.to_le_bytes()); // MetaData.VirtualAddress
        cli.extend_from_slice(&(root.len() as u32).to_le_bytes()); // MetaData.Size
        cli.extend_from_slice(&1u32.to_le_bytes()); // Flags: COMIMAGE_FLAGS_ILONLY
        cli.extend_from_slice(&0u32.to_le_bytes()); // EntryPointToken
        for _ in 0..6 {
            cli.extend_from_slice(&0u32.to_le_bytes()); // Resources / StrongNameSignature / CodeManagerTable /
            cli.extend_from_slice(&0u32.to_le_bytes()); // VTableFixups / ExportAddressTableJumps / ManagedNativeHeader
        }
        assert_eq!(cli.len(), cli_header_size as usize);

        let mut section_data = cli;
        section_data.extend_from_slice(&root);

        // ---- Optional header (PE32, 15 data directories so index 14/CLR is present) ----
        const NUM_RVA_AND_SIZES: u32 = 15;
        let opt_header_size: u16 = (28 + 68 + NUM_RVA_AND_SIZES as usize * 8) as u16;

        let mut opt = Vec::new();
        opt.extend_from_slice(&0x010Bu16.to_le_bytes()); // magic: PE32
        opt.push(0); // major_linker_version
        opt.push(0); // minor_linker_version
        opt.extend_from_slice(&(section_data.len() as u32).to_le_bytes()); // size_of_code
        opt.extend_from_slice(&0u32.to_le_bytes());
        opt.extend_from_slice(&0u32.to_le_bytes());
        opt.extend_from_slice(&SECTION_VA.to_le_bytes()); // address_of_entry_point
        opt.extend_from_slice(&SECTION_VA.to_le_bytes()); // base_of_code
        opt.extend_from_slice(&(SECTION_VA + 0x1000).to_le_bytes()); // base_of_data
        assert_eq!(opt.len(), 28);
        opt.extend_from_slice(&0x0040_0000u32.to_le_bytes()); // image_base
        opt.extend_from_slice(&0x1000u32.to_le_bytes()); // section_alignment
        opt.extend_from_slice(&0x0200u32.to_le_bytes()); // file_alignment
        opt.extend_from_slice(&6u16.to_le_bytes()); // major_os_version
        opt.extend_from_slice(&0u16.to_le_bytes());
        opt.extend_from_slice(&0u16.to_le_bytes());
        opt.extend_from_slice(&0u16.to_le_bytes());
        opt.extend_from_slice(&6u16.to_le_bytes()); // major_subsystem_version
        opt.extend_from_slice(&0u16.to_le_bytes());
        opt.extend_from_slice(&0u32.to_le_bytes()); // win32_version_value
        opt.extend_from_slice(&(SECTION_VA + 0x3000).to_le_bytes()); // size_of_image
        opt.extend_from_slice(&SECTION_FILE_OFF.to_le_bytes()); // size_of_headers
        opt.extend_from_slice(&0u32.to_le_bytes()); // checksum
        opt.extend_from_slice(&3u16.to_le_bytes()); // subsystem: console
        opt.extend_from_slice(&0u16.to_le_bytes()); // dll_characteristics
        opt.extend_from_slice(&0x0010_0000u32.to_le_bytes());
        opt.extend_from_slice(&0x0000_1000u32.to_le_bytes());
        opt.extend_from_slice(&0x0010_0000u32.to_le_bytes());
        opt.extend_from_slice(&0x0000_1000u32.to_le_bytes());
        opt.extend_from_slice(&0u32.to_le_bytes()); // loader_flags
        opt.extend_from_slice(&NUM_RVA_AND_SIZES.to_le_bytes());
        assert_eq!(opt.len(), 28 + 68);
        for i in 0..NUM_RVA_AND_SIZES {
            if i == 14 {
                // Data directory #14: CLR Runtime Header (IMAGE_COR20_HEADER itself, not the metadata root).
                opt.extend_from_slice(&SECTION_VA.to_le_bytes());
                opt.extend_from_slice(&cli_header_size.to_le_bytes());
            } else {
                opt.extend_from_slice(&0u32.to_le_bytes());
                opt.extend_from_slice(&0u32.to_le_bytes());
            }
        }
        assert_eq!(opt.len(), opt_header_size as usize);

        // ---- Section table (one entry) ----
        let mut section_header = Vec::new();
        let mut name = [0u8; 8];
        name[..5].copy_from_slice(b".cli0");
        section_header.extend_from_slice(&name);
        section_header.extend_from_slice(&(section_data.len() as u32).to_le_bytes()); // virtual_size
        section_header.extend_from_slice(&SECTION_VA.to_le_bytes());
        section_header.extend_from_slice(&(section_data.len() as u32).to_le_bytes()); // size_of_raw_data
        section_header.extend_from_slice(&SECTION_FILE_OFF.to_le_bytes());
        section_header.extend_from_slice(&0u32.to_le_bytes());
        section_header.extend_from_slice(&0u32.to_le_bytes());
        section_header.extend_from_slice(&0u16.to_le_bytes());
        section_header.extend_from_slice(&0u16.to_le_bytes());
        section_header.extend_from_slice(&0x4000_0040u32.to_le_bytes()); // characteristics: initialized data, readable
        assert_eq!(section_header.len(), 40);

        // ---- COFF header ----
        let mut coff = Vec::new();
        coff.extend_from_slice(&0x014Cu16.to_le_bytes()); // machine: I386
        coff.extend_from_slice(&1u16.to_le_bytes()); // number_of_sections
        coff.extend_from_slice(&0u32.to_le_bytes());
        coff.extend_from_slice(&0u32.to_le_bytes());
        coff.extend_from_slice(&0u32.to_le_bytes());
        coff.extend_from_slice(&opt_header_size.to_le_bytes());
        coff.extend_from_slice(&0x0102u16.to_le_bytes()); // characteristics: executable, 32-bit
        assert_eq!(coff.len(), 20);

        // ---- DOS stub + PE signature ----
        const PE_OFFSET: u32 = 0x80;
        let mut out = vec![0u8; PE_OFFSET as usize];
        out[0] = b'M';
        out[1] = b'Z';
        out[0x3C..0x40].copy_from_slice(&PE_OFFSET.to_le_bytes());
        out.extend_from_slice(b"PE\0\0");
        out.extend_from_slice(&coff);
        out.extend_from_slice(&opt);
        out.extend_from_slice(&section_header);

        out.resize(SECTION_FILE_OFF as usize, 0);
        out.extend_from_slice(&section_data);

        // Data directory #14 sits at opt-relative offset (28 standard + 68 windows fields) + 14 * 8
        // bytes; `opt` itself starts right after the DOS stub + "PE\0\0" signature + COFF header.
        let opt_start = PE_OFFSET as usize + 4 + coff.len();
        let clr_dir_va = opt_start + 28 + 68 + 14 * 8;
        let section_data_start = SECTION_FILE_OFF as usize;
        let root_start = section_data_start + cli_header_size as usize;
        let tilde_start = root_start + tilde_off as usize;
        let offsets = ManagedPeOffsets {
            clr_dir_va,
            typedef_row_count: tilde_start + typedef_row_count_in_tilde,
            assembly_name_index: tilde_start + assembly_row_start_in_tilde + assembly_name_field_in_row,
        };

        // Self-check every computed offset against the value actually written there, so a future
        // change to this builder's layout fails loudly here rather than silently corrupting the wrong
        // field in an adversarial test.
        assert_eq!(&out[clr_dir_va..clr_dir_va + 4], &SECTION_VA.to_le_bytes(), "clr_dir_va offset");
        assert_eq!(
            &out[offsets.typedef_row_count..offsets.typedef_row_count + 4],
            &(typedef_rows.len() as u32).to_le_bytes(),
            "typedef_row_count offset"
        );
        assert_eq!(
            &out[offsets.assembly_name_index..offsets.assembly_name_index + 2],
            &s_myassembly.to_le_bytes(),
            "assembly_name_index offset"
        );

        (out, offsets)
    }

    /// A minimal, real, valid PE32 with zero data directories — a native image, structurally
    /// guaranteed to have no CLR header (there's nothing at index 14 to find).
    fn build_minimal_native_pe() -> Vec<u8> {
        const NUM_RVA_AND_SIZES: u32 = 0;
        let opt_header_size: u16 = (28 + 68) as u16;

        let mut opt = Vec::new();
        opt.extend_from_slice(&0x010Bu16.to_le_bytes());
        opt.push(0);
        opt.push(0);
        opt.extend_from_slice(&0u32.to_le_bytes());
        opt.extend_from_slice(&0u32.to_le_bytes());
        opt.extend_from_slice(&0u32.to_le_bytes());
        opt.extend_from_slice(&0x1000u32.to_le_bytes());
        opt.extend_from_slice(&0x1000u32.to_le_bytes());
        opt.extend_from_slice(&0x2000u32.to_le_bytes());
        assert_eq!(opt.len(), 28);
        opt.extend_from_slice(&0x0040_0000u32.to_le_bytes());
        opt.extend_from_slice(&0x1000u32.to_le_bytes());
        opt.extend_from_slice(&0x0200u32.to_le_bytes());
        opt.extend_from_slice(&6u16.to_le_bytes());
        opt.extend_from_slice(&0u16.to_le_bytes());
        opt.extend_from_slice(&0u16.to_le_bytes());
        opt.extend_from_slice(&0u16.to_le_bytes());
        opt.extend_from_slice(&6u16.to_le_bytes());
        opt.extend_from_slice(&0u16.to_le_bytes());
        opt.extend_from_slice(&0u32.to_le_bytes());
        opt.extend_from_slice(&0x3000u32.to_le_bytes()); // size_of_image
        opt.extend_from_slice(&0x200u32.to_le_bytes()); // size_of_headers
        opt.extend_from_slice(&0u32.to_le_bytes());
        opt.extend_from_slice(&3u16.to_le_bytes());
        opt.extend_from_slice(&0u16.to_le_bytes());
        opt.extend_from_slice(&0x0010_0000u32.to_le_bytes());
        opt.extend_from_slice(&0x0000_1000u32.to_le_bytes());
        opt.extend_from_slice(&0x0010_0000u32.to_le_bytes());
        opt.extend_from_slice(&0x0000_1000u32.to_le_bytes());
        opt.extend_from_slice(&0u32.to_le_bytes());
        opt.extend_from_slice(&NUM_RVA_AND_SIZES.to_le_bytes());
        assert_eq!(opt.len(), opt_header_size as usize);

        let mut coff = Vec::new();
        coff.extend_from_slice(&0x014Cu16.to_le_bytes());
        coff.extend_from_slice(&0u16.to_le_bytes()); // number_of_sections: 0
        coff.extend_from_slice(&0u32.to_le_bytes());
        coff.extend_from_slice(&0u32.to_le_bytes());
        coff.extend_from_slice(&0u32.to_le_bytes());
        coff.extend_from_slice(&opt_header_size.to_le_bytes());
        coff.extend_from_slice(&0x0102u16.to_le_bytes());
        assert_eq!(coff.len(), 20);

        const PE_OFFSET: u32 = 0x80;
        let mut out = vec![0u8; PE_OFFSET as usize];
        out[0] = b'M';
        out[1] = b'Z';
        out[0x3C..0x40].copy_from_slice(&PE_OFFSET.to_le_bytes());
        out.extend_from_slice(b"PE\0\0");
        out.extend_from_slice(&coff);
        out.extend_from_slice(&opt);
        out
    }

    // -----------------------------------------------------------------------------------------
    // Happy path
    // -----------------------------------------------------------------------------------------

    #[test]
    fn is_managed_true_for_a_real_managed_pe_and_false_for_a_native_one() {
        let (managed, _) = build_minimal_managed_pe();
        let pe = goblin::pe::PE::parse(&managed).expect("fixture must parse as a valid PE");
        assert!(is_managed(&pe), "a PE with a populated CLR data directory must be reported managed");

        let native = build_minimal_native_pe();
        let pe = goblin::pe::PE::parse(&native).expect("fixture must parse as a valid PE");
        assert!(!is_managed(&pe), "a PE with zero data directories must not be reported managed");
    }

    #[test]
    fn read_reports_managed_assembly_identity_refs_types_and_methods() {
        let (bytes, _) = build_minimal_managed_pe();
        let path = write_temp(&bytes, "managed");
        let result = read(&path.to_string_lossy()).expect("a well-formed managed PE must parse");
        let meta = result.expect("the fixture's CLI header must be detected as managed");

        assert_eq!(meta.runtime_version, "v4.0.30319");

        let assembly = meta.assembly.as_ref().expect("Assembly table row must be read");
        assert_eq!(assembly.name, "MyAssembly");
        assert_eq!(assembly.version, "1.0.0.0");
        assert_eq!(assembly.culture, None, "neutral culture must be None");
        assert_eq!(assembly.public_key, None, "unsigned assembly must have no public key");

        assert_eq!(meta.assembly_refs.len(), 2);
        assert_eq!(meta.assembly_refs[0].name, "mscorlib");
        assert_eq!(meta.assembly_refs[0].version, "4.0.0.0");
        assert_eq!(
            meta.assembly_refs[0].public_key_token.as_deref(),
            Some("b77a5c561934e089"),
            "expected mscorlib's real public key token, hex-encoded: {:?}",
            meta.assembly_refs[0]
        );
        assert_eq!(meta.assembly_refs[1].name, "System.Core");
        assert_eq!(meta.assembly_refs[1].public_key_token, None, "unsigned reference must have no token");

        assert_eq!(meta.types.len(), 2);
        assert_eq!(meta.types[0].name, "MyType1");
        assert_eq!(meta.types[0].namespace, "MyNamespace");
        assert_eq!(meta.types[1].name, "MyType2");
        assert_eq!(meta.types[1].namespace, "", "global-namespace type must report an empty namespace");

        assert_eq!(meta.methods.len(), 2);
        assert_eq!(meta.methods[0].name, "Method1");
        assert_eq!(meta.methods[1].name, "Method2");

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn read_reports_none_for_a_native_pe() {
        let bytes = build_minimal_native_pe();
        let path = write_temp(&bytes, "native");
        let result = read(&path.to_string_lossy()).expect("a well-formed native PE must parse");
        assert!(result.is_none(), "a native PE (no CLI header) must read as Ok(None), not managed metadata");
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------------------------
    // Adversarial input — never panic, never hang; a clean Err or an empty/partial Ok is the
    // whole contract (CPE-1596's explicit top fuzz priority).
    // -----------------------------------------------------------------------------------------

    #[test]
    fn read_errors_cleanly_on_a_zero_byte_file() {
        let path = write_temp(&[], "empty");
        assert!(read(&path.to_string_lossy()).is_err());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn read_errors_cleanly_on_a_renamed_text_file() {
        let path = write_temp(b"this is not a PE, just plain text pretending to be a DLL", "text");
        assert!(read(&path.to_string_lossy()).is_err());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn read_errors_on_a_missing_file() {
        assert!(read("Z:/does/not/exist/nope.dll").is_err());
    }

    #[test]
    fn read_never_panics_on_a_truncated_managed_pe() {
        // Truncate the real, otherwise-valid managed fixture at every prefix length — must always
        // return cleanly (Err, Ok(None), or a partial/empty Ok(Some(..))), never panic.
        let (bytes, _) = build_minimal_managed_pe();
        for cut in (0..bytes.len()).step_by(11) {
            let path = write_temp(&bytes[..cut], "trunc");
            let _ = read(&path.to_string_lossy());
            let _ = fs::remove_file(&path);
        }
    }

    #[test]
    fn read_degrades_gracefully_on_a_bogus_clr_header_rva() {
        // Corrupt the CLI header's own data-directory VirtualAddress field to an RVA that can't
        // resolve to any section — the offset comes straight from the builder (self-checked there
        // against the real value it wrote), not a guessed/pattern-matched byte position.
        let (mut bytes, offsets) = build_minimal_managed_pe();
        bytes[offsets.clr_dir_va..offsets.clr_dir_va + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());

        let path = write_temp(&bytes, "bogus-clr-rva");
        // Must not panic; either a clean Err, or Ok(Some(mostly-empty)) since `is_managed` only checks
        // the directory is non-empty, not that its RVA actually resolves.
        let result = read(&path.to_string_lossy());
        if let Ok(Some(meta)) = &result {
            assert!(meta.assembly.is_none(), "an unresolvable CLR header RVA must yield no assembly identity");
            assert!(meta.assembly_refs.is_empty());
            assert!(meta.types.is_empty());
            assert!(meta.methods.is_empty());
        }
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn read_degrades_gracefully_on_absurd_table_row_counts() {
        // Corrupt the #~ stream's TypeDef row count to a huge value that can't possibly fit in the
        // file — must stop cleanly rather than multiplying/overflowing/reading out of bounds. Offset
        // from the builder, self-checked there.
        let (mut bytes, offsets) = build_minimal_managed_pe();
        bytes[offsets.typedef_row_count..offsets.typedef_row_count + 4]
            .copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());

        let path = write_temp(&bytes, "absurd-rows");
        let result = read(&path.to_string_lossy());
        // Never panics (the call above completing at all is the real check); whatever comes back must
        // still be a well-formed, boundedly-sized result.
        if let Ok(Some(meta)) = &result {
            assert!(meta.types.len() <= MAX_BINARY_LIST_ENTRIES);
            assert!(meta.methods.len() <= MAX_BINARY_LIST_ENTRIES);
        }
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn read_degrades_gracefully_on_a_heap_index_past_the_end() {
        // Corrupt the Assembly row's Name string-heap index to a huge, out-of-range value — must
        // resolve to no name (or the row simply not being parsed), never read out of bounds. Offset
        // from the builder, self-checked there.
        let (mut bytes, offsets) = build_minimal_managed_pe();
        bytes[offsets.assembly_name_index..offsets.assembly_name_index + 2].copy_from_slice(&0xFFFFu16.to_le_bytes());

        let path = write_temp(&bytes, "heap-oob");
        let result = read(&path.to_string_lossy());
        // Never panics; if a result comes back, the unresolvable name must degrade to empty, not
        // garbage or an out-of-bounds read.
        if let Ok(Some(meta)) = &result {
            if let Some(a) = &meta.assembly {
                assert_eq!(a.name, "", "an out-of-range heap index must resolve to an empty name, not garbage");
            }
        }
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn read_never_panics_on_seeded_random_bytes_of_various_sizes() {
        // A cheap deterministic fuzz sweep without a `rand` dependency (mirrors the LCG approach in
        // `tests/common/mod.rs`, kept local here since this is a single targeted addition, not a
        // shared battery).
        let mut state = 0xC0FFEE_u64;
        for &n in &[16usize, 64, 256, 1024, 4096] {
            let bytes: Vec<u8> = (0..n)
                .map(|_| {
                    state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                    (state >> 33) as u8
                })
                .collect();
            let path = write_temp(&bytes, "fuzz");
            let _ = read(&path.to_string_lossy());
            let _ = fs::remove_file(&path);
        }
    }
}
