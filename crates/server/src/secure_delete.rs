//! Secure-delete pass planner (CPE-941, epic CPE-738): given a file and an overwrite scheme, compute the
//! sequence of overwrite passes the shred engine will run, plus **honest, platform-aware caveats** about
//! when overwriting can't actually guarantee erasure (SSD wear-levelling, copy-on-write filesystems,
//! snapshots). Pure planning: no filesystem writes; the engine executes the returned passes.

/// What a single overwrite pass writes across the file's bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "pattern", rename_all = "snake_case")]
pub enum PassPattern {
    /// All 0x00.
    Zeros,
    /// All 0xFF.
    Ones,
    /// Cryptographically-random bytes (the engine supplies the RNG).
    Random,
    /// A fixed byte value (used by the multi-pass schemes).
    Byte { value: u8 },
}

/// An overwrite scheme — how many passes and with what patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShredScheme {
    /// One zero pass — fast; fine for most needs on spinning disks.
    Zero,
    /// One random pass.
    Random,
    /// DoD 5220.22-M style: zeros, ones, then random (3 passes).
    Dod3,
    /// A Gutmann-lite spread (7 passes): random bookends around fixed byte patterns.
    Gutmann,
}

/// The passes a scheme runs, in order.
pub fn passes(scheme: ShredScheme) -> Vec<PassPattern> {
    use PassPattern::*;
    match scheme {
        ShredScheme::Zero => vec![Zeros],
        ShredScheme::Random => vec![Random],
        ShredScheme::Dod3 => vec![Zeros, Ones, Random],
        ShredScheme::Gutmann => vec![
            Random,
            Byte { value: 0x55 },
            Byte { value: 0xAA },
            Byte { value: 0x92 },
            Byte { value: 0x49 },
            Byte { value: 0x24 },
            Random,
        ],
    }
}

/// A full shred plan for one file: the passes to run + the honest caveats for the target medium.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ShredPlan {
    pub path: String,
    pub size_bytes: u64,
    pub scheme: ShredScheme,
    pub passes: Vec<PassPattern>,
    /// Total bytes the engine will write (size × pass count).
    pub total_write_bytes: u64,
    /// Plain-language limits the user must know — never claim more than overwriting can deliver.
    pub caveats: Vec<String>,
}

/// Plan a secure delete. `on_ssd` and `copy_on_write` gate the honest caveats: on flash / CoW media,
/// in-place overwrite does **not** reliably reach the original blocks, so we say so and point at the real
/// remedies (full-disk encryption, TRIM, per-file encrypted vaults).
pub fn plan_shred(
    path: &str,
    size_bytes: u64,
    scheme: ShredScheme,
    on_ssd: bool,
    copy_on_write: bool,
) -> ShredPlan {
    let ps = passes(scheme);
    let total_write_bytes = size_bytes.saturating_mul(ps.len() as u64);

    let mut caveats = Vec::new();
    if on_ssd {
        caveats.push(
            "This is an SSD/flash device: wear-levelling remaps writes, so overwriting can't guarantee the \
             original cells are erased. Prefer full-disk encryption + a secure TRIM, or store secrets in an \
             encrypted vault."
                .into(),
        );
    }
    if copy_on_write {
        caveats.push(
            "This filesystem is copy-on-write (e.g. APFS/Btrfs/ZFS): overwriting writes NEW blocks and may \
             leave the old data in snapshots. Delete relevant snapshots, or use an encrypted vault."
                .into(),
        );
    }
    if !on_ssd && !copy_on_write {
        caveats.push(
            "On a conventional disk this overwrites the file's blocks in place; note that copies in \
             backups, temp files, or filesystem journals are not touched."
                .into(),
        );
    }

    ShredPlan { path: path.to_string(), size_bytes, scheme, passes: ps, total_write_bytes, caveats }
}

#[cfg(test)]
mod tests {
    use super::*;
    use PassPattern::*;

    #[test]
    fn schemes_have_the_expected_passes() {
        assert_eq!(passes(ShredScheme::Zero), vec![Zeros]);
        assert_eq!(passes(ShredScheme::Random), vec![Random]);
        assert_eq!(passes(ShredScheme::Dod3), vec![Zeros, Ones, Random]);
        assert_eq!(passes(ShredScheme::Gutmann).len(), 7);
        // Gutmann bookends are random.
        let g = passes(ShredScheme::Gutmann);
        assert_eq!((g[0], *g.last().unwrap()), (Random, Random));
    }

    #[test]
    fn total_write_bytes_is_size_times_passes() {
        let p = plan_shred("/f", 1000, ShredScheme::Dod3, false, false);
        assert_eq!(p.total_write_bytes, 3000);
        assert_eq!(p.passes.len(), 3);
    }

    #[test]
    fn ssd_gets_the_flash_caveat() {
        let p = plan_shred("/f", 10, ShredScheme::Zero, true, false);
        assert!(p.caveats.iter().any(|c| c.to_lowercase().contains("ssd") || c.to_lowercase().contains("flash")));
    }

    #[test]
    fn cow_gets_the_snapshot_caveat() {
        let p = plan_shred("/f", 10, ShredScheme::Zero, false, true);
        assert!(p.caveats.iter().any(|c| c.to_lowercase().contains("snapshot")));
    }

    #[test]
    fn plain_disk_gets_the_in_place_caveat_and_no_scary_flash_note() {
        let p = plan_shred("/f", 10, ShredScheme::Zero, false, false);
        assert_eq!(p.caveats.len(), 1);
        assert!(p.caveats[0].to_lowercase().contains("in place"));
    }

    #[test]
    fn zero_size_file_plans_zero_writes() {
        let p = plan_shred("/empty", 0, ShredScheme::Gutmann, false, false);
        assert_eq!(p.total_write_bytes, 0);
    }
}
