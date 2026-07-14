//! # sidecar-host
//!
//! Host-side platform logic for the Cross-Platform Explorer sidecar platform
//! (ADR 0001 / CPE-260). This crate holds the pieces the *host* needs but that are
//! pure logic — testable without the explorer or a live process. It depends only on
//! [`sidecar_contract`]; the explorer wires it in later behind a feature flag
//! (CPE-272), so the delete-test holds.
//!
//! This module implements the **sidecar manifest schema + registry** (CPE-264):
//! discovery of sidecars by declarative manifest, from a bundled directory and a
//! user-writable directory, with malformed/incompatible manifests skipped (not
//! fatal) — mirroring the explorer's skip-on-error listing rule.

pub mod broker;
pub mod catalog;
pub mod conformance;
pub mod consent;
pub mod enablement;
pub mod errors;
pub mod migrate;
pub mod observability;
pub mod providers;
pub mod registry;
pub mod resources;
pub mod scaffold;
pub mod supervisor;
pub mod trust;

pub use broker::{
    capability_for_method, decide_grants, Broker, CapabilityProvider, GrantRequest,
};
pub use migrate::{Migrations, MigrationStep};
pub use registry::{EntryPoint, LoadWarning, Registry, SidecarManifest, UiMount};
