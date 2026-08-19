//! RAR entry listing (CPE-1347, epic CPE-111): list names/sizes/is_dir for a `.rar` archive without
//! decompressing anything — read-only header walk, ZERO new dependencies. There is no free/pure-Rust
//! decoder for RAR's proprietary compression, and the native `unrar` library is non-free, so full
//! extraction is explicitly out of scope here; this module only needs to parse the plain (uncompressed)
//! header metadata that both RAR4 and RAR5 always store to advertise their directory, which is enough to
//! *list* an archive's contents (mirrors [`crate::archive`]'s zip/tar/7z/iso listers, reusing its
//! [`crate::archive::ArchiveEntry`] shape). Hand-rolled from the published RAR4/RAR5 technical notes
//! rather than pulling in a crate — see the ticket for the format references consulted.
//!
//! Both formats are laid out as a signature marker followed by a sequence of self-describing blocks:
//! each block states its own total size, so the walk never needs to understand a block to skip past it —
//! only file/service blocks (the ones that can hold a name) are actually decoded. That means unknown or
//! future block types are silently skipped rather than misread, EXCEPT where a block's *size* itself is
//! ambiguous without extra parsing (RAR4's `PROTECT_HEAD` recovery-record block — see [`parse_rar4`]),
//! which is rejected as an explicit `Err` rather than guessed at.
//!
//! Every offset computed while walking is bounds-checked before use; nothing here indexes past a slice's
//! length or panics on malformed input — corrupt/truncated/adversarial bytes always come back as `Err`.

use crate::archive::ArchiveEntry;

/// RAR4 signature: `Rar!\x1a\x07\x00`.
const RAR4_MARKER: [u8; 7] = [0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x00];
/// RAR5 signature: `Rar!\x1a\x07\x01\x00`.
const RAR5_MARKER: [u8; 8] = [0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x01, 0x00];

/// Hard cap on the number of entries collected, so a malformed or adversarial archive that describes an
/// unbounded number of zero-size blocks can't grow the result without bound (it simply stops early rather
/// than erroring — the caller still gets a partial-but-safe listing).
const MAX_ENTRIES: usize = 100_000;

/// List the entries of a `.rar` archive (RAR4 or RAR5, detected by signature) without decompressing
/// anything. Read-only header walk: names, unpacked sizes, and directory flags only. Corrupt, truncated,
/// or non-RAR input is a clear `Err` — this never panics, loops forever, or reads out of bounds.
pub fn rar_entries(path: &str) -> Result<Vec<ArchiveEntry>, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    if data.starts_with(&RAR5_MARKER) {
        parse_rar5(&data[RAR5_MARKER.len()..])
    } else if data.starts_with(&RAR4_MARKER) {
        parse_rar4(&data[RAR4_MARKER.len()..])
    } else {
        Err("not a RAR archive (unrecognised signature)".to_string())
    }
}

// ---------------------------------------------------------------------------
// RAR4: fixed-width block headers
// ---------------------------------------------------------------------------

/// RAR4 file-header block type (`FILE_HEAD`). Carries name + size fields.
const RAR4_FILE_HEAD: u8 = 0x74;
/// RAR4 old-style sub-block type (`SUB_HEAD`) — same file-header layout as `FILE_HEAD`.
const RAR4_SUB_HEAD: u8 = 0x77;
/// RAR4 recovery-record block (`PROTECT_HEAD`). Its data payload size lives in a field this walk doesn't
/// parse (it's a different layout to the file header), so skipping past it correctly would need
/// dedicated handling; treated as the one "exotic" case the ticket allows erroring out on rather than
/// guessing at (real-world archives without a recovery record never hit this).
const RAR4_PROTECT_HEAD: u8 = 0x78;
/// RAR4 new-style sub-block type (`NEWSUB_HEAD`) — reuses the file-header layout, may or may not carry a
/// name (a 0-length name is skipped, not reported as an entry).
const RAR4_NEWSUB_HEAD: u8 = 0x7A;
/// RAR4 end-of-archive marker block (`ENDARC_HEAD`) — stop walking once seen.
const RAR4_ENDARC_HEAD: u8 = 0x7B;

/// `LHD_LARGE`: the file header carries an extra 8 bytes (high 32 bits of pack/unp size) for files >4GB.
const RAR4_LHD_LARGE: u16 = 0x0100;
/// `LHD_UNICODE`: the name field holds a NUL-terminated ANSI name followed by a separately-encoded
/// Unicode name. Decoding that secondary encoding is out of scope here (see [`decode_rar4_name`]); this
/// walk falls back to the ANSI portion, which is always present.
const RAR4_LHD_UNICODE: u16 = 0x0200;
/// Directory entries don't need a dictionary-size, so RAR4 repurposes those flag bits (`LHD_WINDOWMASK` =
/// `0x00E0`) as a directory marker when all three bits are set (`LHD_DIRECTORY` = `0x00E0`).
const RAR4_LHD_WINDOWMASK: u16 = 0x00E0;
const RAR4_LHD_DIRECTORY: u16 = 0x00E0;

/// Decode a RAR4 file name field. Under `LHD_UNICODE`, the field is a NUL-terminated ANSI name followed
/// by a RAR-specific encoded Unicode name; only the ANSI portion (always present, always plain bytes) is
/// decoded here — good enough for a listing, and never wrong, just possibly non-ideal for exotic
/// non-ASCII names under the old Unicode encoding. Without the flag, the whole field is the name.
fn decode_rar4_name(name_bytes: &[u8], unicode: bool) -> String {
    if unicode {
        let end = name_bytes.iter().position(|&b| b == 0).unwrap_or(name_bytes.len());
        String::from_utf8_lossy(&name_bytes[..end]).to_string()
    } else {
        String::from_utf8_lossy(name_bytes).to_string()
    }
}

