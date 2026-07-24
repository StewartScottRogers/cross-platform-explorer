//! # cpe-security
//!
//! The pluggable, composable **security core** at the Cross-Platform Explorer contract
//! boundary (epic CPE-810, ticket CPE-816). Security is *not* baked into the transport
//! or into any of the ~113 commands: it sits here as a distinct layer of composable
//! providers behind common traits, so the Server logic stays security-agnostic and only
//! ever receives an *already-authorized* request plus a principal context.
//!
//! ## Three planes
//!
//! | Plane | Trait | Concern |
//! |-------|-------|---------|
//! | Transport security | [`TransportSecurity`] | authenticate/verify the channel |
//! | Authentication     | [`Authenticator`]     | *who* is this client? (establishes the [`Principal`]) |
//! | Authorization      | [`Authorizer`]        | may *this principal* do *this op on this resource*? |
//!
//! Each plane is an **ordered interceptor chain** with a [`CombinePolicy`]
//! (`all-must-pass` / `any-passes` / `first-match`), so multiple providers run at once —
//! e.g. accept **either** an API key **or** an OAuth token (AuthN = `AnyPasses`) **while**
//! always enforcing path-scoping (AuthZ = `AllMustPass`).
//!
//! ## Two non-negotiable invariants (both tested)
//!
//! - **Default-deny at the boundary.** An unconfigured plane (no providers, or every
//!   provider [abstains][Verdict::Abstain]) *denies*. It is structurally impossible to
//!   leave the boundary open by forgetting to configure it — see [`SecurityChain::default_deny`].
//! - **Local = null/passthrough.** In-process local mode uses [`SecurityChain::local`],
//!   a trusted principal with no transport crypto, so the plain explorer stays fast:
//!   security bills only in remote mode.
//!
//! ## Config-driven, no core changes to extend
//!
//! Adding a provider = implement the plane trait + [`ProviderRegistry::register_*`] it,
//! then name it in [`SecurityConfig`]. Which providers are active, their order, and the
//! per-plane combine policy are all data ([`ProviderRegistry::build`]). Concrete providers
//! (API-token / mTLS / OAuth AuthN, path-scope AuthZ, TLS transport) land in CPE-817/818;
//! this crate is the core those plug into.
//!
//! Every terminal decision is offered to an [`AuditSink`] so the boundary is observable;
//! the app bridges that to `audit_journal.rs` at integration time (CPE-818/820).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use cpe_contract::Principal;

/// Concrete authentication providers (API token, mTLS identity, OAuth/OIDC) that plug into
/// the [`Authenticator`] plane (CPE-817).
pub mod authn;

/// Concrete authorization providers (path-scope, capability grant) that plug into the
/// [`Authorizer`] plane (CPE-818).
pub mod authz;

/// Concrete transport-security policy providers (require TLS / mutual TLS) that plug into the
/// [`TransportSecurity`] plane (CPE-818).
pub mod transport;

/// Concrete HS256 JWT [`authn::TokenVerifier`] for the OIDC authenticator — feature-gated (`jwt`) + OFF by
/// default so the security core stays crypto-free; the JWT machinery is opt-in (CPE-965).
#[cfg(feature = "jwt")]
pub mod jwt;

// ---------------------------------------------------------------------------
// Core decision types
// ---------------------------------------------------------------------------

/// What a single provider decides about a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// This provider affirmatively permits the request.
    Allow,
    /// This provider rejects the request, with a non-secret reason.
    Deny(String),
    /// This provider does not apply and defers to the others. An all-abstain plane
    /// denies (default-deny), so abstaining is never a way to *grant* access.
    Abstain,
}

impl Verdict {
    /// Convenience for a deny with a formatted reason.
    pub fn deny(reason: impl Into<String>) -> Self {
        Verdict::Deny(reason.into())
    }
}

/// How a plane's ordered provider chain combines its providers' verdicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombinePolicy {
    /// Every non-abstaining provider must [`Allow`](Verdict::Allow); any
    /// [`Deny`](Verdict::Deny) fails the plane. At least one provider must affirmatively
    /// allow (all-abstain denies). Use for AuthZ (e.g. path-scope *and* capability).
    AllMustPass,
    /// At least one provider must [`Allow`](Verdict::Allow); the first that does wins and
    /// short-circuits (so a later provider can't overwrite an established principal). Use
    /// for AuthN (e.g. API key *or* OAuth).
    AnyPasses,
    /// The first non-[abstaining](Verdict::Abstain) provider decides, allow or deny.
    FirstMatch,
}

