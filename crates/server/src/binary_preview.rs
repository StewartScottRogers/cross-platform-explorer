//! Structured binary previews (CPE-210/214/215/216/218): human-readable text summaries of binary files
//! for the preview pane's read-only "info" provider — a hex dump, a PE (EXE/DLL) header summary, a MIDI
//! track summary, a wasm→WAT disassembly, and a `.torrent` metadata summary. Pure-Rust deps (goblin /
//! midly / wasmprinter / serde_bencode), no system libs; extracted into the Server (CPE-815). The Tauri
//! `read_preview_info` command dispatches per-extension into these.

use std::fs;

use crate::bin_arch::{self, BinaryArch};
use crate::model::{
    BinaryExport, BinaryFormat, BinaryImport, BinaryInfo, BinaryInstruction, BinarySection,
    BinarySymbol, MAX_BINARY_LIST_ENTRIES, MAX_DISASM_INSTRUCTIONS,
};

/// Classic hex + ASCII dump of the first `max` bytes (CPE-214). Reads at most `max` bytes so a multi-GB
/// binary is never slurped into memory (CPE-633).
pub fn hex_dump(path: &str, max: usize) -> Result<String, String> {
    use std::io::Read;
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    // The file's true length, captured before the capped read, so we can tell the user when the dump is
    // partial. (Reading with `.take(max)` alone can't reveal that more bytes exist.)
    let total = file.metadata().map_err(|e| e.to_string())?.len();
    let mut bytes = Vec::new();
    file.take(max as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let n = bytes.len();
    let mut out = String::new();
    for (i, chunk) in bytes[..n].chunks(16).enumerate() {
        let mut hex = String::new();
        let mut ascii = String::new();
        for (j, b) in chunk.iter().enumerate() {
            hex.push_str(&format!("{b:02x} "));
            if j == 7 {
                hex.push(' ');
            }
            ascii.push(if b.is_ascii_graphic() || *b == b' ' { *b as char } else { '.' });
        }
        out.push_str(&format!("{:08x}  {hex:<49}|{ascii}|\n", i * 16));
    }
    if total > n as u64 {
        out.push_str(&format!("\n… {} more bytes (showing first {n} of {total}).\n", total - n as u64));
    }
    Ok(out)
}

/// Report on a single-file compression format (xz/bz2/zst/lz/lzma) without decompressing it (CPE-1439).
/// Unlike gzip (`flate2`, already a dep, used by [`crate::archive::gzip_single_entry`] for the analogous
/// `.gz` case), this crate has no xz/bzip2/zstd decoder wired in — adding one is out of scope here (no new
/// deps) — so there is no entry list or decompressed size to report. Routing these to the archive lister
/// would just be a parse error against the wrong container format (only `.tar.xz`/`.tar.bz2`/`.tar.zst`
/// have an inner directory to browse, and this crate can't peel the compression wrapper off those either).
/// This is the graceful middle ground: identify the format and report the compressed size, which reads
/// better than a raw hex dump of compressed bytes for something the pane otherwise can't say anything about.
pub fn compressed_file_info(path: &str, format: &str) -> Result<String, String> {
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    Ok(format!(
        "{format}-compressed file\nCompressed size: {} bytes\n\n\
This is a single-file compression format, not a browsable archive — there is no directory of \
entries to list. Open it with an external tool (or via a shell) to decompress and inspect its contents.",
        meta.len()
    ))
}

/// Summary of a Windows PE image (EXE/DLL) via goblin (CPE-216).
pub fn pe_info(path: &str) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let pe = goblin::pe::PE::parse(&bytes).map_err(|e| e.to_string())?;
    let mut out = String::new();
    out.push_str(if pe.is_64 { "PE32+ image (64-bit)\n" } else { "PE32 image (32-bit)\n" });
    out.push_str(&format!("Machine: 0x{:04x}\n", pe.header.coff_header.machine));
    out.push_str(&format!("Entry point: 0x{:x}\n", pe.entry));
    out.push_str(&format!("Sections: {}\n", pe.sections.len()));
    out.push_str(&format!(
        "Imports: {} symbols from {} libraries\n",
        pe.imports.len(),
        pe.libraries.len()
    ));
    out.push_str("\nSections:\n");
    for s in &pe.sections {
        let name = String::from_utf8_lossy(&s.name);
        out.push_str(&format!(
            "  {:<9} vaddr 0x{:08x}  vsize {}\n",
            name.trim_end_matches('\0'),
            s.virtual_address,
            s.virtual_size
        ));
    }
    if !pe.libraries.is_empty() {
        out.push_str("\nLinked libraries:\n");
        for lib in &pe.libraries {
            out.push_str(&format!("  {lib}\n"));
        }
    }
    Ok(out)
}

/// Cap on the number of Mach-O fat/universal slices [`binary_info`] will walk into looking for the
/// first thin slice it can parse. `goblin::mach::MultiArch`'s own iterator trusts the file's
/// `nfat_arch` header field directly (unlike [`crate::bin_arch`]'s own fat-header walk, which caps
/// it), so a hostile file claiming billions of slices would otherwise spin the loop that many times
/// (each iteration cheap, but 2^32 of them is not) before giving up — this bounds that scan the same
/// way `bin_arch::detect_arch` bounds its own.
const MAX_FAT_SLICES_TO_INSPECT: usize = 64;

/// Human-readable architecture label(s) for a [`BinaryArch`] — a single label, or (for a Mach-O
/// fat/universal binary) every slice's label joined with ", ".
fn arch_label(arch: BinaryArch) -> String {
    match arch {
        BinaryArch::Single(info) => info.arch.label().to_string(),
        BinaryArch::Fat(slices) => {
            if slices.is_empty() {
                "Unknown (empty fat header)".to_string()
            } else {
                slices
                    .iter()
                    .take(MAX_BINARY_LIST_ENTRIES)
                    .map(|s| s.arch.label())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        }
    }
}

/// Structured summary of a PE, ELF, or Mach-O binary (CPE-1572, epic CPE-1562 "Binary Inspector"
/// slice 1): format + architecture (via [`bin_arch::detect_arch`]) plus bounded Sections / Imports /
/// Exports / Symbols tables, via goblin — parity across all three formats. Added alongside
/// [`pe_info`] (which keeps serving its existing text-summary callers) rather than replacing it.
///
/// Never panics on malformed input: every list is built skip-on-error per entry and capped at
/// [`MAX_BINARY_LIST_ENTRIES`], and a Mach-O fat/universal binary's slice scan is itself bounded
/// (see [`MAX_FAT_SLICES_TO_INSPECT`]). The only `Err` cases are an unreadable file and a header
/// goblin can't recognise as PE/ELF/Mach-O at all — everything else degrades to a partial (or empty)
/// [`BinaryInfo`].
pub fn binary_info(path: &str) -> Result<BinaryInfo, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let arch = bin_arch::detect_arch(&bytes).map(arch_label);
    match goblin::Object::parse(&bytes).map_err(|e| e.to_string())? {
        goblin::Object::PE(pe) => Ok(pe_binary_info(&pe, &bytes, arch)),
        goblin::Object::Elf(elf) => Ok(elf_binary_info(&elf, &bytes, arch)),
        goblin::Object::Mach(mach) => Ok(mach_binary_info(&mach, arch)),
        _ => Err("not a PE, ELF, or Mach-O binary".to_string()),
    }
}

/// x86/x64 machine-code disassembly (CPE-1581, epic CPE-1562 "Binary Inspector" slice 2): decodes
/// `code` (a code section's raw bytes, starting at virtual address `base_address`) via iced-x86 into
/// a bounded, streamable instruction list. `bitness` must be 32 or 64 (any other value decodes
/// nothing, returning an empty list — callers only pass 32/64, gated by [`x86_bitness_from_machine`]
/// below).
///
/// Never panics and never errors: iced-x86's decoder treats undecodable bytes as a single invalid
/// instruction and always advances at least one byte, so malformed or truncated code degrades to a
/// run of `(bad)`-formatted entries rather than looping forever or panicking — exactly the
/// skip-on-error contract [`binary_info`] already holds for its other lists. Decoding stops early at
/// [`MAX_DISASM_INSTRUCTIONS`] so a huge code section can't blow up memory/time.
pub fn disassemble(code: &[u8], base_address: u64, bitness: u32) -> Vec<BinaryInstruction> {
    use iced_x86::{Decoder, DecoderOptions, Formatter, Instruction, NasmFormatter};

    if bitness != 16 && bitness != 32 && bitness != 64 {
        return Vec::new();
    }

    let mut decoder = Decoder::with_ip(bitness, code, base_address, DecoderOptions::NONE);
    let mut formatter = NasmFormatter::new();
    let mut instr = Instruction::default();
    let mut text = String::new();
    let mut out = Vec::new();

    while decoder.can_decode() && out.len() < MAX_DISASM_INSTRUCTIONS {
        let start = decoder.position();
        decoder.decode_out(&mut instr);
        let end = decoder.position();
        let raw = code.get(start..end).unwrap_or(&[]);

        text.clear();
        formatter.format(&instr, &mut text);

        out.push(BinaryInstruction {
            address: instr.ip(),
            bytes: raw.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "),
            text: text.clone(),
        });
    }
    out
}

