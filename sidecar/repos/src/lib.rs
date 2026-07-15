//! # repos
//!
//! The **Repositories** sidecar (CPE-429): connect to and interact with any source-code repository
//! on the internet — browse, clone, and **two-way mirror** — the same way for every forge/VCS. Built
//! as a **sidecar tenant** of the platform (ADR 0001 / CPE-259/260), peer to the AI Console
//! (CPE-261). This crate holds the provider-agnostic **domain logic**; the process entry + contract
//! wiring arrive in CPE-432.
//!
//! The heart of "any forge" is that a provider (GitHub, GitLab, Bitbucket, …) is **data**: a
//! declarative [`providers::ProviderManifest`] describing how to reach and drive it. Adding a forge
//! is a manifest, not host code — mirroring the AI Console's agent registry (CPE-278).

pub mod browse;
pub mod providers;
pub mod status;
pub mod sync;

pub use browse::{parse_github_contents, RemoteEntry};
pub use providers::{ProviderManifest, ProviderRegistry};
pub use status::{parse_status, RepoState};
pub use sync::{plan_sync, DivergePolicy, SyncAction, SyncPlan, SyncPolicy};