/// Walk RAR4 blocks (the bytes after the 7-byte marker). Each block starts with a 7-byte common header
/// (`crc16`, `type: u8`, `flags: u16`, `head_size: u16`); `head_size` is the *whole* header (common
/// header plus type-specific fields plus name), authoritative for where the next block's common header
/// begins. A file/sub/newsub block additionally has a `pack_size` (± its high 32 bits under
/// `LHD_LARGE`) describing a data payload immediately following the header, which must also be skipped.
fn parse_rar4(data: &[u8]) -> Result<Vec<ArchiveEntry>, String> {
    let mut out = Vec::new();
    let len = data.len();
    let mut pos: usize = 0;

    while pos < len {
        if out.len() >= MAX_ENTRIES {
            break;
        }
        if pos + 7 > len {
            return Err("truncated RAR4 block: not enough bytes for the common header".to_string());
        }
        let block_start = pos;
        let head_type = data[pos + 2];
        let flags = u16::from_le_bytes([data[pos + 3], data[pos + 4]]);
        let head_size = u16::from_le_bytes([data[pos + 5], data[pos + 6]]) as usize;
        if head_size < 7 {
            return Err("invalid RAR4 block: head_size smaller than the common header".to_string());
        }
        let header_end = block_start
            .checked_add(head_size)
            .ok_or_else(|| "RAR4 head_size overflowed".to_string())?;
        if header_end > len {
            return Err("RAR4 block header extends past end of file".to_string());
        }

        if head_type == RAR4_PROTECT_HEAD {
            return Err("RAR4 recovery record (PROTECT_HEAD) is not supported".to_string());
        }

        let mut pack_size: u64 = 0;

        if head_type == RAR4_FILE_HEAD || head_type == RAR4_SUB_HEAD || head_type == RAR4_NEWSUB_HEAD {
            // Fixed fields after the 7-byte common header: pack_size(4) unp_size(4) host_os(1)
            // file_crc(4) ftime(4) unp_ver(1) method(1) name_size(2) attr(4) = 25 bytes.
            let fixed_start = block_start + 7;
            let fixed_end = fixed_start
                .checked_add(25)
                .ok_or_else(|| "RAR4 file header field overflow".to_string())?;
            if fixed_end > header_end {
                return Err("truncated RAR4 file header: missing fixed fields".to_string());
            }
            let low_pack = u32::from_le_bytes(data[fixed_start..fixed_start + 4].try_into().unwrap()) as u64;
            let low_unp = u32::from_le_bytes(data[fixed_start + 4..fixed_start + 8].try_into().unwrap()) as u64;
            let name_size = u16::from_le_bytes(data[fixed_start + 19..fixed_start + 21].try_into().unwrap()) as usize;

            let mut field_pos = fixed_end;
            let (high_pack, high_unp) = if flags & RAR4_LHD_LARGE != 0 {
                let large_end = field_pos
                    .checked_add(8)
                    .ok_or_else(|| "RAR4 large-size field overflow".to_string())?;
                if large_end > header_end {
                    return Err("truncated RAR4 file header: missing LHD_LARGE fields".to_string());
                }
                let hp = u32::from_le_bytes(data[field_pos..field_pos + 4].try_into().unwrap()) as u64;
                let hu = u32::from_le_bytes(data[field_pos + 4..field_pos + 8].try_into().unwrap()) as u64;
                field_pos = large_end;
                (hp, hu)
            } else {
                (0u64, 0u64)
            };

            let name_end = field_pos
                .checked_add(name_size)
                .ok_or_else(|| "RAR4 name_size overflow".to_string())?;
            if name_end > header_end {
                return Err("RAR4 file name extends past its header".to_string());
            }
            let name_bytes = &data[field_pos..name_end];
            let unicode = flags & RAR4_LHD_UNICODE != 0;
            let name = decode_rar4_name(name_bytes, unicode);

            let is_dir = flags & RAR4_LHD_WINDOWMASK == RAR4_LHD_DIRECTORY;
            let total_pack = low_pack | (high_pack << 32);
            let total_unp = low_unp | (high_unp << 32);
            pack_size = total_pack;

            if !name.is_empty() {
                out.push(ArchiveEntry { name, size: total_unp, is_dir });
            }
        }

        let next_pos_u64 = (header_end as u64)
            .checked_add(pack_size)
            .ok_or_else(|| "RAR4 data payload size overflowed".to_string())?;
        if next_pos_u64 > len as u64 {
            return Err("RAR4 data payload extends past end of file".to_string());
        }
        pos = next_pos_u64 as usize;

        if head_type == RAR4_ENDARC_HEAD {
            break;
        }
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// RAR5: vint-encoded headers
// ---------------------------------------------------------------------------

/// RAR5 file header type.
const RAR5_HEAD_FILE: u64 = 2;
/// RAR5 service header type — same layout as the file header (used for the recovery record, quick-open
/// index, etc.); treated identically so its name (if any) is reported like a file's.
const RAR5_HEAD_SERVICE: u64 = 3;
/// RAR5 end-of-archive header type.
const RAR5_HEAD_ENDARC: u64 = 5;

/// Header flag: an "extra area" (extension fields) follows the type-specific fields, sized by a vint that
/// appears right after the header flags.
const RAR5_HFL_EXTRA: u64 = 0x0001;
/// Header flag: a "data area" (the packed payload) immediately follows the header, sized by a vint that
/// appears right after the extra-area size (or right after header flags, if there is no extra area).
const RAR5_HFL_DATA: u64 = 0x0002;
/// File flag: this file/service entry is a directory.
const RAR5_FHFL_DIRECTORY: u64 = 0x0001;
/// File flag: an mtime field (4 raw bytes) follows the unpacked size/attributes.
const RAR5_FHFL_UTIME: u64 = 0x0002;
/// File flag: a data-CRC32 field (4 raw bytes) follows (after mtime, if also present).
const RAR5_FHFL_CRC32: u64 = 0x0004;

/// Read a RAR5 vint (variable-length integer): little-endian base-128, 7 data bits per byte, high bit
/// (`0x80`) set means "more bytes follow". Bounds-checked and self-limiting — caps at 10 bytes (70 bits,
/// enough to fill a `u64` since the final byte only contributes bits that fit) so a byte stream that never
/// clears its continuation bit can't loop forever; returns `None` on that as well as on running out of
/// bytes before a terminating byte is seen.
pub fn read_vint(data: &[u8], pos: &mut usize) -> Option<u64> {
    let mut result: u64 = 0;
    let mut shift: u32 = 0;
    loop {
        if shift >= 64 {
            return None; // too many continuation bytes for a u64 — malformed
        }
        let byte = *data.get(*pos)?;
        *pos += 1;
        result |= ((byte & 0x7F) as u64) << shift;
        if byte & 0x80 == 0 {
            return Some(result);
        }
        shift += 7;
    }
}

/// Advance `pos` by `n` bytes if that stays within `data`, returning the skipped-over slice; otherwise
/// `None`. Used for the two fixed-width (non-vint) fields RAR5 file headers can carry: mtime and CRC32.
fn take_bytes<'a>(data: &'a [u8], pos: &mut usize, n: usize) -> Option<&'a [u8]> {
    let end = pos.checked_add(n)?;
    if end > data.len() {
        return None;
    }
    let slice = &data[*pos..end];
    *pos = end;
    Some(slice)
}

