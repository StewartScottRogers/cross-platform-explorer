//! Binary CPU-architecture detection (CPE-1485, epic CPE-1000): given the leading bytes of a
//! detected ELF, PE, or Mach-O binary (see [`crate::file_type`]), parse the small fixed header
//! fields that carry the target CPU architecture and report a normalized [`Arch`] (plus bitness
//! and endianness where the format has one).
//!
//! Pure over an in-memory byte slice — no filesystem I/O, no new dependencies. A handful of
//! byte-offset reads and match tables, mirroring [`crate::file_type::detect_type`]'s style.
//! Bounds-checked throughout: too-short or garbage input returns `None`, never panics.

/// A normalized CPU architecture, decoded from an ELF `e_machine`, PE COFF `Machine`, or Mach-O
/// `cputype` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X86,
    X86_64,
    Arm,
    Arm64,
    RiscV,
    RiscV64,
    Mips,
    PowerPc,
    PowerPc64,
    Sparc,
    /// A recognised binary header whose machine/cputype code isn't one of the architectures above.
    /// Reported rather than falling back to `None` so bitness/endianness are still surfaced for an
    /// architecture this sniffer doesn't yet special-case.
    Unknown,
}

impl Arch {
    /// A short, human-readable label (e.g. for display in the Properties panel).
    pub fn label(self) -> &'static str {
        match self {
            Arch::X86 => "x86",
            Arch::X86_64 => "x86-64",
            Arch::Arm => "ARM",
            Arch::Arm64 => "ARM64",
            Arch::RiscV => "RISC-V",
            Arch::RiscV64 => "RISC-V 64",
            Arch::Mips => "MIPS",
            Arch::PowerPc => "PowerPC",
            Arch::PowerPc64 => "PowerPC64",
            Arch::Sparc => "SPARC",
            Arch::Unknown => "Unknown",
        }
    }
}

/// 32-bit vs. 64-bit, where the container format encodes it directly (ELF, Mach-O). PE encodes
/// bitness only implicitly via the machine type, so PE arch info leaves this `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bitness {
    Bit32,
    Bit64,
}

/// Byte order, where the container format is explicitly bi-endian (ELF; Mach-O fat headers). PE
/// is always little-endian by spec, so PE arch info leaves this `None` (nothing to report).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endian {
    Little,
    Big,
}

/// One decoded architecture slice: the CPU architecture plus bitness/endianness where the format
/// carries them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchInfo {
    pub arch: Arch,
    pub bitness: Option<Bitness>,
    pub endian: Option<Endian>,
}

/// The result of architecture detection: a single architecture for ELF/PE/thin Mach-O, or a list
/// of slices for a Mach-O fat/universal binary (which embeds one full binary per architecture).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinaryArch {
    Single(ArchInfo),
    /// Mach-O fat/universal binary — one entry per contained architecture slice, in header order.
    /// Empty only if the fat header claims zero slices (malformed but not garbage enough to fail
    /// the magic check); callers should treat that the same as "nothing usable was found".
    Fat(Vec<ArchInfo>),
}

fn read_u16(bytes: &[u8], offset: usize, big_endian: bool) -> Option<u16> {
    let b = bytes.get(offset..offset + 2)?;
    Some(if big_endian { u16::from_be_bytes([b[0], b[1]]) } else { u16::from_le_bytes([b[0], b[1]]) })
}

fn read_u32(bytes: &[u8], offset: usize, big_endian: bool) -> Option<u32> {
    let b = bytes.get(offset..offset + 4)?;
    Some(if big_endian {
        u32::from_be_bytes([b[0], b[1], b[2], b[3]])
    } else {
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    })
}

