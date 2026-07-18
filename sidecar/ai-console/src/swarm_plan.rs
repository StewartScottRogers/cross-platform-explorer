//! Production [`LaunchPlanner`](crate::swarm_live::LaunchPlanner) (CPE-541) — turns a coordinator
//! `SwarmLaunch` into a **real agent** `PtyLaunch`, wiring in the live swarm MCP host so a launched
//! agent coordinates + shares memory with the rest of the mission.
//!
//! For each assignment it:
//! 1. resolves the agent manifest from the registry,
//! 2. writes a per-agent MCP config file pointing at `ai-console --swarm-mcp --dir <mission> --agent
//!    <instance>` (so the agent spawns its own host that shares state through the mission dir), and
//! 3. composes the scoped launch via the real [`scope::build_launch`] path (provider/env/keys), passing
//!    the MCP-config flag + the task text as run args.
//!
//! **Unverified until GUI QA (honest scope):** the *composition* here is unit-tested, but the exact
//! **MCP-config flag + task-arg form is agent-specific** (`--mcp-config <file>` + a trailing prompt is
//! the Claude Code convention). Whether a given agent actually loads the host and works the task can only
//! be confirmed by running real agents — that is the remaining GUI QA. The injection form is kept in one
//! place ([`ProductionPlanner::mcp_flag`]) so it's adjustable during QA without touching the loop.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;

use crate::agents::{AgentManifest, AgentRegistry};
use crate::routing::LaunchContext;
use crate::scope::{build_launch, AgentLaunchRequest};
use crate::swarm_bridge::SwarmLaunch;
use crate::swarm_coordinator::Assignment;
use crate::swarm_live::LaunchPlanner;
use crate::swarm_mcp_server::write_members;
use crate::swarm_team::Role;
use crate::{pty::PtyLaunch, swarm_team::TeamManifest};

fn sanitize(id: &str) -> String {
    id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect()
}

/// Make a swarm task safe to pass as a **command-line argument** to a launched agent (CPE-587). On
/// Windows the launch goes through `cmd /c <shim>` (CPE-326, because `claude` is a `.cmd` shim), and
/// `portable_pty` MSVC-quotes each arg (`"` → `\"`) — which `cmd` then mis-parses, splitting the prompt
/// and making the agent error (it echoes the mangled text in red instead of working the task). Neutralise
/// the offenders: double-quotes become typographic quotes (identical meaning to the model) and any
/// newlines/control chars collapse to spaces (a multi-line arg also breaks the `cmd` re-parse). A
/// follow-up will deliver the task verbatim via stdin/file so nothing is rewritten at all.
fn cmd_safe_task(task: &str) -> String {
    task.chars()
        .map(|c| match c {
            '"' => '\u{2019}',                       // → ’  (right single quote; reads the same)
            c if c.is_control() => ' ',              // newlines/tabs/etc. → space
            c => c,
        })
        .collect()
}

/// Builds real agent launches for a swarm mission, wiring each agent to the shared MCP host.
pub struct ProductionPlanner {
    registry: Arc<AgentRegistry>,
    /// The mission directory the swarm shares (memory / mailbox / roster / MCP configs).
    mission_dir: PathBuf,
    /// Path to the `ai-console` executable that hosts `--swarm-mcp`.
    exe: PathBuf,
    /// The repo/folder each agent session is scoped to.
    cwd: String,
    /// Provider id the agents launch against (e.g. `native`, `openrouter`).
    provider: String,
    /// Provider API key (resolved from the vault by the caller; never logged).
    api_key: Option<String>,
    /// Provider base URL, when applicable.
    base_url: Option<String>,
    rows: u16,
    cols: u16,
}

