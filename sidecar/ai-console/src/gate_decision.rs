//! Gate decision fusion (CPE-1077, epic CPE-729): combine a command's risk assessment
//! ([`guardrail::assess_command`](crate::guardrail::assess_command)) with a filesystem scope
//! verdict ([`gate_scope::ScopeVerdict`](crate::gate_scope::ScopeVerdict)) and an approval
//! policy into one gate outcome — auto-allow, needs-approval, or auto-block.
//!
//! Pure, dependency-free. Builds on `guardrail` + `gate_scope` without modifying either.
//!
//! Precedence (strongest signal wins): a `SecretPath`/`DenyListed` scope verdict always forces
//! at least `NeedsApproval`, escalated to `AutoBlock` when `strict` is set — regardless of the
//! command's own risk. `OutOfScope` also forces `NeedsApproval`: `decide` takes a `ScopeVerdict`,
//! not an [`FsOp`](crate::gate_scope::FsOp), so it has no read-vs-write distinction to key off
//! (and `gate_scope` itself treats the op as irrelevant to the verdict — see its
//! `op_kind_does_not_affect_the_verdict` test). Treating every out-of-scope action as needing
//! approval is the conservative reading of the ticket's "write/delete" note; a caller that wants
//! read-only out-of-scope actions to auto-allow can pass a permissive `ScopePolicy` upstream (in
//! `gate_scope`) instead. Otherwise (`InScope`), the command's risk level decided via
//! `guardrail::needs_approval` governs; no command (`None`) auto-allows.

use crate::gate_scope::ScopeVerdict;
use crate::guardrail::{assess_command, needs_approval, ApprovalPolicy};

/// The fused gate outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateOutcome {
    /// Proceed without prompting.
    AutoAllow,
    /// Prompt the user for approve/reject.
    NeedsApproval,
    /// Refuse outright, no prompt.
    AutoBlock,
}

/// The result of fusing command risk + scope verdict + policy for one action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateDecision {
    pub outcome: GateOutcome,
    /// Human-readable reasons for the outcome, sorted + deduped for determinism.
    pub reasons: Vec<String>,
}

