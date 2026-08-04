//! Shared adversarial-input battery + `catch_unwind` harness (CPE-1169, extended by CPE-1311).
//!
//! Extracted out of `parser_panic_safety.rs` so a *second* integration-test file
//! (`binary_data_preview_panic_safety.rs`, CPE-1311) can drive the exact same battery against
//! path-based parser entrypoints without duplicating the generator. Rust compiles every file directly
//! under `tests/` as its own separate test-binary crate, so this lives in a `tests/common/` subdirectory
//! instead — Cargo does not treat `tests/common/mod.rs` as its own test target, only as a module other
//! test files can `mod common;` into (the standard idiom for shared integration-test helpers).
//!
//! See the module doc comment on `parser_panic_safety.rs` for the full rationale behind the battery
//! shape and the harness's the graceful-return philosophy.

use std::panic::{self, AssertUnwindSafe};

// ---------------------------------------------------------------------------------------------
// Deterministic pseudo-random bytes (no `rand` dependency)
// ---------------------------------------------------------------------------------------------

/// A tiny seeded linear-congruential generator (Numerical Recipes' constants) — NOT cryptographic, just
/// a reproducible byte stream so "seeded pseudo-random" input is deterministic across runs/machines
/// without pulling in the `rand` crate (no new dependency).
fn lcg_bytes(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed ^ 0x9E37_79B9_7F4A_7C15;
    (0..len)
        .map(|_| {
            state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            (state >> 33) as u8
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The shared adversarial battery
// ---------------------------------------------------------------------------------------------

/// One adversarial input, named for the panic message when it breaks something.
pub struct Case {
    pub class: String,
    pub bytes: Vec<u8>,
}

fn case(class: impl Into<String>, bytes: Vec<u8>) -> Case {
    Case { class: class.into(), bytes }
}

/// Build the shared adversarial battery for one entrypoint: `magic` is its leading signature (empty if
/// it has none), `header_len` is roughly the smallest structurally-meaningful header the parser reads
/// before it can report anything (used to aim the truncation/overflow boundaries at the interesting
/// spot rather than an arbitrary one).
pub fn battery(magic: &[u8], header_len: usize) -> Vec<Case> {
    let mut cases = Vec::new();
    cases.push(case("empty", Vec::new()));
    cases.push(case("one_byte_0x00", vec![0x00]));
    cases.push(case("one_byte_0xff", vec![0xFF]));

    for &n in &[2usize, 4, 8, 16, 32, 64, 128, 256, 512, 1024] {
        cases.push(case(format!("all_zeros_{n}"), vec![0u8; n]));
        cases.push(case(format!("all_0xff_{n}"), vec![0xFFu8; n]));
        cases.push(case(format!("seeded_random_{n}"), lcg_bytes(0x00C0_FFEE ^ n as u64, n)));
    }

    // Truncated at every prefix length of the magic itself (the classic "cut mid-signature").
    for cut in 0..=magic.len() {
        cases.push(case(format!("truncated_magic_at_{cut}"), magic[..cut].to_vec()));
    }
    // Truncated right around the declared header boundary (magic present, body cut short/long).
    let boundary = header_len.max(magic.len());
    for cut in boundary.saturating_sub(2)..=boundary + 2 {
        let mut buf = magic.to_vec();
        buf.resize(cut, 0);
        cases.push(case(format!("truncated_at_header_boundary_{cut}"), buf));
    }

    // Valid-magic-then-garbage: the real signature followed by an adversarial tail.
    for &n in &[8usize, 64, 256] {
        cases.push(case(format!("magic_then_zeros_{n}"), [magic, &vec![0u8; n][..]].concat()));
        cases.push(case(format!("magic_then_0xff_{n}"), [magic, &vec![0xFFu8; n][..]].concat()));
        cases.push(case(
            format!("magic_then_random_{n}"),
            [magic, &lcg_bytes(0xBAD_F00D ^ n as u64, n)[..]].concat(),
        ));
    }

    // Overflowing length fields: a declared size/length of all-1 bits right after the magic, in both
    // 32-bit and 64-bit, big- and little-endian — covers the common "size prefix right after the
    // signature" convention (ID3 syncsafe size, FLAC/MP4 box length, OGG page fields, PDF window
    // lengths, …) without needing per-format offset knowledge.
    let overflow_fields: [(&str, Vec<u8>); 4] = [
        ("u32_be_max", 0xFFFF_FFFFu32.to_be_bytes().to_vec()),
        ("u32_le_max", 0xFFFF_FFFFu32.to_le_bytes().to_vec()),
        ("u64_be_max", 0xFFFF_FFFF_FFFF_FFFFu64.to_be_bytes().to_vec()),
        ("u64_le_max", 0xFFFF_FFFF_FFFF_FFFFu64.to_le_bytes().to_vec()),
    ];
    for (label, overflow) in overflow_fields {
        let mut buf = magic.to_vec();
        buf.extend_from_slice(&overflow);
        buf.extend_from_slice(&lcg_bytes(0xF00D, 32));
        cases.push(case(format!("overflowing_length_field_{label}"), buf));
    }

    cases
}

// ---------------------------------------------------------------------------------------------
// catch_unwind harness
// ---------------------------------------------------------------------------------------------

/// Run `f` under `catch_unwind`. Any panic — the parser's own, or a graceful-contract assertion failing
/// inside `f` — is re-raised as a single, clearly-attributed failure naming the entrypoint and the
/// adversarial input class that triggered it, which is the whole point of this harness (CPE-1169: "a
/// failure names the entrypoint + input class").
pub fn assert_no_panic(entrypoint: &str, input_class: &str, f: impl FnOnce()) {
    if let Err(payload) = panic::catch_unwind(AssertUnwindSafe(f)) {
        let msg = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".to_string());
        panic!(
            "parser panic-safety violation: entrypoint `{entrypoint}` on adversarial input class \
             `{input_class}` did not return gracefully: {msg}"
        );
    }
}

/// Drive one entrypoint's shared battery through `check`, which calls the real function and may assert
/// its documented graceful-return contract (most usefully on the unambiguous `bytes.is_empty()` case —
/// see `parser_panic_safety.rs`'s module doc comment for why the harness doesn't over-assert on every
/// other class).
pub fn run_battery(name: &str, magic: &[u8], header_len: usize, check: impl Fn(&[u8])) {
    for c in battery(magic, header_len) {
        assert_no_panic(name, &c.class, || check(&c.bytes));
    }
}
