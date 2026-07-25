//! End-to-end swarm proof (CPE-541, epic CPE-528). Closes the last headlessly-verifiable gap: that a
//! **launched agent process** reads the planner's injected `--mcp-config`, spawns the `--swarm-mcp`
//! host, and coordinates over it — so real shared state lands on disk. It runs a real [`SwarmDriver`]
//! over the real [`LocalEngine`], launching the built `ai-console --swarm-agent-sim` binary as a
//! stand-in coding agent (no LLM, no cost, deterministic). The only thing it doesn't exercise is a
//! *real* coding agent honouring `--mcp-config` — that's the standard Claude Code convention, and the
//! remaining piece is pure GUI QA.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ai_console::pty::PtyLaunch;
use ai_console::session_engine::LocalEngine;
use ai_console::swarm_bridge::SwarmLaunch;
use ai_console::swarm_coordinator::{Assignment, Gate, Task};
use ai_console::swarm_live::{assume_success, EngineRunner, LaunchPlanner, SwarmDriver};
use ai_console::swarm_team::{Role, RoleSpec, TeamManifest};

/// Path to the freshly-built `ai-console` binary Cargo provides to integration tests.
const AI_CONSOLE: &str = env!("CARGO_BIN_EXE_ai-console");

/// A planner that reproduces the production launch shape headlessly: it writes the same per-agent
/// swarm MCP config the live `ProductionPlanner` does, then launches the `--swarm-agent-sim` stand-in
/// pointed at that config (instead of a real `claude`).
struct SimPlanner {
    dir: PathBuf,
}

impl LaunchPlanner for SimPlanner {
    fn plan(&self, a: &Assignment, _spec: &SwarmLaunch) -> Result<PtyLaunch, String> {
        // The swarm host command every agent's MCP config points at (same form the ProductionPlanner writes).
        let cfg = serde_json::json!({
            "mcpServers": {
                "swarm": {
                    "command": AI_CONSOLE,
                    "args": ["--swarm-mcp", "--dir", self.dir.to_string_lossy(), "--agent", a.agent_id],
                }
            }
        });
        let cfg_path = self.dir.join(format!("mcp-{}.json", a.agent_id.replace('#', "-")));
        std::fs::write(&cfg_path, cfg.to_string()).map_err(|e| e.to_string())?;

        Ok(PtyLaunch {
            program: AI_CONSOLE.to_string(),
            args: vec![
                "--swarm-agent-sim".into(),
                "--mcp-config".into(),
                cfg_path.to_string_lossy().into_owned(),
                "--agent".into(),
                a.agent_id.clone(),
            ],
            cwd: None,
            env: BTreeMap::new(),
            rows: 24,
            cols: 80,
        })
    }
}

#[test]
fn a_launched_agent_coordinates_over_the_injected_mcp_host() {
    let dir = tempfile::tempdir().unwrap();

    let team = TeamManifest {
        name: "e2e".into(),
        description: String::new(),
        roles: vec![
            RoleSpec { role: Role::Coordinator, agent: "claude".into(), model: None, count: 1 },
            RoleSpec { role: Role::Builder, agent: "claude".into(), model: None, count: 1 },
        ],
    };
    let tasks = vec![Task {
        id: "t1".into(),
        description: "wire the parser".into(),
        role: Role::Builder,
        globs: vec!["src/**".into()],
        gate: Gate::None,
    }];

    let planner = SimPlanner { dir: dir.path().to_path_buf() };
    let runner = Arc::new(EngineRunner::new(Arc::new(LocalEngine)));
    let driver = SwarmDriver::new(team, tasks, runner, Arc::new(planner), assume_success())
        .expect("staff the mission")
        .with_timeout(Duration::from_secs(60));

    let coord = driver.run().expect("driver runs to completion");
    assert!(coord.is_complete(), "the task should be Done");
    assert!(!coord.has_failure(), "no launch/session should have failed");

    // The launched agent posted to the mailbox via the host → the shared post log exists and holds it.
    let mailbox = std::fs::read_to_string(dir.path().join("mailbox.jsonl"))
        .expect("agent posted to the mailbox via the injected MCP host");
    assert!(mailbox.contains("finished"), "the agent's `done` post should be in the shared log: {mailbox}");
    assert!(mailbox.contains("claude#builder1"), "the post is attributed to the launched agent: {mailbox}");

    // …and wrote a memory note via the host → a note file landed in the shared memory dir.
    let notes: Vec<_> = std::fs::read_dir(dir.path().join("memory"))
        .expect("agent wrote to shared memory via the injected MCP host")
        .filter_map(Result::ok)
        .collect();
    assert!(!notes.is_empty(), "the agent's memory.write should have persisted a note");
    let body = std::fs::read_to_string(notes[0].path()).unwrap();
    assert!(body.contains("completed its task"), "the persisted note carries the agent's memory body: {body}");
}