/// Fuse a command's risk, a filesystem scope verdict, and an approval policy into one gate
/// outcome. `command` is scored via [`crate::guardrail::assess_command`] when `Some`; `strict`
/// escalates a `SecretPath`/`DenyListed` scope verdict from `NeedsApproval` to `AutoBlock`.
pub fn decide(
    command: Option<&str>,
    scope: &ScopeVerdict,
    policy: ApprovalPolicy,
    strict: bool,
) -> GateDecision {
    let mut reasons: Vec<String> = Vec::new();
    let mut outcome = GateOutcome::AutoAllow;

    match scope {
        ScopeVerdict::SecretPath(reason) => {
            reasons.push(format!("secret path ({reason})"));
            outcome = if strict { GateOutcome::AutoBlock } else { GateOutcome::NeedsApproval };
        }
        ScopeVerdict::DenyListed(reason) => {
            reasons.push(format!("deny-listed path ({reason})"));
            outcome = if strict { GateOutcome::AutoBlock } else { GateOutcome::NeedsApproval };
        }
        ScopeVerdict::OutOfScope => {
            reasons.push("path is out of scope".to_string());
            outcome = GateOutcome::NeedsApproval;
        }
        ScopeVerdict::InScope => {}
    }

    if let Some(cmd) = command {
        let assessment = assess_command(cmd);
        if needs_approval(assessment.level, policy) {
            if assessment.reasons.is_empty() {
                reasons.push(format!("{:?}-risk command", assessment.level));
            } else {
                reasons.extend(assessment.reasons.iter().map(|r| (*r).to_string()));
            }
            // Strongest signal wins: a scope verdict may have already forced NeedsApproval or
            // AutoBlock; a command's own risk never *downgrades* that, it can only raise a
            // still-AutoAllow outcome up to NeedsApproval.
            if outcome == GateOutcome::AutoAllow {
                outcome = GateOutcome::NeedsApproval;
            }
        }
    }

    reasons.sort_unstable();
    reasons.dedup();
    GateDecision { outcome, reasons }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guardrail::ApprovalPolicy::*;

    #[test]
    fn high_risk_command_in_scope_under_high_only_needs_approval() {
        let d = decide(Some("rm -rf build"), &ScopeVerdict::InScope, HighOnly, false);
        assert_eq!(d.outcome, GateOutcome::NeedsApproval);
        assert!(!d.reasons.is_empty());
    }

    #[test]
    fn low_risk_read_of_secret_path_needs_approval_scope_escalates_past_command_risk() {
        let d = decide(
            Some("cat .env"),
            &ScopeVerdict::SecretPath("*.env".to_string()),
            Off,
            false,
        );
        assert_eq!(d.outcome, GateOutcome::NeedsApproval);
        assert!(d.reasons.iter().any(|r| r.contains("secret")), "{:?}", d.reasons);
    }

    #[test]
    fn off_policy_in_scope_low_risk_auto_allows() {
        let d = decide(Some("ls -la"), &ScopeVerdict::InScope, Off, false);
        assert_eq!(d.outcome, GateOutcome::AutoAllow);
        assert!(d.reasons.is_empty());
    }

    #[test]
    fn no_command_in_scope_auto_allows() {
        let d = decide(None, &ScopeVerdict::InScope, HighOnly, false);
        assert_eq!(d.outcome, GateOutcome::AutoAllow);
        assert!(d.reasons.is_empty());
    }

    #[test]
    fn deny_listed_under_strict_auto_blocks() {
        let d = decide(
            None,
            &ScopeVerdict::DenyListed("*.lock".to_string()),
            Off,
            true,
        );
        assert_eq!(d.outcome, GateOutcome::AutoBlock);
        assert!(d.reasons.iter().any(|r| r.contains("deny-listed")), "{:?}", d.reasons);
    }

    #[test]
    fn deny_listed_without_strict_only_needs_approval() {
        let d = decide(
            None,
            &ScopeVerdict::DenyListed("*.lock".to_string()),
            Off,
            false,
        );
        assert_eq!(d.outcome, GateOutcome::NeedsApproval);
    }

    #[test]
    fn secret_path_under_strict_auto_blocks() {
        let d = decide(
            None,
            &ScopeVerdict::SecretPath("*.env".to_string()),
            Off,
            true,
        );
        assert_eq!(d.outcome, GateOutcome::AutoBlock);
    }

    #[test]
    fn out_of_scope_needs_approval_regardless_of_command() {
        let d = decide(None, &ScopeVerdict::OutOfScope, Off, false);
        assert_eq!(d.outcome, GateOutcome::NeedsApproval);
        assert!(d.reasons.iter().any(|r| r.contains("out of scope")), "{:?}", d.reasons);
    }

    #[test]
    fn medium_and_up_policy_gates_medium_risk_commands() {
        let d = decide(Some("npm install left-pad"), &ScopeVerdict::InScope, MediumAndUp, false);
        assert_eq!(d.outcome, GateOutcome::NeedsApproval);
    }

    #[test]
    fn reasons_are_sorted_and_deduped() {
        // "rm -rf" matches multiple HIGH patterns whose reason text can repeat; scope adds
        // another reason. The combined list must come back sorted with no duplicates.
        let d = decide(
            Some("rm -rf /tmp/x"),
            &ScopeVerdict::DenyListed("*.tmp".to_string()),
            HighOnly,
            false,
        );
        let mut expected = d.reasons.clone();
        expected.sort_unstable();
        expected.dedup();
        assert_eq!(d.reasons, expected);
        assert_eq!(d.outcome, GateOutcome::NeedsApproval);
    }

    #[test]
    fn strict_escalation_survives_a_high_risk_command_too() {
        // Strongest signal wins: strict Secret escalation to AutoBlock is not downgraded by
        // also folding in a high-risk command's reasons.
        let d = decide(
            Some("rm -rf /tmp/x"),
            &ScopeVerdict::SecretPath(".env".to_string()),
            HighOnly,
            true,
        );
        assert_eq!(d.outcome, GateOutcome::AutoBlock);
        assert!(d.reasons.len() > 1, "{:?}", d.reasons);
    }
}
