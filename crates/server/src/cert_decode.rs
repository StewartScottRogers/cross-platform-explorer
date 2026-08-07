//! Certificate/CSR/public-key decoder (CPE-1419, epic CPE-1417 "Crypto/security file viewers"): a
//! read-only VIEWER for X.509 certificates, PKCS#10 certificate-signing requests, SubjectPublicKeyInfo
//! public keys, and (algorithm/size only) private-key files.
//!
//! **Never verifies anything.** No signature check, no chain validation, no trust decision — it only
//! decodes the ASN.1/DER structure and surfaces fields, exactly like `jwt_preview` decodes a JWT without
//! checking its signature. A private-key file's actual key material is NEVER surfaced, by design — only
//! its algorithm and size are reported (see [`PrivateKeyInfo`]).
//!
//! Backed by `x509-parser` (pure Rust, nom-based, no OpenSSL/system TLS library — see the Cargo.toml
//! comment for the full justification) for certificates, CSRs, and standalone public keys. Private-key
//! files (`PKCS#8`/`PKCS#1`/`SEC1`) are outside `x509-parser`'s scope (it only parses `X.509` structures,
//! not private-key formats), so this module hand-rolls a *minimal* DER/TLV walk just far enough to read
//! the `AlgorithmIdentifier` OID and, for RSA, the modulus bit length — never touching the private
//! exponent/prime fields at all.
//!
//! Never panics on malformed input: every parse step returns `Result`/`Option` and the whole entrypoint
//! collapses any failure into [`CertPreview::error`] rather than propagating a panic. Covered by this
//! module's own unit tests and wired into the crate's panic-safety battery
//! (`tests/binary_data_preview_panic_safety.rs`-style bytes battery in `tests/parser_panic_safety.rs`).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sha1::Sha1;
use x509_parser::certification_request::X509CertificationRequest;
use x509_parser::prelude::{FromDer, X509Certificate};
use x509_parser::public_key::PublicKey;
use x509_parser::x509::SubjectPublicKeyInfo;

/// A decoded public/private key's algorithm shape. Never carries key material — only what identifies the
/// algorithm and its size, which is exactly what CPE-1419 asks a private-key branch to report.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct KeyInfo {
    /// Friendly algorithm name (`"RSA"`, `"EC"`, `"Ed25519"`, `"DSA"`, or the raw OID if unrecognized).
    pub algorithm: String,
    /// Key size in bits, when computable (RSA modulus bit length, EC field size).
    pub size_bits: Option<u32>,
    /// Named curve, for an EC key (e.g. `"prime256v1"`).
    pub curve: Option<String>,
}

/// A decoded X.509 certificate's fields.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CertificateInfo {
    pub subject: String,
    pub issuer: String,
    /// Serial number, formatted as lowercase hex.
    pub serial: String,
    /// `"v1"` / `"v2"` / `"v3"`.
    pub version: String,
    pub not_before: String,
    pub not_after: String,
    pub expired: bool,
    pub not_yet_valid: bool,
    pub signature_algorithm: String,
    pub public_key: KeyInfo,
    pub subject_alt_names: Vec<String>,
    pub is_ca: bool,
    pub key_usage: Vec<String>,
    pub extended_key_usage: Vec<String>,
    /// Lowercase hex, no separators.
    pub sha256_fingerprint: String,
    /// Lowercase hex, no separators.
    pub sha1_fingerprint: String,
}

/// A decoded PKCS#10 certificate-signing request's fields.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CsrInfo {
    pub subject: String,
    pub requested_sans: Vec<String>,
    pub public_key: KeyInfo,
}

/// The overall decode result. Exactly one of `certificate`/`csr`/`public_key`/`private_key` is set on
/// success; `error` is set (and everything else left `None`) on failure. Never a panic either way.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CertPreview {
    /// `"certificate"` / `"csr"` / `"public_key"` / `"private_key"`, mirroring whichever field below is set.
    pub kind: Option<String>,
    /// Whether the input was PEM- or DER-encoded.
    pub encoding: Option<String>,
    pub certificate: Option<CertificateInfo>,
    pub csr: Option<CsrInfo>,
    pub public_key: Option<KeyInfo>,
    pub private_key: Option<KeyInfo>,
    pub error: Option<String>,
}

/// A hostile/oversized input is rejected outright rather than handed to the parser — real
/// certs/CSRs/keys are kilobytes at most (even a certificate embedding a large SAN list stays well under
/// this), so anything bigger can only be adversarial. Matches `jwt_preview`'s `MAX_SEGMENT_CHARS` cap in
/// spirit.
const MAX_INPUT_BYTES: usize = 4 * 1024 * 1024;