/// Bitness iced-x86 should decode with for a PE `Machine` / ELF `e_machine` / Mach-O `cputype` value —
/// `None` for anything other than x86 (32-bit) or x64 (64-bit) this slice handles (ARM etc. are a
/// future slice, per the ticket; disasm stays empty rather than erroring for them).
fn x86_bitness_from_machine(is_x64: bool, is_x86_32: bool) -> Option<u32> {
    if is_x64 {
        Some(64)
    } else if is_x86_32 {
        Some(32)
    } else {
        None
    }
}

/// Locate the PE's code section — by name (`.text`, the conventional code section) first, falling
/// back to whichever section contains the entry point RVA (some linkers/obfuscators don't name the
/// code section `.text`) — and return its raw file bytes plus its virtual address. `None` when no
/// section matches, or its `pointer_to_raw_data`/`size_of_raw_data` fall outside the file (a
/// truncated/malformed PE): the disasm list is simply left empty in that case, not an error.
fn pe_code_section<'a>(pe: &goblin::pe::PE, bytes: &'a [u8]) -> Option<(&'a [u8], u64)> {
    let entry_rva = pe.entry as u32;
    let section = pe
        .sections
        .iter()
        .find(|s| String::from_utf8_lossy(&s.name).trim_end_matches('\0').eq_ignore_ascii_case(".text"))
        .or_else(|| {
            pe.sections
                .iter()
                .find(|s| entry_rva >= s.virtual_address && entry_rva < s.virtual_address.wrapping_add(s.virtual_size))
        })?;
    let start = section.pointer_to_raw_data as usize;
    let end = start.checked_add(section.size_of_raw_data as usize)?;
    Some((bytes.get(start..end)?, section.virtual_address as u64))
}

fn pe_binary_info(pe: &goblin::pe::PE, bytes: &[u8], arch: Option<String>) -> BinaryInfo {
    let sections = pe
        .sections
        .iter()
        .take(MAX_BINARY_LIST_ENTRIES)
        .map(|s| BinarySection {
            name: String::from_utf8_lossy(&s.name).trim_end_matches('\0').to_string(),
            address: s.virtual_address as u64,
            size: s.virtual_size as u64,
        })
        .collect();
    let imports = pe
        .imports
        .iter()
        .take(MAX_BINARY_LIST_ENTRIES)
        .map(|i| BinaryImport { name: i.name.to_string(), library: Some(i.dll.to_string()) })
        .collect();
    let exports = pe
        .exports
        .iter()
        .take(MAX_BINARY_LIST_ENTRIES)
        .map(|e| BinaryExport {
            name: e.name.unwrap_or("<unnamed>").to_string(),
            address: Some(e.rva as u64),
        })
        .collect();
    let disasm = x86_bitness_from_machine(
        pe.header.coff_header.machine == 0x8664, // IMAGE_FILE_MACHINE_AMD64
        pe.header.coff_header.machine == 0x014C, // IMAGE_FILE_MACHINE_I386
    )
    .and_then(|bitness| pe_code_section(pe, bytes).map(|(code, addr)| disassemble(code, addr, bitness)))
    .unwrap_or_default();
    BinaryInfo {
        format: BinaryFormat::Pe,
        arch,
        is_64: pe.is_64,
        is_managed: crate::dotnet_metadata::is_managed(pe),
        sections,
        imports,
        exports,
        // A PE EXE/DLL image carries no equivalent symbol table (see `BinarySymbol`'s doc comment).
        symbols: Vec::new(),
        disasm,
    }
}

/// Locate the ELF's code section — the first section header with `SHF_EXECINSTR` set (its
/// `sh_flags` per the ELF spec; there is no dedicated "the" code section name convention the way PE
/// has `.text` by default, though `.text` is what most toolchains call it) — and return its raw
/// file bytes plus its virtual address. `None` when no section is executable, or its
/// `sh_offset`/`sh_size` fall outside the file (truncated/malformed ELF): the disasm list is simply
/// left empty in that case, not an error.
fn elf_code_section<'a>(elf: &goblin::elf::Elf, bytes: &'a [u8]) -> Option<(&'a [u8], u64)> {
    let sh = elf.section_headers.iter().find(|sh| sh.is_executable())?;
    let start = usize::try_from(sh.sh_offset).ok()?;
    let end = start.checked_add(usize::try_from(sh.sh_size).ok()?)?;
    Some((bytes.get(start..end)?, sh.sh_addr))
}

fn elf_binary_info(elf: &goblin::elf::Elf, bytes: &[u8], arch: Option<String>) -> BinaryInfo {
    let sections = elf
        .section_headers
        .iter()
        .take(MAX_BINARY_LIST_ENTRIES)
        .map(|sh| BinarySection {
            name: elf.shdr_strtab.get_at(sh.sh_name).unwrap_or("").to_string(),
            address: sh.sh_addr,
            size: sh.sh_size,
        })
        .collect();

    // ELF has no separate per-symbol "library" concept in its dynamic symbol table: an undefined
    // (imported) global/weak symbol is an import, a defined one is an export (CPE-1572).
    let mut imports = Vec::new();
    let mut exports = Vec::new();
    for sym in elf.dynsyms.iter() {
        if imports.len() >= MAX_BINARY_LIST_ENTRIES && exports.len() >= MAX_BINARY_LIST_ENTRIES {
            break;
        }
        let Some(name) = elf.dynstrtab.get_at(sym.st_name) else { continue };
        if name.is_empty() {
            continue;
        }
        if sym.is_import() {
            if imports.len() < MAX_BINARY_LIST_ENTRIES {
                imports.push(BinaryImport { name: name.to_string(), library: None });
            }
        } else if exports.len() < MAX_BINARY_LIST_ENTRIES {
            exports.push(BinaryExport { name: name.to_string(), address: Some(sym.st_value) });
        }
    }

    let symbols = elf
        .syms
        .iter()
        .take(MAX_BINARY_LIST_ENTRIES)
        .filter_map(|sym| {
            let name = elf.strtab.get_at(sym.st_name)?;
            if name.is_empty() {
                return None;
            }
            Some(BinarySymbol { name: name.to_string(), address: Some(sym.st_value) })
        })
        .collect();

    let disasm = x86_bitness_from_machine(
        elf.header.e_machine == goblin::elf::header::EM_X86_64,
        elf.header.e_machine == goblin::elf::header::EM_386,
    )
    .and_then(|bitness| elf_code_section(elf, bytes).map(|(code, addr)| disassemble(code, addr, bitness)))
    .unwrap_or_default();

    BinaryInfo {
        format: BinaryFormat::Elf,
        arch,
        is_64: elf.is_64,
        // .NET-on-ELF (Mono AOT etc.) is out of this slice's scope — a managed CLI header only
        // exists in a PE's data directory #14, so ELF is never reported managed.
        is_managed: false,
        sections,
        imports,
        exports,
        symbols,
        disasm,
    }
}

fn mach_binary_info(mach: &goblin::mach::Mach, arch: Option<String>) -> BinaryInfo {
    match mach {
        goblin::mach::Mach::Binary(macho) => macho_binary_info(macho, arch),
        goblin::mach::Mach::Fat(multi) => {
            // Parity for a fat/universal binary: summarize the first thin Mach-O slice this bounded
            // scan can actually parse, rather than merging every architecture's tables together into
            // one confusing combined list.
            for entry in multi.into_iter().take(MAX_FAT_SLICES_TO_INSPECT) {
                if let Ok(goblin::mach::SingleArch::MachO(macho)) = entry {
                    return macho_binary_info(&macho, arch);
                }
            }
            BinaryInfo {
                format: BinaryFormat::MachO,
                arch,
                is_64: false,
                is_managed: false,
                sections: Vec::new(),
                imports: Vec::new(),
                exports: Vec::new(),
                symbols: Vec::new(),
                disasm: Vec::new(),
            }
        }
    }
}