impl ProductionPlanner {
    /// A planner for a mission. `exe` is the `ai-console` binary (usually `std::env::current_exe()`);
    /// `cwd` is the repo the swarm works in; `provider`/`api_key`/`base_url` select the model gateway.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        registry: Arc<AgentRegistry>,
        mission_dir: PathBuf,
        exe: PathBuf,
        cwd: String,
        provider: String,
        api_key: Option<String>,
        base_url: Option<String>,
    ) -> ProductionPlanner {
        ProductionPlanner { registry, mission_dir, exe, cwd, provider, api_key, base_url, rows: 30, cols: 100 }
    }

    /// Record the team roster so the shared MCP host resolves `role`/`broadcast` recipients. Call once
    /// before running the mission (staffing is deterministic — mirror the coordinator's instance ids).
    pub fn init_mission(&self, members: &[(String, Role)]) -> Result<(), String> {
        write_members(&self.mission_dir, members).map_err(|e| format!("write roster: {e}"))
    }

    /// Staff a team into `(instance_id, role)` pairs, matching how the coordinator mints them
    /// (`agent#role{n}`, lowercased) — so the roster the host reads lines up with the assignments.
    pub fn roster_for(team: &TeamManifest) -> Vec<(String, Role)> {
        let mut out = Vec::new();
        let mut per_role: BTreeMap<String, u32> = BTreeMap::new();
        for spec in &team.roles {
            for _ in 0..spec.count {
                let role_name = format!("{:?}", spec.role).to_lowercase();
                let n = per_role.entry(role_name.clone()).or_insert(0);
                *n += 1;
                // Match the coordinator's `staff()` exactly: `{agent}#{role}{n}` lowercased whole.
                out.push((format!("{}#{}{}", spec.agent, role_name, n).to_lowercase(), spec.role));
            }
        }
        out
    }

    /// The run args that attach the swarm MCP host and the task to this agent, from the agent's manifest
    /// **`swarm` recipe** (CPE-583) — its non-interactive/print-mode invocation, templated with the
    /// `{task}` and `{mcp_config}` placeholders. This is what makes the agent run the task to completion
    /// and **exit**, so the driver detects completion. Keeping the exact flags in the manifest (data)
    /// means they're tunable during QA without a rebuild — this replaced the hardcoded v1 form.
    ///
    /// Fallback (agent without a `swarm` recipe): the pre-CPE-583 positional `--mcp-config <file> <task>`
    /// form. That launches the agent *interactively*, which won't self-terminate — so the driver can't
    /// detect completion; such an agent needs a `swarm` recipe before it works in a mission.
    fn swarm_args(agent: &AgentManifest, config_path: &str, task: &str) -> Vec<String> {
        let task = cmd_safe_task(task);
        match &agent.swarm {
            Some(recipe) => recipe
                .args
                .iter()
                .map(|a| a.replace("{task}", &task).replace("{mcp_config}", config_path))
                .collect(),
            // Fallback when the manifest has no `swarm` recipe (e.g. an auto-updated catalog that
            // predates CPE-583). It MUST be variadic-safe: `claude`'s `--mcp-config <configs...>` is
            // greedy, so the task can never follow the config path (or it's slurped as a second config —
            // CPE-590). Mirror the print-mode recipe: task via `-p`, then `--mcp-config <cfg>` terminated
            // by a trailing flag so only the config is consumed.
            None => vec![
                "-p".to_string(),
                task,
                "--mcp-config".to_string(),
                config_path.to_string(),
                "--dangerously-skip-permissions".to_string(),
            ],
        }
    }

    /// Write the per-agent MCP config that points at this agent's shared host, returning its path.
    fn write_mcp_config(&self, agent_instance: &str) -> Result<PathBuf, String> {
        std::fs::create_dir_all(&self.mission_dir).map_err(|e| e.to_string())?;
        let path = self.mission_dir.join(format!("mcp-{}.json", sanitize(agent_instance)));
        let cfg = json!({
            "mcpServers": {
                "swarm": {
                    "command": self.exe.to_string_lossy(),
                    "args": [
                        "--swarm-mcp",
                        "--dir", self.mission_dir.to_string_lossy(),
                        "--agent", agent_instance
                    ]
                }
            }
        });
        std::fs::write(&path, serde_json::to_string_pretty(&cfg).unwrap_or_default()).map_err(|e| e.to_string())?;
        Ok(path)
    }
}

