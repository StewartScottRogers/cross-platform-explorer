//! Certificate creation (CPE-1420, epic CPE-1417 "Crypto/security file viewers"): generate a fresh
//! keypair and a self-signed X.509 certificate from user-supplied parameters. The write-side mirror of
//! [`crate::cert_decode`] (CPE-1419) — this module's own round-trip tests feed exactly what
//! [`cert_create`] produces straight back through `cert_decode::cert_decode` to prove the two agree.
//!
//! Backed by `rcgen` (pure Rust, `ring` backend — see the `Cargo.toml` comment for the full
//! justification) for the certificate/CSR machinery, plus the pure-Rust `rsa` crate for RSA key
//! generation specifically: `rcgen`'s default `ring` backend can *sign* with an RSA key but cannot
//! *generate* one (RSA keygen needs the `aws_lc_rs` backend, which needs `cmake`/`nasm`). This is the
//! exact same dependency pair the CPE-1425 `samples/crypto/` fixture generator already proved works
//! end-to-end (see `samples/crypto/README.md`), now adopted as a real dependency instead of a
//! throwaway one-off program.
//!
//! Never panics on malformed/adversarial input: every fallible step returns `Result`, and
//! [`cert_create`] validates its own parameters (empty CN, zero validity, too many SANs, unparsable IP
//! SANs) before touching `rcgen` at all.

use serde::{Deserialize, Serialize};

use rcgen::{
    BasicConstraints, CertificateParams, DnType, IsCa, KeyPair, SanType, PKCS_ECDSA_P256_SHA256,
    PKCS_ECDSA_P384_SHA384, PKCS_RSA_SHA256,
};

/// Which keypair algorithm/size to generate. EC-P256 is the default (fast, small, no big-int keygen);
/// RSA is offered for interop with systems that don't accept EC certificates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum KeyType {
    #[default]
    EcP256,
    EcP384,
    Rsa2048,
    Rsa4096,
}

/// Input parameters for [`cert_create`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CertCreateParams {
    /// Subject Common Name (CN), e.g. `"my-service.local"`. Must not be empty.
    pub common_name: String,
    /// DNS-name Subject Alternative Names.
    #[serde(default)]
    pub san_dns: Vec<String>,
    /// IP-address Subject Alternative Names (IPv4 or IPv6), e.g. `"127.0.0.1"` / `"::1"`.
    #[serde(default)]
    pub san_ips: Vec<String>,
    /// Validity window length in days, starting ~5 minutes ago (a small clock-skew allowance so a
    /// certificate is immediately usable on a machine whose clock runs a little behind). Clamped to
    /// [`MAX_VALIDITY_DAYS`] to keep the resulting `not_after` comfortably inside `time`'s representable
    /// range no matter what a caller passes.
    pub validity_days: u32,
    /// Keypair algorithm/size.
    #[serde(default)]
    pub key_type: KeyType,
    /// Whether the certificate is a CA (sets the `BasicConstraints` CA flag so it can sign other
    /// certificates).
    #[serde(default)]
    pub is_ca: bool,
}

/// The generated keypair + self-signed certificate, both PEM-encoded. The private key is returned so
/// the caller (the `#[tauri::command]` dispatcher) can write it to disk with restrictive permissions —
/// this module itself never writes files or logs key material.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CertCreateResult {
    pub cert_pem: String,
    pub key_pem: String,
}

/// A hostile/oversized SAN list is rejected outright rather than handed to `rcgen` — a real certificate
/// carries at most a handful of SANs, so anything past this can only be adversarial or a UI bug feeding
/// unbounded input.
const MAX_SAN_ENTRIES: usize = 200;

/// Validity window cap (~10 years). Keeps `not_before`/`not_after` arithmetic comfortably inside
/// `time::OffsetDateTime`'s representable range regardless of what a caller passes for `validity_days`
/// (including `u32::MAX`), so the date math below can never overflow/panic. `pub(crate)` so
/// [`crate::cert_sign`] (CPE-1421) can reuse the exact same clamp for issued (CA-signed) certificates
/// instead of duplicating the constant.
pub(crate) const MAX_VALIDITY_DAYS: u32 = 3650;

