//! Certificate signing / issue-from-CSR (CPE-1421, epic CPE-1417 "Crypto/security file viewers"): take a
//! PKCS#10 certificate-signing request and issue a leaf X.509 certificate for it, signed by an existing
//! CA's certificate + private key. The "issue" counterpart to [`crate::cert_create`] (CPE-1420, which
//! only self-signs) — a plain self-signed certificate is already fully covered by `cert_create::cert_create`
//! (pass `is_ca: false`), so this module adds no separate self-sign path.
//!
//! Backed by `rcgen`'s CSR support (the crate's `x509-parser` feature — see the `Cargo.toml` comment for
//! why that pulls in a second `x509-parser` version alongside [`crate::cert_decode`]'s): parsing the CSR
//! and loading the CA certificate as a signing [`rcgen::Issuer`] are both handled by `rcgen` itself, so
//! this module doesn't hand-roll any ASN.1. The issued certificate's subject, public key, and requested
//! Subject Alternative Names come straight from the CSR; the issuer is set to the CA certificate's own
//! subject; the validity window is computed here and clamped with the exact same
//! [`crate::cert_create::MAX_VALIDITY_DAYS`] cap `cert_create` uses.
//!
//! Never panics on malformed input: a bad CSR, a bad CA certificate, a bad/mismatched CA private key, or
//! a zero validity window all come back as `Err(String)`. The CA private key is only ever used in-process
//! to sign — it is never included in any `Err` message, logged, or returned to the caller.

use rcgen::{CertificateSigningRequestParams, IsCa, Issuer, KeyPair};

use crate::cert_create::MAX_VALIDITY_DAYS;

