//! Transport-security providers (CPE-818) plugging into the [`TransportSecurity`] plane.
//!
//! These are the **policy** side of channel crypto: they assert requirements about the
//! channel over transport attributes that the socket layer (the Client/Server binaries,
//! CPE-820) populates after the real TLS/mTLS handshake — `tls.established` and the
//! verified `tls.client_cert.subject`. Keeping the requirement checks here (and the raw
//! rustls/socket setup at the edge) keeps this crate transport-agnostic and headless.
//!
//! - [`RequireTls`] — the channel must be TLS-encrypted.
//! - [`RequireMtls`] — the channel must be TLS-encrypted **and** carry a verified client
//!   certificate (mutual TLS).
//!
//! Both short-circuit to [`Allow`](Verdict::Allow) for an in-process local request
//! (`ctx.is_local`), so local mode stays null/passthrough and pays nothing — even if a
//! remote-shaped config is reused locally.

use crate::{SecurityContext, TransportSecurity, Verdict};

/// Context attribute the socket layer sets to `"true"` once a TLS session is established.
pub const ATTR_TLS_ESTABLISHED: &str = "tls.established";
/// Context attribute the socket layer sets to the verified peer-certificate subject after a
/// successful mutual-TLS handshake.
pub const ATTR_CLIENT_CERT_SUBJECT: &str = "tls.client_cert.subject";

fn tls_established(ctx: &SecurityContext) -> bool {
    ctx.attributes
        .get(ATTR_TLS_ESTABLISHED)
        .is_some_and(|v| v == "true")
}

/// Requires the channel to be TLS-encrypted (unless the request is local in-process).
#[derive(Debug, Default, Clone, Copy)]
pub struct RequireTls;

impl TransportSecurity for RequireTls {
    fn name(&self) -> &str {
        "require_tls"
    }

    fn check(&self, ctx: &SecurityContext) -> Verdict {
        if ctx.is_local {
            return Verdict::Allow; // local = trusted in-process channel
        }
        if tls_established(ctx) {
            Verdict::Allow
        } else {
            Verdict::deny("transport: TLS is required on this channel")
        }
    }
}

/// Requires mutual TLS: an established TLS session **and** a verified client-certificate
/// subject (unless the request is local in-process).
#[derive(Debug, Default, Clone, Copy)]
pub struct RequireMtls;

impl TransportSecurity for RequireMtls {
    fn name(&self) -> &str {
        "require_mtls"
    }

    fn check(&self, ctx: &SecurityContext) -> Verdict {
        if ctx.is_local {
            return Verdict::Allow;
        }
        if !tls_established(ctx) {
            return Verdict::deny("transport: mutual TLS requires an established TLS session");
        }
        let has_cert = ctx
            .attributes
            .get(ATTR_CLIENT_CERT_SUBJECT)
            .is_some_and(|s| !s.is_empty());
        if has_cert {
            Verdict::Allow
        } else {
            Verdict::deny("transport: a verified client certificate is required")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Principal;

    fn remote(method: &str) -> SecurityContext {
        SecurityContext::new(Principal::local(), method)
    }

    #[test]
    fn require_tls_allows_only_encrypted_channels() {
        let p = RequireTls;
        // No TLS attribute on a remote request → deny.
        assert!(matches!(p.check(&remote("op")), Verdict::Deny(_)));
        // TLS established → allow.
        let ctx = remote("op").with_attribute(ATTR_TLS_ESTABLISHED, "true");
        assert_eq!(p.check(&ctx), Verdict::Allow);
    }

    #[test]
    fn require_mtls_needs_tls_and_a_client_cert() {
        let p = RequireMtls;
        // Plain remote → deny.
        assert!(matches!(p.check(&remote("op")), Verdict::Deny(_)));
        // TLS but no client cert → deny.
        let tls_only = remote("op").with_attribute(ATTR_TLS_ESTABLISHED, "true");
        assert!(matches!(p.check(&tls_only), Verdict::Deny(_)));
        // TLS + verified client cert → allow.
        let mtls = remote("op")
            .with_attribute(ATTR_TLS_ESTABLISHED, "true")
            .with_attribute(ATTR_CLIENT_CERT_SUBJECT, "CN=alice");
        assert_eq!(p.check(&mtls), Verdict::Allow);
    }

    #[test]
    fn local_requests_bypass_transport_requirements() {
        // Local mode stays null/passthrough even under a strict transport provider.
        let ctx = SecurityContext::local("op");
        assert_eq!(RequireTls.check(&ctx), Verdict::Allow);
        assert_eq!(RequireMtls.check(&ctx), Verdict::Allow);
    }
}