/// Auto-detect PEM vs DER and decode `bytes` as an X.509 certificate, a PKCS#10 CSR, a standalone public
/// key, or (algorithm/size only) a private key. Never panics — anything that doesn't parse as one of
/// those four shapes comes back as `CertPreview { error: Some(..), .. }`.
pub fn cert_decode(bytes: &[u8]) -> CertPreview {
    if bytes.is_empty() {
        return CertPreview { error: Some("empty input".to_string()), ..Default::default() };
    }
    if bytes.len() > MAX_INPUT_BYTES {
        return CertPreview {
            error: Some(format!("input too large ({} bytes, cap {MAX_INPUT_BYTES})", bytes.len())),
            ..Default::default()
        };
    }

    match find_pem_block(bytes) {
        Some((label, der)) => decode_der_by_label(&label, &der, "PEM"),
        None => decode_der_unlabeled(bytes, "DER"),
    }
}

/// Look for a `-----BEGIN <label>-----` PEM armor in `bytes` and, if found, decode the base64 body
/// between it and the matching `-----END-----` line. Returns `None` for anything that isn't PEM-armored
/// (including malformed/truncated PEM — that falls through to the DER path, which will then also fail
/// gracefully rather than silently misreporting a broken PEM as "not PEM at all").
fn find_pem_block(bytes: &[u8]) -> Option<(String, Vec<u8>)> {
    let text = std::str::from_utf8(bytes).ok()?;
    let begin_marker = "-----BEGIN ";
    let begin_at = text.find(begin_marker)?;
    let after_begin = &text[begin_at + begin_marker.len()..];
    let label_end = after_begin.find("-----")?;
    let label = after_begin[..label_end].trim().to_string();
    let body_start = begin_at + begin_marker.len() + label_end + "-----".len();

    let end_marker = format!("-----END {label}-----");
    let end_at = text[body_start..].find(&end_marker)? + body_start;
    let body: String = text[body_start..end_at].chars().filter(|c| !c.is_whitespace()).collect();

    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let der = STANDARD.decode(body.as_bytes()).ok()?;
    Some((label, der))
}

fn decode_der_by_label(label: &str, der: &[u8], encoding: &str) -> CertPreview {
    match label {
        "CERTIFICATE" | "X509 CERTIFICATE" | "TRUSTED CERTIFICATE" => decode_certificate(der, encoding),
        "CERTIFICATE REQUEST" | "NEW CERTIFICATE REQUEST" => decode_csr(der, encoding),
        "PUBLIC KEY" | "RSA PUBLIC KEY" => decode_public_key(der, encoding),
        "PRIVATE KEY" | "RSA PRIVATE KEY" | "EC PRIVATE KEY" | "ENCRYPTED PRIVATE KEY" | "DSA PRIVATE KEY" => {
            decode_private_key(label, der, encoding)
        }
        other => CertPreview {
            encoding: Some(encoding.to_string()),
            error: Some(format!("unrecognized PEM label \"{other}\"")),
            ..Default::default()
        },
    }
}

/// No PEM armor found — try each DER shape in turn (certificate, then CSR, then public key), since DER
/// carries no self-describing label the way a PEM header does.
fn decode_der_unlabeled(der: &[u8], encoding: &str) -> CertPreview {
    let cert_attempt = decode_certificate(der, encoding);
    if cert_attempt.error.is_none() {
        return cert_attempt;
    }
    let csr_attempt = decode_csr(der, encoding);
    if csr_attempt.error.is_none() {
        return csr_attempt;
    }
    let pubkey_attempt = decode_public_key(der, encoding);
    if pubkey_attempt.error.is_none() {
        return pubkey_attempt;
    }
    let privkey_attempt = private_key_info_from_der(der);
    if let Ok(info) = privkey_attempt {
        return CertPreview {
            kind: Some("private_key".to_string()),
            encoding: Some(encoding.to_string()),
            private_key: Some(info),
            ..Default::default()
        };
    }
    CertPreview {
        encoding: Some(encoding.to_string()),
        error: Some("not a recognizable certificate, CSR, public key, or private key (DER)".to_string()),
        ..Default::default()
    }
}