// ---- ELF ----
//
// Layout (e_ident is the first 16 bytes of every ELF header, regardless of class):
//   [0..4]   magic: 7F 45 4C 46 ("\x7FELF")
//   [4]      EI_CLASS: 1 = 32-bit, 2 = 64-bit
//   [5]      EI_DATA:  1 = little-endian, 2 = big-endian
//   ...
//   [16..18] e_type (2 bytes, endianness per EI_DATA)
//   [18..20] e_machine (2 bytes, endianness per EI_DATA) — the field we want.

/// ELF `e_machine` values we recognise (a small curated subset of the full registry).
fn elf_machine_to_arch(machine: u16) -> Arch {
    match machine {
        0x03 => Arch::X86,        // EM_386
        0x3E => Arch::X86_64,     // EM_X86_64
        0x28 => Arch::Arm,        // EM_ARM
        0xB7 => Arch::Arm64,      // EM_AARCH64
        0xF3 => Arch::RiscV,      // EM_RISCV (bitness carried separately by EI_CLASS)
        0x08 => Arch::Mips,       // EM_MIPS
        0x14 => Arch::PowerPc,    // EM_PPC
        0x15 => Arch::PowerPc64,  // EM_PPC64
        0x02 => Arch::Sparc,      // EM_SPARC
        _ => Arch::Unknown,
    }
}

fn detect_elf(bytes: &[u8]) -> Option<BinaryArch> {
    let class = *bytes.get(4)?;
    let data = *bytes.get(5)?;
    let bitness = match class {
        1 => Some(Bitness::Bit32),
        2 => Some(Bitness::Bit64),
        _ => None,
    };
    let big_endian = match data {
        1 => false,
        2 => true,
        _ => return None, // neither valid EI_DATA value — too garbled to trust e_machine's endianness
    };
    let endian = Some(if big_endian { Endian::Big } else { Endian::Little });
    let machine = read_u16(bytes, 18, big_endian)?;
    let arch = elf_machine_to_arch(machine);
    // RISC-V doesn't have a distinct 64-bit e_machine code — EI_CLASS is what carries bitness.
    let arch = if arch == Arch::RiscV && bitness == Some(Bitness::Bit64) { Arch::RiscV64 } else { arch };
    Some(BinaryArch::Single(ArchInfo { arch, bitness, endian }))
}

// ---- PE ----
//
// The DOS stub starts "MZ"; offset 0x3C holds a little-endian u32 pointing to the PE header,
// which is tagged "PE\0\0" followed immediately by the COFF header's 2-byte little-endian
// `Machine` field.

fn pe_machine_to_arch(machine: u16) -> Arch {
    match machine {
        0x8664 => Arch::X86_64, // IMAGE_FILE_MACHINE_AMD64
        0x014C => Arch::X86,    // IMAGE_FILE_MACHINE_I386
        0xAA64 => Arch::Arm64,  // IMAGE_FILE_MACHINE_ARM64
        0x01C0 | 0x01C4 => Arch::Arm, // IMAGE_FILE_MACHINE_ARM / ARMNT
        _ => Arch::Unknown,
    }
}

fn detect_pe(bytes: &[u8]) -> Option<BinaryArch> {
    let pe_offset = read_u32(bytes, 0x3C, false)? as usize;
    if bytes.get(pe_offset..pe_offset + 4)? != b"PE\0\0" {
        return None;
    }
    let machine = read_u16(bytes, pe_offset + 4, false)?;
    let arch = pe_machine_to_arch(machine);
    // PE is always little-endian by spec; bitness is implied by the machine type, not encoded
    // separately in the COFF header, so neither is reported explicitly here.
    Some(BinaryArch::Single(ArchInfo { arch, bitness: None, endian: None }))
}

