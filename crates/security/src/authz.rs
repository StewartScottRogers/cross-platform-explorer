//! Authorization providers (CPE-818) plugging into the [`Authorizer`] plane trait.
//!
//! Two providers, composed `all-must-pass`, decide whether a [`Principal`] may run *this
//! op on this resource* — the file-explorer-specific risk surface:
//!
//! - [`PathScopeAuthorizer`] — the resource path must resolve to somewhere under one of the
//!   granted roots. Traversal that escapes a root (`..`, absolute re-rooting) is denied.
//! - [`CapabilityAuthorizer`] — the op's required capability must be among the ones granted
//!   to the principal.
//!
//! Each [`Abstain`](Verdict::Abstain)s when it does not apply (an op with no resource, or a
//! method that requires no capability) so composing them `all-must-pass` guards path ops
//! *and* capability ops without one plane spuriously denying the other's requests.
//!
//! **Containment is lexical**, not filesystem-canonical: it resolves `.`/`..` purely on the
//! path string without touching disk, so it is deterministic and headless-testable and does
//! not follow symlinks. A real remote deployment should additionally canonicalize against
//! the filesystem at the socket boundary (CPE-820); this provider is the policy core.

use std::collections::{BTreeMap, BTreeSet};

use crate::{Authorizer, Principal, SecurityContext, Verdict};

/// Normalize a path into its meaningful segments, resolving `.` and `..` lexically. Splits
/// on both `/` and `\` so it is agnostic to the caller's separator. Returns `None` if a `..`
/// would pop above the root — i.e. the path escapes upward — which callers treat as a denial.
fn normalize_segments(path: &str) -> Option<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for raw in path.split(['/', '\\']) {
        match raw {
            "" | "." => continue,
            // `?` bails to `None` when `..` would pop above the root — an upward escape.
            ".." => {
                out.pop()?;
            }
            seg => out.push(seg.to_string()),
        }
    }
    Some(out)
}

/// True if `root` segments are a prefix of `resource` segments (so `resource` is at or under
/// `root`).
fn is_within(root: &[String], resource: &[String]) -> bool {
    resource.len() >= root.len() && root.iter().zip(resource).all(|(a, b)| a == b)
}

/// Authorizes an op by requiring its resource path to sit under one of the granted roots.
/// Abstains for ops that carry no resource.
pub struct PathScopeAuthorizer {
    name: String,
    roots: Vec<Vec<String>>,
}

impl PathScopeAuthorizer {
    /// Grant access under each of `roots` (each normalized lexically).
    pub fn new<I, S>(roots: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let roots = roots
            .into_iter()
            .filter_map(|r| normalize_segments(r.as_ref()))
            .collect();
        Self {
            name: "path_scope".to_string(),
            roots,
        }
    }
}

impl Authorizer for PathScopeAuthorizer {
    fn name(&self) -> &str {
        &self.name
    }

    fn authorize(&self, ctx: &SecurityContext) -> Verdict {
        let resource = match &ctx.resource {
            Some(r) => r,
            None => return Verdict::Abstain, // op has no path — path-scope doesn't apply
        };
        let segments = match normalize_segments(resource) {
            Some(s) => s,
            None => return Verdict::deny("path_scope: path escapes above a granted root"),
        };
        if self.roots.iter().any(|root| is_within(root, &segments)) {
            Verdict::Allow
        } else {
            Verdict::deny("path_scope: resource is outside every granted root")
        }
    }
}

/// Authorizes an op by requiring the capability it needs to be among those granted to the
/// principal. Abstains for methods that declare no required capability.
pub struct CapabilityAuthorizer {
    name: String,
    /// method → capability it requires.
    required: BTreeMap<String, String>,
    /// principal id → capabilities granted to that principal.
    grants: BTreeMap<String, BTreeSet<String>>,
}

impl CapabilityAuthorizer {
    pub fn new(
        required: BTreeMap<String, String>,
        grants: BTreeMap<String, BTreeSet<String>>,
    ) -> Self {
        Self {
            name: "capability".to_string(),
            required,
            grants,
        }
    }
}

impl Authorizer for CapabilityAuthorizer {
    fn name(&self) -> &str {
        &self.name
    }

    fn authorize(&self, ctx: &SecurityContext) -> Verdict {
        let needed = match self.required.get(&ctx.method) {
            Some(cap) => cap,
            None => return Verdict::Abstain, // this method needs no capability
        };
        let granted = self.grants.get(&ctx.principal.id);
        if granted.is_some_and(|caps| caps.contains(needed)) {
            Verdict::Allow
        } else {
            Verdict::deny(format!("capability: {needed:?} not granted to principal"))
        }
    }
}