fn decode_certificate(der: &[u8], encoding: &str) -> CertPreview {
    let cert = match X509Certificate::from_der(der) {
        Ok((_, cert)) => cert,
        Err(e) => {
            return CertPreview { encoding: Some(encoding.to_string()), error: Some(format!("not a certificate: {e}")), ..Default::default() };
        }
    };

    let validity = cert.validity();
    let now = x509_parser::time::ASN1Time::now();
    let not_before = validity.not_before;
    let not_after = validity.not_after;

    let mut sans: Vec<String> = Vec::new();
    if let Ok(Some(san)) = cert.subject_alternative_name() {
        for name in &san.value.general_names {
            sans.push(format!("{name}"));
        }
    }

    let is_ca = cert.is_ca();

    let mut key_usage: Vec<String> = Vec::new();
    if let Ok(Some(ku)) = cert.key_usage() {
        let u = &ku.value;
        for (flag, name) in [
            (u.digital_signature(), "digitalSignature"),
            (u.non_repudiation(), "nonRepudiation"),
            (u.key_encipherment(), "keyEncipherment"),
            (u.data_encipherment(), "dataEncipherment"),
            (u.key_agreement(), "keyAgreement"),
            (u.key_cert_sign(), "keyCertSign"),
            (u.crl_sign(), "cRLSign"),
            (u.encipher_only(), "encipherOnly"),
            (u.decipher_only(), "decipherOnly"),
        ] {
            if flag {
                key_usage.push(name.to_string());
            }
        }
    }

    let mut eku: Vec<String> = Vec::new();
    if let Ok(Some(x)) = cert.extended_key_usage() {
        let v = &x.value;
        for (flag, name) in [
            (v.server_auth, "serverAuth"),
            (v.client_auth, "clientAuth"),
            (v.code_signing, "codeSigning"),
            (v.email_protection, "emailProtection"),
            (v.time_stamping, "timeStamping"),
            (v.ocsp_signing, "OCSPSigning"),
        ] {
            if flag {
                eku.push(name.to_string());
            }
        }
        for other in &v.other {
            eku.push(other.to_id_string());
        }
    }

    let info = CertificateInfo {
        subject: cert.subject().to_string(),
        issuer: cert.issuer().to_string(),
        serial: cert.tbs_certificate.raw_serial_as_string(),
        version: format!("v{}", cert.version().0 + 1),
        not_before: asn1_time_to_rfc3339(&not_before),
        not_after: asn1_time_to_rfc3339(&not_after),
        expired: now > not_after,
        not_yet_valid: now < not_before,
        signature_algorithm: oid_friendly_name(&cert.signature_algorithm.algorithm),
        public_key: key_info_from_spki(cert.public_key()),
        subject_alt_names: sans,
        is_ca,
        key_usage,
        extended_key_usage: eku,
        sha256_fingerprint: hex_lower(&Sha256::digest(der)),
        sha1_fingerprint: hex_lower(&Sha1::digest(der)),
    };

    CertPreview {
        kind: Some("certificate".to_string()),
        encoding: Some(encoding.to_string()),
        certificate: Some(info),
        ..Default::default()
    }
}

fn decode_csr(der: &[u8], encoding: &str) -> CertPreview {
    let csr = match X509CertificationRequest::from_der(der) {
        Ok((_, csr)) => csr,
        Err(e) => {
            return CertPreview { encoding: Some(encoding.to_string()), error: Some(format!("not a CSR: {e}")), ..Default::default() };
        }
    };

    let info_req = &csr.certification_request_info;
    let mut sans: Vec<String> = Vec::new();
    if let Some(exts) = csr.requested_extensions() {
        for ext in exts {
            if let x509_parser::extensions::ParsedExtension::SubjectAlternativeName(san) = ext {
                for name in &san.general_names {
                    sans.push(format!("{name}"));
                }
            }
        }
    }

    let info = CsrInfo {
        subject: info_req.subject.to_string(),
        requested_sans: sans,
        public_key: key_info_from_spki(&info_req.subject_pki),
    };

    CertPreview { kind: Some("csr".to_string()), encoding: Some(encoding.to_string()), csr: Some(info), ..Default::default() }
}

fn decode_public_key(der: &[u8], encoding: &str) -> CertPreview {
    let spki = match SubjectPublicKeyInfo::from_der(der) {
        Ok((_, spki)) => spki,
        Err(e) => {
            return CertPreview { encoding: Some(encoding.to_string()), error: Some(format!("not a public key: {e}")), ..Default::default() };
        }
    };
    CertPreview {
        kind: Some("public_key".to_string()),
        encoding: Some(encoding.to_string()),
        public_key: Some(key_info_from_spki(&spki)),
        ..Default::default()
    }
}

fn decode_private_key(label: &str, der: &[u8], encoding: &str) -> CertPreview {
    match private_key_info_from_der(der).or_else(|_| private_key_info_from_label_hint(label, der)) {
        Ok(info) => CertPreview {
            kind: Some("private_key".to_string()),
            encoding: Some(encoding.to_string()),
            private_key: Some(info),
            ..Default::default()
        },
        Err(e) => CertPreview { encoding: Some(encoding.to_string()), error: Some(format!("not a private key: {e}")), ..Default::default() },
    }
}

