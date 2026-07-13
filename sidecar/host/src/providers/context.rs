//! Context capability provider (CPE-267).
//!
//! The one place a sidecar may learn about the explorer's world — through a narrow,
//! read-only, host-brokered API. It exposes the current folder, git repo root/remote,
//! and selection as immutable snapshots. No explorer types cross the boundary (only
//! the DTOs here), and there is no mutation method — a sidecar cannot change explorer
//! state through this capability.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sidecar_contract::{Capability, ContractError, ErrorCode, Request};

use crate::broker::CapabilityProvider;

/// An immutable view of what the explorer is showing. Serialized to the sidecar.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextSnapshot {
    /// Absolute path of the folder currently open, if any.
    pub folder: Option<String>,
    /// Git repository root containing `folder`, if it is inside a repo.
    pub repo_root: Option<String>,
    /// The repo's browsable remote URL, if known.
    pub remote: Option<String>,
    /// Absolute paths of the currently selected entries.
    pub selection: Vec<String>,
}

/// Supplies the live snapshot. The host (explorer) implements this; the provider
/// only reads through it, so the explorer keeps ownership of its state.
pub trait ContextSource: Send + Sync {
    fn snapshot(&self) -> ContextSnapshot;
}

/// The [`CapabilityProvider`] for [`Capability::Context`]. Handles read-only methods:
/// `context.current`.
pub struct ContextProvider<S: ContextSource> {
    source: S,
}

impl<S: ContextSource> ContextProvider<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: ContextSource> CapabilityProvider for ContextProvider<S> {
    fn capability(&self) -> Capability {
        Capability::Context
    }

    fn handle(&self, _sidecar_id: &str, request: &Request) -> Result<Value, ContractError> {
        match request.method.as_str() {
            "context.current" => serde_json::to_value(self.source.snapshot())
                .map_err(|e| ContractError::new(ErrorCode::Internal, e.to_string(), false)),
            other => Err(ContractError::new(
                ErrorCode::ToolFailure,
                format!("unknown context method '{other}'"),
                false,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::{Broker, GrantRequest};
    use crate::broker::decide_grants;
    use sidecar_contract::Request;
    use std::collections::BTreeSet;

    struct FixedSource(ContextSnapshot);
    impl ContextSource for FixedSource {
        fn snapshot(&self) -> ContextSnapshot {
            self.0.clone()
        }
    }

    fn sample() -> ContextSnapshot {
        ContextSnapshot {
            folder: Some("/repo/src".into()),
            repo_root: Some("/repo".into()),
            remote: Some("https://example.com/x".into()),
            selection: vec!["/repo/src/a.rs".into()],
        }
    }

    #[test]
    fn current_returns_the_live_snapshot() {
        let p = ContextProvider::new(FixedSource(sample()));
        let out = p
            .handle("s1", &Request { method: "context.current".into(), params: Value::Null })
            .unwrap();
        let got: ContextSnapshot = serde_json::from_value(out).unwrap();
        assert_eq!(got, sample());
    }

    #[test]
    fn unknown_method_is_an_error_not_a_panic() {
        let p = ContextProvider::new(FixedSource(sample()));
        let err = p
            .handle("s1", &Request { method: "context.write".into(), params: Value::Null })
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::ToolFailure);
    }

    #[test]
    fn flows_through_the_broker_only_when_granted() {
        let mut broker = Broker::new();
        broker.register_provider(Box::new(ContextProvider::new(FixedSource(sample()))));

        // Not granted → denied.
        let denied = broker.dispatch(
            "s1",
            &Request { method: "context.current".into(), params: Value::Null },
        );
        assert!(denied.result.is_err());

        // Grant Context (as consent would), then it flows to the provider.
        let granted = decide_grants(&GrantRequest {
            requested: vec![Capability::Context],
            consented: BTreeSet::from([Capability::Context]),
            policy_allow: None,
        });
        broker.set_grants("s1", granted);
        let ok = broker.dispatch(
            "s1",
            &Request { method: "context.current".into(), params: Value::Null },
        );
        let snap: ContextSnapshot = serde_json::from_value(ok.result.unwrap()).unwrap();
        assert_eq!(snap.folder.as_deref(), Some("/repo/src"));
    }
}
