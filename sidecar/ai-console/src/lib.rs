//! # ai-console
//!
//! The **Agent Deck** sidecar (CPE-261): an agentic CLI manager that installs/manages
//! any coding-agent CLI with any provider and any model, and runs the native CLI in an
//! in-app console. This crate holds the sidecar's **backend/domain logic**; the process
//! entry point is `src/main.rs`. It depends ONLY on `sidecar-contract` (ADR 0001).
//!
//! This module (CPE-277) is the skeleton: the handshake identity + capability request,
//! and the base protocol loop as a pure, testable state machine. Domain modules —
//! agent registry, provider routing, secret vault, lifecycle — are added by later
//! tickets and re-exported here.

/// Suppress the console window Windows would otherwise pop for a spawned CLI (CPE-325).
/// A no-op off Windows. Apply to every `std::process::Command` before spawning.
#[cfg(windows)]
pub(crate) fn hide_console(cmd: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
}
#[cfg(not(windows))]
pub(crate) fn hide_console(_cmd: &mut std::process::Command) {}

/// Resolve a CLI invocation for the current OS (CPE-326). On Windows, run it through
/// `cmd /c` so the shell applies PATHEXT and resolves script shims — npm/pip installers
/// create `foo.cmd`/`foo.ps1`, not a bare `foo.exe`, and executing the extensionless `foo`
/// fails with "not a valid Win32 application". Elsewhere the command runs as-is.
#[cfg(windows)]
pub(crate) fn cli_command(program: &str, args: &[String]) -> (String, Vec<String>) {
    let mut wrapped = Vec::with_capacity(args.len() + 2);
    wrapped.push("/c".to_string());
    wrapped.push(program.to_string());
    wrapped.extend_from_slice(args);
    ("cmd".to_string(), wrapped)
}
#[cfg(not(windows))]
pub(crate) fn cli_command(program: &str, args: &[String]) -> (String, Vec<String>) {
    (program.to_string(), args.to_vec())
}

pub mod agent_reads;
pub mod aggregate;
pub mod agents;
pub mod broker_client;
pub mod catalog;
pub mod conflict;
pub mod cost;
pub mod console;
pub mod guardrail;
pub mod history;
pub mod http;
pub mod keycheck;
pub mod lifecycle;

pub use history::{HistoryBackend, MemHistory, SessionHistory, SessionRecord};
pub mod lmstudio;
pub mod mcp;
pub mod model_catalog;
pub mod model_snapshot;
pub mod plugins;
pub mod presets;
pub mod pty;

pub use mcp::{McpManager, McpProcess, McpServerSpec};
pub mod routing;
pub mod scope;
pub mod session_diag;
pub mod session_daemon;
pub use session_daemon::{Attachment, SessionDaemon};
pub mod session_client;
pub mod session_engine;
pub use session_engine::{DaemonEngine, LocalEngine, SessionEngine, SessionIo};
pub mod session_server;
pub mod session_supervisor;
pub mod swarm_locks;
pub use swarm_locks::{path_globs_overlap, ClaimOutcome, LockManager};
pub mod swarm_team;
pub use swarm_team::{default_team, Role, RoleSpec, TeamError, TeamManifest};
pub mod swarm_mailbox;
pub use swarm_mailbox::Mailbox; // Message/Recipient via swarm_mailbox:: (Message clashes with the contract's)
pub mod swarm_coordinator;
pub use swarm_coordinator::{Assignment, Budget, Coordinator, Gate, Task, TaskState};
pub mod agent_memory;
pub use agent_memory::{load_dir as load_memory_dir, memory_tool, parse_note, save_note, MemoryGraph, Note};
pub mod swarm_bridge;
pub use swarm_bridge::{launch_spec_for, SwarmLaunch};
pub mod swarm_driver;
pub use swarm_driver::{apply_outcome, SessionOutcome};
pub mod swarm_mcp;
pub use swarm_mcp::{dispatch_tool as swarm_tool_call, tools_manifest as swarm_tools_manifest};
pub mod swarm_mcp_server;
pub mod swarm_live;
pub use swarm_live::{assume_success, LaunchPlanner, SwarmDriver};
pub mod swarm_plan;
pub use swarm_plan::ProductionPlanner;
pub use session_client::{SessionClient, StreamMsg};

pub use plugins::{install_plugin, uninstall_plugin, PluginApplier, PluginManifest, PluginRegistry};
pub use scope::{build_launch, dangerous_flags, AgentLaunchRequest};
pub mod ui;
pub use ui::{serve as serve_ui, UiServer};
pub mod usage;
pub use usage::{Usage, UsageScanner};
pub mod vault;

pub use pty::{PtyLaunch, PtySession};

pub use vault::{resolve_env, CredentialProfile, ProfileSet, SecretAccess};
pub use broker_client::{
    BrokerClient, BrokerDialogs, BrokerHistory, BrokerPresets, BrokerSecrets, HostDialogs,
    MemSecrets, NoopDialogs, SharedWriter,
};
pub use presets::{CredentialRef, MemPresets, Preset, PresetStore, PresetsBackend};

pub use lmstudio::{detect as detect_lmstudio, LmStudio};