fn key_info_from_spki(spki: &SubjectPublicKeyInfo) -> KeyInfo {
    match spki.parsed() {
        Ok(PublicKey::RSA(rsa)) => KeyInfo { algorithm: "RSA".to_string(), size_bits: Some(rsa.key_size() as u32), curve: None },
        Ok(PublicKey::EC(ec)) => {
            let curve = spki
                .algorithm
                .parameters
                .as_ref()
                .and_then(|p| p.as_oid().ok())
                .map(|oid| oid_friendly_name(&oid));
            KeyInfo { algorithm: "EC".to_string(), size_bits: Some(ec.key_size() as u32), curve }
        }
        Ok(PublicKey::DSA(y)) => KeyInfo { algorithm: "DSA".to_string(), size_bits: Some((y.len() * 8) as u32), curve: None },
        Ok(PublicKey::GostR3410(_)) => KeyInfo { algorithm: "GOST R 34.10-94".to_string(), size_bits: None, curve: None },
        Ok(PublicKey::GostR3410_2012(_)) => KeyInfo { algorithm: "GOST R 34.10-2012".to_string(), size_bits: None, curve: None },
        Ok(PublicKey::Unknown(_)) | Err(_) => {
            KeyInfo { algorithm: oid_friendly_name(&spki.algorithm.algorithm), size_bits: None, curve: None }
        }
    }
}

fn oid_friendly_name(oid: &x509_parser::der_parser::oid::Oid) -> String {
    oid_registry::OidRegistry::default()
        .with_all_crypto()
        .get(oid)
        .map(|entry| entry.sn().to_string())
        .unwrap_or_else(|| oid.to_id_string())
}

fn asn1_time_to_rfc3339(t: &x509_parser::time::ASN1Time) -> String {
    crate::fsutil::unix_to_rfc3339(t.timestamp())
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------------------------
// Private-key algorithm/size sniffing (PKCS#8 "PRIVATE KEY" / PKCS#1 "RSA PRIVATE KEY" / SEC1
// "EC PRIVATE KEY"). `x509-parser` doesn't parse private-key formats (out of its X.509 scope), so this
// is a minimal hand-rolled DER/TLV walk — deliberately narrow: it reads just enough to name the
// algorithm and, for RSA, the modulus bit length. It never reaches (let alone returns) the private
// exponent, primes, or any other key-material field. This mirrors `jwt_preview`'s "hand-roll a small
// decode rather than pull in a crate" approach, scaled to ASN.1's minimal TLV shape.
// ---------------------------------------------------------------------------------------------

/// One decoded ASN.1 DER TLV: `tag`, and the raw `value` bytes.
struct Tlv<'a> {
    tag: u8,
    value: &'a [u8],
}

/// Read one DER TLV (tag/length/value) starting at `buf`'s front. `Err` on truncated/malformed length
/// encoding — never panics or indexes out of bounds.
fn read_tlv(buf: &[u8]) -> Result<(Tlv<'_>, &[u8]), String> {
    if buf.len() < 2 {
        return Err("truncated TLV".to_string());
    }
    let tag = buf[0];
    let (len, len_bytes) = read_der_length(&buf[1..])?;
    let value_start = 1 + len_bytes;
    let value_end = value_start.checked_add(len).ok_or_else(|| "length overflow".to_string())?;
    if value_end > buf.len() {
        return Err("length exceeds buffer".to_string());
    }
    Ok((Tlv { tag, value: &buf[value_start..value_end] }, &buf[value_end..]))
}

/// Decode a DER length field (short or long form). Returns `(length, bytes_consumed)`.
fn read_der_length(buf: &[u8]) -> Result<(usize, usize), String> {
    let first = *buf.first().ok_or("truncated length")?;
    if first & 0x80 == 0 {
        return Ok((first as usize, 1));
    }
    let n = (first & 0x7F) as usize;
    if n == 0 || n > 8 {
        return Err("unsupported length form".to_string());
    }
    if buf.len() < 1 + n {
        return Err("truncated long-form length".to_string());
    }
    let mut len: u64 = 0;
    for &b in &buf[1..1 + n] {
        len = len.checked_shl(8).ok_or("length overflow")?.wrapping_add(b as u64);
    }
    Ok((len as usize, 1 + n))
}