/// Which plane produced an outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Plane {
    Transport,
    Authentication,
    Authorization,
}

/// The result of combining one plane's provider chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaneOutcome {
    pub verdict: Verdict,
    /// Name of the provider that decided the plane (if any non-abstaining one did).
    pub decided_by: Option<String>,
}

// ---------------------------------------------------------------------------
// Request context flowing through the chain
// ---------------------------------------------------------------------------

/// The security context for one request as it flows through the planes. Authenticators
/// may replace [`principal`](SecurityContext::principal); authorizers read it. Concrete
/// providers read credential/transport material from [`attributes`](SecurityContext::attributes)
/// so this core stays free of provider-specific fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityContext {
    /// The principal the request currently acts as. Starts from the envelope's session
    /// (local ⇒ [`Principal::local`]); an [`Authenticator`] may establish a real one.
    pub principal: Principal,
    /// The contract method being called (e.g. `list_dir`).
    pub method: String,
    /// The resource the operation targets, when it has one (e.g. a filesystem path) —
    /// what path-scope authorization keys off.
    pub resource: Option<String>,
    /// True for the trusted in-process local path (drives the passthrough fast path).
    pub is_local: bool,
    /// Opaque, provider-specific material (credential tokens, peer cert subject, TLS
    /// state, …). Kept generic so adding a provider needs no change to this struct.
    pub attributes: BTreeMap<String, String>,
}

impl SecurityContext {
    /// A remote request context with the given (proposed) principal.
    pub fn new(principal: Principal, method: impl Into<String>) -> Self {
        Self {
            principal,
            method: method.into(),
            resource: None,
            is_local: false,
            attributes: BTreeMap::new(),
        }
    }

    /// A trusted local (in-process) request — the null/passthrough fast path.
    pub fn local(method: impl Into<String>) -> Self {
        Self {
            principal: Principal::local(),
            method: method.into(),
            resource: None,
            is_local: true,
            attributes: BTreeMap::new(),
        }
    }

    /// Builder: set the targeted resource (path).
    pub fn with_resource(mut self, resource: impl Into<String>) -> Self {
        self.resource = Some(resource.into());
        self
    }

    /// Builder: attach a provider attribute (credential, transport metadata).
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }
}

// ---------------------------------------------------------------------------
// Plane traits — a provider implements one of these and is registered by name.
// ---------------------------------------------------------------------------

/// Transport-security provider: authenticate/verify the channel itself (TLS, mTLS,
/// Noise, …). Local mode uses a passthrough that trusts the in-process channel.
pub trait TransportSecurity: Send + Sync {
    fn name(&self) -> &str;
    fn check(&self, ctx: &SecurityContext) -> Verdict;
}

/// Authentication provider: decide *who* the client is. On [`Allow`](Verdict::Allow) it
/// may replace `ctx.principal` with the established identity.
pub trait Authenticator: Send + Sync {
    fn name(&self) -> &str;
    fn authenticate(&self, ctx: &mut SecurityContext) -> Verdict;
}

/// Authorization provider: decide whether `ctx.principal` may perform `ctx.method` on
/// `ctx.resource`.
pub trait Authorizer: Send + Sync {
    fn name(&self) -> &str;
    fn authorize(&self, ctx: &SecurityContext) -> Verdict;
}

// ---------------------------------------------------------------------------
// The combine algorithm (shared by all three planes)
// ---------------------------------------------------------------------------