// ---- Mach-O ----
//
// Thin (single-architecture) magics:
//   FE ED FA CE — 32-bit, big-endian byte order in the file
//   CE FA ED FE — 32-bit, little-endian byte order in the file (i.e. host-native on x86/ARM)
//   FE ED FA CF — 64-bit, big-endian
//   CF FA ED FE — 64-bit, little-endian
// followed immediately by a mach_header(_64): magic(4) cputype(4) cpusubtype(4) ...
//
// Fat/universal magics:
//   CA FE BA BE — big-endian slice count + fat_arch entries (the common case: `lipo`-built)
//   BE BA FE CA — little-endian variant (rare; some big-endian host toolchains)
// followed by a big-endian (or little-endian, per magic) u32 `nfat_arch`, then that many
// fat_arch entries of cputype(4) cpusubtype(4) offset(4) size(4) align(4) — 20 bytes each.

/// Mach-O `cputype` values (the low bits identify the family; the 64-bit ABI variant sets the
/// `CPU_ARCH_ABI64` bit, 0x0100_0000, on top of the 32-bit type).
fn macho_cputype_to_arch(cputype: u32) -> Arch {
    const ABI64: u32 = 0x0100_0000;
    match cputype {
        7 => Arch::X86,                        // CPU_TYPE_X86 / I386
        c if c == (7 | ABI64) => Arch::X86_64,  // CPU_TYPE_X86_64
        12 => Arch::Arm,                        // CPU_TYPE_ARM
        c if c == (12 | ABI64) => Arch::Arm64,  // CPU_TYPE_ARM64
        18 => Arch::PowerPc,                    // CPU_TYPE_POWERPC
        c if c == (18 | ABI64) => Arch::PowerPc64, // CPU_TYPE_POWERPC64
        _ => Arch::Unknown,
    }
}

/// Cap on the number of fat-binary slices we'll parse, guarding against a malformed/hostile
/// `nfat_arch` (a 4-byte field could otherwise claim billions of 20-byte entries) while staying
/// well above any real-world universal binary (Apple's own tooling tops out at a handful).
const MAX_FAT_SLICES: u32 = 64;

fn detect_macho_thin(bytes: &[u8], magic: [u8; 4]) -> Option<BinaryArch> {
    let (bitness, big_endian) = match magic {
        [0xFE, 0xED, 0xFA, 0xCE] => (Bitness::Bit32, true),
        [0xCE, 0xFA, 0xED, 0xFE] => (Bitness::Bit32, false),
        [0xFE, 0xED, 0xFA, 0xCF] => (Bitness::Bit64, true),
        [0xCF, 0xFA, 0xED, 0xFE] => (Bitness::Bit64, false),
        _ => return None,
    };
    let cputype = read_u32(bytes, 4, big_endian)?;
    let arch = macho_cputype_to_arch(cputype);
    let endian = Some(if big_endian { Endian::Big } else { Endian::Little });
    Some(BinaryArch::Single(ArchInfo { arch, bitness: Some(bitness), endian }))
}

fn detect_macho_fat(bytes: &[u8], big_endian: bool) -> Option<BinaryArch> {
    let nfat = read_u32(bytes, 4, big_endian)?.min(MAX_FAT_SLICES);
    let mut slices = Vec::with_capacity(nfat as usize);
    for i in 0..nfat {
        let entry_off = 8 + (i as usize) * 20;
        let cputype = read_u32(bytes, entry_off, big_endian)?;
        // A fat_arch's cputype/cpusubtype are themselves stored as a full mach-o cputype value
        // (including the ABI64 bit for 64-bit slices), and it carries no separate byte-order flag
        // of its own — each contained slice's *own* byte order is determined by its own thin
        // mach_header at `offset`, which we don't walk into here (out of scope: this reports which
        // architectures are present, not each slice's individual endianness).
        let arch = macho_cputype_to_arch(cputype);
        let bitness = Some(if cputype & 0x0100_0000 != 0 { Bitness::Bit64 } else { Bitness::Bit32 });
        slices.push(ArchInfo { arch, bitness, endian: None });
    }
    Some(BinaryArch::Fat(slices))
}

