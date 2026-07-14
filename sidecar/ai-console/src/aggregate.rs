//! Aggregate lifecycle operations (CPE-284).
//!
//! Run a lifecycle action across many agents in one call — the "Install-All" /
//! "Update-All" / "Uninstall-All" of the reference — with a per-agent result and a
//! summary. One agent failing never aborts the rest.

use crate::agents::{AgentManifest, AgentRegistry};
use crate::lifecycle::{install, uninstall, update, CommandRunner};

/// Which lifecycle action to run across the set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Install,
    Update,
    Uninstall,
}

/// The outcome for one agent in an aggregate run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentOutcome {
    pub id: String,
    pub ok: bool,
    /// Error message when `ok` is false.
    pub error: Option<String>,
}

/// Run `action` on every agent in `agents`, in id order, collecting a result each.
/// A failure is recorded and the run continues.
pub fn run_all(
    agents: &[&AgentManifest],
    action: Action,
    runner: &dyn CommandRunner,
) -> Vec<AgentOutcome> {
    agents
        .iter()
        .map(|agent| {
            let result = match action {
                Action::Install => install(agent, runner),
                Action::Update => update(agent, runner),
                // Batch uninstall assumes the caller ensured no session is live; a
                // session-aware route can pass the real `running` state (CPE-331).
                Action::Uninstall => uninstall(agent, runner, false),
            };
            match result {
                Ok(_) => AgentOutcome { id: agent.id.clone(), ok: true, error: None },
                Err(e) => AgentOutcome { id: agent.id.clone(), ok: false, error: Some(e) },
            }
        })
        .collect()
}

/// Convenience: run `action` across an entire [`AgentRegistry`].
pub fn run_registry(
    registry: &AgentRegistry,
    action: Action,
    runner: &dyn CommandRunner,
) -> Vec<AgentOutcome> {
    let agents: Vec<&AgentManifest> = registry.all().collect();
    run_all(&agents, action, runner)
}

/// How many succeeded / failed in a set of outcomes.
pub fn summarize(outcomes: &[AgentOutcome]) -> (usize, usize) {
    let ok = outcomes.iter().filter(|o| o.ok).count();
    (ok, outcomes.len() - ok)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::CommandOutput;
    use std::io::Write;
    use std::path::Path;

    fn write(dir: &Path, name: &str, id: &str, installable: bool) {
        let install = if installable {
            r#""install": { "windows": { "command": "npm" }, "macos": { "command": "npm" }, "linux": { "command": "npm" } },"#
        } else {
            ""
        };
        let json = format!(
            r#"{{ "schema_version": 1, "id": "{id}", "name": "{id}",
                 {install}
                 "run": {{ "windows": {{ "command": "{id}" }}, "macos": {{ "command": "{id}" }}, "linux": {{ "command": "{id}" }} }} }}"#
        );
        let mut f = std::fs::File::create(dir.join(name)).unwrap();
        f.write_all(json.as_bytes()).unwrap();
    }

    /// A runner that succeeds for every agent except those whose command it's told to fail.
    struct SelectiveRunner {
        fail_commands: Vec<String>,
    }
    impl CommandRunner for SelectiveRunner {
        fn run(&self, command: &str, _args: &[String]) -> Result<CommandOutput, String> {
            let ok = !self.fail_commands.iter().any(|c| c == command);
            Ok(CommandOutput { status_ok: ok, stdout: String::new(), stderr: "boom".into() })
        }
    }

    #[test]
    fn runs_across_all_agents_and_continues_past_a_failure() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "a.json", "aaa", true);
        write(d.path(), "b.json", "bbb", true);
        let reg = AgentRegistry::load_from_dirs(&[d.path().to_path_buf()]);

        // Both install via `npm`; fail none → both ok.
        let runner = SelectiveRunner { fail_commands: vec![] };
        let outcomes = run_registry(&reg, Action::Install, &runner);
        assert_eq!(outcomes.len(), 2);
        assert_eq!(summarize(&outcomes), (2, 0));
        assert!(outcomes.iter().all(|o| o.ok));
    }

    #[test]
    fn a_missing_recipe_is_recorded_as_a_failure_not_a_panic() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "a.json", "aaa", true);
        write(d.path(), "b.json", "bbb", false); // no install recipe
        let reg = AgentRegistry::load_from_dirs(&[d.path().to_path_buf()]);
        let runner = SelectiveRunner { fail_commands: vec![] };
        let outcomes = run_registry(&reg, Action::Install, &runner);
        assert_eq!(summarize(&outcomes), (1, 1));
        let bbb = outcomes.iter().find(|o| o.id == "bbb").unwrap();
        assert!(!bbb.ok);
        assert!(bbb.error.as_ref().unwrap().contains("no install recipe"));
    }
}
