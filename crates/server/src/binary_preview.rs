//! Structured binary previews (CPE-210/214/215/216/218): human-readable text summaries of binary files
//! for the preview pane's read-only "info" provider — a hex dump, a PE (EXE/DLL) header summary, a MIDI
//! track summary, a wasm→WAT disassembly, and a `.torrent` metadata summary. Pure-Rust deps (goblin /
//! midly / wasmprinter / serde_bencode), no system libs; extracted into the Server (CPE-815). The Tauri
//! `read_preview_info` command dispatches per-extension into these.

use std::fs;

/// Classic hex + ASCII dump of the first `max` bytes (CPE-214). Reads at most `max` bytes so a multi-GB
/// binary is never slurped into memory (CPE-633).
pub fn hex_dump(path: &str, max: usize) -> Result<String, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(max as u64)
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
    if bytes.len() > n {
        out.push_str(&format!("\n… {} more bytes (showing first {n}).\n", bytes.len() - n));
    }
    Ok(out)
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
        let _ = fs::remove_dir_all(&d);
    }
}