/// Combine an ordered provider chain under `policy`, using `name_of` to label the
/// deciding provider and `eval` to evaluate each provider (lazily, so `AnyPasses` /
/// `FirstMatch` short-circuit and side-effecting authenticators don't run past the
/// decision).
///
/// **Default-deny** is structural: an empty chain, or one where every provider abstains,
/// returns [`Verdict::Deny`] regardless of `policy`.
pub fn combine<T: ?Sized>(
    policy: CombinePolicy,
    providers: &[Box<T>],
    name_of: impl Fn(&T) -> String,
    mut eval: impl FnMut(&T) -> Verdict,
) -> PlaneOutcome {
    if providers.is_empty() {
        return PlaneOutcome {
            verdict: Verdict::deny("default-deny: no providers configured for this plane"),
            decided_by: None,
        };
    }

    let mut allowed_by: Option<String> = None;
    let mut first_deny: Option<(String, String)> = None; // (provider, reason)

    for p in providers {
        let name = name_of(p);
        match eval(p) {
            Verdict::Allow => match policy {
                // First allow wins and short-circuits.
                CombinePolicy::AnyPasses | CombinePolicy::FirstMatch => {
                    return PlaneOutcome {
                        verdict: Verdict::Allow,
                        decided_by: Some(name),
                    };
                }
                // Need everyone; remember we saw an affirmative allow and keep going.
                CombinePolicy::AllMustPass => {
                    allowed_by.get_or_insert(name);
                }
            },
            Verdict::Deny(reason) => match policy {
                // A single deny fails an all-must-pass plane immediately.
                CombinePolicy::AllMustPass | CombinePolicy::FirstMatch => {
                    return PlaneOutcome {
                        verdict: Verdict::Deny(reason),
                        decided_by: Some(name),
                    };
                }
                // Under any-passes a deny doesn't end it — a later provider may allow.
                CombinePolicy::AnyPasses => {
                    first_deny.get_or_insert((name, reason));
                }
            },
            Verdict::Abstain => {}
        }
    }

    // Fell through without a short-circuit decision.
    match policy {
        CombinePolicy::AllMustPass => match allowed_by {
            Some(name) => PlaneOutcome {
                verdict: Verdict::Allow,
                decided_by: Some(name),
            },
            None => PlaneOutcome {
                verdict: Verdict::deny("default-deny: every provider abstained"),
                decided_by: None,
            },
        },
        CombinePolicy::AnyPasses => match first_deny {
            Some((name, reason)) => PlaneOutcome {
                verdict: Verdict::Deny(reason),
                decided_by: Some(name),
            },
            None => PlaneOutcome {
                verdict: Verdict::deny("default-deny: no provider allowed"),
                decided_by: None,
            },
        },
        CombinePolicy::FirstMatch => PlaneOutcome {
            verdict: Verdict::deny("default-deny: every provider abstained"),
            decided_by: None,
        },
    }
}

// ---------------------------------------------------------------------------
// Audit hook
// ---------------------------------------------------------------------------

/// A terminal security decision, handed to the [`AuditSink`] on every request so the
/// boundary is observable. Never carries secret values (only a non-secret reason).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditDecision {
    pub allowed: bool,
    /// The plane that denied (`None` when allowed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plane: Option<Plane>,
    /// The provider that decided (`None` for a default-deny with no provider).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    pub principal: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Sink for security decisions. The app implements this to bridge into
/// `audit_journal.rs` (CPE-818/820); tests and local mode can use [`NullAudit`].
pub trait AuditSink: Send + Sync {
    fn record(&self, decision: &AuditDecision);
}

/// An audit sink that discards decisions — the default for the local fast path.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullAudit;

impl AuditSink for NullAudit {
    fn record(&self, _decision: &AuditDecision) {}
}

/// An in-memory audit sink for tests: keeps every recorded decision.
#[derive(Debug, Default)]
pub struct MemoryAudit {
    decisions: std::sync::Mutex<Vec<AuditDecision>>,
}

impl MemoryAudit {
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of all decisions recorded so far.
    pub fn decisions(&self) -> Vec<AuditDecision> {
        self.decisions.lock().unwrap().clone()
    }

    /// How many decisions have been recorded.
    pub fn len(&self) -> usize {
        self.decisions.lock().unwrap().len()
    }

    /// True if no decision has been recorded yet.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl AuditSink for MemoryAudit {
    fn record(&self, decision: &AuditDecision) {
        self.decisions.lock().unwrap().push(decision.clone());
    }
}

// ---------------------------------------------------------------------------
// Built-in passthrough providers (local null stack)
// ---------------------------------------------------------------------------

/// Registered name of the built-in passthrough provider (present in every plane).
pub const PASSTHROUGH: &str = "passthrough";

/// The null/passthrough provider for local, in-process mode: trusts the channel, the
/// principal, and the operation. Implements all three planes so a fully passthrough
/// chain can be built from it. It costs nothing beyond a match — the local fast path.
#[derive(Debug, Default, Clone, Copy)]
pub struct Passthrough;