pub use aggregate::{run_all, run_registry, summarize, Action, AgentOutcome};

pub use agents::{AgentManifest, AgentRegistry, OsCommand, ProviderRecipe};
pub use lifecycle::{detect, install, uninstall, update, CommandRunner, DetectResult, RealRunner};
pub use routing::{compose_launch, compose_reseller_launch, Launch, LaunchContext, ResellerDescriptor};

use sidecar_contract::{
    Capability, Envelope, Hello, Lifecycle, Message, Response, CONTRACT_VERSION,
};

/// The sidecar's stable id (matches its manifest).
pub const SIDECAR_ID: &str = "ai-console";

/// Capabilities the Agent Deck requests at handshake:
/// - [`Capability::Context`] — scope a session to the open repo,
/// - [`Capability::Secrets`] — store provider keys / credential profiles,
/// - [`Capability::Storage`] — its own settings, catalog cache, session history,
/// - [`Capability::Events`] — surface install/session notifications.
pub const REQUESTED_CAPABILITIES: [Capability; 4] = [
    Capability::Context,
    Capability::Secrets,
    Capability::Storage,
    Capability::Events,
];

/// What the process should do after handling one inbound message. Keeping the protocol
/// as a pure function makes the sidecar loop unit-testable without stdio.
#[derive(Debug, PartialEq)]
pub enum Reaction {
    /// Write this envelope to the host.
    Send(Envelope),
    /// Exit the process with this code.
    Exit(i32),
    /// Do nothing.
    Nothing,
}

/// The opening `Hello` the sidecar sends, declaring its identity and requested
/// capabilities.
pub fn hello() -> Envelope {
    Envelope::new(
        0,
        Message::Hello(Hello {
            sidecar_id: SIDECAR_ID.into(),
            sidecar_version: env!("CARGO_PKG_VERSION").into(),
            contract_version: CONTRACT_VERSION,
            capabilities_requested: REQUESTED_CAPABILITIES.to_vec(),
            auth_token: std::env::var(sidecar_contract::AUTH_TOKEN_ENV).ok(),
        }),
    )
}

/// The base protocol reaction to one inbound message: reach `Ready` after `Welcome`,
/// exit on `sidecar.shutdown`, and answer other requests. Later tickets extend the
/// request handling (agent methods) on top of this.
pub fn on_message(env: Envelope) -> Reaction {
    match env.message {
        Message::Welcome(_) => Reaction::Send(Envelope::new(0, Message::Lifecycle(Lifecycle::Ready))),
        Message::Rejected(_) => Reaction::Exit(1),
        Message::Signal(sidecar_contract::HostSignal::WillQuit) => Reaction::Exit(0),
        Message::Request(req) if req.method == "sidecar.shutdown" => Reaction::Exit(0),
        Message::Request(_) => Reaction::Send(Envelope::new(
            env.id,
            Message::Response(Response { result: Ok(serde_json::json!({ "ok": true })) }),
        )),
        _ => Reaction::Nothing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sidecar_contract::{HostSignal, Request, Welcome};

    #[test]
    fn requests_the_four_capabilities() {
        assert!(REQUESTED_CAPABILITIES.contains(&Capability::Context));
        assert!(REQUESTED_CAPABILITIES.contains(&Capability::Secrets));
        assert!(REQUESTED_CAPABILITIES.contains(&Capability::Storage));
        assert!(REQUESTED_CAPABILITIES.contains(&Capability::Events));
    }

    #[test]
    fn hello_identifies_the_sidecar() {
        match hello().message {
            Message::Hello(h) => {
                assert_eq!(h.sidecar_id, "ai-console");
                assert_eq!(h.contract_version, CONTRACT_VERSION);
                assert_eq!(h.capabilities_requested.len(), 4);
            }
            _ => panic!("hello() must be a Hello"),
        }
    }

    #[test]
    fn welcome_yields_ready() {
        let r = on_message(Envelope::new(
            0,
            Message::Welcome(Welcome {
                negotiated_version: CONTRACT_VERSION,
                capabilities_granted: vec![],
            }),
        ));
        assert!(matches!(
            r,
            Reaction::Send(e) if matches!(e.message, Message::Lifecycle(Lifecycle::Ready))
        ));
    }

    #[test]
    fn shutdown_request_exits_zero() {
        let r = on_message(Envelope::new(
            5,
            Message::Request(Request { method: "sidecar.shutdown".into(), params: serde_json::Value::Null }),
        ));
        assert_eq!(r, Reaction::Exit(0));
    }

    #[test]
    fn will_quit_signal_exits_zero() {
        let r = on_message(Envelope::new(0, Message::Signal(HostSignal::WillQuit)));
        assert_eq!(r, Reaction::Exit(0));
    }

    #[test]
    fn other_requests_are_answered_correlated() {
        let r = on_message(Envelope::new(
            7,
            Message::Request(Request { method: "agents.list".into(), params: serde_json::Value::Null }),
        ));
        match r {
            Reaction::Send(e) => {
                assert_eq!(e.id, 7);
                assert!(matches!(e.message, Message::Response(_)));
            }
            _ => panic!("a request should be answered"),
        }
    }
}
