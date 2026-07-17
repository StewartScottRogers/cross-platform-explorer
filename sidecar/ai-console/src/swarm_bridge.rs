//! Swarm → live session bridge (CPE-528). The Swarm coordinator ([[CPE-517]]) emits abstract
//! `Assignment` dispatch intents ("run task T on agent instance A"). To actually run one, the live
//! driver needs a concrete **launch spec**: which base agent, which model, and the task text. This
//! module is the **pure** mapping from a coordinator assignment (+ the team manifest) to that spec — the
//! one part of the live-wiring that is unit-testable without a running sidecar. Turning a `SwarmLaunch`
//! into a real Agent-Grid session (and reporting its result back into the coordinator) is the live
//! driver, which needs the running app + GUI QA and is tracked as the rest of CPE-528.

use crate::swarm_team::{Role, TeamManifest};

/// A concrete request to launch one agent session for a Swarm task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwarmLaunch {
    /// The base agent id (e.g. `claude`), from the assignment's instance id.
    pub agent: String,
    /// The model for this role, from the team manifest (None ⇒ the agent's default).
    pub model: Option<String>,
    /// The task the agent should work (injected as its initial task, reusing CPE-313).
    pub task: String,
}

/// The role encoded in an agent instance id like `claude#builder1` (base `#` role + index) → `builder`.
fn role_of(instance: &str) -> Option<String> {
    instance
        .split('#')
        .nth(1)
        .map(|s| s.trim_end_matches(|c: char| c.is_ascii_digit()).to_string())
        .filter(|s| !s.is_empty())
}

/// Build the launch spec for a coordinator assignment (CPE-528): the base agent from the instance id,
/// the model from the team manifest for that agent+role, and the task text. Pure.
pub fn launch_spec_for(agent_instance: &str, team: &TeamManifest, task: &str) -> SwarmLaunch {
    let base = agent_instance.split('#').next().unwrap_or(agent_instance).to_string();
    let role = role_of(agent_instance);
    let model = team
        .roles
        .iter()
        .find(|r| r.agent == base && role.as_deref().is_none_or(|rl| role_name(r.role) == rl))
        .and_then(|r| r.model.clone());
    SwarmLaunch { agent: base, model, task: task.to_string() }
}

/// The lowercase name of a role, matching how instance ids are minted (`{:?}` lowercased).
fn role_name(role: Role) -> String {
    format!("{role:?}").to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swarm_team::RoleSpec;

    fn team() -> TeamManifest {
        TeamManifest {
            name: "T".into(),
            description: String::new(),
            roles: vec![
                RoleSpec { role: Role::Coordinator, agent: "claude".into(), model: Some("opus".into()), count: 1 },
                RoleSpec { role: Role::Builder, agent: "claude".into(), model: Some("sonnet".into()), count: 2 },
                RoleSpec { role: Role::Reviewer, agent: "aider".into(), model: None, count: 1 },
            ],
        }
    }

    #[test]
    fn maps_an_assignment_to_agent_model_and_task() {
        let s = launch_spec_for("claude#builder1", &team(), "build the parser");
        assert_eq!(s.agent, "claude");
        assert_eq!(s.model.as_deref(), Some("sonnet")); // the Builder role's model
        assert_eq!(s.task, "build the parser");
    }

    #[test]
    fn picks_the_model_for_the_matching_role_not_just_the_agent() {
        // Same base agent (claude) fills coordinator (opus) + builder (sonnet) — role disambiguates.
        assert_eq!(launch_spec_for("claude#coordinator1", &team(), "plan").model.as_deref(), Some("opus"));
        assert_eq!(launch_spec_for("claude#builder2", &team(), "build").model.as_deref(), Some("sonnet"));
    }

    #[test]
    fn a_role_without_a_model_yields_none() {
        assert_eq!(launch_spec_for("aider#reviewer1", &team(), "review").model, None);
    }

    #[test]
    fn tolerates_a_bare_instance_id() {
        let s = launch_spec_for("claude", &team(), "do it");
        assert_eq!(s.agent, "claude");
        assert_eq!(s.task, "do it");
    }
}
