//! Error presentation model (CPE-299).
//!
//! The contract defines a stable [`sidecar_contract::ErrorCode`] taxonomy with a
//! message and a `retryable` flag. This module maps those categories to a consistent
//! *presentation* the host UI uses — a severity, a human title, a recovery hint, and
//! whether a retry is worth offering — so failures surface predictably instead of as
//! raw strings or silent drops, no matter how large the surface grows.

use serde::{Deserialize, Serialize};
use sidecar_contract::{ContractError, ErrorCode};

/// How prominently the host should surface a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Not really an error — e.g. the user cancelled. Usually shown quietly or not at all.
    Info,
    /// A transient/soft problem; a toast is appropriate.
    Warning,
    /// A hard failure that should block or interrupt the flow.
    Blocking,
}

/// A UI-ready view of a failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Presentation {
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    pub recovery_hint: Option<String>,
    pub retryable: bool,
}

/// Map a structured error to how the host should present it. `detail` is the error's
/// own message (which by contract never carries secret values).
pub fn present(error: &ContractError) -> Presentation {
    let (severity, title, hint): (Severity, &str, Option<&str>) = match error.code {
        ErrorCode::Handshake => (
            Severity::Blocking,
            "Couldn't start the sidecar",
            Some("Try disabling and re-enabling it; if it persists, reinstall it."),
        ),
        ErrorCode::VersionIncompatible => (
            Severity::Blocking,
            "Sidecar is incompatible",
            Some("Update the app or the sidecar so their versions match."),
        ),
        ErrorCode::CapabilityDenied => (
            Severity::Warning,
            "Permission not granted",
            Some("Grant the capability in the sidecar's settings to use this feature."),
        ),
        ErrorCode::Transport => (
            Severity::Blocking,
            "Lost connection to the sidecar",
            Some("It may have crashed — it will be restarted automatically."),
        ),
        ErrorCode::SidecarCrash => (
            Severity::Blocking,
            "The sidecar stopped unexpectedly",
            Some("It will be restarted; check its logs if this keeps happening."),
        ),
        ErrorCode::ToolFailure => (Severity::Warning, "The operation failed", None),
        ErrorCode::Network => (
            Severity::Warning,
            "Network problem",
            Some("Check your connection or proxy settings and try again."),
        ),
        ErrorCode::UserCancelled => (Severity::Info, "Cancelled", None),
        ErrorCode::Internal => (
            Severity::Blocking,
            "Something went wrong",
            Some("Export diagnostics and report this if it continues."),
        ),
    };

    Presentation {
        severity,
        title: title.to_string(),
        detail: error.message.clone(),
        recovery_hint: hint.map(str::to_string),
        retryable: error.retryable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(code: ErrorCode, retryable: bool) -> ContractError {
        ContractError::new(code, "some detail", retryable)
    }

    #[test]
    fn user_cancel_is_informational_not_an_error() {
        let p = present(&err(ErrorCode::UserCancelled, false));
        assert_eq!(p.severity, Severity::Info);
    }

    #[test]
    fn capability_denied_is_a_warning_with_a_recovery_hint() {
        let p = present(&err(ErrorCode::CapabilityDenied, false));
        assert_eq!(p.severity, Severity::Warning);
        assert!(p.recovery_hint.is_some());
    }

    #[test]
    fn crashes_and_handshake_failures_block() {
        for code in [ErrorCode::SidecarCrash, ErrorCode::Handshake, ErrorCode::Internal] {
            assert_eq!(present(&err(code, false)).severity, Severity::Blocking);
        }
    }

    #[test]
    fn retryable_flag_is_carried_through() {
        assert!(present(&err(ErrorCode::Transport, true)).retryable);
        assert!(!present(&err(ErrorCode::ToolFailure, false)).retryable);
    }

    #[test]
    fn detail_is_the_error_message() {
        let p = present(&ContractError::new(ErrorCode::Network, "DNS failed", true));
        assert_eq!(p.detail, "DNS failed");
    }

    #[test]
    fn every_code_has_a_nonempty_title() {
        let codes = [
            ErrorCode::Handshake,
            ErrorCode::VersionIncompatible,
            ErrorCode::CapabilityDenied,
            ErrorCode::Transport,
            ErrorCode::SidecarCrash,
            ErrorCode::ToolFailure,
            ErrorCode::Network,
            ErrorCode::UserCancelled,
            ErrorCode::Internal,
        ];
        for c in codes {
            assert!(!present(&err(c, false)).title.is_empty());
        }
    }
}
