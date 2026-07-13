//! Agent lifecycle — detection (CPE-281).
//!
//! Runs an agent manifest's per-OS `detect` command (e.g. `claude --version`) to report
//! whether the CLI is installed and at what version. Command execution is abstracted
//! behind [`CommandRunner`] so the detection logic is unit-testable without touching the
//! system; [`RealRunner`] does the actual `std::process` call. No shell scripts — Rust
//! runs the probe and parses the result.

use crate::agents::AgentManifest;

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
        let out = std::process::Command::new(command)
            .args(args)
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
}