fn macho_binary_info(macho: &goblin::mach::MachO, arch: Option<String>) -> BinaryInfo {
    use goblin::mach::constants::cputype::{CPU_TYPE_X86, CPU_TYPE_X86_64};

    let is_x86 = x86_bitness_from_machine(
        macho.header.cputype == CPU_TYPE_X86_64,
        macho.header.cputype == CPU_TYPE_X86,
    );

    let mut sections = Vec::new();
    let mut disasm = Vec::new();
    'segs: for section_iter in macho.segments.sections() {
        for entry in section_iter {
            if sections.len() >= MAX_BINARY_LIST_ENTRIES {
                break 'segs;
            }
            if let Ok((sect, data)) = entry {
                // The Mach-O code section is conventionally named `__text` inside the `__TEXT`
                // segment; disassemble it once we've located it (skip-on-error: an unreadable
                // section, e.g. one truncated by a malformed/hostile file, just leaves disasm empty).
                if disasm.is_empty() {
                    if let Some(bitness) = is_x86 {
                        if sect.name().unwrap_or("") == "__text" {
                            disasm = disassemble(data, sect.addr, bitness);
                        }
                    }
                }
                sections.push(BinarySection {
                    name: sect.name().unwrap_or("").to_string(),
                    address: sect.addr,
                    size: sect.size,
                });
            }
        }
    }

    let imports = macho
        .imports()
        .unwrap_or_default()
        .into_iter()
        .take(MAX_BINARY_LIST_ENTRIES)
        .map(|i| BinaryImport { name: i.name.to_string(), library: Some(i.dylib.to_string()) })
        .collect();

    let exports = macho
        .exports()
        .unwrap_or_default()
        .into_iter()
        .take(MAX_BINARY_LIST_ENTRIES)
        .map(|e| BinaryExport { name: e.name, address: Some(e.offset) })
        .collect();

    let symbols = macho
        .symbols()
        .take(MAX_BINARY_LIST_ENTRIES)
        .filter_map(|entry| {
            let (name, nlist) = entry.ok()?;
            if name.is_empty() {
                return None;
            }
            Some(BinarySymbol { name: name.to_string(), address: Some(nlist.n_value) })
        })
        .collect();

    BinaryInfo {
        format: BinaryFormat::MachO,
        arch,
        is_64: macho.is_64,
        is_managed: false,
        sections,
        imports,
        exports,
        symbols,
        disasm,
    }
}

/// Summary of a MIDI file via midly (CPE-210).
pub fn midi_info(path: &str) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let smf = midly::Smf::parse(&bytes).map_err(|e| e.to_string())?;
    let mut out = String::new();
    out.push_str(&format!("MIDI format: {:?}\n", smf.header.format));
    out.push_str(&format!("Timing: {:?}\n", smf.header.timing));
    out.push_str(&format!("Tracks: {}\n", smf.tracks.len()));
    let total: usize = smf.tracks.iter().map(|t| t.len()).sum();
    out.push_str(&format!("Total events: {total}\n\n"));
    for (i, t) in smf.tracks.iter().enumerate() {
        let mut name = String::new();
        for ev in t.iter() {
            if let midly::TrackEventKind::Meta(midly::MetaMessage::TrackName(n)) = ev.kind {
                name = String::from_utf8_lossy(n).to_string();
                break;
            }
        }
        let suffix = if name.is_empty() { String::new() } else { format!(" — {name}") };
        out.push_str(&format!("  Track {}: {} events{suffix}\n", i + 1, t.len()));
    }
    Ok(out)
}

/// Disassemble a WebAssembly binary to its text form (WAT) via wasmprinter, capped so a huge module
/// can't flood the pane (CPE-215).
pub fn wasm_info(path: &str, max: usize) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let wat = wasmprinter::print_bytes(&bytes).map_err(|e| e.to_string())?;
    if wat.len() > max {
        let mut cut = max;
        while cut > 0 && !wat.is_char_boundary(cut) {
            cut -= 1;
        }
        Ok(format!("{}\n\n… truncated ({cut} of {} bytes shown).\n", &wat[..cut], wat.len()))
    } else {
        Ok(wat)
    }
}

