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

    /// The run args that attach the swarm MCP host + the task to this agent, from the agent's manifest
    /// **`swarm` recipe** (CPE-583; a variadic-safe print-mode fallback when absent, CPE-590), with the
    /// `{mcp_config}` placeholder filled.
    ///
    /// **Task delivery (CPE-588)** — the task reaches the agent **verbatim**, never rewritten:
    /// - **Windows**: the launch goes through `cmd /c <shim>`, which mangles quotes/specials in an argv
    ///   ([CPE-587]). So the `{task}` argv is dropped and the task is written to `<mission>/task-*.txt`
    ///   fed into the agent's **stdin** (`claude -p` reads its prompt from stdin) via an appended `<`
    ///   redirect — a file's bytes never touch the command line. `portable_pty` passes a bare `<`
    ///   un-quoted, so `cmd` performs the redirect.
    /// - **Unix**: launched directly (no shell), so `{task}` is safe **verbatim in argv**.
    fn swarm_args(&self, agent: &AgentManifest, config_path: &str, task: &str, agent_instance: &str) -> Result<Vec<String>, String> {
        let recipe: Vec<String> = match &agent.swarm {
            Some(r) => r.args.clone(),
            None => vec![
                "-p".into(),
                "{task}".into(),
                "--mcp-config".into(),
                "{mcp_config}".into(),
                "--dangerously-skip-permissions".into(),
            ],
        };
        if cfg!(windows) {
            let task_file = self.mission_dir.join(format!("task-{}.txt", sanitize(agent_instance)));
            std::fs::write(&task_file, task).map_err(|e| format!("write task file: {e}"))?;
            let mut out: Vec<String> = recipe
                .iter()
                .filter(|a| !a.contains("{task}")) // the task is delivered via stdin, not argv
                .map(|a| a.replace("{mcp_config}", config_path))
                .collect();
            out.push("<".to_string());
            out.push(task_file.to_string_lossy().into_owned());
            Ok(out)
        } else {
            Ok(recipe
                .iter()
                .map(|a| a.replace("{task}", task).replace("{mcp_config}", config_path))
                .collect())
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
        let extra_args = self.swarm_args(&agent, &config_path.to_string_lossy(), &spec.task, &assignment.agent_id)?;

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
        assert!(launch.args.contains(&"-p".to_string()), "print mode injected so the agent exits");
        assert!(launch.args.contains(&"--dangerously-skip-permissions".to_string()), "non-interactive permissions");
        assert_eq!(launch.cwd.as_deref(), Some("/repo"));
        // CPE-588: the task reaches the agent verbatim — in argv on Unix, or via a stdin-redirected file
        // on Windows (where argv goes through cmd).
        if cfg!(windows) {
            let lt = launch.args.iter().position(|a| a == "<").expect("stdin redirect on Windows");
            let task_file = &launch.args[lt + 1];
            assert_eq!(std::fs::read_to_string(task_file).unwrap(), "build the parser", "task file is byte-for-byte");
            assert!(!launch.args.contains(&"build the parser".to_string()), "task is not on the command line");
        } else {
            let p_at = launch.args.iter().position(|a| a == "-p").unwrap();
            assert_eq!(launch.args.get(p_at + 1), Some(&"build the parser".to_string()), "task is the print prompt");
        }

        // The per-agent MCP config file exists and points the agent at its shared host.
        let cfg_path = mission.path().join("mcp-claude-builder1.json");
        assert!(cfg_path.exists(), "per-agent MCP config written");
        let cfg: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&cfg_path).unwrap()).unwrap();
        let args = &cfg["mcpServers"]["swarm"]["args"];
        assert_eq!(args[0], json!("--swarm-mcp"));
        assert!(args.as_array().unwrap().iter().any(|a| a == "claude#builder1"), "host launched for this agent instance");
    }

    #[test]
    fn swarm_args_delivers_the_task_verbatim() {
        let mission = tempfile::tempdir().unwrap();
        let p = planner(mission.path().to_path_buf());
        let agent = AgentManifest {
            swarm: Some(crate::agents::SwarmRecipe {
                args: vec!["-p".into(), "{task}".into(), "--mcp-config".into(), "{mcp_config}".into()],
            }),
            ..AgentManifest::default()
        };
        // A task full of characters that a cmd re-parse would mangle.
        let task = "post a \"done\" note; keep 100% of it & don't edit.";
        let args = p.swarm_args(&agent, "/m/x.json", task, "claude#builder1").unwrap();
        assert!(args.contains(&"--mcp-config".to_string()) && args.contains(&"/m/x.json".to_string()));
        if cfg!(windows) {
            assert!(!args.iter().any(|a| a.contains("done")), "the raw task never touches the command line: {args:?}");
            let lt = args.iter().position(|a| a == "<").expect("stdin redirect");
            assert_eq!(std::fs::read_to_string(&args[lt + 1]).unwrap(), task, "task file is byte-for-byte");
        } else {
            assert!(args.contains(&task.to_string()), "task verbatim in argv on Unix: {args:?}");
        }
    }

    #[test]
    fn without_a_swarm_recipe_the_fallback_never_lets_mcp_config_swallow_the_task() {
        // No recipe (e.g. a stale catalog) → the variadic-safe fallback (CPE-590) is used, and the task
        // is still delivered verbatim (CPE-588). Either way the token after the config path is a flag or
        // the `<` redirect — never the task (which claude's greedy `--mcp-config` would slurp).
        let mission = tempfile::tempdir().unwrap();
        let p = planner(mission.path().to_path_buf());
        let agent = AgentManifest::default(); // no swarm recipe
        let args = p.swarm_args(&agent, "/m/mcp-x.json", "do the thing", "claude#builder1").unwrap();
        assert!(args.contains(&"-p".to_string()) && args.contains(&"--dangerously-skip-permissions".to_string()));
        let cfg_at = args.iter().position(|a| a == "--mcp-config").unwrap();
        assert_eq!(args[cfg_at + 1], "/m/mcp-x.json");
        let after = args.get(cfg_at + 2).map(String::as_str).unwrap_or("");
        assert!(after.starts_with("--") || after == "<", "config path is not followed by the task: {args:?}");
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
