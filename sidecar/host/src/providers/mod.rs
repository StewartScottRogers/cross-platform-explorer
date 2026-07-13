//! Capability providers — the host-side implementations behind each [`crate::broker::Capability`]
//! grant. Each is a [`crate::broker::CapabilityProvider`] the broker dispatches to
//! once a sidecar has been granted the matching capability.
//!
//! - [`context`] — read-only explorer context (CPE-267).
//! - [`storage`] — a private per-sidecar storage directory (CPE-269).
//! - [`events`] — sidecar→host notifications and host→sidecar signals (CPE-270).

pub mod context;
pub mod events;
pub mod storage;

pub use context::{ContextProvider, ContextSnapshot, ContextSource};
pub use events::{signal_envelope, EventRouter, EventSink};
pub use storage::StorageProvider;