impl TransportSecurity for Passthrough {
    fn name(&self) -> &str {
        PASSTHROUGH
    }
    fn check(&self, _ctx: &SecurityContext) -> Verdict {
        Verdict::Allow
    }
}

impl Authenticator for Passthrough {
    fn name(&self) -> &str {
        PASSTHROUGH
    }
    fn authenticate(&self, _ctx: &mut SecurityContext) -> Verdict {
        Verdict::Allow
    }
}

impl Authorizer for Passthrough {
    fn name(&self) -> &str {
        PASSTHROUGH
    }
    fn authorize(&self, _ctx: &SecurityContext) -> Verdict {
        Verdict::Allow
    }
}

// ---------------------------------------------------------------------------
// The chain
// ---------------------------------------------------------------------------

/// The reason a request was denied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Denial {
    pub plane: Plane,
    pub provider: Option<String>,
    pub reason: String,
}

/// The terminal decision for a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Allowed; carries the established principal (which an authenticator may have set).
    Allow(Principal),
    Deny(Denial),
}

impl Decision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Decision::Allow(_))
    }
}

/// The assembled security stack: one ordered provider chain per plane (each with its own
/// combine policy) plus the audit sink. Built by [`ProviderRegistry::build`] from a
/// [`SecurityConfig`], or directly via [`SecurityChain::local`] / [`SecurityChain::default_deny`].
pub struct SecurityChain {
    transport: (CombinePolicy, Vec<Box<dyn TransportSecurity>>),
    authn: (CombinePolicy, Vec<Box<dyn Authenticator>>),
    authz: (CombinePolicy, Vec<Box<dyn Authorizer>>),
    audit: Box<dyn AuditSink>,
}

impl SecurityChain {
    /// The trusted local stack: passthrough in every plane, no audit. This is the
    /// null/passthrough fast path that keeps the plain explorer fast (CPE-810 tiebreaker).
    pub fn local() -> Self {
        Self {
            transport: (CombinePolicy::FirstMatch, vec![Box::new(Passthrough)]),
            authn: (CombinePolicy::FirstMatch, vec![Box::new(Passthrough)]),
            authz: (CombinePolicy::FirstMatch, vec![Box::new(Passthrough)]),
            audit: Box::new(NullAudit),
        }
    }

    /// An empty stack — every plane has no providers, so it **denies everything**. This
    /// is the structural default-deny: a boundary you forget to configure is closed, not
    /// open.
    pub fn default_deny() -> Self {
        Self {
            transport: (CombinePolicy::AllMustPass, Vec::new()),
            authn: (CombinePolicy::AllMustPass, Vec::new()),
            authz: (CombinePolicy::AllMustPass, Vec::new()),
            audit: Box::new(NullAudit),
        }
    }

    /// Swap the audit sink (e.g. bridge to `audit_journal.rs`).
    pub fn with_audit(mut self, audit: Box<dyn AuditSink>) -> Self {
        self.audit = audit;
        self
    }

    /// Evaluate a request through Transport → AuthN → AuthZ. The first plane to deny
    /// stops the chain. Every terminal decision (allow or deny) is recorded to the audit
    /// sink.
    pub fn evaluate(&self, ctx: &mut SecurityContext) -> Decision {
        // 1. Transport.
        let t = combine(
            self.transport.0,
            &self.transport.1,
            |p| p.name().to_string(),
            |p| p.check(ctx),
        );
        if let Verdict::Deny(reason) = t.verdict {
            return self.finish_deny(ctx, Plane::Transport, t.decided_by, reason);
        }

        // 2. Authentication (may establish the principal).
        let a = combine(
            self.authn.0,
            &self.authn.1,
            |p| p.name().to_string(),
            |p| p.authenticate(ctx),
        );
        if let Verdict::Deny(reason) = a.verdict {
            return self.finish_deny(ctx, Plane::Authentication, a.decided_by, reason);
        }

        // 3. Authorization.
        let z = combine(
            self.authz.0,
            &self.authz.1,
            |p| p.name().to_string(),
            |p| p.authorize(ctx),
        );
        if let Verdict::Deny(reason) = z.verdict {
            return self.finish_deny(ctx, Plane::Authorization, z.decided_by, reason);
        }

        // Allowed by every plane.
        self.audit.record(&AuditDecision {
            allowed: true,
            plane: None,
            provider: z.decided_by,
            principal: ctx.principal.id.clone(),
            method: ctx.method.clone(),
            resource: ctx.resource.clone(),
            reason: None,
        });
        Decision::Allow(ctx.principal.clone())
    }

