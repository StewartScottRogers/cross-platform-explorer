//! MCP server lifecycle & credentials (CPE-307).
//!
//! Plugins (CPE-288) install MCP servers — long-running processes with their own
//! credentials (e.g. the reference's cipher server reading `~/.cipher/cipher.yml`).
//! Managing them is more than editing a config: they need start / stop / health, their
//! credentials provisioned from the vault (never plaintext), and cleanup when the plugin
//! is uninstalled. This module manages those background processes.

use std::collections::BTreeMap;
use std::process::{Child, Command, Stdio};

use crate::agents::OsCommand;

/// How to launch one MCP server.
pub struct McpServerSpec {
    pub id: String,
    pub command: OsCommand,
    /// Environment for the server (credentials resolved from the vault by the caller;
    /// injected into the child only, never logged).
    pub env: BTreeMap<String, String>,
}

/// A running MCP server process.
pub struct McpProcess {
    id: String,
    child: Child,
}

impl McpProcess {
    /// Spawn the server as a background process (stdio detached).
    pub fn spawn(spec: &McpServerSpec) -> Result<McpProcess, String> {
        let mut cmd = Command::new(&spec.command.command);
        cmd.args(&spec.command.args);
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        crate::hide_console(&mut cmd); // no flashing console window on Windows (CPE-325)
        let child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("spawn MCP server '{}': {e}", spec.id))?;
        Ok(McpProcess { id: spec.id.clone(), child })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// True while the server is still running (health = liveness).
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Stop the server.
    pub fn stop(&mut self) -> Result<(), String> {
        self.child.kill().map_err(|e| e.to_string())?;
        let _ = self.child.wait();
        Ok(())
    }
}

impl Drop for McpProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Manages the set of running MCP servers, keyed by id.
#[derive(Default)]
pub struct McpManager {
    servers: BTreeMap<String, McpProcess>,
}

impl McpManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start a server (replacing any existing one with the same id).
    pub fn start(&mut self, spec: &McpServerSpec) -> Result<(), String> {
        if let Some(mut old) = self.servers.remove(&spec.id) {
            let _ = old.stop();
        }
        let proc = McpProcess::spawn(spec)?;
        self.servers.insert(spec.id.clone(), proc);
        Ok(())
    }

    pub fn is_running(&mut self, id: &str) -> bool {
        self.servers.get_mut(id).map(|p| p.is_running()).unwrap_or(false)
    }

    /// Stop and forget a server (e.g. on plugin uninstall).
    pub fn stop(&mut self, id: &str) -> Result<(), String> {
        match self.servers.remove(id) {
            Some(mut p) => p.stop(),
            None => Ok(()),
        }
    }

    /// Stop every server (on shutdown).
    pub fn stop_all(&mut self) {
        for (_, mut p) in std::mem::take(&mut self.servers) {
            let _ = p.stop();
        }
    }

    pub fn running_ids(&self) -> impl Iterator<Item = &String> {
        self.servers.keys()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pty::shell_command;

    fn long_running_spec(id: &str) -> McpServerSpec {
        let inline = if cfg!(target_os = "windows") { "ping -n 20 127.0.0.1 >NUL" } else { "sleep 20" };
        let (command, args) = shell_command(inline);
        McpServerSpec { id: id.into(), command: OsCommand { command, args }, env: BTreeMap::new() }
    }

    #[test]
    fn starts_reports_running_then_stops() {
        let mut mgr = McpManager::new();
        mgr.start(&long_running_spec("cipher")).unwrap();
        assert!(mgr.is_running("cipher"));
        assert_eq!(mgr.running_ids().count(), 1);
        mgr.stop("cipher").unwrap();
        assert!(!mgr.is_running("cipher"));
    }

    #[test]
    fn stop_all_clears_everything() {
        let mut mgr = McpManager::new();
        mgr.start(&long_running_spec("a")).unwrap();
        mgr.start(&long_running_spec("b")).unwrap();
        assert_eq!(mgr.running_ids().count(), 2);
        mgr.stop_all();
        assert_eq!(mgr.running_ids().count(), 0);
    }

    #[test]
    fn stopping_an_unknown_server_is_ok() {
        let mut mgr = McpManager::new();
        assert!(mgr.stop("nope").is_ok());
    }

    #[test]
    fn spawn_failure_is_reported() {
        let spec = McpServerSpec {
            id: "bad".into(),
            command: OsCommand { command: "definitely-not-a-real-binary-xyz".into(), args: vec![] },
            env: BTreeMap::new(),
        };
        assert!(McpProcess::spawn(&spec).is_err());
    }
}