impl LaunchPlanner for ProductionPlanner {
    fn plan(&self, assignment: &Assignment, spec: &SwarmLaunch) -> Result<PtyLaunch, String> {
        let agent = self
            .registry
            .get(&spec.agent)
            .cloned()
            .ok_or_else(|| format!("unknown agent '{}'", spec.agent))?;

        let config_path = self.write_mcp_config(&assignment.agent_id)?;
        let extra_args = Self::swarm_args(&agent, &config_path.to_string_lossy(), &spec.task);

        let ctx = LaunchContext {
            model: spec.model.clone(),
            small_model: None,
            api_key: self.api_key.clone(),
            base_url: self.base_url.clone(),
        };
        let req = AgentLaunchRequest {
            agent: &agent,
            provider: &self.provider,
            reseller: None,
            ctx,
            profile_env: BTreeMap::new(),
            cwd: self.cwd.clone(),
            extra_args,
            rows: self.rows,
            cols: self.cols,
        };
        build_launch(&req)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swarm_team::RoleSpec;

    fn registry() -> Arc<AgentRegistry> {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("agents");
        Arc::new(AgentRegistry::load_from_dirs(&[dir]))
    }

    fn planner(mission: PathBuf) -> ProductionPlanner {
        ProductionPlanner::new(
            registry(),
            mission,
            PathBuf::from("/opt/ai-console"),
            "/repo".into(),
            "openrouter".into(),
            Some("sk-or".into()),
            None,
        )
    }

    #[test]
    fn plans_a_real_claude_launch_with_the_task_and_mcp_config() {
        let mission = tempfile::tempdir().unwrap();
        let p = planner(mission.path().to_path_buf());
        let assignment = Assignment { task_id: "t1".into(), agent_id: "claude#builder1".into() };
        let spec = SwarmLaunch { agent: "claude".into(), model: Some("anthropic/claude-sonnet-4.5".into()), task: "build the parser".into() };

        let launch = p.plan(&assignment, &spec).expect("plan a claude launch");
        assert_eq!(launch.program, "claude");
        assert!(launch.args.contains(&"--mcp-config".to_string()), "MCP config flag injected");
        assert!(launch.args.contains(&"build the parser".to_string()), "task passed as a run arg");
        // CPE-583: from claude's `swarm` recipe — print mode so the agent completes + exits, and a
        // non-interactive permission posture so tool calls don't block.
        assert!(launch.args.contains(&"-p".to_string()), "print mode injected so the agent exits");
        assert!(launch.args.contains(&"--dangerously-skip-permissions".to_string()), "non-interactive permissions");
        let p_at = launch.args.iter().position(|a| a == "-p").unwrap();
        assert_eq!(launch.args.get(p_at + 1), Some(&"build the parser".to_string()), "task is the print prompt");
        assert_eq!(launch.cwd.as_deref(), Some("/repo"));

        // The per-agent MCP config file exists and points the agent at its shared host.
        let cfg_path = mission.path().join("mcp-claude-builder1.json");
        assert!(cfg_path.exists(), "per-agent MCP config written");
        let cfg: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&cfg_path).unwrap()).unwrap();
        let args = &cfg["mcpServers"]["swarm"]["args"];
        assert_eq!(args[0], json!("--swarm-mcp"));
        assert!(args.as_array().unwrap().iter().any(|a| a == "claude#builder1"), "host launched for this agent instance");
    }

    #[test]
    fn swarm_recipe_templates_task_and_mcp_config() {
        let agent = AgentManifest {
            swarm: Some(crate::agents::SwarmRecipe {
                args: vec!["-p".into(), "{task}".into(), "--mcp-config".into(), "{mcp_config}".into()],
            }),
            ..AgentManifest::default()
        };
        let args = ProductionPlanner::swarm_args(&agent, "/m/mcp-x.json", "do the thing");
        assert_eq!(args, vec!["-p", "do the thing", "--mcp-config", "/m/mcp-x.json"]);
    }

    #[test]
    fn cmd_safe_task_neutralises_argv_breaking_characters() {
        // Double-quotes (which break the Windows `cmd /c` re-parse) become typographic quotes; control
        // chars collapse to spaces. Ordinary punctuation (commas, semicolons, periods) is untouched.
        let out = cmd_safe_task("post a \"done\" message; then stop.\nnext line");
        assert!(!out.contains('"'), "no straight double-quotes remain: {out}");
        assert!(!out.contains('\n'), "newlines collapsed: {out}");
        assert!(out.contains("post a \u{2019}done\u{2019} message; then stop. next line"));
        // A quote-free task is unchanged.
        assert_eq!(cmd_safe_task("build the parser"), "build the parser");
    }

    #[test]
    fn swarm_args_sanitises_a_task_with_quotes_for_argv() {
        let agent = AgentManifest {
            swarm: Some(crate::agents::SwarmRecipe { args: vec!["-p".into(), "{task}".into()] }),
            ..AgentManifest::default()
        };
        let args = ProductionPlanner::swarm_args(&agent, "/m/x.json", "say \"hi\"");
        assert_eq!(args, vec!["-p", "say \u{2019}hi\u{2019}"]); // quotes neutralised before argv
    }

    #[test]
    fn without_a_swarm_recipe_the_fallback_is_variadic_safe() {
        // No recipe (e.g. a stale catalog) → the fallback must NOT put the task after --mcp-config, or
        // claude's greedy `--mcp-config <configs...>` slurps it as a second config file (CPE-590).
        let agent = AgentManifest::default();
        let args = ProductionPlanner::swarm_args(&agent, "/m/mcp-x.json", "do the thing");
        assert_eq!(args, vec!["-p", "do the thing", "--mcp-config", "/m/mcp-x.json", "--dangerously-skip-permissions"]);
        // The config path is the last token before a flag — nothing the variadic option can swallow.
        let cfg_at = args.iter().position(|a| a == "--mcp-config").unwrap();
        assert!(args[cfg_at + 2].starts_with("--"), "a flag must follow the config path, not the task");
    }

    #[test]
    fn an_unknown_agent_is_an_error_not_a_panic() {
        let mission = tempfile::tempdir().unwrap();
        let p = planner(mission.path().to_path_buf());
        let assignment = Assignment { task_id: "t1".into(), agent_id: "ghost#builder1".into() };
        let spec = SwarmLaunch { agent: "ghost".into(), model: None, task: "x".into() };
        assert!(p.plan(&assignment, &spec).is_err());
    }

    #[test]
    fn roster_for_matches_the_coordinator_instance_ids() {
        let team = TeamManifest {
            name: "T".into(),
            description: String::new(),
            roles: vec![
                RoleSpec { role: Role::Coordinator, agent: "claude".into(), model: None, count: 1 },
                RoleSpec { role: Role::Builder, agent: "claude".into(), model: None, count: 2 },
            ],
        };
        let roster = ProductionPlanner::roster_for(&team);
        let ids: Vec<&str> = roster.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, vec!["claude#coordinator1", "claude#builder1", "claude#builder2"]);
    }

    #[test]
    fn init_mission_writes_a_roster_the_host_can_read() {
        let mission = tempfile::tempdir().unwrap();
        let p = planner(mission.path().to_path_buf());
        p.init_mission(&[("claude#builder1".into(), Role::Builder)]).unwrap();
        assert!(mission.path().join("members.json").exists());
    }
}