    fn finish_deny(
        &self,
        ctx: &SecurityContext,
        plane: Plane,
        provider: Option<String>,
        reason: String,
    ) -> Decision {
        self.audit.record(&AuditDecision {
            allowed: false,
            plane: Some(plane),
            provider: provider.clone(),
            principal: ctx.principal.id.clone(),
            method: ctx.method.clone(),
            resource: ctx.resource.clone(),
            reason: Some(reason.clone()),
        });
        Decision::Deny(Denial {
            plane,
            provider,
            reason,
        })
    }
}

// ---------------------------------------------------------------------------
// Config-driven registry
// ---------------------------------------------------------------------------

/// Config for one plane: which providers are active (by registered name), in order, and
/// how their verdicts combine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaneConfig {
    pub policy: CombinePolicy,
    #[serde(default)]
    pub providers: Vec<String>,
}

/// The whole security stack as data. Adding a provider is: implement its trait, register
/// it, and name it here — no core changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub transport: PlaneConfig,
    pub authentication: PlaneConfig,
    pub authorization: PlaneConfig,
}

impl SecurityConfig {
    /// The local passthrough configuration (passthrough provider in every plane).
    pub fn local() -> Self {
        let plane = || PlaneConfig {
            policy: CombinePolicy::FirstMatch,
            providers: vec![PASSTHROUGH.to_string()],
        };
        Self {
            transport: plane(),
            authentication: plane(),
            authorization: plane(),
        }
    }
}

/// Error assembling a [`SecurityChain`] from a [`SecurityConfig`]: a named provider isn't
/// registered for its plane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildError {
    pub plane: Plane,
    pub provider: String,
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "no provider named {:?} is registered for the {:?} plane",
            self.provider, self.plane
        )
    }
}

impl std::error::Error for BuildError {}

type TransportFactory = Box<dyn Fn() -> Box<dyn TransportSecurity> + Send + Sync>;
type AuthnFactory = Box<dyn Fn() -> Box<dyn Authenticator> + Send + Sync>;
type AuthzFactory = Box<dyn Fn() -> Box<dyn Authorizer> + Send + Sync>;

/// The registry of available providers, keyed by name per plane. A [`SecurityConfig`]
/// selects and orders providers from here; [`build`](ProviderRegistry::build) assembles
/// the runtime [`SecurityChain`].
#[derive(Default)]
pub struct ProviderRegistry {
    transport: BTreeMap<String, TransportFactory>,
    authn: BTreeMap<String, AuthnFactory>,
    authz: BTreeMap<String, AuthzFactory>,
}