/// Summary of a .torrent's bencode metadata via serde_bencode (CPE-218).
pub fn torrent_info(path: &str) -> Result<String, String> {
    use serde_bencode::value::Value;
    use std::collections::HashMap;

    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let val: Value = serde_bencode::from_bytes(&bytes).map_err(|e| e.to_string())?;
    let Value::Dict(top) = &val else {
        return Err("Not a bencode dictionary".to_string());
    };
    let get = |d: &'_ HashMap<Vec<u8>, Value>, k: &str| -> Option<Value> { d.get(k.as_bytes()).cloned() };
    let as_str = |v: &Value| match v {
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        _ => String::new(),
    };
    let as_int = |v: &Value| match v {
        Value::Int(i) => *i,
        _ => 0,
    };

    let mut out = String::new();
    if let Some(a) = get(top, "announce") {
        out.push_str(&format!("Announce: {}\n", as_str(&a)));
    }
    if let Some(Value::Dict(info)) = get(top, "info") {
        if let Some(n) = get(&info, "name") {
            out.push_str(&format!("Name: {}\n", as_str(&n)));
        }
        if let Some(pl) = get(&info, "piece length") {
            out.push_str(&format!("Piece length: {} bytes\n", as_int(&pl)));
        }
        match get(&info, "files") {
            Some(Value::List(files)) => {
                out.push_str(&format!("Files: {}\n", files.len()));
                let mut total = 0i64;
                for f in &files {
                    if let Value::Dict(fd) = f {
                        let len = get(fd, "length").map(|v| as_int(&v)).unwrap_or(0);
                        total += len;
                        let parts = match get(fd, "path") {
                            Some(Value::List(ps)) => ps.iter().map(as_str).collect::<Vec<_>>().join("/"),
                            _ => String::new(),
                        };
                        out.push_str(&format!("  {parts} ({len} bytes)\n"));
                    }
                }
                out.push_str(&format!("Total size: {total} bytes\n"));
            }
            _ => {
                if let Some(l) = get(&info, "length") {
                    out.push_str(&format!("Size: {} bytes (single file)\n", as_int(&l)));
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-binprev-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    // -----------------------------------------------------------------------------------------
    // disassemble (CPE-1581): x86/x64 machine-code decoding via iced-x86
    // -----------------------------------------------------------------------------------------

    #[test]
    fn disassemble_decodes_a_known_x64_prologue_and_epilogue() {
        // push rbp; mov rbp,rsp; pop rbp; ret — a real, unambiguous x64 instruction sequence, not a
        // hollow/all-nop fixture, so this actually proves iced-x86's decode + NASM formatting.
        let code: [u8; 6] = [0x55, 0x48, 0x89, 0xE5, 0x5D, 0xC3];
        let instrs = disassemble(&code, 0x1000, 64);
        assert_eq!(instrs.len(), 4, "expected 4 decoded instructions: {instrs:?}");

        assert_eq!(instrs[0].address, 0x1000);
        assert_eq!(instrs[0].bytes, "55");
        let t0 = instrs[0].text.to_lowercase();
        assert!(t0.contains("push") && t0.contains("rbp"), "expected push rbp, got {t0:?}");

        assert_eq!(instrs[1].address, 0x1001);
        assert_eq!(instrs[1].bytes, "48 89 e5");
        let t1 = instrs[1].text.to_lowercase();
        assert!(t1.contains("mov") && t1.contains("rbp") && t1.contains("rsp"), "expected mov rbp,rsp, got {t1:?}");

        assert_eq!(instrs[2].address, 0x1004);
        assert_eq!(instrs[2].bytes, "5d");
        let t2 = instrs[2].text.to_lowercase();
        assert!(t2.contains("pop") && t2.contains("rbp"), "expected pop rbp, got {t2:?}");

        assert_eq!(instrs[3].address, 0x1005);
        assert_eq!(instrs[3].bytes, "c3");
        assert_eq!(instrs[3].text.to_lowercase(), "ret");
    }

    #[test]
    fn disassemble_decodes_x86_32bit_too() {
        // add eax, ebx (0x01 0xD8) then ret (0xC3) — a distinct real x86 (32-bit) sequence, proving
        // the 32-bit decode path independently of the x64 test above.
        let code: [u8; 3] = [0x01, 0xD8, 0xC3];
        let instrs = disassemble(&code, 0x400000, 32);
        assert_eq!(instrs.len(), 2, "expected 2 decoded instructions: {instrs:?}");
        let t0 = instrs[0].text.to_lowercase();
        assert!(t0.contains("add") && t0.contains("eax") && t0.contains("ebx"), "got {t0:?}");
        assert_eq!(instrs[1].text.to_lowercase(), "ret");
    }

    #[test]
    fn disassemble_returns_empty_for_a_non_x86_bitness() {
        // Only 16/32/64-bit x86 modes are meaningful to iced-x86; any other value (e.g. an
        // unsupported architecture's own "bitness" concept) must yield an empty list, not an error
        // or a panic (CPE-1581's "non-x86 input yields no disasm without erroring").
        let code: [u8; 6] = [0x55, 0x48, 0x89, 0xE5, 0x5D, 0xC3];
        assert!(disassemble(&code, 0x1000, 0).is_empty());
        assert!(disassemble(&code, 0x1000, 8).is_empty());
        assert!(disassemble(&code, 0x1000, 65).is_empty());
    }

    #[test]
    fn disassemble_caps_at_max_disasm_instructions() {
        // MAX_DISASM_INSTRUCTIONS-many `nop`s plus a few thousand extra must decode to exactly the
        // cap, not more — guards against a huge/hostile code section blowing up memory/time.
        let code = vec![0x90u8; MAX_DISASM_INSTRUCTIONS + 5_000];
        let instrs = disassemble(&code, 0, 64);
        assert_eq!(instrs.len(), MAX_DISASM_INSTRUCTIONS);
    }

    #[test]
    fn disassemble_never_panics_on_truncated_or_malformed_bytes() {
        // A quick in-module sweep (the full adversarial battery lives in
        // `tests/binary_data_preview_panic_safety.rs`): truncating a real instruction mid-encoding,
        // and a run of the 0x0F "needs a second opcode byte" prefix with nothing after it, must
        // decode to a graceful `(bad)`-shaped instruction rather than panicking or looping forever.
        let real: [u8; 6] = [0x55, 0x48, 0x89, 0xE5, 0x5D, 0xC3];
        for cut in 0..=real.len() {
            let _ = disassemble(&real[..cut], 0x1000, 64);
        }
        let _ = disassemble(&[0x0F], 0x1000, 64);
        let _ = disassemble(&[0x0F; 32], 0x1000, 64);
        let _ = disassemble(&[], 0x1000, 64);
    }

    #[test]
    fn hex_dump_formats_offsets_and_ascii() {
        let d = scratch("hex");
        let f = d.join("b.bin");
        fs::write(&f, [0x00u8, 0x41, 0x42, 0x43]).unwrap(); // .ABC
        let out = hex_dump(&f.to_string_lossy(), 64 * 1024).unwrap();
        assert!(out.contains("00000000"));
        assert!(out.contains("00 41 42 43"));
        assert!(out.contains("|.ABC|"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn hex_dump_caps_output_at_max_bytes() {
        let d = scratch("hexcap");
        fs::write(d.join("f.bin"), vec![0xABu8; 10_000]).unwrap();
        // 32 bytes = two 16-byte rows; a third row offset (00000020) must not appear.
        let out = hex_dump(&d.join("f.bin").to_string_lossy(), 32).unwrap();
        assert!(out.contains("00000000") && out.contains("00000010"));
        assert!(!out.contains("00000020"), "dumped past the max");
        assert!(out.contains("ab ab"), "bytes rendered");
        // A partial dump MUST tell the user the file continues (this notice was previously dead code —
        // `n == bytes.len()` made the "more bytes" branch unreachable, so a huge file looked complete).
        assert!(out.contains("9968 more bytes"), "truncation notice with the remaining count: {out}");
        assert!(out.contains("first 32 of 10000"), "notice states shown/total: {out}");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn hex_dump_of_a_fully_shown_file_has_no_truncation_notice() {
        let d = scratch("hexfull");
        fs::write(d.join("s.bin"), [0x41u8, 0x42, 0x43]).unwrap(); // 3 bytes, cap far above
        let out = hex_dump(&d.join("s.bin").to_string_lossy(), 64 * 1024).unwrap();
        assert!(!out.contains("more bytes"), "a complete dump must not claim there's more: {out}");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn hex_dump_of_an_empty_file_is_empty_and_safe() {
        let d = scratch("hexempty");
        fs::write(d.join("e.bin"), b"").unwrap();
        let out = hex_dump(&d.join("e.bin").to_string_lossy(), 1024).unwrap();
        assert!(out.is_empty(), "no rows, no truncation notice for a 0-byte file: {out:?}");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn compressed_file_info_reports_format_and_size_without_decoding() {
        let d = scratch("compressed");
        let f = d.join("a.xz");
        fs::write(&f, [0u8; 42]).unwrap();
        let out = compressed_file_info(&f.to_string_lossy(), "XZ").unwrap();
        assert!(out.contains("XZ-compressed file"), "{out}");
        assert!(out.contains("42 bytes"), "{out}");
        assert!(out.contains("not a browsable archive"), "{out}");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn compressed_file_info_errors_on_a_missing_file() {
        assert!(compressed_file_info("Z:/does/not/exist.zst", "Zstandard").is_err());
    }

    // -----------------------------------------------------------------------------------------
    // binary_info (CPE-1572): structured PE/ELF/Mach-O DTO
    // -----------------------------------------------------------------------------------------

    fn write_temp_binary_info(bytes: &[u8], tag: &str) -> std::path::PathBuf {
        let d = scratch(tag);
        let f = d.join("bin.dat");
        fs::write(&f, bytes).unwrap();
        f
    }

    /// Builds a minimal-but-real PE32 image with one section, one import, and one export — enough
    /// for goblin's `PE::parse` to succeed and populate every list `binary_info` reads.
    fn build_minimal_pe32() -> Vec<u8> {
        // ---- Optional header (standard COFF fields + a couple of data directories) ----
        // PE32 standard fields: magic(2) + 22 bytes, then Windows-specific fields, then data
        // directories (8 bytes each). We only need the Import and Export directory entries
        // (indices 0 and 1) to be present and correct; NumberOfRvaAndSizes covers just those two so
        // the optional header stays small.
        const NUM_RVA_AND_SIZES: u32 = 2;
        let opt_header_size: u16 = (28 + 68 + NUM_RVA_AND_SIZES as usize * 8) as u16;

        // Section layout: one ".text"-ish section holding both the import and export directory
        // tables back to back, at a fixed virtual address so RVAs are simple to compute.
        const SECTION_VA: u32 = 0x2000;
        const SECTION_FILE_OFF: u32 = 0x400; // arbitrary, page-ish aligned

        // --- Export directory table (goblin::pe::export) ---
        // ExportDirectoryTable(40 bytes) + one name-pointer(4) + one ordinal(2) + one address(4) +
        // the DLL name string + the exported symbol's name string.
        let export_dir_rva = SECTION_VA;
        let export_dll_name = b"test.dll\0";
        let export_dll_name_rva = export_dir_rva + 40 + 4 + 2 + 4;
        let export_sym_name = b"exported_fn\0";
        let export_sym_name_rva = export_dll_name_rva + export_dll_name.len() as u32;
        let export_addr_table_rva = export_dir_rva + 40;
        let export_name_ptr_table_rva = export_addr_table_rva + 4;
        let export_ordinal_table_rva = export_name_ptr_table_rva + 4;

        let mut export_dir = Vec::new();
        export_dir.extend_from_slice(&0u32.to_le_bytes()); // export_flags
        export_dir.extend_from_slice(&0u32.to_le_bytes()); // time_date_stamp
        export_dir.extend_from_slice(&0u16.to_le_bytes()); // major_version
        export_dir.extend_from_slice(&0u16.to_le_bytes()); // minor_version
        export_dir.extend_from_slice(&export_dll_name_rva.to_le_bytes()); // name_rva
        export_dir.extend_from_slice(&1u32.to_le_bytes()); // ordinal_base
        export_dir.extend_from_slice(&1u32.to_le_bytes()); // address_table_entries
        export_dir.extend_from_slice(&1u32.to_le_bytes()); // number_of_name_pointers
        export_dir.extend_from_slice(&export_addr_table_rva.to_le_bytes());
        export_dir.extend_from_slice(&export_name_ptr_table_rva.to_le_bytes());
        export_dir.extend_from_slice(&export_ordinal_table_rva.to_le_bytes());
        assert_eq!(export_dir.len(), 40);

        let text_rva = export_sym_name_rva + export_sym_name.len() as u32; // exported function body
        export_dir.extend_from_slice(&text_rva.to_le_bytes()); // export address table[0]
        export_dir.extend_from_slice(&export_name_ptr_table_rva.to_le_bytes()); // placeholder, fixed below
        // Fix up: name pointer table[0] must point at the symbol name, not itself.
        let fixup_at = export_dir.len() - 4;
        export_dir[fixup_at..fixup_at + 4].copy_from_slice(&export_sym_name_rva.to_le_bytes());
        export_dir.extend_from_slice(&0u16.to_le_bytes()); // ordinal table[0] (ordinal_base-relative)
        export_dir.extend_from_slice(export_dll_name);
        export_dir.extend_from_slice(export_sym_name);

        let export_dir_size = export_dir.len() as u32;

        // --- Import directory table (goblin::pe::import) ---
        // One IMAGE_IMPORT_DESCRIPTOR (20 bytes, all-zero terminator follows) + a 2-entry (1 real +
        // 1 null terminator) Import Lookup/Address Table + the imported name (hint(2) + name).
        let import_dir_rva = export_dir_rva + export_dir_size;
        // PE32 (32-bit) uses 4-byte ILT/IAT entries; each table holds one real entry + a null terminator.
        let ilt_rva = import_dir_rva + 20 * 2; // descriptor + null terminator
        let iat_rva = ilt_rva + 4 * 2;
        let import_name_rva = iat_rva + 4 * 2;
        let imported_lib_name_rva = import_name_rva + 2 + b"imported_fn\0".len() as u32;

        let mut import_dir = Vec::new();
        import_dir.extend_from_slice(&ilt_rva.to_le_bytes()); // import_lookup_table_rva
        import_dir.extend_from_slice(&0u32.to_le_bytes()); // time_date_stamp
        import_dir.extend_from_slice(&0u32.to_le_bytes()); // forwarder_chain
        import_dir.extend_from_slice(&imported_lib_name_rva.to_le_bytes()); // name_rva
        import_dir.extend_from_slice(&iat_rva.to_le_bytes()); // import_address_table_rva
        import_dir.extend_from_slice(&[0u8; 20]); // null terminator descriptor
        // ILT: one entry (RVA to hint/name, top bit clear = import-by-name) + null terminator.
        import_dir.extend_from_slice(&import_name_rva.to_le_bytes());
        import_dir.extend_from_slice(&0u32.to_le_bytes());
        // IAT: identical layout to the ILT before binding.
        import_dir.extend_from_slice(&import_name_rva.to_le_bytes());
        import_dir.extend_from_slice(&0u32.to_le_bytes());
        // Hint/Name table entry: hint(2) + name + NUL.
        import_dir.extend_from_slice(&0u16.to_le_bytes());
        import_dir.extend_from_slice(b"imported_fn\0");
        import_dir.extend_from_slice(b"imported.dll\0");

        let import_dir_size = import_dir.len() as u32;

        let mut section_data = export_dir;
        section_data.extend(import_dir);
        // Pad so the section has a little real "code" too (cosmetic only).
        section_data.extend_from_slice(&[0x90u8; 8]);

        // ---- Optional header ----
        let mut opt = Vec::new();
        opt.extend_from_slice(&0x010Bu16.to_le_bytes()); // magic: PE32
        opt.push(0); // major_linker_version
        opt.push(0); // minor_linker_version
        opt.extend_from_slice(&(section_data.len() as u32).to_le_bytes()); // size_of_code
        opt.extend_from_slice(&0u32.to_le_bytes()); // size_of_initialized_data
        opt.extend_from_slice(&0u32.to_le_bytes()); // size_of_uninitialized_data
        opt.extend_from_slice(&SECTION_VA.to_le_bytes()); // address_of_entry_point
        opt.extend_from_slice(&SECTION_VA.to_le_bytes()); // base_of_code
        opt.extend_from_slice(&(SECTION_VA + 0x1000).to_le_bytes()); // base_of_data (PE32 only)
        assert_eq!(opt.len(), 28);
        opt.extend_from_slice(&0x0040_0000u32.to_le_bytes()); // image_base
        opt.extend_from_slice(&0x1000u32.to_le_bytes()); // section_alignment
        opt.extend_from_slice(&0x0200u32.to_le_bytes()); // file_alignment
        opt.extend_from_slice(&6u16.to_le_bytes()); // major_os_version
        opt.extend_from_slice(&0u16.to_le_bytes()); // minor_os_version
        opt.extend_from_slice(&0u16.to_le_bytes()); // major_image_version
        opt.extend_from_slice(&0u16.to_le_bytes()); // minor_image_version
        opt.extend_from_slice(&6u16.to_le_bytes()); // major_subsystem_version
        opt.extend_from_slice(&0u16.to_le_bytes()); // minor_subsystem_version
        opt.extend_from_slice(&0u32.to_le_bytes()); // win32_version_value
        opt.extend_from_slice(&(SECTION_VA + 0x3000).to_le_bytes()); // size_of_image
        opt.extend_from_slice(&SECTION_FILE_OFF.to_le_bytes()); // size_of_headers
        opt.extend_from_slice(&0u32.to_le_bytes()); // checksum
        opt.extend_from_slice(&3u16.to_le_bytes()); // subsystem: console
        opt.extend_from_slice(&0u16.to_le_bytes()); // dll_characteristics
        opt.extend_from_slice(&0x0010_0000u32.to_le_bytes()); // size_of_stack_reserve
        opt.extend_from_slice(&0x0000_1000u32.to_le_bytes()); // size_of_stack_commit
        opt.extend_from_slice(&0x0010_0000u32.to_le_bytes()); // size_of_heap_reserve
        opt.extend_from_slice(&0x0000_1000u32.to_le_bytes()); // size_of_heap_commit
        opt.extend_from_slice(&0u32.to_le_bytes()); // loader_flags
        opt.extend_from_slice(&NUM_RVA_AND_SIZES.to_le_bytes()); // number_of_rva_and_sizes
        assert_eq!(opt.len(), 28 + 68);
        opt.extend_from_slice(&export_dir_rva.to_le_bytes()); // data directory 0: Export
        opt.extend_from_slice(&export_dir_size.to_le_bytes());
        opt.extend_from_slice(&import_dir_rva.to_le_bytes()); // data directory 1: Import
        opt.extend_from_slice(&import_dir_size.to_le_bytes());
        assert_eq!(opt.len(), opt_header_size as usize);

        // ---- Section table (one entry, ".data" holding both directory tables) ----
        let mut section_header = Vec::new();
        let mut name = [0u8; 8];
        name[..5].copy_from_slice(b".data");
        section_header.extend_from_slice(&name);
        section_header.extend_from_slice(&(section_data.len() as u32).to_le_bytes()); // virtual_size
        section_header.extend_from_slice(&SECTION_VA.to_le_bytes());
        section_header.extend_from_slice(&(section_data.len() as u32).to_le_bytes()); // size_of_raw_data
        section_header.extend_from_slice(&SECTION_FILE_OFF.to_le_bytes());
        section_header.extend_from_slice(&0u32.to_le_bytes()); // pointer_to_relocations
        section_header.extend_from_slice(&0u32.to_le_bytes()); // pointer_to_linenumbers
        section_header.extend_from_slice(&0u16.to_le_bytes()); // number_of_relocations
        section_header.extend_from_slice(&0u16.to_le_bytes()); // number_of_linenumbers
        section_header.extend_from_slice(&0xC000_0040u32.to_le_bytes()); // characteristics: initialized data, read/write
        assert_eq!(section_header.len(), 40);

        // ---- COFF header ----
        let mut coff = Vec::new();
        coff.extend_from_slice(&0x014Cu16.to_le_bytes()); // machine: I386
        coff.extend_from_slice(&1u16.to_le_bytes()); // number_of_sections
        coff.extend_from_slice(&0u32.to_le_bytes()); // time_date_stamp
        coff.extend_from_slice(&0u32.to_le_bytes()); // pointer_to_symbol_table
        coff.extend_from_slice(&0u32.to_le_bytes()); // number_of_symbols
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

        // Pad up to the section's file offset, then place the section's raw data there.
        out.resize(SECTION_FILE_OFF as usize, 0);
        out.extend_from_slice(&section_data);
        out
    }

    #[test]
    fn binary_info_reports_pe_sections_imports_and_exports() {
        let bytes = build_minimal_pe32();
        let path = write_temp_binary_info(&bytes, "pe");
        let info = binary_info(&path.to_string_lossy()).expect("a well-formed minimal PE must parse");
        assert_eq!(info.format, crate::model::BinaryFormat::Pe);
        assert!(!info.is_64, "this fixture is PE32, not PE32+");
        assert!(!info.sections.is_empty(), "expected at least one section: {info:?}");
        assert!(
            info.sections.iter().any(|s| s.name == ".data"),
            "expected the .data section: {info:?}"
        );
        assert!(
            info.imports.iter().any(|i| i.name == "imported_fn" && i.library.as_deref() == Some("imported.dll")),
            "expected the imported_fn import from imported.dll: {info:?}"
        );
        assert!(
            info.exports.iter().any(|e| e.name == "exported_fn"),
            "expected the exported_fn export: {info:?}"
        );
        assert!(info.symbols.is_empty(), "a PE EXE/DLL image carries no symbol table: {info:?}");
        let _ = fs::remove_file(&path);
    }

    /// Builds a minimal-but-real, statically-identity-mapped ELF64 x86-64 binary: one PT_LOAD
    /// segment covering the whole file (so every virtual address a `PT_DYNAMIC` entry names equals
    /// its own file offset — the standard trick for a tiny hand-built ELF fixture, since goblin
    /// resolves `DT_*` virtual addresses through the program headers), a `.symtab`/`.strtab` pair,
    /// and a dynamic section exposing one imported (undefined) and one exported (defined) symbol via
    /// `.dynsym`/`.dynstr` plus a `DT_HASH` table (`nchain` alone drives goblin's dynamic-symbol
    /// count when no relocations reference a symbol index).
    fn build_minimal_elf64() -> Vec<u8> {
        build_minimal_elf64_with_machine(0x3E) // EM_X86_64
    }

    /// Same fixture as [`build_minimal_elf64`], parameterized on `e_machine` (CPE-1581) so a
    /// non-x86 test double (e.g. AArch64) can reuse the same real, otherwise-valid ELF64 shape.
    fn build_minimal_elf64_with_machine(machine: u16) -> Vec<u8> {
        fn push_name(table: &mut Vec<u8>, s: &str) -> u32 {
            let off = table.len() as u32;
            table.extend_from_slice(s.as_bytes());
            table.push(0);
            off
        }
        fn sym64(name: u32, info: u8, shndx: u16, value: u64, size: u64) -> Vec<u8> {
            let mut b = Vec::with_capacity(24);
            b.extend_from_slice(&name.to_le_bytes());
            b.push(info);
            b.push(0); // st_other
            b.extend_from_slice(&shndx.to_le_bytes());
            b.extend_from_slice(&value.to_le_bytes());
            b.extend_from_slice(&size.to_le_bytes());
            b
        }
        // `flags`/`addr` default to 0 via the `shdr64` wrapper below for every section except
        // `.text`, which CPE-1581's disasm test needs marked `SHF_EXECINSTR`/`SHF_ALLOC` with a real
        // virtual address so `elf_code_section` (`binary_preview.rs`) can find and disassemble it.
        #[allow(clippy::too_many_arguments)] // test-only fixture builder, mirrors shdr64 below
        fn shdr64_ex(
            name: u32,
            typ: u32,
            offset: u64,
            size: u64,
            link: u32,
            info: u32,
            entsize: u64,
            flags: u64,
            addr: u64,
        ) -> Vec<u8> {
            let mut b = Vec::with_capacity(64);
            b.extend_from_slice(&name.to_le_bytes());
            b.extend_from_slice(&typ.to_le_bytes());
            b.extend_from_slice(&flags.to_le_bytes()); // sh_flags
            b.extend_from_slice(&addr.to_le_bytes()); // sh_addr
            b.extend_from_slice(&offset.to_le_bytes());
            b.extend_from_slice(&size.to_le_bytes());
            b.extend_from_slice(&link.to_le_bytes());
            b.extend_from_slice(&info.to_le_bytes());
            b.extend_from_slice(&1u64.to_le_bytes()); // sh_addralign
            b.extend_from_slice(&entsize.to_le_bytes());
            b
        }
        fn shdr64(name: u32, typ: u32, offset: u64, size: u64, link: u32, info: u32, entsize: u64) -> Vec<u8> {
            shdr64_ex(name, typ, offset, size, link, info, entsize, 0, 0)
        }
        fn phdr64(p_type: u32, offset: u64, filesz: u64, memsz: u64) -> Vec<u8> {
            let mut b = Vec::with_capacity(56);
            b.extend_from_slice(&p_type.to_le_bytes());
            b.extend_from_slice(&6u32.to_le_bytes()); // p_flags: R+W
            b.extend_from_slice(&offset.to_le_bytes()); // p_offset
            b.extend_from_slice(&offset.to_le_bytes()); // p_vaddr (identity-mapped)
            b.extend_from_slice(&offset.to_le_bytes()); // p_paddr
            b.extend_from_slice(&filesz.to_le_bytes());
            b.extend_from_slice(&memsz.to_le_bytes());
            b.extend_from_slice(&1u64.to_le_bytes()); // p_align
            b
        }

        const STB_GLOBAL: u8 = 1;
        const STT_FUNC: u8 = 2;
        let stinfo = |bind: u8, typ: u8| -> u8 { (bind << 4) | typ };

        let mut shstrtab: Vec<u8> = vec![0];
        let off_shstrtab = push_name(&mut shstrtab, ".shstrtab");
        let off_text = push_name(&mut shstrtab, ".text");
        let off_strtab = push_name(&mut shstrtab, ".strtab");
        let off_symtab = push_name(&mut shstrtab, ".symtab");

        let mut strtab: Vec<u8> = vec![0];
        let sym_name_off = push_name(&mut strtab, "local_fn");

        let mut dynstr: Vec<u8> = vec![0];
        let dyn_export_off = push_name(&mut dynstr, "exported_fn");
        let dyn_import_off = push_name(&mut dynstr, "imported_fn");

        // A real x86-64 function prologue/epilogue (push rbp; mov rbp,rsp; pop rbp; ret) rather than
        // plain NOPs, so CPE-1581's disasm test gets a substantive, non-hollow decode.
        let text: Vec<u8> = vec![0x55, 0x48, 0x89, 0xE5, 0x5D, 0xC3];

        let mut symtab_data = sym64(0, 0, 0, 0, 0); // mandatory null symbol
        symtab_data.extend(sym64(sym_name_off, stinfo(STB_GLOBAL, STT_FUNC), 1, 0x1000, 8));

        let mut dynsym_data = sym64(0, 0, 0, 0, 0); // index 0: mandatory null symbol
        dynsym_data.extend(sym64(dyn_export_off, stinfo(STB_GLOBAL, STT_FUNC), 1, 0x2000, 16)); // index 1: defined -> export
        dynsym_data.extend(sym64(dyn_import_off, stinfo(STB_GLOBAL, STT_FUNC), 0, 0, 0)); // index 2: undefined -> import

        let hash_data: Vec<u8> = {
            let mut h = Vec::new();
            h.extend_from_slice(&1u32.to_le_bytes()); // nbucket (unused by goblin's num_syms calc)
            h.extend_from_slice(&3u32.to_le_bytes()); // nchain: drives dynsyms' symbol count
            h
        };

        const EHDR_SIZE: u64 = 64;
        const PHDR_SIZE: u64 = 56;
        const PHNUM: u64 = 2;

        let mut offset = EHDR_SIZE + PHDR_SIZE * PHNUM;
        let shstrtab_off = offset;
        offset += shstrtab.len() as u64;
        let text_off = offset;
        offset += text.len() as u64;
        let strtab_off = offset;
        offset += strtab.len() as u64;
        let symtab_off = offset;
        offset += symtab_data.len() as u64;
        let dynstr_off = offset;
        offset += dynstr.len() as u64;
        let dynsym_off = offset;
        offset += dynsym_data.len() as u64;
        let hash_off = offset;
        offset += hash_data.len() as u64;

        // The `.dynamic` array itself (identity-mapped, so `d_val`s below double as file offsets).
        const DT_NULL: u64 = 0;
        const DT_HASH: u64 = 4;
        const DT_STRTAB: u64 = 5;
        const DT_SYMTAB: u64 = 6;
        const DT_STRSZ: u64 = 10;
        const DT_SYMENT: u64 = 11;
        let mut dynamic = Vec::new();
        for (tag, val) in [
            (DT_HASH, hash_off),
            (DT_STRTAB, dynstr_off),
            (DT_STRSZ, dynstr.len() as u64),
            (DT_SYMTAB, dynsym_off),
            (DT_SYMENT, 24),
            (DT_NULL, 0),
        ] {
            dynamic.extend_from_slice(&tag.to_le_bytes());
            dynamic.extend_from_slice(&val.to_le_bytes());
        }
        let dynamic_off = offset;
        offset += dynamic.len() as u64;

        let shoff = offset;

        const SHT_NULL: u32 = 0;
        const SHT_PROGBITS: u32 = 1;
        const SHT_SYMTAB: u32 = 2;
        const SHT_STRTAB: u32 = 3;
        let mut shdrs = Vec::new();
        shdrs.extend(shdr64(0, SHT_NULL, 0, 0, 0, 0, 0)); // index 0: mandatory null section
        shdrs.extend(shdr64(off_shstrtab, SHT_STRTAB, shstrtab_off, shstrtab.len() as u64, 0, 0, 0));
        const SHF_ALLOC_EXECINSTR: u64 = 0x6; // SHF_ALLOC (0x2) | SHF_EXECINSTR (0x4)
        shdrs.extend(shdr64_ex(
            off_text,
            SHT_PROGBITS,
            text_off,
            text.len() as u64,
            0,
            0,
            0,
            SHF_ALLOC_EXECINSTR,
            text_off, // identity-mapped, matching this fixture's own convention elsewhere
        ));
        shdrs.extend(shdr64(off_strtab, SHT_STRTAB, strtab_off, strtab.len() as u64, 0, 0, 0));
        shdrs.extend(shdr64(off_symtab, SHT_SYMTAB, symtab_off, symtab_data.len() as u64, 3, 1, 24));

        let mut ehdr = Vec::with_capacity(64);
        ehdr.extend_from_slice(&[0x7F, b'E', b'L', b'F']);
        ehdr.push(2); // EI_CLASS: 64-bit
        ehdr.push(1); // EI_DATA: little-endian
        ehdr.push(1); // EI_VERSION
        ehdr.push(0); // EI_OSABI
        ehdr.extend_from_slice(&[0u8; 8]); // EI_ABIVERSION + padding
        ehdr.extend_from_slice(&2u16.to_le_bytes()); // e_type: ET_EXEC
        ehdr.extend_from_slice(&machine.to_le_bytes()); // e_machine
        ehdr.extend_from_slice(&1u32.to_le_bytes()); // e_version
        ehdr.extend_from_slice(&0x1000u64.to_le_bytes()); // e_entry
        ehdr.extend_from_slice(&EHDR_SIZE.to_le_bytes()); // e_phoff
        ehdr.extend_from_slice(&shoff.to_le_bytes()); // e_shoff
        ehdr.extend_from_slice(&0u32.to_le_bytes()); // e_flags
        ehdr.extend_from_slice(&(EHDR_SIZE as u16).to_le_bytes()); // e_ehsize
        ehdr.extend_from_slice(&(PHDR_SIZE as u16).to_le_bytes()); // e_phentsize
        ehdr.extend_from_slice(&(PHNUM as u16).to_le_bytes()); // e_phnum
        ehdr.extend_from_slice(&64u16.to_le_bytes()); // e_shentsize
        ehdr.extend_from_slice(&5u16.to_le_bytes()); // e_shnum
        ehdr.extend_from_slice(&1u16.to_le_bytes()); // e_shstrndx
        assert_eq!(ehdr.len(), EHDR_SIZE as usize);

        // A single PT_LOAD covering a generous identity-mapped range, plus the PT_DYNAMIC pointing
        // at the `.dynamic` array built above.
        let mut phdrs = Vec::new();
        phdrs.extend(phdr64(1 /* PT_LOAD */, 0, 0x0010_0000, 0x0010_0000));
        phdrs.extend(phdr64(2 /* PT_DYNAMIC */, dynamic_off, dynamic.len() as u64, dynamic.len() as u64));
        assert_eq!(phdrs.len(), (PHDR_SIZE * PHNUM) as usize);

        let mut out = ehdr;
        out.extend(phdrs);
        out.extend_from_slice(&shstrtab);
        out.extend_from_slice(&text);
        out.extend_from_slice(&strtab);
        out.extend_from_slice(&symtab_data);
        out.extend_from_slice(&dynstr);
        out.extend_from_slice(&dynsym_data);
        out.extend_from_slice(&hash_data);
        out.extend_from_slice(&dynamic);
        out.extend_from_slice(&shdrs);
        out
    }

    #[test]
    fn binary_info_reports_elf_sections_imports_exports_and_symbols() {
        let bytes = build_minimal_elf64();
        let path = write_temp_binary_info(&bytes, "elf");
        let info = binary_info(&path.to_string_lossy()).expect("a well-formed minimal ELF64 must parse");
        assert_eq!(info.format, crate::model::BinaryFormat::Elf);
        assert!(info.is_64);
        assert!(
            info.sections.iter().any(|s| s.name == ".text"),
            "expected the .text section: {info:?}"
        );
        assert!(
            info.imports.iter().any(|i| i.name == "imported_fn"),
            "expected the undefined dynamic symbol as an import: {info:?}"
        );
        assert!(
            info.exports.iter().any(|e| e.name == "exported_fn" && e.address == Some(0x2000)),
            "expected the defined dynamic symbol as an export: {info:?}"
        );
        assert!(
            info.symbols.iter().any(|s| s.name == "local_fn" && s.address == Some(0x1000)),
            "expected the .symtab entry: {info:?}"
        );
        // CPE-1581: the `.text` section carries a real push-rbp/mov/pop-rbp/ret prologue+epilogue and
        // is marked SHF_EXECINSTR — `elf_binary_info` must find it via `elf_code_section` and
        // disassemble it, end to end through `binary_info`.
        assert_eq!(info.disasm.len(), 4, "expected 4 decoded instructions: {info:?}");
        assert!(
            info.disasm[0].text.to_lowercase().contains("push") && info.disasm[0].text.to_lowercase().contains("rbp"),
            "expected a push rbp: {:?}",
            info.disasm[0]
        );
        assert!(
            info.disasm.last().unwrap().text.to_lowercase().contains("ret"),
            "expected the last instruction to be a ret: {info:?}"
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn binary_info_leaves_disasm_empty_for_a_non_x86_architecture() {
        // Same fixture shape (real .text/SHF_EXECINSTR code section) but declaring AArch64
        // (EM_AARCH64 = 0xB7) instead of x86-64 — ARM disassembly is a future slice (yaxpeax-arm),
        // so `binary_info` must report an empty `disasm`, never an error, for it (CPE-1581).
        let bytes = build_minimal_elf64_with_machine(0xB7);
        let path = write_temp_binary_info(&bytes, "elf-arm64");
        let info = binary_info(&path.to_string_lossy()).expect("a well-formed ELF64 must still parse");
        assert!(info.disasm.is_empty(), "expected no disasm for a non-x86 architecture: {info:?}");
        let _ = fs::remove_file(&path);
    }

    /// Builds a minimal-but-real thin Mach-O 64-bit little-endian x86-64 executable: one
    /// `LC_SEGMENT_64` (`__TEXT`) with a single `__text` section, and one `LC_SYMTAB` naming one
    /// external defined symbol.
    fn build_minimal_macho64() -> Vec<u8> {
        const SECTION_ADDR: u64 = 0x1000;
        let text: Vec<u8> = vec![0x90; 8];

        let mut strtab: Vec<u8> = vec![0]; // index 0 is always the empty string
        let sym_name_off = strtab.len() as u32;
        strtab.extend_from_slice(b"my_symbol\0");

        const HEADER_SIZE: u64 = 32;
        const SEGCMD_SIZE: u64 = 72;
        const SECTION_SIZE: u64 = 80;
        const SYMTABCMD_SIZE: u64 = 24;
        let sizeofcmds = SEGCMD_SIZE + SECTION_SIZE + SYMTABCMD_SIZE;
        let load_cmds_end = HEADER_SIZE + sizeofcmds;

        let section_file_off = load_cmds_end;
        let symoff = section_file_off + text.len() as u64;
        let nsyms = 2u64;
        let nlist_size = 16u64;
        let stroff = symoff + nsyms * nlist_size;
        let total_len = stroff + strtab.len() as u64;

        let mut segname = [0u8; 16];
        segname[..6].copy_from_slice(b"__TEXT");
        let mut sectname = [0u8; 16];
        sectname[..6].copy_from_slice(b"__text");

        let mut seg = Vec::new();
        seg.extend_from_slice(&0x19u32.to_le_bytes()); // cmd: LC_SEGMENT_64
        seg.extend_from_slice(&((SEGCMD_SIZE + SECTION_SIZE) as u32).to_le_bytes()); // cmdsize
        seg.extend_from_slice(&segname);
        seg.extend_from_slice(&0u64.to_le_bytes()); // vmaddr
        seg.extend_from_slice(&total_len.to_le_bytes()); // vmsize
        seg.extend_from_slice(&0u64.to_le_bytes()); // fileoff
        seg.extend_from_slice(&total_len.to_le_bytes()); // filesize: must not exceed the real file length
        seg.extend_from_slice(&7u32.to_le_bytes()); // maxprot: rwx
        seg.extend_from_slice(&7u32.to_le_bytes()); // initprot: rwx
        seg.extend_from_slice(&1u32.to_le_bytes()); // nsects
        seg.extend_from_slice(&0u32.to_le_bytes()); // flags
        assert_eq!(seg.len(), SEGCMD_SIZE as usize);

        let mut sect = Vec::new();
        sect.extend_from_slice(&sectname);
        sect.extend_from_slice(&segname);
        sect.extend_from_slice(&SECTION_ADDR.to_le_bytes()); // addr
        sect.extend_from_slice(&(text.len() as u64).to_le_bytes()); // size
        sect.extend_from_slice(&(section_file_off as u32).to_le_bytes()); // offset
        sect.extend_from_slice(&0u32.to_le_bytes()); // align
        sect.extend_from_slice(&0u32.to_le_bytes()); // reloff
        sect.extend_from_slice(&0u32.to_le_bytes()); // nreloc
        sect.extend_from_slice(&0u32.to_le_bytes()); // flags
        sect.extend_from_slice(&0u32.to_le_bytes()); // reserved1
        sect.extend_from_slice(&0u32.to_le_bytes()); // reserved2
        sect.extend_from_slice(&0u32.to_le_bytes()); // reserved3
        assert_eq!(sect.len(), SECTION_SIZE as usize);

        let mut symtab_cmd = Vec::new();
        symtab_cmd.extend_from_slice(&0x2u32.to_le_bytes()); // cmd: LC_SYMTAB
        symtab_cmd.extend_from_slice(&(SYMTABCMD_SIZE as u32).to_le_bytes()); // cmdsize
        symtab_cmd.extend_from_slice(&(symoff as u32).to_le_bytes());
        symtab_cmd.extend_from_slice(&(nsyms as u32).to_le_bytes());
        symtab_cmd.extend_from_slice(&(stroff as u32).to_le_bytes());
        symtab_cmd.extend_from_slice(&(strtab.len() as u32).to_le_bytes());
        assert_eq!(symtab_cmd.len(), SYMTABCMD_SIZE as usize);

        // nlist_64: n_strx(4) n_type(1) n_sect(1) n_desc(2) n_value(8).
        const N_SECT: u8 = 0xe;
        const N_EXT: u8 = 0x1;
        let mut nlists = Vec::new();
        nlists.extend_from_slice(&0u32.to_le_bytes()); // index 0: unnamed (skipped by binary_info)
        nlists.push(0);
        nlists.push(0);
        nlists.extend_from_slice(&0u16.to_le_bytes());
        nlists.extend_from_slice(&0u64.to_le_bytes());
        nlists.extend_from_slice(&sym_name_off.to_le_bytes()); // index 1: "my_symbol"
        nlists.push(N_SECT | N_EXT);
        nlists.push(1); // n_sect: the one section
        nlists.extend_from_slice(&0u16.to_le_bytes());
        nlists.extend_from_slice(&SECTION_ADDR.to_le_bytes());
        assert_eq!(nlists.len(), (nsyms * nlist_size) as usize);

        let mut header = Vec::new();
        header.extend_from_slice(&[0xCF, 0xFA, 0xED, 0xFE]); // magic: 64-bit little-endian
        header.extend_from_slice(&(7u32 | 0x0100_0000).to_le_bytes()); // cputype: x86_64
        header.extend_from_slice(&3u32.to_le_bytes()); // cpusubtype
        header.extend_from_slice(&2u32.to_le_bytes()); // filetype: MH_EXECUTE
        header.extend_from_slice(&2u32.to_le_bytes()); // ncmds
        header.extend_from_slice(&(sizeofcmds as u32).to_le_bytes());
        header.extend_from_slice(&0u32.to_le_bytes()); // flags
        header.extend_from_slice(&0u32.to_le_bytes()); // reserved
        assert_eq!(header.len(), HEADER_SIZE as usize);

        let mut out = header;
        out.extend(seg);
        out.extend(sect);
        out.extend(symtab_cmd);
        out.extend_from_slice(&text);
        out.extend_from_slice(&nlists);
        out.extend_from_slice(&strtab);
        out
    }

    #[test]
    fn binary_info_reports_macho_sections_and_symbols() {
        let bytes = build_minimal_macho64();
        let path = write_temp_binary_info(&bytes, "macho");
        let info = binary_info(&path.to_string_lossy()).expect("a well-formed minimal Mach-O must parse");
        assert_eq!(info.format, crate::model::BinaryFormat::MachO);
        assert!(info.is_64);
        assert!(
            info.sections.iter().any(|s| s.name == "__text"),
            "expected the __text section: {info:?}"
        );
        assert!(
            info.symbols.iter().any(|s| s.name == "my_symbol"),
            "expected the named LC_SYMTAB entry: {info:?}"
        );
        // CPE-1581: the `__text` section is 8 real NOP bytes at an x86-64 cputype — `macho_binary_info`
        // must locate it (by name, within the segment/section walk) and disassemble it end to end.
        assert_eq!(info.disasm.len(), 8, "expected 8 decoded nop instructions: {info:?}");
        assert!(
            info.disasm.iter().all(|i| i.text.to_lowercase() == "nop"),
            "expected every decoded instruction to be a nop: {info:?}"
        );
        assert!(
            info.disasm.iter().all(|i| i.bytes == "90"),
            "expected every instruction's raw bytes to be a single 0x90: {info:?}"
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn binary_info_errors_gracefully_on_unrecognized_bytes() {
        let path = write_temp_binary_info(b"not a binary at all, just plain text", "junk");
        assert!(binary_info(&path.to_string_lossy()).is_err());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn binary_info_errors_on_a_missing_file() {
        assert!(binary_info("Z:/does/not/exist.exe").is_err());
    }

    #[test]
    fn binary_info_never_panics_on_truncated_real_fixtures() {
        // Truncate each real, otherwise-valid fixture at every prefix length — a cheap, targeted
        // panic-safety sweep (the broader adversarial battery lives in
        // `tests/binary_data_preview_panic_safety.rs`).
        for (tag, bytes) in [
            ("pe-trunc", build_minimal_pe32()),
            ("elf-trunc", build_minimal_elf64()),
            ("macho-trunc", build_minimal_macho64()),
        ] {
            for cut in (0..bytes.len()).step_by(7) {
                let path = write_temp_binary_info(&bytes[..cut], tag);
                let _ = binary_info(&path.to_string_lossy()); // must not panic, Ok or Err both fine
                let _ = fs::remove_file(&path);
            }
        }
    }
}
