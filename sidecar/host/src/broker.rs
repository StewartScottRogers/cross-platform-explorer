//! Capability broker core (CPE-266).
//!
//! A sidecar gets no ambient authority: it *requests* capabilities, the user
//! *consents* (CPE-296), optional policy *allows*, and only the intersection is
//! granted. Every capability call a sidecar makes is routed through the broker,
//! which enforces the grant before dispatching to the capability's provider
//! (context/secrets/storage/events — CPE-267..270, plugged in later).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use sidecar_contract::{Capability, ContractError, ErrorCode, Request, Response};

/// Inputs to the grant decision for one sidecar.
pub struct GrantRequest {
    /// What the sidecar's manifest asked for.
    pub requested: Vec<Capability>,
    /// What the user consented to (CPE-296). Only these may be granted.
    pub consented: BTreeSet<Capability>,
    /// Optional enterprise/policy allowlist. `None` = no policy restriction.
    pub policy_allow: Option<BTreeSet<Capability>>,
}

/// The granted set is `requested ∩ consented ∩ policy` — least privilege by
/// construction. Anything the sidecar didn't ask for, the user didn't consent to,
/// or policy forbids is excluded.
pub fn decide_grants(req: &GrantRequest) -> BTreeSet<Capability> {
    req.requested
        .iter()
        .copied()
        .filter(|c| req.consented.contains(c))
        .filter(|c| req.policy_allow.as_ref().is_none_or(|a| a.contains(c)))
        .collect()
}

/// Map a method name like `"secrets.get"` to the capability it belongs to.
pub fn capability_for_method(method: &str) -> Option<Capability> {
    match method.split('.').next()? {
        "context" => Some(Capability::Context),
        "secrets" => Some(Capability::Secrets),
        "storage" => Some(Capability::Storage),
        "events" => Some(Capability::Events),
        "network" => Some(Capability::Network),
        _ => None,
    }
}

/// A handler for one capability's methods, implemented by the capability providers
/// (CPE-267..270). Providers never see a request the broker hasn't authorized.
pub trait CapabilityProvider: Send + Sync {
    /// Which capability this provider serves.
    fn capability(&self) -> Capability;
    /// Handle an authorized request from `sidecar_id`, returning a JSON result or a
    /// structured error.
    fn handle(&self, sidecar_id: &str, request: &Request) -> Result<Value, ContractError>;
}

/// Routes sidecar capability calls: enforces the per-sidecar grant, then dispatches
/// to the registered provider.
#[derive(Default)]
pub struct Broker {
    grants: BTreeMap<String, BTreeSet<Capability>>,
    providers: BTreeMap<Capability, Box<dyn CapabilityProvider>>,
}

impl Broker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register the provider for its capability (replacing any prior one).
    pub fn register_provider(&mut self, provider: Box<dyn CapabilityProvider>) {
        self.providers.insert(provider.capability(), provider);
    }

    /// Record the capabilities granted to a sidecar (from [`decide_grants`]).
    pub fn set_grants(&mut self, sidecar_id: impl Into<String>, granted: BTreeSet<Capability>) {
        self.grants.insert(sidecar_id.into(), granted);
    }

    pub fn is_granted(&self, sidecar_id: &str, cap: Capability) -> bool {
        self.grants
            .get(sidecar_id)
            .is_some_and(|set| set.contains(&cap))
    }

    /// Enforce + dispatch one request from a sidecar. Always returns a [`Response`]
    /// (never panics): denied calls come back as `CapabilityDenied`.
    pub fn dispatch(&self, sidecar_id: &str, request: &Request) -> Response {
        let cap = match capability_for_method(&request.method) {
            Some(c) => c,
            None => {
                return err(
                    ErrorCode::Internal,
                    format!("no capability maps to method '{}'", request.method),
                )
            }
        };
        if !self.is_granted(sidecar_id, cap) {
            return err(
                ErrorCode::CapabilityDenied,
                format!("{cap:?} is not granted to '{sidecar_id}'"),
            );
        }
        match self.providers.get(&cap) {
            Some(provider) => match provider.handle(sidecar_id, request) {
                Ok(value) => Response { result: Ok(value) },
                Err(e) => Response { result: Err(e) },
            },
            None => err(
                ErrorCode::Internal,
                format!("{cap:?} is granted but has no registered provider"),
            ),
        }
    }
}

fn err(code: ErrorCode, message: impl Into<String>) -> Response {
    Response {
        result: Err(ContractError::new(code, message, false)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn set(caps: &[Capability]) -> BTreeSet<Capability> {
        caps.iter().copied().collect()
    }

    #[test]
    fn grants_are_the_intersection_of_request_consent_and_policy() {
        let req = GrantRequest {
            requested: vec![Capability::Context, Capability::Secrets, Capability::Network],
            consented: set(&[Capability::Context, Capability::Secrets]),
            policy_allow: Some(set(&[Capability::Context, Capability::Network])),
        };
        // Context is in all three; Secrets fails policy; Network fails consent.
        assert_eq!(decide_grants(&req), set(&[Capability::Context]));
    }

    #[test]
    fn no_policy_means_no_policy_restriction() {
        let req = GrantRequest {
            requested: vec![Capability::Storage],
            consented: set(&[Capability::Storage]),
            policy_allow: None,
        };
        assert_eq!(decide_grants(&req), set(&[Capability::Storage]));
    }

    #[test]
    fn method_names_map_to_capabilities() {
        assert_eq!(capability_for_method("secrets.get"), Some(Capability::Secrets));
        assert_eq!(capability_for_method("context.current"), Some(Capability::Context));
        assert_eq!(capability_for_method("bogus.thing"), None);
    }

    struct EchoSecrets;
    impl CapabilityProvider for EchoSecrets {
        fn capability(&self) -> Capability {
            Capability::Secrets
        }
        fn handle(&self, _sidecar_id: &str, request: &Request) -> Result<Value, ContractError> {
            Ok(json!({ "echoed": request.method }))
        }
    }

    #[test]
    fn dispatch_denies_an_ungranted_capability() {
        let mut b = Broker::new();
        b.register_provider(Box::new(EchoSecrets));
        b.set_grants("s1", set(&[Capability::Context])); // NOT secrets
        let resp = b.dispatch("s1", &Request { method: "secrets.get".into(), params: json!({}) });
        let e = resp.result.unwrap_err();
        assert_eq!(e.code, ErrorCode::CapabilityDenied);
    }

    #[test]
    fn dispatch_routes_a_granted_call_to_its_provider() {
        let mut b = Broker::new();
        b.register_provider(Box::new(EchoSecrets));
        b.set_grants("s1", set(&[Capability::Secrets]));
        let resp = b.dispatch("s1", &Request { method: "secrets.get".into(), params: json!({}) });
        assert_eq!(resp.result.unwrap(), json!({ "echoed": "secrets.get" }));
    }

    #[test]
    fn dispatch_errors_on_unknown_method() {
        let b = Broker::new();
        let resp = b.dispatch("s1", &Request { method: "nope".into(), params: json!({}) });
        assert_eq!(resp.result.unwrap_err().code, ErrorCode::Internal);
    }

    #[test]
    fn dispatch_errors_when_granted_but_no_provider() {
        let mut b = Broker::new();
        b.set_grants("s1", set(&[Capability::Secrets])); // granted, but no provider registered
        let resp = b.dispatch("s1", &Request { method: "secrets.get".into(), params: json!({}) });
        assert_eq!(resp.result.unwrap_err().code, ErrorCode::Internal);
    }
}