fn detect_macho(bytes: &[u8]) -> Option<BinaryArch> {
    let magic: [u8; 4] = bytes.get(0..4)?.try_into().ok()?;
    match magic {
        [0xCA, 0xFE, 0xBA, 0xBE] => detect_macho_fat(bytes, true),
        [0xBE, 0xBA, 0xFE, 0xCA] => detect_macho_fat(bytes, false),
        _ => detect_macho_thin(bytes, magic),
    }
}

/// Detect the CPU architecture(s) encoded in `bytes`' ELF, PE, or Mach-O header. `bytes` need only
/// be a bounded leading prefix of the file (a few hundred bytes is enough for every format handled
/// here, including a fat Mach-O with up to [`MAX_FAT_SLICES`] slices). Returns `None` for input
/// that isn't one of these three formats, or that is too short / too garbled to parse; never
/// panics regardless of length (including empty).
///
/// Note: unlike [`crate::file_type::detect_type`], this does not itself sniff which *container*
/// format `bytes` is — callers that already know the format (via `detect_type`) can call the
/// format straight through; this function re-checks the same leading magic itself so it is safe
/// to call directly on arbitrary bytes too.
pub fn detect_arch(bytes: &[u8]) -> Option<BinaryArch> {
    if bytes.len() >= 4 && bytes[0..4] == [0x7F, 0x45, 0x4C, 0x46] {
        return detect_elf(bytes);
    }
    if bytes.len() >= 2 && bytes[0..2] == [0x4D, 0x5A] {
        return detect_pe(bytes);
    }
    detect_macho(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- ELF ----

    fn elf_header(class: u8, data: u8, machine: u16) -> Vec<u8> {
        let mut b = vec![0x7F, 0x45, 0x4C, 0x46, class, data, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        // e_type (2 bytes) — irrelevant to detection, zero-filled.
        b.extend_from_slice(&[0, 0]);
        let big_endian = data == 2;
        if big_endian {
            b.extend_from_slice(&machine.to_be_bytes());
        } else {
            b.extend_from_slice(&machine.to_le_bytes());
        }
        b
    }

    #[test]
    fn detects_x86_64_elf_little_endian() {
        let bytes = elf_header(2, 1, 0x3E);
        let got = detect_arch(&bytes).expect("must detect");
        assert_eq!(
            got,
            BinaryArch::Single(ArchInfo {
                arch: Arch::X86_64,
                bitness: Some(Bitness::Bit64),
                endian: Some(Endian::Little)
            })
        );
    }

    #[test]
    fn detects_arm64_elf() {
        let bytes = elf_header(2, 1, 0xB7);
        let got = detect_arch(&bytes).expect("must detect");
        assert_eq!(
            got,
            BinaryArch::Single(ArchInfo {
                arch: Arch::Arm64,
                bitness: Some(Bitness::Bit64),
                endian: Some(Endian::Little)
            })
        );
    }

    #[test]
    fn detects_32bit_x86_elf() {
        let bytes = elf_header(1, 1, 0x03);
        let got = detect_arch(&bytes).expect("must detect");
        assert_eq!(
            got,
            BinaryArch::Single(ArchInfo {
                arch: Arch::X86,
                bitness: Some(Bitness::Bit32),
                endian: Some(Endian::Little)
            })
        );
    }

    #[test]
    fn detects_big_endian_elf() {
        // A big-endian 32-bit ARM ELF (e.g. some embedded targets) — e_machine must be read
        // big-endian per EI_DATA, not little-endian.
        let bytes = elf_header(1, 2, 0x28);
        let got = detect_arch(&bytes).expect("must detect");
        assert_eq!(
            got,
            BinaryArch::Single(ArchInfo {
                arch: Arch::Arm,
                bitness: Some(Bitness::Bit32),
                endian: Some(Endian::Big)
            })
        );
    }

    #[test]
    fn detects_riscv_32_and_64_via_ei_class() {
        let bytes32 = elf_header(1, 1, 0xF3);
        assert_eq!(
            detect_arch(&bytes32),
            Some(BinaryArch::Single(ArchInfo {
                arch: Arch::RiscV,
                bitness: Some(Bitness::Bit32),
                endian: Some(Endian::Little)
            }))
        );
        let bytes64 = elf_header(2, 1, 0xF3);
        assert_eq!(
            detect_arch(&bytes64),
            Some(BinaryArch::Single(ArchInfo {
                arch: Arch::RiscV64,
                bitness: Some(Bitness::Bit64),
                endian: Some(Endian::Little)
            }))
        );
    }

    #[test]
    fn unrecognised_elf_machine_is_unknown_not_none() {
        let bytes = elf_header(2, 1, 0xBEEF);
        let got = detect_arch(&bytes).expect("a well-formed ELF header must still detect");
        assert_eq!(got, BinaryArch::Single(ArchInfo {
            arch: Arch::Unknown,
            bitness: Some(Bitness::Bit64),
            endian: Some(Endian::Little)
        }));
    }

    #[test]
    fn elf_header_shorter_than_e_machine_is_none_not_a_panic() {
        for n in 0..20 {
            let bytes = elf_header(2, 1, 0x3E);
            assert_eq!(detect_arch(&bytes[..n]), None, "len {n}");
        }
    }

    // ---- PE ----

    /// Builds a minimal PE image: MZ stub + e_lfanew pointer at 0x3C + "PE\0\0" + Machine field.
    fn pe_bytes(machine: u16) -> Vec<u8> {
        let pe_offset: u32 = 0x80;
        let mut b = vec![0u8; pe_offset as usize + 6];
        b[0] = 0x4D;
        b[1] = 0x5A;
        b[0x3C..0x40].copy_from_slice(&pe_offset.to_le_bytes());
        b[pe_offset as usize..pe_offset as usize + 4].copy_from_slice(b"PE\0\0");
        b[pe_offset as usize + 4..pe_offset as usize + 6].copy_from_slice(&machine.to_le_bytes());
        b
    }

    #[test]
    fn detects_pe_x86_64() {
        let bytes = pe_bytes(0x8664);
        assert_eq!(
            detect_arch(&bytes),
            Some(BinaryArch::Single(ArchInfo { arch: Arch::X86_64, bitness: None, endian: None }))
        );
    }

    #[test]
    fn detects_pe_x86() {
        let bytes = pe_bytes(0x014C);
        assert_eq!(
            detect_arch(&bytes),
            Some(BinaryArch::Single(ArchInfo { arch: Arch::X86, bitness: None, endian: None }))
        );
    }

    #[test]
    fn detects_pe_arm64_and_arm() {
        assert_eq!(
            detect_arch(&pe_bytes(0xAA64)),
            Some(BinaryArch::Single(ArchInfo { arch: Arch::Arm64, bitness: None, endian: None }))
        );
        assert_eq!(
            detect_arch(&pe_bytes(0x01C0)),
            Some(BinaryArch::Single(ArchInfo { arch: Arch::Arm, bitness: None, endian: None }))
        );
        assert_eq!(
            detect_arch(&pe_bytes(0x01C4)),
            Some(BinaryArch::Single(ArchInfo { arch: Arch::Arm, bitness: None, endian: None }))
        );
    }

    #[test]
    fn mz_stub_without_a_valid_pe_signature_is_none() {
        // MZ header whose e_lfanew points at garbage (not "PE\0\0") — a real DOS-only executable,
        // or a truncated/corrupted PE. Must not panic or misreport.
        let mut b = vec![0u8; 0x44];
        b[0] = 0x4D;
        b[1] = 0x5A;
        b[0x3C..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        b[0x40..0x44].copy_from_slice(b"JUNK");
        assert_eq!(detect_arch(&b), None);
    }

    #[test]
    fn mz_stub_truncated_before_e_lfanew_is_none_not_a_panic() {
        for n in 0..0x3E {
            let bytes = pe_bytes(0x8664);
            assert_eq!(detect_arch(&bytes[..n]), None, "len {n}");
        }
    }

    #[test]
    fn pe_header_offset_beyond_buffer_is_none_not_a_panic() {
        // e_lfanew points far past the end of the (short) buffer we actually have.
        let mut b = vec![0u8; 0x40];
        b[0] = 0x4D;
        b[1] = 0x5A;
        b[0x3C..0x40].copy_from_slice(&0xFFFF_FF00u32.to_le_bytes());
        assert_eq!(detect_arch(&b), None);
    }

    // ---- Mach-O thin ----

    fn macho_thin_bytes(magic: [u8; 4], big_endian: bool, cputype: u32) -> Vec<u8> {
        let mut b = magic.to_vec();
        if big_endian {
            b.extend_from_slice(&cputype.to_be_bytes());
        } else {
            b.extend_from_slice(&cputype.to_le_bytes());
        }
        b.extend_from_slice(&[0, 0, 0, 0]); // cpusubtype, irrelevant to arch detection
        b
    }

    #[test]
    fn detects_macho_thin_64bit_little_endian_x86_64() {
        let bytes = macho_thin_bytes([0xCF, 0xFA, 0xED, 0xFE], false, 7 | 0x0100_0000);
        assert_eq!(
            detect_arch(&bytes),
            Some(BinaryArch::Single(ArchInfo {
                arch: Arch::X86_64,
                bitness: Some(Bitness::Bit64),
                endian: Some(Endian::Little)
            }))
        );
    }

    #[test]
    fn detects_macho_thin_64bit_little_endian_arm64() {
        let bytes = macho_thin_bytes([0xCF, 0xFA, 0xED, 0xFE], false, 12 | 0x0100_0000);
        assert_eq!(
            detect_arch(&bytes),
            Some(BinaryArch::Single(ArchInfo {
                arch: Arch::Arm64,
                bitness: Some(Bitness::Bit64),
                endian: Some(Endian::Little)
            }))
        );
    }

    #[test]
    fn detects_macho_thin_32bit_big_endian() {
        let bytes = macho_thin_bytes([0xFE, 0xED, 0xFA, 0xCE], true, 7);
        assert_eq!(
            detect_arch(&bytes),
            Some(BinaryArch::Single(ArchInfo {
                arch: Arch::X86,
                bitness: Some(Bitness::Bit32),
                endian: Some(Endian::Big)
            }))
        );
    }

    #[test]
    fn detects_macho_thin_32bit_little_endian() {
        let bytes = macho_thin_bytes([0xCE, 0xFA, 0xED, 0xFE], false, 12);
        assert_eq!(
            detect_arch(&bytes),
            Some(BinaryArch::Single(ArchInfo {
                arch: Arch::Arm,
                bitness: Some(Bitness::Bit32),
                endian: Some(Endian::Little)
            }))
        );
    }

    #[test]
    fn macho_thin_header_shorter_than_cputype_is_none_not_a_panic() {
        let bytes = macho_thin_bytes([0xCF, 0xFA, 0xED, 0xFE], false, 7 | 0x0100_0000);
        for n in 0..8 {
            assert_eq!(detect_arch(&bytes[..n]), None, "len {n}");
        }
    }

    // ---- Mach-O fat/universal ----

    fn macho_fat_bytes(slices: &[(u32, u32)]) -> Vec<u8> {
        // Big-endian fat header — the common `lipo`-built form.
        let mut b = vec![0xCA, 0xFE, 0xBA, 0xBE];
        b.extend_from_slice(&(slices.len() as u32).to_be_bytes());
        for &(cputype, cpusubtype) in slices {
            b.extend_from_slice(&cputype.to_be_bytes());
            b.extend_from_slice(&cpusubtype.to_be_bytes());
            b.extend_from_slice(&0u32.to_be_bytes()); // offset
            b.extend_from_slice(&0u32.to_be_bytes()); // size
            b.extend_from_slice(&0u32.to_be_bytes()); // align
        }
        b
    }

    #[test]
    fn detects_fat_macho_with_two_slices() {
        let bytes = macho_fat_bytes(&[(7 | 0x0100_0000, 3), (12 | 0x0100_0000, 0)]);
        let got = detect_arch(&bytes).expect("must detect");
        assert_eq!(
            got,
            BinaryArch::Fat(vec![
                ArchInfo { arch: Arch::X86_64, bitness: Some(Bitness::Bit64), endian: None },
                ArchInfo { arch: Arch::Arm64, bitness: Some(Bitness::Bit64), endian: None },
            ])
        );
    }

    #[test]
    fn detects_fat_macho_little_endian_variant() {
        let mut b = vec![0xBE, 0xBA, 0xFE, 0xCA];
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&(7u32).to_le_bytes()); // cputype: x86 (32-bit)
        b.extend_from_slice(&0u32.to_le_bytes()); // cpusubtype
        b.extend_from_slice(&[0u8; 12]); // offset, size, align
        assert_eq!(
            detect_arch(&b),
            Some(BinaryArch::Fat(vec![ArchInfo {
                arch: Arch::X86,
                bitness: Some(Bitness::Bit32),
                endian: None
            }]))
        );
    }

    #[test]
    fn fat_macho_slice_count_is_capped_against_a_hostile_header() {
        // nfat_arch claims far more slices than the buffer could possibly contain — must not try
        // to read past the end (would return None on the first out-of-range read) and must never
        // allocate based on the unclamped claimed count.
        let mut b = vec![0xCA, 0xFE, 0xBA, 0xBE];
        b.extend_from_slice(&u32::MAX.to_be_bytes());
        // No slice data follows — first read_u32 for entry 0's cputype fails.
        assert_eq!(detect_arch(&b), None);
    }

    #[test]
    fn fat_macho_header_shorter_than_nfat_arch_is_none_not_a_panic() {
        for n in 0..8 {
            let bytes = macho_fat_bytes(&[(7, 0)]);
            assert_eq!(detect_arch(&bytes[..n]), None, "len {n}");
        }
    }

    #[test]
    fn fat_macho_with_zero_slices_is_an_empty_list_not_none() {
        let bytes = macho_fat_bytes(&[]);
        assert_eq!(detect_arch(&bytes), Some(BinaryArch::Fat(vec![])));
    }

    // ---- non-binary / garbage / too-short input ----

    #[test]
    fn non_binary_bytes_are_none() {
        assert_eq!(detect_arch(b"just some plain text, not a binary format"), None);
    }

    #[test]
    fn empty_input_is_none_not_a_panic() {
        assert_eq!(detect_arch(&[]), None);
    }

    #[test]
    fn one_byte_input_is_none_not_a_panic() {
        assert_eq!(detect_arch(&[0x7F]), None);
        assert_eq!(detect_arch(&[0x4D]), None);
        assert_eq!(detect_arch(&[0xCA]), None);
    }

    #[test]
    fn short_input_shorter_than_every_magic_never_panics() {
        for n in 0..8 {
            let bytes = vec![0xFFu8; n];
            let _ = detect_arch(&bytes); // must not panic regardless of outcome
        }
    }

    // ---- Arch::label() sanity ----

    #[test]
    fn arch_labels_are_human_readable() {
        assert_eq!(Arch::X86.label(), "x86");
        assert_eq!(Arch::X86_64.label(), "x86-64");
        assert_eq!(Arch::Arm.label(), "ARM");
        assert_eq!(Arch::Arm64.label(), "ARM64");
        assert_eq!(Arch::RiscV.label(), "RISC-V");
        assert_eq!(Arch::RiscV64.label(), "RISC-V 64");
        assert_eq!(Arch::Unknown.label(), "Unknown");
    }
}