/// Try PKCS#8 `PrivateKeyInfo ::= SEQUENCE { version INTEGER, privateKeyAlgorithm SEQUENCE { OID, ... },
/// privateKey OCTET STRING, ... }` — the modern, algorithm-agnostic private-key wrapper (what a
/// `-----BEGIN PRIVATE KEY-----` PEM label contains). Reports the OID's friendly name; RSA size is then
/// read from the *inner* PKCS#1 structure the OCTET STRING wraps (still never touching `d`/primes beyond
/// walking past them by length).
fn private_key_info_from_der(der: &[u8]) -> Result<KeyInfo, String> {
    let (outer, _) = read_tlv(der)?;
    if outer.tag != 0x30 {
        return Err("not a SEQUENCE".to_string());
    }
    let (_version, rest) = read_tlv(outer.value)?; // version INTEGER — skipped, not needed
    let (alg_id, rest) = read_tlv(rest)?;
    if alg_id.tag != 0x30 {
        return Err("missing AlgorithmIdentifier".to_string());
    }
    let (oid_tlv, alg_rest) = read_tlv(alg_id.value)?;
    if oid_tlv.tag != 0x06 {
        return Err("AlgorithmIdentifier missing OID".to_string());
    }
    let oid = x509_parser::der_parser::oid::Oid::new(oid_tlv.value.into());
    let algorithm = oid_friendly_name(&oid);

    let (key_octets, _) = read_tlv(rest)?;
    if key_octets.tag != 0x04 {
        return Err("missing privateKey OCTET STRING".to_string());
    }

    // RSA's OID (1.2.840.113549.1.1.1): the OCTET STRING wraps a PKCS#1 RSAPrivateKey SEQUENCE whose
    // second field (after `version`) is the modulus `n` — its bit length is the RSA key size.
    let size_bits = if algorithm.eq_ignore_ascii_case("rsaEncryption") || oid_tlv.value == [0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01] {
        rsa_modulus_bits_from_pkcs1(key_octets.value)
    } else {
        None
    };

    // EC's OID (1.2.840.10045.2.1): the curve is carried in AlgorithmIdentifier's parameters, a second
    // TLV after the OID (an OID naming the curve, for a named curve — the common case).
    let curve = if !alg_rest.is_empty() {
        read_tlv(alg_rest).ok().and_then(|(params, _)| {
            if params.tag == 0x06 {
                Some(oid_friendly_name(&x509_parser::der_parser::oid::Oid::new(params.value.into())))
            } else {
                None
            }
        })
    } else {
        None
    };

    Ok(KeyInfo { algorithm: friendly_key_algorithm_name(&algorithm), size_bits, curve })
}

/// PKCS#1 `RSAPrivateKey ::= SEQUENCE { version INTEGER, modulus INTEGER, publicExponent INTEGER, d
/// INTEGER, ... }` (what a `-----BEGIN RSA PRIVATE KEY-----` PEM label contains directly, or what the
/// PKCS#8 OCTET STRING wraps for an RSA key). Reads only the modulus's bit length — never the exponent
/// `d` or the primes that follow it.
fn rsa_modulus_bits_from_pkcs1(der: &[u8]) -> Option<u32> {
    let (seq, _) = read_tlv(der).ok()?;
    if seq.tag != 0x30 {
        return None;
    }
    let (_version, rest) = read_tlv(seq.value).ok()?;
    let (modulus, _) = read_tlv(rest).ok()?;
    if modulus.tag != 0x02 {
        return None;
    }
    Some(integer_bit_length(modulus.value))
}

/// Bit length of a DER `INTEGER`'s value bytes, accounting for the mandatory leading `0x00` DER pads on
/// an integer whose high bit would otherwise look negative.
fn integer_bit_length(bytes: &[u8]) -> u32 {
    let trimmed = {
        let mut b = bytes;
        while b.len() > 1 && b[0] == 0x00 {
            b = &b[1..];
        }
        b
    };
    if trimmed.is_empty() {
        return 0;
    }
    let leading_zeros = trimmed[0].leading_zeros();
    (trimmed.len() as u32) * 8 - leading_zeros
}

/// SEC1 `ECPrivateKey ::= SEQUENCE { version INTEGER, privateKey OCTET STRING, parameters [0] EXPLICIT
/// OID OPTIONAL, publicKey [1] BIT STRING OPTIONAL }` — a `-----BEGIN EC PRIVATE KEY-----` PEM label's
/// direct content (not PKCS#8-wrapped). Used as a fallback when [`private_key_info_from_der`] doesn't
/// recognize the outer structure as PKCS#8 (e.g. legacy RSA/EC PEM labels).
fn private_key_info_from_label_hint(label: &str, der: &[u8]) -> Result<KeyInfo, String> {
    match label {
        "RSA PRIVATE KEY" => {
            let bits = rsa_modulus_bits_from_pkcs1(der).ok_or("could not read RSA modulus")?;
            Ok(KeyInfo { algorithm: "RSA".to_string(), size_bits: Some(bits), curve: None })
        }
        "EC PRIVATE KEY" => {
            let (seq, _) = read_tlv(der)?;
            if seq.tag != 0x30 {
                return Err("not a SEQUENCE".to_string());
            }
            let (_version, rest) = read_tlv(seq.value)?;
            let (priv_octets, rest) = read_tlv(rest)?;
            if priv_octets.tag != 0x04 {
                return Err("missing privateKey OCTET STRING".to_string());
            }
            // field length in bytes -> bits is a reasonable size estimate even without the curve OID.
            let size_bits = Some((priv_octets.value.len() as u32) * 8);
            // parameters [0] EXPLICIT OID OPTIONAL — context tag 0xA0 wrapping an OID.
            let curve = read_tlv(rest).ok().and_then(|(ctx0, _)| {
                if ctx0.tag == 0xA0 {
                    read_tlv(ctx0.value).ok().and_then(|(oid_tlv, _)| {
                        if oid_tlv.tag == 0x06 {
                            Some(oid_friendly_name(&x509_parser::der_parser::oid::Oid::new(oid_tlv.value.into())))
                        } else {
                            None
                        }
                    })
                } else {
                    None
                }
            });
            Ok(KeyInfo { algorithm: "EC".to_string(), size_bits, curve })
        }
        "DSA PRIVATE KEY" => Ok(KeyInfo { algorithm: "DSA".to_string(), size_bits: None, curve: None }),
        "ENCRYPTED PRIVATE KEY" => Ok(KeyInfo { algorithm: "(encrypted)".to_string(), size_bits: None, curve: None }),
        _ => Err(format!("unrecognized private-key label \"{label}\"")),
    }
}