/// Generate a fresh keypair and a self-signed X.509 certificate from `params`. Never panics — every
/// failure (empty CN, zero validity, too many SANs, an unparsable IP SAN, or a keygen/signing failure)
/// comes back as `Err(String)`.
pub fn cert_create(params: &CertCreateParams) -> Result<CertCreateResult, String> {
    if params.common_name.trim().is_empty() {
        return Err("common name must not be empty".to_string());
    }
    if params.validity_days == 0 {
        return Err("validity_days must be at least 1".to_string());
    }
    if params.san_dns.len() + params.san_ips.len() > MAX_SAN_ENTRIES {
        return Err(format!("too many subject alternative names (max {MAX_SAN_ENTRIES})"));
    }

    let key_pair = generate_key_pair(params.key_type)?;

    let mut cert_params =
        CertificateParams::new(params.san_dns.clone()).map_err(|e| format!("invalid DNS SAN: {e}"))?;
    cert_params.distinguished_name.push(DnType::CommonName, params.common_name.as_str());

    for ip in &params.san_ips {
        let addr: std::net::IpAddr = ip.parse().map_err(|_| format!("invalid IP SAN \"{ip}\""))?;
        cert_params.subject_alt_names.push(SanType::IpAddress(addr));
    }

    let validity_days = params.validity_days.min(MAX_VALIDITY_DAYS);
    let now = time::OffsetDateTime::now_utc();
    cert_params.not_before = now - time::Duration::minutes(5);
    cert_params.not_after = now + time::Duration::days(validity_days as i64);

    cert_params.is_ca =
        if params.is_ca { IsCa::Ca(BasicConstraints::Unconstrained) } else { IsCa::NoCa };

    let cert = cert_params.self_signed(&key_pair).map_err(|e| format!("failed to self-sign certificate: {e}"))?;

    Ok(CertCreateResult { cert_pem: cert.pem(), key_pem: key_pair.serialize_pem() })
}

fn generate_key_pair(key_type: KeyType) -> Result<KeyPair, String> {
    match key_type {
        KeyType::EcP256 => {
            KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).map_err(|e| format!("EC-P256 keygen failed: {e}"))
        }
        KeyType::EcP384 => {
            KeyPair::generate_for(&PKCS_ECDSA_P384_SHA384).map_err(|e| format!("EC-P384 keygen failed: {e}"))
        }
        KeyType::Rsa2048 => generate_rsa_key_pair(2048),
        KeyType::Rsa4096 => generate_rsa_key_pair(4096),
    }
}

