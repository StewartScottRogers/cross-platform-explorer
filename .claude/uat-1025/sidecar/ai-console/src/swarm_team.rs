//! Swarm role/team manifest (CPE-515) — declarative **team templates** for [CPE-502] Swarm
//! orchestration. A team is a manifest listing **roles** (coordinator / builder / scout / reviewer),
//! each bound to an agent + model and a count, mirroring the agent-manifest pattern (`agents.rs`,
//! CPE-278). Pure parse + validation; the coordinator (CPE-517) consumes a validated team to staff a
//! mission.

use serde::{Deserialize, Serialize};

/// A role an agent can play in a Swarm team. Closed vocabulary with defined responsibilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Plans the mission, splits it into tasks, dispatches + sequences the team, holds authority.
    Coordinator,
    /// Implements work on the files its task exclusively owns (see the CPE-514 lock manager).
    Builder,
    /// Gathers context / researches read-only; never owns files for writing.
    Scout,
    /// Checks a builder's output against the quality gates before a task is "done".
    Reviewer,
}

impl Role {
    /// One-line responsibility, for the UI / docs.
    pub fn responsibility(self) -> &'static str {
        match self {
            Role::Coordinator => "plans the mission, dispatches tasks, sequences the team, holds authority",
            Role::Builder => "implements work on the files its task owns",
            Role::Scout => "gathers context / researches, read-only",
            Role::Reviewer => "reviews output against the quality gates before done",
        }
    }
}

fn one() -> u32 {
    1
}

/// One role slot in a team: which agent (+ optional model) fills it, and how many instances.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleSpec {
    pub role: Role,
    pub agent: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default = "one")]
    pub count: u32,
}

/// A declarative Swarm team template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamManifest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub roles: Vec<RoleSpec>,
}

/// Why a team manifest is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeamError {
    BlankName,
    NoRoles,
    NoCoordinator,
    ManyCoordinators,
    NoBuilder,
    ZeroCount(Role),
    BlankAgent(Role),
}

impl std::fmt::Display for TeamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TeamError::BlankName => write!(f, "team needs a name"),
            TeamError::NoRoles => write!(f, "team has no roles"),
            TeamError::NoCoordinator => write!(f, "team needs exactly one coordinator (found none)"),
            TeamError::ManyCoordinators => write!(f, "team needs exactly one coordinator (found more than one)"),
            TeamError::NoBuilder => write!(f, "team needs at least one builder"),
            TeamError::ZeroCount(r) => write!(f, "role {r:?} has a count of zero"),
            TeamError::BlankAgent(r) => write!(f, "role {r:?} has a blank agent"),
        }
    }
}
impl std::error::Error for TeamError {}

impl TeamManifest {
    /// Parse + validate a team manifest from JSON. A malformed document or an invalid team is an error.
    pub fn parse(json: &str) -> Result<TeamManifest, String> {
        let m: TeamManifest =
            serde_json::from_str(json).map_err(|e| format!("invalid team manifest: {e}"))?;
        m.validate().map_err(|e| e.to_string())?;
        Ok(m)
    }

    /// Total agents cast in `role` across all slots.
    pub fn agents_in_role(&self, role: Role) -> u32 {
        self.roles.iter().filter(|r| r.role == role).map(|r| r.count).sum()
    }

    /// Total agents in the team.
    pub fn team_size(&self) -> u32 {
        self.roles.iter().map(|r| r.count).sum()
    }

    /// Validate the invariants: a named team with roles, **exactly one** coordinator, **at least one**
    /// builder, every slot non-zero with a real agent.
    pub fn validate(&self) -> Result<(), TeamError> {
        if self.name.trim().is_empty() {
            return Err(TeamError::BlankName);
        }
        if self.roles.is_empty() {
            return Err(TeamError::NoRoles);
        }
        for r in &self.roles {
            if r.count == 0 {
                return Err(TeamError::ZeroCount(r.role));
            }
            if r.agent.trim().is_empty() {
                return Err(TeamError::BlankAgent(r.role));
            }
        }
        match self.agents_in_role(Role::Coordinator) {
            0 => return Err(TeamError::NoCoordinator),
            1 => {}
            _ => return Err(TeamError::ManyCoordinators),
        }
        if self.agents_in_role(Role::Builder) == 0 {
            return Err(TeamError::NoBuilder);
        }
        Ok(())
    }
}