/// Convenience: a `Principal` with the given id (no display name).
pub fn principal(id: &str) -> Principal {
    Principal {
        id: id.into(),
        display_name: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AuditSink, CombinePolicy, Decision, MemoryAudit, Plane, PlaneConfig, ProviderRegistry,
        SecurityConfig, SecurityContext, PASSTHROUGH,
    };
    use std::sync::Arc;

    fn authz_ctx(principal_id: &str, method: &str, resource: Option<&str>) -> SecurityContext {
        let mut ctx = SecurityContext::new(principal(principal_id), method);
        ctx.resource = resource.map(|r| r.to_string());
        ctx
    }

    #[test]
    fn path_scope_allows_within_and_denies_outside() {
        let az = PathScopeAuthorizer::new(["/srv/data"]);

        assert_eq!(
            az.authorize(&authz_ctx("u", "read", Some("/srv/data/file.txt"))),
            Verdict::Allow
        );
        assert_eq!(
            az.authorize(&authz_ctx("u", "read", Some("/srv/data/sub/deep/file.txt"))),
            Verdict::Allow
        );
        // A sibling outside the root.
        assert!(matches!(
            az.authorize(&authz_ctx("u", "read", Some("/srv/other/file.txt"))),
            Verdict::Deny(_)
        ));
    }

    #[test]
    fn path_scope_denies_traversal_escapes() {
        let az = PathScopeAuthorizer::new(["/srv/data"]);
        for escape in [
            "/srv/data/../../etc/passwd",
            "/srv/data/../secret",
            "/srv/../etc",
            "/../etc/passwd",
            "/srv/data/sub/../../../etc",
        ] {
            assert!(
                matches!(az.authorize(&authz_ctx("u", "read", Some(escape))), Verdict::Deny(_)),
                "escape not denied: {escape}"
            );
        }
        // A legitimate `..` that stays within the root is fine.
        assert_eq!(
            az.authorize(&authz_ctx("u", "read", Some("/srv/data/sub/../keep.txt"))),
            Verdict::Allow
        );
    }

    #[test]
    fn path_scope_is_separator_agnostic() {
        let az = PathScopeAuthorizer::new(["/srv/data"]);
        assert_eq!(
            az.authorize(&authz_ctx("u", "read", Some("\\srv\\data\\win.txt"))),
            Verdict::Allow
        );
    }

    #[test]
    fn path_scope_abstains_without_a_resource() {
        let az = PathScopeAuthorizer::new(["/srv/data"]);
        assert_eq!(az.authorize(&authz_ctx("u", "ping", None)), Verdict::Abstain);
    }

    #[test]
    fn capability_requires_the_grant() {
        let mut required = BTreeMap::new();
        required.insert("delete".to_string(), "fs.delete".to_string());
        let mut grants = BTreeMap::new();
        grants.insert(
            "alice".to_string(),
            BTreeSet::from(["fs.delete".to_string(), "fs.read".to_string()]),
        );
        let az = CapabilityAuthorizer::new(required, grants);

        // Has the grant.
        assert_eq!(
            az.authorize(&authz_ctx("alice", "delete", Some("/x"))),
            Verdict::Allow
        );
        // Lacks the grant.
        assert!(matches!(
            az.authorize(&authz_ctx("bob", "delete", Some("/x"))),
            Verdict::Deny(_)
        ));
        // Method requires no capability → abstain.
        assert_eq!(
            az.authorize(&authz_ctx("bob", "list", Some("/x"))),
            Verdict::Abstain
        );
    }

    // The headline AC: path-scope AND capability, all-must-pass, end to end + audited.
    fn all_must_pass_chain() -> (crate::SecurityChain, Arc<MemoryAudit>) {
        let mut reg = ProviderRegistry::with_builtins();
        reg.register_authz("path_scope", || {
            Box::new(PathScopeAuthorizer::new(["/srv/data"]))
        });
        reg.register_authz("capability", || {
            let mut required = BTreeMap::new();
            required.insert("delete".to_string(), "fs.delete".to_string());
            let mut grants = BTreeMap::new();
            grants.insert("alice".to_string(), BTreeSet::from(["fs.delete".to_string()]));
            Box::new(CapabilityAuthorizer::new(required, grants))
        });
        let config = SecurityConfig {
            transport: PlaneConfig {
                policy: CombinePolicy::FirstMatch,
                providers: vec![PASSTHROUGH.into()],
            },
            authentication: PlaneConfig {
                policy: CombinePolicy::FirstMatch,
                providers: vec![PASSTHROUGH.into()],
            },
            authorization: PlaneConfig {
                policy: CombinePolicy::AllMustPass,
                providers: vec!["path_scope".into(), "capability".into()],
            },
        };
        let audit = Arc::new(MemoryAudit::new());
        struct ArcAudit(Arc<MemoryAudit>);
        impl AuditSink for ArcAudit {
            fn record(&self, d: &crate::AuditDecision) {
                self.0.record(d);
            }
        }
        let chain = reg
            .build(&config, Box::new(ArcAudit(audit.clone())))
            .unwrap();
        (chain, audit)
    }

    #[test]
    fn all_must_pass_needs_both_path_and_capability() {
        // Passthrough authn leaves the principal as local; grant to "local" for this test.
        // Use alice explicitly by seeding the principal.
        let (chain, _audit) = all_must_pass_chain();
        let mut ok = SecurityContext {
            principal: principal("alice"),
            method: "delete".into(),
            resource: Some("/srv/data/old.txt".into()),
            is_local: false,
            attributes: Default::default(),
        };
        assert!(chain.evaluate(&mut ok).is_allowed());

        // In-scope path but principal lacks the capability → deny at Authorization.
        let (chain, _) = all_must_pass_chain();
        let mut no_cap = SecurityContext {
            principal: principal("bob"),
            method: "delete".into(),
            resource: Some("/srv/data/old.txt".into()),
            is_local: false,
            attributes: Default::default(),
        };
        match chain.evaluate(&mut no_cap) {
            Decision::Deny(d) => assert_eq!(d.plane, Plane::Authorization),
            other => panic!("missing capability must deny: {other:?}"),
        }

        // Has the capability but the path escapes the root → deny at Authorization, audited.
        let (chain, audit2) = all_must_pass_chain();
        let mut escape = SecurityContext {
            principal: principal("alice"),
            method: "delete".into(),
            resource: Some("/srv/data/../../etc/passwd".into()),
            is_local: false,
            attributes: Default::default(),
        };
        match chain.evaluate(&mut escape) {
            Decision::Deny(d) => {
                assert_eq!(d.plane, Plane::Authorization);
                assert_eq!(d.provider.as_deref(), Some("path_scope"));
            }
            other => panic!("path escape must deny: {other:?}"),
        }
        assert!(audit2.decisions().iter().any(|d| !d.allowed));
    }
}