impl ProviderRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// A registry pre-seeded with the built-in [`Passthrough`] provider in all three
    /// planes, so [`SecurityConfig::local`] builds out of the box.
    pub fn with_builtins() -> Self {
        let mut r = Self::new();
        r.register_transport(PASSTHROUGH, || Box::new(Passthrough));
        r.register_authn(PASSTHROUGH, || Box::new(Passthrough));
        r.register_authz(PASSTHROUGH, || Box::new(Passthrough));
        r
    }

    pub fn register_transport(
        &mut self,
        name: impl Into<String>,
        factory: impl Fn() -> Box<dyn TransportSecurity> + Send + Sync + 'static,
    ) {
        self.transport.insert(name.into(), Box::new(factory));
    }

    pub fn register_authn(
        &mut self,
        name: impl Into<String>,
        factory: impl Fn() -> Box<dyn Authenticator> + Send + Sync + 'static,
    ) {
        self.authn.insert(name.into(), Box::new(factory));
    }

    pub fn register_authz(
        &mut self,
        name: impl Into<String>,
        factory: impl Fn() -> Box<dyn Authorizer> + Send + Sync + 'static,
    ) {
        self.authz.insert(name.into(), Box::new(factory));
    }

    /// Assemble a [`SecurityChain`] from `config`, resolving every named provider against
    /// this registry. Unknown names are a [`BuildError`] — you cannot accidentally build a
    /// stack that silently drops a provider.
    pub fn build(
        &self,
        config: &SecurityConfig,
        audit: Box<dyn AuditSink>,
    ) -> Result<SecurityChain, BuildError> {
        let transport = self.build_transport(&config.transport)?;
        let authn = self.build_authn(&config.authentication)?;
        let authz = self.build_authz(&config.authorization)?;
        Ok(SecurityChain {
            transport: (config.transport.policy, transport),
            authn: (config.authentication.policy, authn),
            authz: (config.authorization.policy, authz),
            audit,
        })
    }

    fn build_transport(
        &self,
        cfg: &PlaneConfig,
    ) -> Result<Vec<Box<dyn TransportSecurity>>, BuildError> {
        cfg.providers
            .iter()
            .map(|name| {
                self.transport
                    .get(name)
                    .map(|f| f())
                    .ok_or_else(|| BuildError {
                        plane: Plane::Transport,
                        provider: name.clone(),
                    })
            })
            .collect()
    }

    fn build_authn(&self, cfg: &PlaneConfig) -> Result<Vec<Box<dyn Authenticator>>, BuildError> {
        cfg.providers
            .iter()
            .map(|name| {
                self.authn.get(name).map(|f| f()).ok_or_else(|| BuildError {
                    plane: Plane::Authentication,
                    provider: name.clone(),
                })
            })
            .collect()
    }

    fn build_authz(&self, cfg: &PlaneConfig) -> Result<Vec<Box<dyn Authorizer>>, BuildError> {
        cfg.providers
            .iter()
            .map(|name| {
                self.authz.get(name).map(|f| f()).ok_or_else(|| BuildError {
                    plane: Plane::Authorization,
                    provider: name.clone(),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A trivial authorizer that allows/denies/abstains by fixed verdict, for testing the
    // combine policies without waiting on the real providers (CPE-817/818).
    struct FixedAuthz {
        name: String,
        verdict: Verdict,
    }
    impl Authorizer for FixedAuthz {
        fn name(&self) -> &str {
            &self.name
        }
        fn authorize(&self, _ctx: &SecurityContext) -> Verdict {
            self.verdict.clone()
        }
    }
    fn az(name: &str, verdict: Verdict) -> Box<dyn Authorizer> {
        Box::new(FixedAuthz {
            name: name.into(),
            verdict,
        })
    }

    fn run(policy: CombinePolicy, providers: &[Box<dyn Authorizer>]) -> PlaneOutcome {
        let ctx = SecurityContext::new(Principal::local(), "op");
        combine(policy, providers, |p| p.name().to_string(), |p| {
            p.authorize(&ctx)
        })
    }

    #[test]
    fn empty_chain_is_default_deny_under_every_policy() {
        for policy in [
            CombinePolicy::AllMustPass,
            CombinePolicy::AnyPasses,
            CombinePolicy::FirstMatch,
        ] {
            let out = run(policy, &[]);
            assert!(matches!(out.verdict, Verdict::Deny(_)), "{policy:?} should deny");
            assert_eq!(out.decided_by, None);
        }
    }

    #[test]
    fn all_abstain_is_default_deny() {
        let providers = [az("a", Verdict::Abstain), az("b", Verdict::Abstain)];
        for policy in [
            CombinePolicy::AllMustPass,
            CombinePolicy::AnyPasses,
            CombinePolicy::FirstMatch,
        ] {
            assert!(matches!(run(policy, &providers).verdict, Verdict::Deny(_)));
        }
    }

    #[test]
    fn all_must_pass_requires_every_provider() {
        let ok = [az("a", Verdict::Allow), az("b", Verdict::Allow)];
        assert_eq!(run(CombinePolicy::AllMustPass, &ok).verdict, Verdict::Allow);

        let one_deny = [az("a", Verdict::Allow), az("b", Verdict::deny("nope"))];
        let out = run(CombinePolicy::AllMustPass, &one_deny);
        assert_eq!(out.verdict, Verdict::deny("nope"));
        assert_eq!(out.decided_by.as_deref(), Some("b"));

        // Abstains don't block, but at least one must affirmatively allow.
        let allow_and_abstain = [az("a", Verdict::Allow), az("b", Verdict::Abstain)];
        assert_eq!(
            run(CombinePolicy::AllMustPass, &allow_and_abstain).verdict,
            Verdict::Allow
        );
    }

    #[test]
    fn any_passes_needs_one_allow_and_short_circuits() {
        // Deny then allow ⇒ allowed (the deny doesn't end an any-passes plane).
        let providers = [az("a", Verdict::deny("bad key")), az("b", Verdict::Allow)];
        let out = run(CombinePolicy::AnyPasses, &providers);
        assert_eq!(out.verdict, Verdict::Allow);
        assert_eq!(out.decided_by.as_deref(), Some("b"));

        // All deny ⇒ denied.
        let all_deny = [az("a", Verdict::deny("x")), az("b", Verdict::deny("y"))];
        assert!(matches!(
            run(CombinePolicy::AnyPasses, &all_deny).verdict,
            Verdict::Deny(_)
        ));
    }

    #[test]
    fn first_match_takes_the_first_non_abstain() {
        let providers = [
            az("a", Verdict::Abstain),
            az("b", Verdict::deny("b decides")),
            az("c", Verdict::Allow),
        ];
        let out = run(CombinePolicy::FirstMatch, &providers);
        assert_eq!(out.verdict, Verdict::deny("b decides"));
        assert_eq!(out.decided_by.as_deref(), Some("b"));
    }

    #[test]
    fn local_chain_allows_and_does_not_audit() {
        let chain = SecurityChain::local();
        let mut ctx = SecurityContext::local("list_dir").with_resource("/home");
        assert!(chain.evaluate(&mut ctx).is_allowed());
    }

    #[test]
    fn default_deny_chain_denies_an_unconfigured_boundary() {
        let chain = SecurityChain::default_deny();
        let mut ctx = SecurityContext::new(Principal::local(), "read_file");
        match chain.evaluate(&mut ctx) {
            Decision::Deny(d) => assert_eq!(d.plane, Plane::Transport),
            other => panic!("unconfigured boundary must deny, got {other:?}"),
        }
    }

    #[test]
    fn every_decision_hits_the_audit_sink() {
        // Build a chain that passes transport+authn but denies authz, and confirm exactly
        // one audit record with the denying plane.
        let mut reg = ProviderRegistry::with_builtins();
        reg.register_authz("deny_all", || {
            Box::new(FixedAuthz {
                name: "deny_all".into(),
                verdict: Verdict::deny("not allowed"),
            })
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
                providers: vec!["deny_all".into()],
            },
        };
        let audit = std::sync::Arc::new(MemoryAudit::new());
        let chain = reg
            .build(&config, Box::new(ArcAudit(audit.clone())))
            .unwrap();

        let mut ctx = SecurityContext::new(Principal::local(), "delete").with_resource("/x");
        assert!(!chain.evaluate(&mut ctx).is_allowed());
        assert_eq!(audit.len(), 1);
        let d = &audit.decisions()[0];
        assert!(!d.allowed);
        assert_eq!(d.plane, Some(Plane::Authorization));
        assert_eq!(d.provider.as_deref(), Some("deny_all"));
        assert_eq!(d.method, "delete");
    }

    // Lets a test share one MemoryAudit between the chain and the assertions.
    struct ArcAudit(std::sync::Arc<MemoryAudit>);
    impl AuditSink for ArcAudit {
        fn record(&self, decision: &AuditDecision) {
            self.0.record(decision);
        }
    }

    #[test]
    fn build_rejects_an_unknown_provider() {
        let reg = ProviderRegistry::with_builtins();
        let mut config = SecurityConfig::local();
        config.authentication.providers = vec!["ghost".into()];
        // Can't `unwrap_err()` — SecurityChain holds trait objects and isn't Debug — so match.
        match reg.build(&config, Box::new(NullAudit)) {
            Err(err) => {
                assert_eq!(err.plane, Plane::Authentication);
                assert_eq!(err.provider, "ghost");
            }
            Ok(_) => panic!("build should reject an unregistered provider name"),
        }
    }

    #[test]
    fn config_round_trips_through_json() {
        let config = SecurityConfig::local();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("first_match"));
        assert!(json.contains(PASSTHROUGH));
        let back: SecurityConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn built_local_config_allows() {
        let reg = ProviderRegistry::with_builtins();
        let chain = reg
            .build(&SecurityConfig::local(), Box::new(NullAudit))
            .unwrap();
        let mut ctx = SecurityContext::local("list_dir");
        assert!(chain.evaluate(&mut ctx).is_allowed());
    }
}