/// A sensible default team: a coordinator, two builders, and a reviewer (CPE-515).
pub fn default_team() -> TeamManifest {
    TeamManifest {
        name: "Default squad".to_string(),
        description: "A coordinator, two builders, and a reviewer.".to_string(),
        roles: vec![
            RoleSpec { role: Role::Coordinator, agent: "claude".into(), model: None, count: 1 },
            RoleSpec { role: Role::Builder, agent: "claude".into(), model: None, count: 2 },
            RoleSpec { role: Role::Reviewer, agent: "claude".into(), model: None, count: 1 },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_valid_team_manifest() {
        let json = r#"{
            "name": "Feature squad",
            "description": "ships a feature",
            "roles": [
                { "role": "coordinator", "agent": "claude" },
                { "role": "builder", "agent": "claude", "model": "sonnet", "count": 3 },
                { "role": "scout", "agent": "aider" },
                { "role": "reviewer", "agent": "claude" }
            ]
        }"#;
        let m = TeamManifest::parse(json).unwrap();
        assert_eq!(m.name, "Feature squad");
        assert_eq!(m.agents_in_role(Role::Builder), 3);
        assert_eq!(m.team_size(), 1 + 3 + 1 + 1);
        // count defaults to 1 when omitted.
        assert_eq!(m.roles[0].count, 1);
        assert_eq!(m.roles[2].model, None);
    }

    #[test]
    fn the_default_team_is_valid() {
        let m = default_team();
        m.validate().unwrap();
        assert_eq!(m.agents_in_role(Role::Coordinator), 1);
        assert_eq!(m.agents_in_role(Role::Builder), 2);
        assert_eq!(m.team_size(), 4);
    }

    fn spec(role: Role, agent: &str, count: u32) -> RoleSpec {
        RoleSpec { role, agent: agent.into(), model: None, count }
    }
    fn team(roles: Vec<RoleSpec>) -> TeamManifest {
        TeamManifest { name: "T".into(), description: String::new(), roles }
    }

    #[test]
    fn rejects_missing_or_duplicate_coordinator() {
        assert_eq!(
            team(vec![spec(Role::Builder, "a", 1)]).validate(),
            Err(TeamError::NoCoordinator)
        );
        assert_eq!(
            team(vec![spec(Role::Coordinator, "a", 2), spec(Role::Builder, "a", 1)]).validate(),
            Err(TeamError::ManyCoordinators)
        );
        assert_eq!(
            team(vec![
                spec(Role::Coordinator, "a", 1),
                spec(Role::Coordinator, "b", 1),
                spec(Role::Builder, "a", 1),
            ])
            .validate(),
            Err(TeamError::ManyCoordinators)
        );
    }

    #[test]
    fn rejects_missing_builder_and_bad_slots() {
        assert_eq!(
            team(vec![spec(Role::Coordinator, "a", 1)]).validate(),
            Err(TeamError::NoBuilder)
        );
        assert_eq!(
            team(vec![spec(Role::Coordinator, "a", 1), spec(Role::Builder, "a", 0)]).validate(),
            Err(TeamError::ZeroCount(Role::Builder))
        );
        assert_eq!(
            team(vec![spec(Role::Coordinator, "a", 1), spec(Role::Builder, "  ", 1)]).validate(),
            Err(TeamError::BlankAgent(Role::Builder))
        );
    }

    #[test]
    fn rejects_blank_name_and_no_roles() {
        assert_eq!(team(vec![]).validate(), Err(TeamError::NoRoles));
        let mut m = default_team();
        m.name = "   ".into();
        assert_eq!(m.validate(), Err(TeamError::BlankName));
    }

    #[test]
    fn malformed_json_is_a_clean_error_not_a_panic() {
        assert!(TeamManifest::parse("not json").is_err());
        assert!(TeamManifest::parse(r#"{"name":"x"}"#).is_err()); // missing roles
        assert!(TeamManifest::parse(r#"{"name":"x","roles":[{"role":"wizard","agent":"a"}]}"#).is_err());
    }

    #[test]
    fn serde_round_trips() {
        let m = default_team();
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(TeamManifest::parse(&json).unwrap(), m);
    }
}