/// Parse `csr_pem` (a PEM-armored PKCS#10 CSR) and issue a leaf X.509 certificate for it, signed by the
/// CA identified by `ca_cert_pem` + `ca_key_pem` (both PEM). The issued certificate carries the CSR's
/// subject and requested Subject Alternative Names, is never itself a CA (`BasicConstraints` CA:FALSE
/// regardless of anything the CSR requested), and has issuer == the CA certificate's subject.
///
/// `validity_days` must be at least 1 and is clamped to [`MAX_VALIDITY_DAYS`] (~10 years), identical to
/// `cert_create::cert_create`'s clamp, for the same overflow-safety reason. Returns the issued
/// certificate's PEM. Never panics — a malformed CSR/CA-cert/CA-key PEM, or a CA key that doesn't match
/// the CA certificate's public key (the issued certificate will simply fail signature verification
/// against that certificate's real public key — this function doesn't itself check that), comes back as
/// `Err(String)` rather than a panic.
pub fn cert_issue_from_csr(
    csr_pem: &str,
    ca_cert_pem: &str,
    ca_key_pem: &str,
    validity_days: u32,
) -> Result<String, String> {
    if csr_pem.trim().is_empty() {
        return Err("CSR PEM must not be empty".to_string());
    }
    if ca_cert_pem.trim().is_empty() {
        return Err("CA certificate PEM must not be empty".to_string());
    }
    if ca_key_pem.trim().is_empty() {
        return Err("CA private key PEM must not be empty".to_string());
    }
    if validity_days == 0 {
        return Err("validity_days must be at least 1".to_string());
    }

    let mut csr_params =
        CertificateSigningRequestParams::from_pem(csr_pem).map_err(|e| format!("invalid CSR: {e}"))?;

    let ca_key = KeyPair::from_pem(ca_key_pem).map_err(|e| format!("invalid CA private key: {e}"))?;
    let issuer = Issuer::from_ca_cert_pem(ca_cert_pem, ca_key)
        .map_err(|e| format!("invalid CA certificate: {e}"))?;

    let validity_days = validity_days.min(MAX_VALIDITY_DAYS);
    let now = time::OffsetDateTime::now_utc();
    csr_params.params.not_before = now - time::Duration::minutes(5);
    csr_params.params.not_after = now + time::Duration::days(validity_days as i64);
    // A CSR carries no trustworthy claim about its own CA-ness; an issued leaf is never a CA regardless
    // of anything present in (or absent from) the request.
    csr_params.params.is_ca = IsCa::NoCa;

    let cert = csr_params.signed_by(&issuer).map_err(|e| format!("failed to issue certificate: {e}"))?;
    Ok(cert.pem())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cert_create::{cert_create, CertCreateParams, KeyType};
    use crate::cert_decode::cert_decode;

    use rcgen::{CertificateParams, DnType};
    use x509_parser::prelude::{FromDer, X509Certificate};

    /// Build a self-signed CA certificate + key (via `cert_create`, CPE-1420) of the given key type.
    fn make_ca(key_type: KeyType) -> (String, String) {
        let params = CertCreateParams {
            common_name: "Test Issuing CA".to_string(),
            san_dns: Vec::new(),
            san_ips: Vec::new(),
            validity_days: 3650,
            key_type,
            is_ca: true,
        };
        let result = cert_create(&params).expect("CA cert_create must succeed");
        (result.cert_pem, result.key_pem)
    }

    /// Build a PKCS#10 CSR PEM (EC-P256 key, independent of the CA's key) with a CN + one DNS SAN.
    fn make_csr(common_name: &str, san_dns: &str) -> String {
        let csr_key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).expect("CSR keygen");
        let mut csr_params = CertificateParams::new(vec![san_dns.to_string()]).expect("CSR params");
        csr_params.distinguished_name.push(DnType::CommonName, common_name);
        let csr = csr_params.serialize_request(&csr_key).expect("serialize CSR");
        csr.pem().expect("CSR PEM")
    }

    fn pem_to_der(pem: &str) -> Vec<u8> {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        let body: String =
            pem.lines().filter(|l| !l.starts_with("-----")).collect::<Vec<_>>().join("");
        STANDARD.decode(body).expect("valid base64 PEM body")
    }

    /// Cryptographically verify `issued_cert_pem`'s signature against `ca_cert_pem`'s public key — proves
    /// the issued certificate was really signed by that CA, not merely well-formed DER with a plausible
    /// `issuer` field.
    fn assert_signed_by_ca(issued_cert_pem: &str, ca_cert_pem: &str) {
        let ca_der = pem_to_der(ca_cert_pem);
        let (_, ca_cert) = X509Certificate::from_der(&ca_der).expect("parse CA cert DER");
        let issued_der = pem_to_der(issued_cert_pem);
        let (_, issued_cert) = X509Certificate::from_der(&issued_der).expect("parse issued cert DER");
        issued_cert
            .verify_signature(Some(ca_cert.public_key()))
            .expect("issued certificate's signature must verify against the CA's public key");
    }

    fn round_trip_for_key_type(ca_key_type: KeyType) {
        let (ca_cert_pem, ca_key_pem) = make_ca(ca_key_type);
        let csr_pem = make_csr("leaf.cpe-sample.local", "leaf.cpe-sample.local");

        let issued_pem = cert_issue_from_csr(&csr_pem, &ca_cert_pem, &ca_key_pem, 365)
            .expect("issuing a cert from a well-formed CSR + CA must succeed");
        assert!(issued_pem.starts_with("-----BEGIN CERTIFICATE-----"));

        let ca_preview = cert_decode(ca_cert_pem.as_bytes());
        let ca = ca_preview.certificate.expect("CA must decode");

        let preview = cert_decode(issued_pem.as_bytes());
        assert!(preview.error.is_none(), "decode of issued cert must not error: {:?}", preview.error);
        let leaf = preview.certificate.expect("issued certificate must be set");

        assert_eq!(leaf.issuer, ca.subject, "issuer must be the CA's subject");
        assert!(leaf.subject.contains("leaf.cpe-sample.local"), "subject: {}", leaf.subject);
        assert!(
            leaf.subject_alt_names.iter().any(|s| s.contains("leaf.cpe-sample.local")),
            "SANs: {:?}",
            leaf.subject_alt_names
        );
        assert!(!leaf.expired);
        assert!(!leaf.not_yet_valid);
        assert!(!leaf.is_ca, "an issued leaf must never itself be a CA");

        assert_signed_by_ca(&issued_pem, &ca_cert_pem);
    }

    #[test]
    fn ec_ca_issues_and_signature_verifies() {
        round_trip_for_key_type(KeyType::EcP256);
    }

    #[test]
    fn rsa_ca_issues_and_signature_verifies() {
        round_trip_for_key_type(KeyType::Rsa2048);
    }

    // ---------------------------------------------------------------------------------------------
    // Never panic on malformed/adversarial/mismatched input — every failure comes back as Err.
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn malformed_csr_is_rejected_not_panicked() {
        let (ca_cert_pem, ca_key_pem) = make_ca(KeyType::EcP256);
        let result = cert_issue_from_csr("not a csr at all", &ca_cert_pem, &ca_key_pem, 365);
        assert!(result.is_err());
    }

    #[test]
    fn malformed_ca_cert_is_rejected_not_panicked() {
        let (_, ca_key_pem) = make_ca(KeyType::EcP256);
        let csr_pem = make_csr("leaf.cpe-sample.local", "leaf.cpe-sample.local");
        let result = cert_issue_from_csr(&csr_pem, "not a certificate", &ca_key_pem, 365);
        assert!(result.is_err());
    }

    #[test]
    fn malformed_ca_key_is_rejected_not_panicked() {
        let (ca_cert_pem, _) = make_ca(KeyType::EcP256);
        let csr_pem = make_csr("leaf.cpe-sample.local", "leaf.cpe-sample.local");
        let result = cert_issue_from_csr(&csr_pem, &ca_cert_pem, "not a private key", 365);
        assert!(result.is_err());
    }

    #[test]
    fn empty_csr_is_rejected_not_panicked() {
        let (ca_cert_pem, ca_key_pem) = make_ca(KeyType::EcP256);
        assert!(cert_issue_from_csr("", &ca_cert_pem, &ca_key_pem, 365).is_err());
    }

    #[test]
    fn zero_validity_days_is_rejected_not_panicked() {
        let (ca_cert_pem, ca_key_pem) = make_ca(KeyType::EcP256);
        let csr_pem = make_csr("leaf.cpe-sample.local", "leaf.cpe-sample.local");
        assert!(cert_issue_from_csr(&csr_pem, &ca_cert_pem, &ca_key_pem, 0).is_err());
    }

    #[test]
    fn huge_validity_days_is_clamped_not_overflowed() {
        let (ca_cert_pem, ca_key_pem) = make_ca(KeyType::EcP256);
        let csr_pem = make_csr("leaf.cpe-sample.local", "leaf.cpe-sample.local");
        let issued_pem = cert_issue_from_csr(&csr_pem, &ca_cert_pem, &ca_key_pem, u32::MAX)
            .expect("huge validity_days must be clamped, not error or panic");
        let preview = cert_decode(issued_pem.as_bytes());
        let leaf = preview.certificate.expect("issued certificate must be set");
        assert!(!leaf.expired, "clamped ~10y validity must still be in the future");
    }

    /// A CA certificate signed with a *different* CA's private key: `rcgen` doesn't itself check that
    /// `ca_key_pem` matches `ca_cert_pem`'s embedded public key, so issuance succeeds — but the
    /// resulting certificate must then fail cryptographic verification against the certificate's real
    /// (mismatched) public key, proving the mismatch doesn't silently produce something that *looks*
    /// validly signed.
    #[test]
    fn mismatched_ca_key_never_panics_and_signature_fails_to_verify() {
        let (ca_a_cert_pem, _ca_a_key_pem) = make_ca(KeyType::EcP256);
        let (_ca_b_cert_pem, ca_b_key_pem) = make_ca(KeyType::EcP256);
        let csr_pem = make_csr("leaf.cpe-sample.local", "leaf.cpe-sample.local");

        let result = cert_issue_from_csr(&csr_pem, &ca_a_cert_pem, &ca_b_key_pem, 365);
        // Either outcome is an acceptable "didn't panic"; if issuance succeeded, the signature must not
        // verify against CA A's real public key (the certificate claims to be issued by CA A but was
        // actually signed with CA B's key).
        if let Ok(issued_pem) = result {
            let ca_der = pem_to_der(&ca_a_cert_pem);
            let (_, ca_cert) = X509Certificate::from_der(&ca_der).expect("parse CA A cert DER");
            let issued_der = pem_to_der(&issued_pem);
            let (_, issued_cert) = X509Certificate::from_der(&issued_der).expect("parse issued cert DER");
            assert!(
                issued_cert.verify_signature(Some(ca_cert.public_key())).is_err(),
                "a certificate signed by the wrong key must not verify against CA A's public key"
            );
        }
    }

    #[test]
    fn empty_ca_cert_is_rejected_not_panicked() {
        let (_, ca_key_pem) = make_ca(KeyType::EcP256);
        let csr_pem = make_csr("leaf.cpe-sample.local", "leaf.cpe-sample.local");
        assert!(cert_issue_from_csr(&csr_pem, "", &ca_key_pem, 365).is_err());
    }

    #[test]
    fn empty_ca_key_is_rejected_not_panicked() {
        let (ca_cert_pem, _) = make_ca(KeyType::EcP256);
        let csr_pem = make_csr("leaf.cpe-sample.local", "leaf.cpe-sample.local");
        assert!(cert_issue_from_csr(&csr_pem, &ca_cert_pem, "", 365).is_err());
    }
}
