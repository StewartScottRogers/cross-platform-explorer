//! Agent lifecycle — detection (CPE-281).
//!
//! Runs an agent manifest's per-OS `detect` command (e.g. `claude --version`) to report
//! whether the CLI is installed and at what version. Command execution is abstracted
//! behind [`CommandRunner`] so the detection logic is unit-testable without touching the
//! system; [`RealRunner`] does the actual `std::process` call. No shell scripts — Rust
//! runs the probe and parses the result.

use crate::agents::{AgentManifest, OsCommand};

/// The captured result of running a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub status_ok: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Runs a command and captures its output. Abstracted for testability.
pub trait CommandRunner {
    fn run(&self, command: &str, args: &[String]) -> Result<CommandOutput, String>;
}

/// The production runner: spawns the process and captures stdout/stderr.
pub struct RealRunner;

impl CommandRunner for RealRunner {
    fn run(&self, command: &str, args: &[String]) -> Result<CommandOutput, String> {
        let mut cmd = std::process::Command::new(command);
        cmd.args(args);
        crate::hide_console(&mut cmd); // no flashing console window on Windows (CPE-325)
        let out = cmd
            .output()
            .map_err(|e| format!("could not run {command}: {e}"))?;
        Ok(CommandOutput {
            status_ok: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

/// Whether an agent CLI is installed, and its reported version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectResult {
    pub installed: bool,
    pub version: Option<String>,
}

/// Detect whether `agent` is installed, using its per-OS detect command via `runner`.
/// A manifest with no detect command for this OS reports not-installed (undetectable);
/// a command that runs but exits non-zero (or fails to spawn — not on PATH) reports
/// not-installed.
pub fn detect(agent: &AgentManifest, runner: &dyn CommandRunner) -> DetectResult {
    let Some(cmd) = agent.detect_for_current_os() else {
        return DetectResult { installed: false, version: None };
    };
    match runner.run(&cmd.command, &cmd.args) {
        Ok(out) if out.status_ok => DetectResult {
            installed: true,
            version: parse_version(&out.stdout),
        },
        _ => DetectResult { installed: false, version: None },
    }
}

/// Best-effort version string: the first non-empty line of stdout, trimmed. Many CLIs
/// print `1.2.3` or `tool 1.2.3`; we keep the whole line rather than guess a format.
fn parse_version(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

/// Install the agent by running its per-OS `install` recipe via `runner` (CPE-282).
/// Rust orchestrates the underlying package manager (npm/winget/brew/pipx) — the
/// recipe is data, there are no shell scripts. Returns the captured output on success,
/// or an error if there is no install recipe, the command can't be spawned, or it exits
/// non-zero.
pub fn install(agent: &AgentManifest, runner: &dyn CommandRunner) -> Result<CommandOutput, String> {
    let cmd = agent
        .install_for_current_os()
        .ok_or_else(|| format!("agent '{}' has no install recipe for this OS", agent.id))?;
    run_step(agent, "install", cmd, runner)
}

/// Update the agent. Uses the `update` recipe if present, else falls back to `install`
/// (re-running a package-manager install updates it — as in the reference).
pub fn update(agent: &AgentManifest, runner: &dyn CommandRunner) -> Result<CommandOutput, String> {
    match agent.update_for_current_os() {
        Some(cmd) => run_step(agent, "update", cmd, runner),
        None => install(agent, runner),
    }
}

/// Uninstall the agent by running its per-OS `uninstall` recipe via `runner`
/// (CPE-283). Never removes prerequisites shared by other agents — the recipe should
/// remove only the agent's own package.
pub fn uninstall(agent: &AgentManifest, runner: &dyn CommandRunner) -> Result<CommandOutput, String> {
    let cmd = agent
        .uninstall_for_current_os()
        .ok_or_else(|| format!("agent '{}' has no uninstall recipe for this OS", agent.id))?;
    run_step(agent, "uninstall", cmd, runner)
}

fn run_step(
    agent: &AgentManifest,
    step: &str,
    cmd: &OsCommand,
    runner: &dyn CommandRunner,
) -> Result<CommandOutput, String> {
    let out = runner.run(&cmd.command, &cmd.args)?;
    if out.status_ok {
        Ok(out)
    } else {
        Err(format!(
            "{step} of '{}' failed: {}",
            agent.id,
            out.stderr.trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::AgentRegistry;
    use std::io::Write;

    fn agent_with_detect(detect: bool) -> AgentManifest {
        let d = tempfile::tempdir().unwrap();
        let detect_block = if detect {
            r#""detect": { "windows": { "command": "claude", "args": ["--version"] },
                          "macos": { "command": "claude", "args": ["--version"] },
                          "linux": { "command": "claude", "args": ["--version"] } },"#
        } else {
            ""
        };
        let json = format!(
            r#"{{ "schema_version": 1, "id": "claude", "name": "Claude Code",
                 {detect_block}
                 "run": {{ "windows": {{ "command": "claude" }}, "macos": {{ "command": "claude" }}, "linux": {{ "command": "claude" }} }} }}"#
        );
        let mut f = std::fs::File::create(d.path().join("claude.json")).unwrap();
        f.write_all(json.as_bytes()).unwrap();
        AgentRegistry::load_from_dirs(&[d.path().to_path_buf()])
            .get("claude")
            .unwrap()
            .clone()
    }

    struct FakeRunner {
        result: Result<CommandOutput, String>,
    }
    impl CommandRunner for FakeRunner {
        fn run(&self, _c: &str, _a: &[String]) -> Result<CommandOutput, String> {
            self.result.clone()
        }
    }

    #[test]
    fn installed_when_detect_succeeds_and_parses_version() {
        let runner = FakeRunner {
            result: Ok(CommandOutput { status_ok: true, stdout: "claude 1.2.3\n".into(), stderr: String::new() }),
        };
        let r = detect(&agent_with_detect(true), &runner);
        assert!(r.installed);
        assert_eq!(r.version.as_deref(), Some("claude 1.2.3"));
    }

    #[test]
    fn not_installed_when_command_exits_nonzero() {
        let runner = FakeRunner {
            result: Ok(CommandOutput { status_ok: false, stdout: String::new(), stderr: "not found".into() }),
        };
        assert!(!detect(&agent_with_detect(true), &runner).installed);
    }

    #[test]
    fn not_installed_when_spawn_fails() {
        let runner = FakeRunner { result: Err("no such file".into()) };
        assert!(!detect(&agent_with_detect(true), &runner).installed);
    }

    #[test]
    fn undetectable_when_no_detect_command() {
        let runner = FakeRunner {
            result: Ok(CommandOutput { status_ok: true, stdout: "x".into(), stderr: String::new() }),
        };
        let r = detect(&agent_with_detect(false), &runner);
        assert!(!r.installed);
        assert_eq!(r.version, None);
    }

    /// A manifest with install (and optionally update) recipes for every OS.
    fn agent_with_install(with_update: bool) -> AgentManifest {
        let d = tempfile::tempdir().unwrap();
        let update_block = if with_update {
            r#""update": { "windows": { "command": "npm", "args": ["update"] },
                          "macos": { "command": "npm", "args": ["update"] },
                          "linux": { "command": "npm", "args": ["update"] } },"#
        } else {
            ""
        };
        let json = format!(
            r#"{{ "schema_version": 1, "id": "claude", "name": "Claude Code",
                 "install": {{ "windows": {{ "command": "npm", "args": ["i", "-g", "x"] }},
                              "macos": {{ "command": "npm", "args": ["i", "-g", "x"] }},
                              "linux": {{ "command": "npm", "args": ["i", "-g", "x"] }} }},
                 {update_block}
                 "run": {{ "windows": {{ "command": "claude" }}, "macos": {{ "command": "claude" }}, "linux": {{ "command": "claude" }} }} }}"#
        );
        let mut f = std::fs::File::create(d.path().join("claude.json")).unwrap();
        f.write_all(json.as_bytes()).unwrap();
        AgentRegistry::load_from_dirs(&[d.path().to_path_buf()]).get("claude").unwrap().clone()
    }

    fn ok_runner() -> FakeRunner {
        FakeRunner { result: Ok(CommandOutput { status_ok: true, stdout: "done".into(), stderr: String::new() }) }
    }

    #[test]
    fn install_succeeds_when_the_command_succeeds() {
        assert!(install(&agent_with_install(false), &ok_runner()).is_ok());
    }

    #[test]
    fn install_errors_on_nonzero_with_stderr() {
        let runner = FakeRunner {
            result: Ok(CommandOutput { status_ok: false, stdout: String::new(), stderr: "npm boom".into() }),
        };
        let err = install(&agent_with_install(false), &runner).unwrap_err();
        assert!(err.contains("install of 'claude' failed"));
        assert!(err.contains("npm boom"));
    }

    #[test]
    fn install_errors_when_no_recipe() {
        // agent_with_detect(false) has no install recipe.
        let err = install(&agent_with_detect(false), &ok_runner()).unwrap_err();
        assert!(err.contains("no install recipe"));
    }

    #[test]
    fn update_falls_back_to_install_when_no_update_recipe() {
        // No update recipe → uses install, which succeeds.
        assert!(update(&agent_with_install(false), &ok_runner()).is_ok());
    }

    #[test]
    fn update_uses_the_update_recipe_when_present() {
        assert!(update(&agent_with_install(true), &ok_runner()).is_ok());
    }

    fn agent_with_uninstall() -> AgentManifest {
        let d = tempfile::tempdir().unwrap();
        let json = r#"{ "schema_version": 1, "id": "claude", "name": "Claude Code",
             "uninstall": { "windows": { "command": "npm", "args": ["rm", "-g", "x"] },
                            "macos": { "command": "npm", "args": ["rm", "-g", "x"] },
                            "linux": { "command": "npm", "args": ["rm", "-g", "x"] } },
             "run": { "windows": { "command": "claude" }, "macos": { "command": "claude" }, "linux": { "command": "claude" } } }"#;
        let mut f = std::fs::File::create(d.path().join("claude.json")).unwrap();
        f.write_all(json.as_bytes()).unwrap();
        AgentRegistry::load_from_dirs(&[d.path().to_path_buf()]).get("claude").unwrap().clone()
    }

    #[test]
    fn uninstall_succeeds_and_errors_appropriately() {
        assert!(uninstall(&agent_with_uninstall(), &ok_runner()).is_ok());
        // No uninstall recipe → error.
        let err = uninstall(&agent_with_install(false), &ok_runner()).unwrap_err();
        assert!(err.contains("no uninstall recipe"));
    }
}