/// Generate an RSA key of `bits` size with the pure-Rust `rsa` crate (see the module doc comment for why
/// `rcgen` alone can't do this), then hand it to `rcgen` as a PKCS#8 PEM key for signing.
fn generate_rsa_key_pair(bits: usize) -> Result<KeyPair, String> {
    use rsa::pkcs8::{EncodePrivateKey, LineEnding};
    use rsa::RsaPrivateKey;

    let mut rng = rand::thread_rng();
    let rsa_priv = RsaPrivateKey::new(&mut rng, bits).map_err(|e| format!("RSA-{bits} keygen failed: {e}"))?;
    let pkcs8_pem =
        rsa_priv.to_pkcs8_pem(LineEnding::LF).map_err(|e| format!("failed to encode RSA key: {e}"))?.to_string();
    KeyPair::from_pkcs8_pem_and_sign_algo(&pkcs8_pem, &PKCS_RSA_SHA256)
        .map_err(|e| format!("failed to load generated RSA key: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cert_decode::cert_decode;
    use x509_parser::prelude::FromDer;

    fn base_params(key_type: KeyType, is_ca: bool) -> CertCreateParams {
        CertCreateParams {
            common_name: "test.cpe-sample.local".to_string(),
            san_dns: vec!["test.cpe-sample.local".to_string(), "alt.cpe-sample.local".to_string()],
            san_ips: vec!["127.0.0.1".to_string(), "::1".to_string()],
            validity_days: 365,
            key_type,
            is_ca,
        }
    }

    /// Re-sign a fresh, minimal certificate with `key_pem` reloaded via `rcgen::KeyPair::from_pem`
    /// (which auto-detects the algorithm from the PKCS#8 `AlgorithmIdentifier`) and compare its
    /// SubjectPublicKeyInfo DER bytes to `cert_pem`'s. SPKI encoding is a pure function of the public
    /// key alone (no signature randomness involved), so identical SPKI bytes prove `key_pem` really is
    /// the private half of `cert_pem`'s keypair — exactly the "rcgen reload / key+cert consistency"
    /// check the ticket asks for, using only crates this module already depends on.
    fn assert_key_pairs_with_cert(cert_pem: &str, key_pem: &str) {
        let reloaded = KeyPair::from_pem(key_pem).expect("reload the private key we just generated");
        let mut probe_params = CertificateParams::new(Vec::<String>::new()).expect("empty-SAN params");
        probe_params.distinguished_name.push(DnType::CommonName, "probe");
        let probe_cert = probe_params.self_signed(&reloaded).expect("self-sign probe cert with reloaded key");

        let original_der = pem_to_der(cert_pem);
        let (_, original) = x509_parser::prelude::X509Certificate::from_der(original_der.as_slice())
            .expect("parse original cert DER");
        let (_, probe) = x509_parser::prelude::X509Certificate::from_der(probe_cert.der())
            .expect("parse probe cert DER");

        assert_eq!(
            original.public_key().raw,
            probe.public_key().raw,
            "SPKI of the returned cert must match the SPKI derived from reloading the returned key"
        );
    }

    fn pem_to_der(pem: &str) -> Vec<u8> {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        let body: String = pem
            .lines()
            .filter(|l| !l.starts_with("-----"))
            .collect::<Vec<_>>()
            .join("");
        STANDARD.decode(body).expect("valid base64 PEM body")
    }

    #[test]
    fn ec_p256_round_trips_through_cert_decode() {
        let result = cert_create(&base_params(KeyType::EcP256, false)).expect("EC-P256 cert_create must succeed");
        assert!(result.cert_pem.starts_with("-----BEGIN CERTIFICATE-----"));
        assert!(result.key_pem.starts_with("-----BEGIN PRIVATE KEY-----"));

        let preview = cert_decode(result.cert_pem.as_bytes());
        assert!(preview.error.is_none(), "decode of freshly created cert must not error: {:?}", preview.error);
        assert_eq!(preview.kind.as_deref(), Some("certificate"));
        let c = preview.certificate.expect("certificate must be set");

        assert!(c.subject.contains("test.cpe-sample.local"), "subject: {}", c.subject);
        assert!(c.subject_alt_names.iter().any(|s| s.contains("test.cpe-sample.local")), "SANs: {:?}", c.subject_alt_names);
        assert!(c.subject_alt_names.iter().any(|s| s.contains("alt.cpe-sample.local")), "SANs: {:?}", c.subject_alt_names);
        // x509-parser's `GeneralName` Display renders an IP SAN as raw address bytes in colon-hex (not
        // dotted-decimal/`::`-compressed), so match its actual output rather than the input strings —
        // 127.0.0.1 -> 7f:00:00:01, ::1 -> a run of 00 octets ending in 01.
        assert!(c.subject_alt_names.iter().any(|s| s.contains("7f:00:00:01")), "SANs: {:?}", c.subject_alt_names);
        assert!(
            c.subject_alt_names.iter().any(|s| s.contains("00:00:00:00:00:00:00:00:00:00:00:00:00:00:01")),
            "SANs: {:?}",
            c.subject_alt_names
        );
        assert_eq!(c.public_key.algorithm, "EC");
        assert_eq!(c.public_key.size_bits, Some(256));
        assert!(!c.expired);
        assert!(!c.not_yet_valid);
        assert!(!c.is_ca);

        assert_key_pairs_with_cert(&result.cert_pem, &result.key_pem);
    }

    #[test]
    fn ec_p384_reports_384_bit_key() {
        let params = CertCreateParams { validity_days: 30, ..base_params(KeyType::EcP384, false) };
        let result = cert_create(&params).expect("EC-P384 cert_create must succeed");
        let preview = cert_decode(result.cert_pem.as_bytes());
        assert!(preview.error.is_none());
        let c = preview.certificate.expect("certificate must be set");
        assert_eq!(c.public_key.algorithm, "EC");
        assert_eq!(c.public_key.size_bits, Some(384));
        assert_key_pairs_with_cert(&result.cert_pem, &result.key_pem);
    }

    #[test]
    fn rsa_2048_round_trips_through_cert_decode() {
        let result = cert_create(&base_params(KeyType::Rsa2048, false)).expect("RSA-2048 cert_create must succeed");
        assert!(result.cert_pem.starts_with("-----BEGIN CERTIFICATE-----"));
        assert!(result.key_pem.starts_with("-----BEGIN PRIVATE KEY-----"));

        let preview = cert_decode(result.cert_pem.as_bytes());
        assert!(preview.error.is_none(), "decode of freshly created RSA cert must not error: {:?}", preview.error);
        let c = preview.certificate.expect("certificate must be set");

        assert!(c.subject.contains("test.cpe-sample.local"));
        assert_eq!(c.public_key.algorithm, "RSA");
        assert_eq!(c.public_key.size_bits, Some(2048));
        assert!(!c.expired);
        assert!(!c.not_yet_valid);
        assert!(!c.is_ca);

        assert_key_pairs_with_cert(&result.cert_pem, &result.key_pem);
    }

    /// RSA-4096 (CPE-1427, CPE-1420/PR #694 reviewer follow-up): same parameterized `cert_create` path
    /// as RSA-2048 above, just the larger key size — full key-size coverage for `generate_rsa_key_pair`.
    /// RSA-4096 keygen is noticeably slower than the other key types but still completes in well under a
    /// second in practice, so this runs as a normal (non-`#[ignore]`d) test rather than being skipped.
    #[test]
    fn rsa_4096_round_trips_through_cert_decode() {
        let result = cert_create(&base_params(KeyType::Rsa4096, false)).expect("RSA-4096 cert_create must succeed");
        assert!(result.cert_pem.starts_with("-----BEGIN CERTIFICATE-----"));
        assert!(result.key_pem.starts_with("-----BEGIN PRIVATE KEY-----"));

        let preview = cert_decode(result.cert_pem.as_bytes());
        assert!(preview.error.is_none(), "decode of freshly created RSA-4096 cert must not error: {:?}", preview.error);
        let c = preview.certificate.expect("certificate must be set");

        assert!(c.subject.contains("test.cpe-sample.local"));
        assert_eq!(c.public_key.algorithm, "RSA");
        assert_eq!(c.public_key.size_bits, Some(4096));
        assert!(!c.expired);
        assert!(!c.not_yet_valid);
        assert!(!c.is_ca);

        assert_key_pairs_with_cert(&result.cert_pem, &result.key_pem);
    }

    #[test]
    fn is_ca_true_sets_basic_constraints() {
        let result = cert_create(&base_params(KeyType::EcP256, true)).expect("CA cert_create must succeed");
        let preview = cert_decode(result.cert_pem.as_bytes());
        assert!(preview.error.is_none());
        let c = preview.certificate.expect("certificate must be set");
        assert!(c.is_ca, "is_ca:true param must produce a CA:TRUE certificate");
    }

    #[test]
    fn is_ca_false_leaves_no_ca_capability() {
        let result = cert_create(&base_params(KeyType::EcP256, false)).expect("leaf cert_create must succeed");
        let preview = cert_decode(result.cert_pem.as_bytes());
        assert!(preview.error.is_none());
        let c = preview.certificate.expect("certificate must be set");
        assert!(!c.is_ca);
    }

    // ---------------------------------------------------------------------------------------------
    // Never panic on odd/adversarial params — every failure comes back as Err, not a panic.
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn empty_common_name_is_rejected_not_panicked() {
        let params = CertCreateParams { common_name: String::new(), ..base_params(KeyType::EcP256, false) };
        assert!(cert_create(&params).is_err());
    }

    #[test]
    fn whitespace_only_common_name_is_rejected() {
        let params = CertCreateParams { common_name: "   ".to_string(), ..base_params(KeyType::EcP256, false) };
        assert!(cert_create(&params).is_err());
    }

    #[test]
    fn zero_validity_days_is_rejected_not_panicked() {
        let params = CertCreateParams { validity_days: 0, ..base_params(KeyType::EcP256, false) };
        assert!(cert_create(&params).is_err());
    }

    #[test]
    fn huge_validity_days_is_clamped_not_overflowed() {
        let params = CertCreateParams { validity_days: u32::MAX, ..base_params(KeyType::EcP256, false) };
        let result = cert_create(&params).expect("huge validity_days must be clamped, not error or panic");
        let preview = cert_decode(result.cert_pem.as_bytes());
        assert!(preview.error.is_none());
        let c = preview.certificate.expect("certificate must be set");
        assert!(!c.expired, "clamped ~10y validity must still be in the future");
    }

    #[test]
    fn invalid_ip_san_is_rejected_not_panicked() {
        let params = CertCreateParams {
            san_ips: vec!["not-an-ip-address".to_string()],
            ..base_params(KeyType::EcP256, false)
        };
        assert!(cert_create(&params).is_err());
    }

    #[test]
    fn too_many_sans_is_rejected_not_panicked() {
        let params = CertCreateParams {
            san_dns: (0..MAX_SAN_ENTRIES + 1).map(|i| format!("host{i}.cpe-sample.local")).collect(),
            ..base_params(KeyType::EcP256, false)
        };
        assert!(cert_create(&params).is_err());
    }

    #[test]
    fn no_sans_at_all_still_succeeds() {
        let params = CertCreateParams {
            san_dns: Vec::new(),
            san_ips: Vec::new(),
            ..base_params(KeyType::EcP256, false)
        };
        let result = cert_create(&params).expect("CN-only cert must still be creatable");
        let preview = cert_decode(result.cert_pem.as_bytes());
        assert!(preview.error.is_none());
    }

    #[test]
    fn key_type_default_is_ec_p256() {
        assert_eq!(KeyType::default(), KeyType::EcP256);
    }
}