/// Map an OID short name to the friendly algorithm label used elsewhere in [`KeyInfo::algorithm`]
/// (matching [`key_info_from_spki`]'s `"RSA"`/`"EC"` naming rather than the raw OID short-name spelling).
fn friendly_key_algorithm_name(oid_short_name: &str) -> String {
    match oid_short_name {
        "rsaEncryption" => "RSA".to_string(),
        "id-ecPublicKey" => "EC".to_string(),
        "id-Ed25519" => "Ed25519".to_string(),
        "id-Ed448" => "Ed448".to_string(),
        "id-dsa" => "DSA".to_string(),
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------------------------
// Real, fully-valid fixtures (RSA self-signed cert, EC self-signed cert, DER, an expired cert, and a
// CSR) — generated ONCE with the local `openssl` CLI (see the PR description for the exact commands) and
// checked in under `testdata/certs/`, the same "generate once, embed via include_bytes!, no runtime CLI
// dependency" convention `thumb_font.rs`'s `DEMO_TTF` fixture uses. This keeps the test suite fully
// offline and 3-OS-CI-portable (no dependency on `openssl` being on PATH on every CI runner) while still
// exercising the real certificate/CSR/EC code paths end-to-end, not just the hand-built-DER private-key
// path below. All test-only, throwaway keys — never used for anything but decoding in these tests.
// ---------------------------------------------------------------------------------------------
#[cfg(test)]
const RSA_SELF_SIGNED_PEM: &[u8] = include_bytes!("testdata/certs/rsa_self_signed.pem");
#[cfg(test)]
const RSA_SELF_SIGNED_DER: &[u8] = include_bytes!("testdata/certs/rsa_self_signed.der");
#[cfg(test)]
const EC_SELF_SIGNED_PEM: &[u8] = include_bytes!("testdata/certs/ec_self_signed.pem");
#[cfg(test)]
const EXPIRED_CERT_PEM: &[u8] = include_bytes!("testdata/certs/expired.pem");
#[cfg(test)]
const CSR_PEM: &[u8] = include_bytes!("testdata/certs/request.csr.pem");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rsa_self_signed_pem_decodes_full_certificate_fields() {
        let p = cert_decode(RSA_SELF_SIGNED_PEM);
        assert!(p.error.is_none(), "well-formed RSA cert must not error: {:?}", p.error);
        assert_eq!(p.kind.as_deref(), Some("certificate"));
        assert_eq!(p.encoding.as_deref(), Some("PEM"));
        let c = p.certificate.expect("certificate must be set");
        assert!(c.subject.contains("rsa.example.test"), "subject: {}", c.subject);
        assert!(c.issuer.contains("rsa.example.test"), "self-signed: issuer == subject");
        assert_eq!(c.version, "v3");
        assert!(!c.serial.is_empty());
        assert_eq!(c.public_key.algorithm, "RSA");
        assert_eq!(c.public_key.size_bits, Some(2048));
        assert!(!c.expired, "10-year cert issued today must not be expired");
        assert!(!c.not_yet_valid);
        assert!(c.subject_alt_names.iter().any(|s| s.contains("rsa.example.test")), "SANs: {:?}", c.subject_alt_names);
        assert!(c.subject_alt_names.iter().any(|s| s.contains("www.rsa.example.test")));
        assert_eq!(c.sha256_fingerprint.len(), 64, "SHA-256 fingerprint must be 32 bytes of hex");
        assert_eq!(c.sha1_fingerprint.len(), 40, "SHA-1 fingerprint must be 20 bytes of hex");
        assert!(c.sha256_fingerprint.chars().all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase()));
        assert!(c.signature_algorithm.to_lowercase().contains("rsa"), "sig alg: {}", c.signature_algorithm);
    }

    #[test]
    fn rsa_self_signed_der_decodes_identically_to_pem() {
        let p = cert_decode(RSA_SELF_SIGNED_DER);
        assert!(p.error.is_none(), "well-formed DER cert must not error: {:?}", p.error);
        assert_eq!(p.encoding.as_deref(), Some("DER"));
        let c = p.certificate.expect("certificate must be set");
        assert!(c.subject.contains("rsa.example.test"));
        assert_eq!(c.public_key.size_bits, Some(2048));
    }

    #[test]
    fn ec_self_signed_pem_reports_curve_and_algorithm() {
        let p = cert_decode(EC_SELF_SIGNED_PEM);
        assert!(p.error.is_none(), "well-formed EC cert must not error: {:?}", p.error);
        let c = p.certificate.expect("certificate must be set");
        assert!(c.subject.contains("ec.example.test"));
        assert_eq!(c.public_key.algorithm, "EC");
        assert_eq!(c.public_key.size_bits, Some(256), "P-256 must report a 256-bit key");
        assert!(c.subject_alt_names.iter().any(|s| s.contains("ec.example.test")));
    }

    #[test]
    fn expired_cert_flags_expired_true() {
        let p = cert_decode(EXPIRED_CERT_PEM);
        assert!(p.error.is_none(), "well-formed (if expired) cert must not error: {:?}", p.error);
        let c = p.certificate.expect("certificate must be set");
        assert!(c.expired, "a cert valid only in Jan 2020 must be reported expired");
        assert!(!c.not_yet_valid);
        assert!(c.not_before.starts_with("2020-01-01"));
        assert!(c.not_after.starts_with("2020-01-02"));
    }

    #[test]
    fn csr_pem_decodes_subject_sans_and_public_key() {
        let p = cert_decode(CSR_PEM);
        assert!(p.error.is_none(), "well-formed CSR must not error: {:?}", p.error);
        assert_eq!(p.kind.as_deref(), Some("csr"));
        let csr = p.csr.expect("csr must be set");
        assert!(csr.subject.contains("csr.example.test"), "subject: {}", csr.subject);
        assert!(csr.requested_sans.iter().any(|s| s.contains("csr.example.test")), "SANs: {:?}", csr.requested_sans);
        assert!(csr.requested_sans.iter().any(|s| s.contains("alt.csr.example.test")));
        assert_eq!(csr.public_key.algorithm, "RSA");
        assert_eq!(csr.public_key.size_bits, Some(2048));
    }

    // ---------------------------------------------------------------------------------------------
    // Small hand-built DER fixtures for the private-key path — `x509-parser` doesn't parse private-key
    // formats at all (out of its X.509 scope, see the module doc comment), so there's no "real" parser
    // to exercise beyond this module's own hand-rolled TLV walker; these fixtures are just enough
    // well-formed ASN.1 to walk that walker's real field-extraction logic.
    // ---------------------------------------------------------------------------------------------

    fn der_len(len: usize) -> Vec<u8> {
        if len < 0x80 {
            vec![len as u8]
        } else {
            let bytes = len.to_be_bytes();
            let significant: Vec<u8> = bytes.iter().copied().skip_while(|&b| b == 0).collect();
            let mut out = vec![0x80 | significant.len() as u8];
            out.extend(significant);
            out
        }
    }

    fn der_tlv(tag: u8, value: &[u8]) -> Vec<u8> {
        let mut out = vec![tag];
        out.extend(der_len(value.len()));
        out.extend_from_slice(value);
        out
    }

    /// A minimal-but-real PKCS#1 `RSAPrivateKey` DER: version=0, a 256-byte (2048-bit) modulus with the
    /// high bit set (so no leading-zero-pad ambiguity), a small odd exponent, and zeroed placeholders for
    /// the remaining required fields (`d`, `p`, `q`, `dP`, `dQ`, `qInv`) — this decoder never reads past
    /// `modulus`, so their content doesn't matter, only that the SEQUENCE stays well-formed.
    fn rsa_pkcs1_der(modulus_bits: usize) -> Vec<u8> {
        let mut modulus = vec![0u8; modulus_bits / 8];
        modulus[0] = 0x80; // ensure the top bit is set -> exact bit length, no ambiguity
        let version = der_tlv(0x02, &[0x00]);
        let n = der_tlv(0x02, &modulus);
        let e = der_tlv(0x02, &[0x01, 0x00, 0x01]); // 65537
        let placeholder = der_tlv(0x02, &[0x01]);
        let mut body = Vec::new();
        body.extend(version);
        body.extend(n);
        body.extend(e);
        for _ in 0..5 {
            body.extend(placeholder.clone());
        }
        der_tlv(0x30, &body)
    }

    /// Wrap PKCS#1 RSA key bytes in a PKCS#8 `PrivateKeyInfo` (the shape a `-----BEGIN PRIVATE KEY-----`
    /// label carries): `SEQUENCE { version INTEGER, AlgorithmIdentifier { rsaEncryption OID, NULL },
    /// privateKey OCTET STRING(pkcs1) }`.
    fn pkcs8_wrap_rsa(pkcs1: &[u8]) -> Vec<u8> {
        let rsa_oid: [u8; 11] = [0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01];
        let null = der_tlv(0x05, &[]);
        let mut alg_id_body = Vec::new();
        alg_id_body.extend_from_slice(&rsa_oid);
        alg_id_body.extend(null);
        let alg_id = der_tlv(0x30, &alg_id_body);
        let version = der_tlv(0x02, &[0x00]);
        let octets = der_tlv(0x04, pkcs1);
        let mut body = Vec::new();
        body.extend(version);
        body.extend(alg_id);
        body.extend(octets);
        der_tlv(0x30, &body)
    }

    fn to_pem(label: &str, der: &[u8]) -> String {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        let b64 = STANDARD.encode(der);
        let mut body = String::new();
        for chunk in b64.as_bytes().chunks(64) {
            body.push_str(std::str::from_utf8(chunk).unwrap());
            body.push('\n');
        }
        format!("-----BEGIN {label}-----\n{body}-----END {label}-----\n")
    }

    #[test]
    fn empty_input_is_reported_not_panicked() {
        let p = cert_decode(&[]);
        assert!(p.error.is_some());
    }

    #[test]
    fn garbage_bytes_never_panics() {
        let p = cert_decode(b"this is not a certificate at all, just some text bytes 0123456789");
        assert!(p.error.is_some());
    }

    #[test]
    fn garbage_der_never_panics() {
        let p = cert_decode(&[0x30, 0x82, 0xFF, 0xFF, 0x01, 0x02, 0x03]);
        assert!(p.error.is_some());
    }

    #[test]
    fn oversized_input_is_rejected() {
        let huge = vec![0u8; MAX_INPUT_BYTES + 1];
        let p = cert_decode(&huge);
        assert!(p.error.is_some());
    }

    #[test]
    fn pkcs8_rsa_private_key_pem_reports_algorithm_and_size_only() {
        let pkcs1 = rsa_pkcs1_der(2048);
        let der = pkcs8_wrap_rsa(&pkcs1);
        let pem = to_pem("PRIVATE KEY", &der);
        let p = cert_decode(pem.as_bytes());
        assert_eq!(p.kind.as_deref(), Some("private_key"));
        let key = p.private_key.expect("private_key must be set");
        assert_eq!(key.algorithm, "RSA");
        assert_eq!(key.size_bits, Some(2048));
        assert!(p.error.is_none());
    }

    #[test]
    fn pkcs1_rsa_private_key_pem_legacy_label_reports_size() {
        let pkcs1 = rsa_pkcs1_der(1024);
        let pem = to_pem("RSA PRIVATE KEY", &pkcs1);
        let p = cert_decode(pem.as_bytes());
        assert_eq!(p.kind.as_deref(), Some("private_key"));
        let key = p.private_key.expect("private_key must be set");
        assert_eq!(key.algorithm, "RSA");
        assert_eq!(key.size_bits, Some(1024));
    }

    #[test]
    fn malformed_private_key_pem_never_panics() {
        let pem = to_pem("PRIVATE KEY", b"not a valid der sequence at all");
        let p = cert_decode(pem.as_bytes());
        assert!(p.error.is_some());
    }

    #[test]
    fn der_length_reader_rejects_truncated_and_overflowing_lengths_without_panicking() {
        // Long-form length claiming 8 bytes but only 2 are present.
        assert!(read_der_length(&[0x88, 0x01, 0x02]).is_err());
        // Long-form length of exactly 8 0xFF bytes: parses as a huge value but must not panic — the
        // caller (`read_tlv`) is what rejects it once the claimed length exceeds the buffer.
        let big = read_der_length(&[0x88, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        assert!(big.is_ok());
        // read_tlv on that same buffer (no value bytes to satisfy the claimed length) must error, not panic.
        assert!(read_tlv(&[0x04, 0x88, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]).is_err());
    }

    #[test]
    fn unrecognized_pem_label_is_reported_not_panicked() {
        let pem = to_pem("SOMETHING WEIRD", b"whatever");
        let p = cert_decode(pem.as_bytes());
        assert!(p.error.is_some());
    }

    #[test]
    fn integer_bit_length_handles_der_zero_pad() {
        // A DER INTEGER with a leading 0x00 pad byte (because the true high bit is set) followed by a
        // byte whose top bit is 1 — the pad must be stripped so the reported bit length matches the real
        // value, not the padded encoding.
        assert_eq!(integer_bit_length(&[0x00, 0x80]), 8);
        assert_eq!(integer_bit_length(&[0x01]), 1);
        assert_eq!(integer_bit_length(&[0x00, 0x00, 0x01]), 1);
        assert_eq!(integer_bit_length(&[]), 0);
    }
}