/// Walk RAR5 headers (the bytes after the 8-byte marker). Every header shares the same envelope:
/// `HeaderCRC32` (4 raw bytes, not validated here — this is a listing, not an integrity check),
/// `HeaderSize` (vint, the length of everything from `HeaderType` through the end of the extra area), then
/// the header body itself: `HeaderType`, `HeaderFlags`, then an optional `ExtraAreaSize` vint and/or
/// optional `DataSize` vint (per `HeaderFlags`), then type-specific fields. Because `HeaderSize` and
/// `DataSize` are always in the same place regardless of `HeaderType`, the walk can skip any header type —
/// known or not — without understanding it; only file/service headers are decoded further, for the name.
fn parse_rar5(data: &[u8]) -> Result<Vec<ArchiveEntry>, String> {
    let mut out = Vec::new();
    let len = data.len();
    let mut pos: usize = 0;

    while pos < len {
        if out.len() >= MAX_ENTRIES {
            break;
        }
        if take_bytes(data, &mut pos, 4).is_none() {
            return Err("truncated RAR5 header: missing HeaderCRC32".to_string());
        }
        let header_size = read_vint(data, &mut pos)
            .ok_or_else(|| "truncated RAR5 header: missing HeaderSize vint".to_string())? as usize;
        let header_start = pos;
        let header_end = header_start
            .checked_add(header_size)
            .ok_or_else(|| "RAR5 header_size overflowed".to_string())?;
        if header_end > len {
            return Err("RAR5 header extends past end of file".to_string());
        }
        let header = &data[header_start..header_end];

        let mut hp: usize = 0;
        let header_type =
            read_vint(header, &mut hp).ok_or_else(|| "truncated RAR5 header: missing HeaderType".to_string())?;
        let header_flags =
            read_vint(header, &mut hp).ok_or_else(|| "truncated RAR5 header: missing HeaderFlags".to_string())?;

        if header_flags & RAR5_HFL_EXTRA != 0 {
            read_vint(header, &mut hp).ok_or_else(|| "truncated RAR5 header: missing ExtraAreaSize".to_string())?;
        }
        let data_size = if header_flags & RAR5_HFL_DATA != 0 {
            read_vint(header, &mut hp).ok_or_else(|| "truncated RAR5 header: missing DataSize".to_string())?
        } else {
            0
        };

        if header_type == RAR5_HEAD_FILE || header_type == RAR5_HEAD_SERVICE {
            let file_flags = read_vint(header, &mut hp)
                .ok_or_else(|| "truncated RAR5 file header: missing FileFlags".to_string())?;
            let unpacked_size = read_vint(header, &mut hp)
                .ok_or_else(|| "truncated RAR5 file header: missing UnpackedSize".to_string())?;
            read_vint(header, &mut hp)
                .ok_or_else(|| "truncated RAR5 file header: missing Attributes".to_string())?;
            if file_flags & RAR5_FHFL_UTIME != 0 {
                take_bytes(header, &mut hp, 4)
                    .ok_or_else(|| "truncated RAR5 file header: missing mtime".to_string())?;
            }
            if file_flags & RAR5_FHFL_CRC32 != 0 {
                take_bytes(header, &mut hp, 4)
                    .ok_or_else(|| "truncated RAR5 file header: missing data CRC32".to_string())?;
            }
            read_vint(header, &mut hp)
                .ok_or_else(|| "truncated RAR5 file header: missing CompressionInfo".to_string())?;
            read_vint(header, &mut hp).ok_or_else(|| "truncated RAR5 file header: missing HostOS".to_string())?;
            let name_length = read_vint(header, &mut hp)
                .ok_or_else(|| "truncated RAR5 file header: missing NameLength".to_string())? as usize;
            let name_bytes = take_bytes(header, &mut hp, name_length)
                .ok_or_else(|| "RAR5 file name extends past its header".to_string())?;
            let name = String::from_utf8_lossy(name_bytes).to_string();
            let is_dir = file_flags & RAR5_FHFL_DIRECTORY != 0;

            if !name.is_empty() {
                out.push(ArchiveEntry { name, size: unpacked_size, is_dir });
            }
        }

        let next_pos_u64 = (header_end as u64)
            .checked_add(data_size)
            .ok_or_else(|| "RAR5 data area size overflowed".to_string())?;
        if next_pos_u64 > len as u64 {
            return Err("RAR5 data area extends past end of file".to_string());
        }
        pos = next_pos_u64 as usize;

        if header_type == RAR5_HEAD_ENDARC {
            break;
        }
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// Single-entry extraction (CPE-1360): STORED (uncompressed) entries only.
// ---------------------------------------------------------------------------
//
// RAR's compression is proprietary and has no free/pure-Rust decoder (the native `unrar` lib is
// non-free — see this module's header), so a real decompressor is explicitly out of scope. But a RAR
// entry stored with method STORE keeps its bytes verbatim in the archive's data area, so those can be
// returned by a copy — no decompression involved. This is enough for the archive-preview's
// extract-then-preview path (CPE-1360) to render an uncompressed inner file; a compressed entry degrades
// to a clean `Err` rather than crashing or showing garbage. Same bounds-checking discipline as the
// listing walkers above: every offset is validated, malformed input is an `Err`, never a panic.

/// RAR4 compression-method byte for a STORED (uncompressed) entry. Any other value is a genuine
/// compression algorithm this module deliberately does not implement.
const RAR4_METHOD_STORE: u8 = 0x30;

/// Extract a single **STORED** entry's bytes from a `.rar` archive by name, without decompressing
/// (CPE-1360). Detects RAR4/RAR5 by signature and walks to the named file header (reusing the same
/// header-walk logic as [`rar_entries`]). For a STORE entry (RAR4 method `0x30` / RAR5 compression
/// method `0`) it returns the data-area bytes that follow the header verbatim; for a COMPRESSED entry it
/// returns a clear `Err` (no RAR decompressor exists here). A missing name, or corrupt/truncated/oversized
/// input, is a clean `Err` — this never panics or reads out of bounds. `inner`'s `\` are normalised to
/// `/` before matching, mirroring how the frontend builds archive-relative paths.
pub fn rar_extract_entry(path: &str, inner: &str) -> Result<Vec<u8>, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let target = inner.replace('\\', "/");
    if data.starts_with(&RAR5_MARKER) {
        extract_rar5(&data[RAR5_MARKER.len()..], &target)
    } else if data.starts_with(&RAR4_MARKER) {
        extract_rar4(&data[RAR4_MARKER.len()..], &target)
    } else {
        Err("not a RAR archive (unrecognised signature)".to_string())
    }
}

/// Walk RAR4 blocks looking for the named file entry; return its stored data-area bytes. Mirrors
/// [`parse_rar4`]'s bounds-checked walk, but instead of collecting a listing it stops at the matching
/// name and returns its payload (STORE only).
fn extract_rar4(data: &[u8], target: &str) -> Result<Vec<u8>, String> {
    let len = data.len();
    let mut pos: usize = 0;

    while pos < len {
        if pos + 7 > len {
            return Err("truncated RAR4 block: not enough bytes for the common header".to_string());
        }
        let block_start = pos;
        let head_type = data[pos + 2];
        let flags = u16::from_le_bytes([data[pos + 3], data[pos + 4]]);
        let head_size = u16::from_le_bytes([data[pos + 5], data[pos + 6]]) as usize;
        if head_size < 7 {
            return Err("invalid RAR4 block: head_size smaller than the common header".to_string());
        }
        let header_end = block_start
            .checked_add(head_size)
            .ok_or_else(|| "RAR4 head_size overflowed".to_string())?;
        if header_end > len {
            return Err("RAR4 block header extends past end of file".to_string());
        }
        if head_type == RAR4_PROTECT_HEAD {
            return Err("RAR4 recovery record (PROTECT_HEAD) is not supported".to_string());
        }

        let mut pack_size: u64 = 0;
        let mut matched = false;
        let mut method: u8 = 0;

        if head_type == RAR4_FILE_HEAD || head_type == RAR4_SUB_HEAD || head_type == RAR4_NEWSUB_HEAD {
            let fixed_start = block_start + 7;
            let fixed_end = fixed_start
                .checked_add(25)
                .ok_or_else(|| "RAR4 file header field overflow".to_string())?;
            if fixed_end > header_end {
                return Err("truncated RAR4 file header: missing fixed fields".to_string());
            }
            let low_pack = u32::from_le_bytes(data[fixed_start..fixed_start + 4].try_into().unwrap()) as u64;
            let this_method = data[fixed_start + 18];
            let name_size = u16::from_le_bytes(data[fixed_start + 19..fixed_start + 21].try_into().unwrap()) as usize;

            let mut field_pos = fixed_end;
            let high_pack = if flags & RAR4_LHD_LARGE != 0 {
                let large_end = field_pos
                    .checked_add(8)
                    .ok_or_else(|| "RAR4 large-size field overflow".to_string())?;
                if large_end > header_end {
                    return Err("truncated RAR4 file header: missing LHD_LARGE fields".to_string());
                }
                let hp = u32::from_le_bytes(data[field_pos..field_pos + 4].try_into().unwrap()) as u64;
                field_pos = large_end;
                hp
            } else {
                0u64
            };

            let name_end = field_pos
                .checked_add(name_size)
                .ok_or_else(|| "RAR4 name_size overflow".to_string())?;
            if name_end > header_end {
                return Err("RAR4 file name extends past its header".to_string());
            }
            let name = decode_rar4_name(&data[field_pos..name_end], flags & RAR4_LHD_UNICODE != 0);
            pack_size = low_pack | (high_pack << 32);
            let is_dir = flags & RAR4_LHD_WINDOWMASK == RAR4_LHD_DIRECTORY;
            if !is_dir && name.replace('\\', "/") == target {
                matched = true;
                method = this_method;
            }
        }

        let data_start = header_end;
        let next_pos_u64 = (header_end as u64)
            .checked_add(pack_size)
            .ok_or_else(|| "RAR4 data payload size overflowed".to_string())?;
        if next_pos_u64 > len as u64 {
            return Err("RAR4 data payload extends past end of file".to_string());
        }
        let next_pos = next_pos_u64 as usize;

        if matched {
            if method != RAR4_METHOD_STORE {
                return Err("compressed RAR entries aren't supported for preview".to_string());
            }
            return Ok(data[data_start..next_pos].to_vec());
        }

        pos = next_pos;
        if head_type == RAR4_ENDARC_HEAD {
            break;
        }
    }

    Err(format!("entry not found: {target}"))
}

/// Walk RAR5 headers looking for the named file entry; return its stored data-area bytes. Mirrors
/// [`parse_rar5`]'s bounds-checked walk, decoding a file/service header far enough to read its
/// `CompressionInfo` (method lives in bits 7..=9) and name, then returns the data area for a STORE entry.
fn extract_rar5(data: &[u8], target: &str) -> Result<Vec<u8>, String> {
    let len = data.len();
    let mut pos: usize = 0;

    while pos < len {
        if take_bytes(data, &mut pos, 4).is_none() {
            return Err("truncated RAR5 header: missing HeaderCRC32".to_string());
        }
        let header_size = read_vint(data, &mut pos)
            .ok_or_else(|| "truncated RAR5 header: missing HeaderSize vint".to_string())? as usize;
        let header_start = pos;
        let header_end = header_start
            .checked_add(header_size)
            .ok_or_else(|| "RAR5 header_size overflowed".to_string())?;
        if header_end > len {
            return Err("RAR5 header extends past end of file".to_string());
        }
        let header = &data[header_start..header_end];

        let mut hp: usize = 0;
        let header_type =
            read_vint(header, &mut hp).ok_or_else(|| "truncated RAR5 header: missing HeaderType".to_string())?;
        let header_flags =
            read_vint(header, &mut hp).ok_or_else(|| "truncated RAR5 header: missing HeaderFlags".to_string())?;

        if header_flags & RAR5_HFL_EXTRA != 0 {
            read_vint(header, &mut hp).ok_or_else(|| "truncated RAR5 header: missing ExtraAreaSize".to_string())?;
        }
        let data_size = if header_flags & RAR5_HFL_DATA != 0 {
            read_vint(header, &mut hp).ok_or_else(|| "truncated RAR5 header: missing DataSize".to_string())?
        } else {
            0
        };

        let mut matched = false;
        let mut method: u64 = 0;

        if header_type == RAR5_HEAD_FILE || header_type == RAR5_HEAD_SERVICE {
            let file_flags = read_vint(header, &mut hp)
                .ok_or_else(|| "truncated RAR5 file header: missing FileFlags".to_string())?;
            read_vint(header, &mut hp)
                .ok_or_else(|| "truncated RAR5 file header: missing UnpackedSize".to_string())?;
            read_vint(header, &mut hp)
                .ok_or_else(|| "truncated RAR5 file header: missing Attributes".to_string())?;
            if file_flags & RAR5_FHFL_UTIME != 0 {
                take_bytes(header, &mut hp, 4)
                    .ok_or_else(|| "truncated RAR5 file header: missing mtime".to_string())?;
            }
            if file_flags & RAR5_FHFL_CRC32 != 0 {
                take_bytes(header, &mut hp, 4)
                    .ok_or_else(|| "truncated RAR5 file header: missing data CRC32".to_string())?;
            }
            let compression_info = read_vint(header, &mut hp)
                .ok_or_else(|| "truncated RAR5 file header: missing CompressionInfo".to_string())?;
            read_vint(header, &mut hp).ok_or_else(|| "truncated RAR5 file header: missing HostOS".to_string())?;
            let name_length = read_vint(header, &mut hp)
                .ok_or_else(|| "truncated RAR5 file header: missing NameLength".to_string())? as usize;
            let name_bytes = take_bytes(header, &mut hp, name_length)
                .ok_or_else(|| "RAR5 file name extends past its header".to_string())?;
            let name = String::from_utf8_lossy(name_bytes).to_string();
            let is_dir = file_flags & RAR5_FHFL_DIRECTORY != 0;
            if !is_dir && name.replace('\\', "/") == target {
                matched = true;
                // RAR5 CompressionInfo: bits 0..=5 = version, bit 6 = solid, bits 7..=9 = method.
                method = (compression_info >> 7) & 0x7;
            }
        }

        let data_start = header_end;
        let next_pos_u64 = (header_end as u64)
            .checked_add(data_size)
            .ok_or_else(|| "RAR5 data area size overflowed".to_string())?;
        if next_pos_u64 > len as u64 {
            return Err("RAR5 data area extends past end of file".to_string());
        }
        let next_pos = next_pos_u64 as usize;

        if matched {
            if method != 0 {
                return Err("compressed RAR entries aren't supported for preview".to_string());
            }
            return Ok(data[data_start..next_pos].to_vec());
        }

        pos = next_pos;
        if header_type == RAR5_HEAD_ENDARC {
            break;
        }
    }

    Err(format!("entry not found: {target}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-rar-{tag}"))
    }

    fn write_rar(dir: &std::path::Path, name: &str, bytes: &[u8]) -> String {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(bytes).unwrap();
        path.to_string_lossy().to_string()
    }

    // --- RAR4 fixture builders -------------------------------------------------------------

    /// Build one RAR4 block: common header + (optionally) a file-header body + name.
    fn rar4_block(head_type: u8, flags: u16, name: &str, unp_size: u32) -> Vec<u8> {
        let mut body = Vec::new();
        if head_type == RAR4_FILE_HEAD || head_type == RAR4_SUB_HEAD || head_type == RAR4_NEWSUB_HEAD {
            body.extend_from_slice(&0u32.to_le_bytes()); // pack_size
            body.extend_from_slice(&unp_size.to_le_bytes()); // unp_size
            body.push(0); // host_os
            body.extend_from_slice(&0u32.to_le_bytes()); // file_crc
            body.extend_from_slice(&0u32.to_le_bytes()); // ftime
            body.push(0); // unp_ver
            body.push(0); // method
            body.extend_from_slice(&(name.len() as u16).to_le_bytes()); // name_size
            body.extend_from_slice(&0u32.to_le_bytes()); // attr
            body.extend_from_slice(name.as_bytes());
        }
        let head_size = (7 + body.len()) as u16;
        let mut block = Vec::new();
        block.extend_from_slice(&0u16.to_le_bytes()); // crc16 (unchecked)
        block.push(head_type);
        block.extend_from_slice(&flags.to_le_bytes());
        block.extend_from_slice(&head_size.to_le_bytes());
        block.extend_from_slice(&body);
        block
    }

    fn synthetic_rar4() -> Vec<u8> {
        let mut buf = RAR4_MARKER.to_vec();
        buf.extend(rar4_block(0x73, 0, "", 0)); // MAIN_HEAD, no file fields
        buf.extend(rar4_block(RAR4_FILE_HEAD, 0, "hello.txt", 8));
        buf.extend(rar4_block(RAR4_FILE_HEAD, RAR4_LHD_DIRECTORY, "sub", 0));
        buf.extend(rar4_block(RAR4_FILE_HEAD, 0, "sub/inner.bin", 4096));
        buf
    }

    #[test]
    fn rar4_lists_files_and_a_directory() {
        let d = scratch("rar4");
        let path = write_rar(&d, "a.rar", &synthetic_rar4());
        let entries = rar_entries(&path).unwrap();
        let hello = entries.iter().find(|e| e.name == "hello.txt").unwrap();
        assert!(!hello.is_dir);
        assert_eq!(hello.size, 8);
        let sub = entries.iter().find(|e| e.name == "sub").unwrap();
        assert!(sub.is_dir);
        let inner = entries.iter().find(|e| e.name == "sub/inner.bin").unwrap();
        assert!(!inner.is_dir);
        assert_eq!(inner.size, 4096);
        // MAIN_HEAD carries no name, so it must not show up as a (spurious) empty entry.
        assert_eq!(entries.len(), 3);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rar4_large_flag_reconstructs_a_64_bit_size() {
        let mut body = Vec::new();
        body.extend_from_slice(&0u32.to_le_bytes()); // pack_size (low)
        body.extend_from_slice(&1u32.to_le_bytes()); // unp_size (low) = 1
        body.push(0);
        body.extend_from_slice(&0u32.to_le_bytes());
        body.extend_from_slice(&0u32.to_le_bytes());
        body.push(0);
        body.push(0);
        let name = "big.bin";
        body.extend_from_slice(&(name.len() as u16).to_le_bytes());
        body.extend_from_slice(&0u32.to_le_bytes());
        // LHD_LARGE fields: high_pack_size, high_unp_size
        body.extend_from_slice(&0u32.to_le_bytes());
        body.extend_from_slice(&1u32.to_le_bytes()); // high_unp_size = 1 -> total unp_size = (1<<32)|1
        body.extend_from_slice(name.as_bytes());
        let head_size = (7 + body.len()) as u16;
        let mut block = Vec::new();
        block.extend_from_slice(&0u16.to_le_bytes());
        block.push(RAR4_FILE_HEAD);
        block.extend_from_slice(&RAR4_LHD_LARGE.to_le_bytes());
        block.extend_from_slice(&head_size.to_le_bytes());
        block.extend_from_slice(&body);

        let mut buf = RAR4_MARKER.to_vec();
        buf.extend(block);
        let d = scratch("rar4-large");
        let path = write_rar(&d, "big.rar", &buf);
        let entries = rar_entries(&path).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "big.bin");
        assert_eq!(entries[0].size, (1u64 << 32) | 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rar4_huge_pack_size_overflowing_next_block_offset_is_an_error_not_a_hang() {
        // A near-u64::MAX pack_size (reconstructed from LHD_LARGE's high half) makes
        // `header_end + pack_size` overflow u64. Regression for the reviewer-found bug: this must
        // return Err via checked_add, not panic (debug) or wrap to a position behind the current
        // block and loop forever (release). Empty name means no entry is pushed, so MAX_ENTRIES
        // could never mask a wrap-around hang — the overflow check is the only thing standing
        // between this input and an infinite loop.
        let mut body = Vec::new();
        body.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // pack_size (low) = u32::MAX
        body.extend_from_slice(&0u32.to_le_bytes()); // unp_size (low)
        body.push(0);
        body.extend_from_slice(&0u32.to_le_bytes());
        body.extend_from_slice(&0u32.to_le_bytes());
        body.push(0);
        body.push(0);
        body.extend_from_slice(&0u16.to_le_bytes()); // name_size = 0 (empty name)
        body.extend_from_slice(&0u32.to_le_bytes()); // attr
        // LHD_LARGE fields: high_pack_size = u32::MAX -> total pack_size = u64::MAX
        body.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        body.extend_from_slice(&0u32.to_le_bytes()); // high_unp_size
        // (no name bytes: name_size is 0)
        let head_size = (7 + body.len()) as u16;
        let mut block = Vec::new();
        block.extend_from_slice(&0u16.to_le_bytes());
        block.push(RAR4_FILE_HEAD);
        block.extend_from_slice(&RAR4_LHD_LARGE.to_le_bytes());
        block.extend_from_slice(&head_size.to_le_bytes());
        block.extend_from_slice(&body);

        let mut buf = RAR4_MARKER.to_vec();
        buf.extend(block);
        let d = scratch("rar4-overflow");
        let path = write_rar(&d, "overflow.rar", &buf);
        // ArchiveEntry doesn't derive Debug, so assert by matching rather than formatting the Ok value.
        match rar_entries(&path) {
            Err(_) => {}
            Ok(entries) => panic!("expected Err on an overflowing pack_size, got Ok with {} entries", entries.len()),
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    // --- RAR4 single-entry extraction (CPE-1360) -------------------------------------------

    /// Build one RAR4 file block that carries a real data payload after its header, with an explicit
    /// compression method byte (`0x30` = STORE). `pack_size`/`unp_size` are set to the payload length.
    fn rar4_file_block_with_data(name: &str, payload: &[u8], method: u8) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&(payload.len() as u32).to_le_bytes()); // pack_size
        body.extend_from_slice(&(payload.len() as u32).to_le_bytes()); // unp_size
        body.push(0); // host_os
        body.extend_from_slice(&0u32.to_le_bytes()); // file_crc
        body.extend_from_slice(&0u32.to_le_bytes()); // ftime
        body.push(0); // unp_ver
        body.push(method); // method
        body.extend_from_slice(&(name.len() as u16).to_le_bytes()); // name_size
        body.extend_from_slice(&0u32.to_le_bytes()); // attr
        body.extend_from_slice(name.as_bytes());
        let head_size = (7 + body.len()) as u16;
        let mut block = Vec::new();
        block.extend_from_slice(&0u16.to_le_bytes()); // crc16 (unchecked)
        block.push(RAR4_FILE_HEAD);
        block.extend_from_slice(&0u16.to_le_bytes()); // flags
        block.extend_from_slice(&head_size.to_le_bytes());
        block.extend_from_slice(&body);
        block.extend_from_slice(payload); // data payload immediately follows the header
        block
    }

    #[test]
    fn rar4_extracts_a_stored_entry() {
        let mut buf = RAR4_MARKER.to_vec();
        buf.extend(rar4_block(0x73, 0, "", 0)); // MAIN_HEAD
        buf.extend(rar4_file_block_with_data("hello.txt", b"hello rar stored", RAR4_METHOD_STORE));
        buf.extend(rar4_file_block_with_data("sub/inner.bin", &[1, 2, 3, 4, 5], RAR4_METHOD_STORE));
        let d = scratch("rar4-extract");
        let path = write_rar(&d, "a.rar", &buf);
        assert_eq!(rar_extract_entry(&path, "hello.txt").unwrap(), b"hello rar stored");
        // A subfolder-nested entry extracts too.
        assert_eq!(rar_extract_entry(&path, "sub/inner.bin").unwrap(), vec![1, 2, 3, 4, 5]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rar4_compressed_entry_is_a_graceful_error() {
        let mut buf = RAR4_MARKER.to_vec();
        // method 0x33 = a real compression level, which we can't decode.
        buf.extend(rar4_file_block_with_data("packed.txt", b"whatever", 0x33));
        let d = scratch("rar4-compressed");
        let path = write_rar(&d, "a.rar", &buf);
        let err = rar_extract_entry(&path, "packed.txt").unwrap_err();
        assert!(err.contains("compressed"), "unexpected error: {err}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rar4_missing_entry_is_an_error() {
        let mut buf = RAR4_MARKER.to_vec();
        buf.extend(rar4_file_block_with_data("present.txt", b"x", RAR4_METHOD_STORE));
        let d = scratch("rar4-missing");
        let path = write_rar(&d, "a.rar", &buf);
        assert!(rar_extract_entry(&path, "absent.txt").is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rar_extract_on_non_rar_or_malformed_input_is_an_error_not_a_panic() {
        let d = scratch("rar-extract-malformed");
        // Not a RAR at all.
        let plain = write_rar(&d, "plain.txt", b"not a rar archive");
        assert!(rar_extract_entry(&plain, "anything").is_err());
        // RAR4 marker + truncated block.
        let mut trunc = RAR4_MARKER.to_vec();
        trunc.extend_from_slice(&[0x00, 0x00, RAR4_FILE_HEAD]);
        let trunc_path = write_rar(&d, "trunc.rar", &trunc);
        let _ = rar_extract_entry(&trunc_path, "x"); // must not panic
        let _ = std::fs::remove_dir_all(&d);
    }

    // --- RAR5 fixture builders -------------------------------------------------------------

    fn write_vint(buf: &mut Vec<u8>, mut v: u64) {
        loop {
            let mut byte = (v & 0x7F) as u8;
            v >>= 7;
            if v != 0 {
                byte |= 0x80;
            }
            buf.push(byte);
            if v == 0 {
                break;
            }
        }
    }

    /// Build one RAR5 header: `HeaderCRC32`(4, zeroed/unchecked) + `HeaderSize` vint + body.
    fn rar5_header(header_type: u64, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&0u32.to_le_bytes()); // HeaderCRC32 (unchecked)
        write_vint(&mut out, body.len() as u64);
        out.extend_from_slice(body);
        let _ = header_type; // (kept for readability at call sites; type lives inside `body`)
        out
    }

    fn rar5_main_header_body() -> Vec<u8> {
        let mut body = Vec::new();
        write_vint(&mut body, 1); // HeaderType = Main
        write_vint(&mut body, 0); // HeaderFlags = 0 (no extra, no data)
        body
    }

    fn rar5_file_header_body(name: &str, size: u64, is_dir: bool) -> Vec<u8> {
        let mut body = Vec::new();
        write_vint(&mut body, RAR5_HEAD_FILE);
        write_vint(&mut body, 0); // HeaderFlags = 0 (no extra area, no data area)
        let file_flags = if is_dir { RAR5_FHFL_DIRECTORY } else { 0 };
        write_vint(&mut body, file_flags);
        write_vint(&mut body, size); // UnpackedSize
        write_vint(&mut body, 0); // Attributes
        write_vint(&mut body, 0); // CompressionInfo
        write_vint(&mut body, 0); // HostOS
        write_vint(&mut body, name.len() as u64); // NameLength
        body.extend_from_slice(name.as_bytes());
        body
    }

    fn synthetic_rar5() -> Vec<u8> {
        let mut buf = RAR5_MARKER.to_vec();
        buf.extend(rar5_header(1, &rar5_main_header_body()));
        buf.extend(rar5_header(RAR5_HEAD_FILE, &rar5_file_header_body("hello.txt", 8, false)));
        buf.extend(rar5_header(RAR5_HEAD_FILE, &rar5_file_header_body("sub", 0, true)));
        buf.extend(rar5_header(RAR5_HEAD_FILE, &rar5_file_header_body("sub/inner.bin", 4096, false)));
        buf
    }

    /// Build a RAR5 file header (with data area) plus its trailing data area, for extraction tests.
    /// `method` goes into CompressionInfo bits 7..=9 (0 = STORE).
    fn rar5_stored_file(name: &str, payload: &[u8], method: u64) -> (Vec<u8>, Vec<u8>) {
        let mut body = Vec::new();
        write_vint(&mut body, RAR5_HEAD_FILE);
        write_vint(&mut body, RAR5_HFL_DATA); // has a data area
        write_vint(&mut body, payload.len() as u64); // DataSize
        write_vint(&mut body, 0); // FileFlags
        write_vint(&mut body, payload.len() as u64); // UnpackedSize
        write_vint(&mut body, 0); // Attributes
        write_vint(&mut body, method << 7); // CompressionInfo (method in bits 7..=9)
        write_vint(&mut body, 0); // HostOS
        write_vint(&mut body, name.len() as u64); // NameLength
        body.extend_from_slice(name.as_bytes());
        (rar5_header(RAR5_HEAD_FILE, &body), payload.to_vec())
    }

    #[test]
    fn rar5_extracts_a_stored_entry() {
        let mut buf = RAR5_MARKER.to_vec();
        buf.extend(rar5_header(1, &rar5_main_header_body()));
        let (hdr, area) = rar5_stored_file("hello.txt", b"hi rar5 stored", 0);
        buf.extend(hdr);
        buf.extend(area);
        let d = scratch("rar5-extract");
        let path = write_rar(&d, "a.rar", &buf);
        assert_eq!(rar_extract_entry(&path, "hello.txt").unwrap(), b"hi rar5 stored");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rar5_compressed_entry_is_a_graceful_error() {
        let mut buf = RAR5_MARKER.to_vec();
        let (hdr, area) = rar5_stored_file("packed.txt", b"whatever", 1); // method 1 = compressed
        buf.extend(hdr);
        buf.extend(area);
        let d = scratch("rar5-compressed");
        let path = write_rar(&d, "a.rar", &buf);
        let err = rar_extract_entry(&path, "packed.txt").unwrap_err();
        assert!(err.contains("compressed"), "unexpected error: {err}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rar5_lists_files_and_a_directory() {
        let d = scratch("rar5");
        let path = write_rar(&d, "a.rar", &synthetic_rar5());
        let entries = rar_entries(&path).unwrap();
        let hello = entries.iter().find(|e| e.name == "hello.txt").unwrap();
        assert!(!hello.is_dir);
        assert_eq!(hello.size, 8);
        let sub = entries.iter().find(|e| e.name == "sub").unwrap();
        assert!(sub.is_dir);
        let inner = entries.iter().find(|e| e.name == "sub/inner.bin").unwrap();
        assert!(!inner.is_dir);
        assert_eq!(inner.size, 4096);
        assert_eq!(entries.len(), 3);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rar5_with_extra_and_data_area_flags_skips_both_correctly() {
        // A file header that declares an extra area AND a data area, to prove the walk correctly skips
        // past both (the extra area is inside HeaderSize; the data area is appended after it).
        let mut body = Vec::new();
        write_vint(&mut body, RAR5_HEAD_FILE);
        write_vint(&mut body, RAR5_HFL_EXTRA | RAR5_HFL_DATA); // has extra area + data area
        write_vint(&mut body, 3); // ExtraAreaSize = 3 bytes
        write_vint(&mut body, 5); // DataSize = 5 bytes (payload lives after the header, not counted here)
        write_vint(&mut body, 0); // FileFlags
        write_vint(&mut body, 123); // UnpackedSize
        write_vint(&mut body, 0); // Attributes
        write_vint(&mut body, 0); // CompressionInfo
        write_vint(&mut body, 0); // HostOS
        write_vint(&mut body, 4); // NameLength
        body.extend_from_slice(b"x.db");
        body.extend_from_slice(&[0xAA, 0xBB, 0xCC]); // the 3-byte extra area, trailing the name

        let mut buf = RAR5_MARKER.to_vec();
        buf.extend(rar5_header(RAR5_HEAD_FILE, &body));
        buf.extend_from_slice(&[0u8; 5]); // the 5-byte data area (packed payload, contents irrelevant)
        // A second header right after, to prove the walk landed exactly past the data area.
        buf.extend(rar5_header(RAR5_HEAD_FILE, &rar5_file_header_body("after.txt", 1, false)));

        let d = scratch("rar5-extra-data");
        let path = write_rar(&d, "a.rar", &buf);
        let entries = rar_entries(&path).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "x.db");
        assert_eq!(entries[0].size, 123);
        assert_eq!(entries[1].name, "after.txt");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rar5_huge_data_size_overflowing_next_header_offset_is_an_error_not_a_hang() {
        // A DataSize vint near u64::MAX makes `header_end + data_size` overflow u64. Regression for
        // the reviewer-found bug: must return Err via checked_add, not panic (debug) or wrap to a
        // position behind the current header and loop forever (release). Header type is Main (not
        // File/Service), so this exercises the overflow purely off HeaderFlags/DataSize — no name is
        // ever parsed, so nothing could push an entry to trip MAX_ENTRIES as an accidental safety net.
        let mut body = Vec::new();
        write_vint(&mut body, 1); // HeaderType = Main
        write_vint(&mut body, RAR5_HFL_DATA); // HeaderFlags: has a data area
        write_vint(&mut body, u64::MAX); // DataSize = u64::MAX

        let mut buf = RAR5_MARKER.to_vec();
        buf.extend(rar5_header(1, &body));

        let d = scratch("rar5-overflow");
        let path = write_rar(&d, "overflow.rar", &buf);
        match rar_entries(&path) {
            Err(_) => {}
            Ok(entries) => panic!("expected Err on an overflowing data_size, got Ok with {} entries", entries.len()),
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    // --- read_vint --------------------------------------------------------------------------

    #[test]
    fn read_vint_single_byte() {
        let data = [0x05];
        let mut pos = 0;
        assert_eq!(read_vint(&data, &mut pos), Some(5));
        assert_eq!(pos, 1);
    }

    #[test]
    fn read_vint_multi_byte() {
        // 0x80 (continue, low7=0) then 0x01 (stop, low7=1) -> 0 | (1 << 7) = 128.
        let data = [0x80, 0x01];
        let mut pos = 0;
        assert_eq!(read_vint(&data, &mut pos), Some(128));
        assert_eq!(pos, 2);
    }

    #[test]
    fn read_vint_truncated_returns_none() {
        // Continuation bit set on the last byte in the buffer: no terminating byte ever arrives.
        let data = [0x80];
        let mut pos = 0;
        assert_eq!(read_vint(&data, &mut pos), None);
    }

    #[test]
    fn read_vint_empty_returns_none() {
        let data: [u8; 0] = [];
        let mut pos = 0;
        assert_eq!(read_vint(&data, &mut pos), None);
    }

    // --- panic-safety on malformed / non-RAR input -------------------------------------------

    #[test]
    fn non_rar_input_is_an_error_not_a_panic() {
        let d = scratch("not-rar");
        let path = write_rar(&d, "plain.txt", b"just a plain text file, not a rar archive at all");
        assert!(rar_entries(&path).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn empty_file_is_an_error() {
        let d = scratch("empty");
        let path = write_rar(&d, "empty.rar", b"");
        assert!(rar_entries(&path).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn malformed_buffers_never_panic() {
        let d = scratch("malformed");
        let cases: Vec<Vec<u8>> = vec![
            // Marker with nothing after it: fine (empty archive), must not panic.
            RAR4_MARKER.to_vec(),
            RAR5_MARKER.to_vec(),
            // Marker + a partial common header (RAR4 needs 7 bytes).
            {
                let mut v = RAR4_MARKER.to_vec();
                v.extend_from_slice(&[0x00, 0x00, RAR4_FILE_HEAD]);
                v
            },
            // RAR4 block whose head_size claims to run past EOF.
            {
                let mut v = RAR4_MARKER.to_vec();
                v.extend_from_slice(&0u16.to_le_bytes());
                v.push(RAR4_FILE_HEAD);
                v.extend_from_slice(&0u16.to_le_bytes());
                v.extend_from_slice(&0xFFFFu16.to_le_bytes()); // head_size way bigger than the file
                v
            },
            // RAR4 block with head_size < 7 (invalid, must error rather than underflow).
            {
                let mut v = RAR4_MARKER.to_vec();
                v.extend_from_slice(&0u16.to_le_bytes());
                v.push(RAR4_FILE_HEAD);
                v.extend_from_slice(&0u16.to_le_bytes());
                v.extend_from_slice(&3u16.to_le_bytes());
                v
            },
            // RAR4 file header truncated mid-fixed-fields.
            {
                let mut v = RAR4_MARKER.to_vec();
                v.extend_from_slice(&0u16.to_le_bytes());
                v.push(RAR4_FILE_HEAD);
                v.extend_from_slice(&0u16.to_le_bytes());
                v.extend_from_slice(&20u16.to_le_bytes()); // claims 20 bytes but none follow
                v
            },
            // RAR5 marker + a HeaderSize vint that runs past EOF.
            {
                let mut v = RAR5_MARKER.to_vec();
                v.extend_from_slice(&0u32.to_le_bytes());
                v.push(0xFF); // vint continuation byte with nothing to terminate it
                v
            },
            // RAR5 marker + valid small HeaderSize but body truncated (missing HeaderFlags).
            {
                let mut v = RAR5_MARKER.to_vec();
                v.extend_from_slice(&0u32.to_le_bytes());
                v.push(5); // HeaderSize = 5 but nothing follows
                v
            },
            // Random noise after each marker, several different byte patterns.
            {
                let mut v = RAR4_MARKER.to_vec();
                v.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0xFF, 0x11, 0x22, 0x33]);
                v
            },
            {
                let mut v = RAR5_MARKER.to_vec();
                v.extend_from_slice(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09]);
                v
            },
            // Not RAR at all.
            b"\x00\x01\x02random bytes, no signature here".to_vec(),
        ];
        for (i, bytes) in cases.into_iter().enumerate() {
            let path = write_rar(&d, &format!("case-{i}.rar"), &bytes);
            // The only contract: never panic. Either Ok (possibly empty) or Err is acceptable.
            let _ = rar_entries(&path);
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn protect_head_is_rejected_as_the_documented_exotic_case() {
        let mut buf = RAR4_MARKER.to_vec();
        let mut block = Vec::new();
        block.extend_from_slice(&0u16.to_le_bytes());
        block.push(RAR4_PROTECT_HEAD);
        block.extend_from_slice(&0u16.to_le_bytes());
        block.extend_from_slice(&7u16.to_le_bytes()); // just the common header
        buf.extend(block);
        let d = scratch("protect-head");
        let path = write_rar(&d, "protect.rar", &buf);
        assert!(rar_entries(&path).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
