//! # repos
//!
//! The **Repositories** sidecar (CPE-429): connect to and interact with any source-code repository
//! on the internet — browse, clone, and **two-way mirror** — the same way for every forge/VCS. Built
//! as a **sidecar tenant** of the platform (ADR 0001 / CPE-259/260), peer to the AI Console
//! (CPE-261). This crate holds the provider-agnostic **domain logic** plus the base contract
//! handshake ([`protocol`], CPE-432); the process entry lives in `main.rs`.
//!
//! The heart of "any forge" is that a provider (GitHub, GitLab, Bitbucket, …) is **data**: a
//! declarative [`providers::ProviderManifest`] describing how to reach and drive it. Adding a forge
//! is a manifest, not host code — mirroring the AI Console's agent registry (CPE-278).

pub mod browse;
pub mod clone;
pub mod conflict;
pub mod generic;
pub mod protocol;
pub mod providers;
pub mod status;
pub mod sync;
pub mod ui;

pub use browse::{parse_github_contents, RemoteEntry};
pub use clone::{build_clone_args, CloneError, CloneRequest};
pub use conflict::{parse_conflicts, Conflict, ConflictKind};
pub use generic::{normalize_host, parse_remote, GitRemote, RemoteScheme};
pub use protocol::{hello, on_message, Reaction, REQUESTED_CAPABILITIES, SIDECAR_ID};
pub use providers::{ProviderManifest, ProviderRegistry};
pub use status::{parse_status, RepoState};
pub use sync::{plan_sync, DivergePolicy, SyncAction, SyncPlan, SyncPolicy};
