//! # sidecar-contract
//!
//! The single shared surface between the Cross-Platform Explorer **host** and any
//! **sidecar** process (ADR 0001 / CPE-259). Both sides depend on this crate and on
//! nothing of each other's — that one-way boundary is what keeps Mega-Features from
//! entangling the explorer.
//!
//! This crate defines *what travels on the wire* — the framed [`Envelope`], the
//! [`Message`] types, the handshake, capability model, lifecycle, and a structured
//! error taxonomy. It is transport-agnostic: the concrete transport (stdio frames vs
//! a local socket) is chosen by the host/sidecar runtime (CPE-294), and framing is
//! provided by [`codec`].
//!
//! Covered tickets: CPE-262 (protocol & envelope), CPE-263 (version negotiation).
//! Seeds: CPE-299 (error taxonomy), CPE-300 (schema versioning).

use serde::{Deserialize, Serialize};

/// The contract version this build of the crate implements. Bump `minor` for
/// additive changes, `major` for breaking ones (CPE-263).
pub const CONTRACT_VERSION: ContractVersion = ContractVersion::new(1, 0);

/// Schema version of the [`Envelope`] frame format itself (CPE-300). Distinct from
/// the contract version so the outer frame can evolve independently of message
/// payloads.
pub const ENVELOPE_SCHEMA_VERSION: u16 = 1;

// ---------------------------------------------------------------------------
// Versioning & negotiation (CPE-263)
// ---------------------------------------------------------------------------

/// Semantic version of the wire contract. Same `major` = compatible; a higher
/// `minor` only adds messages a peer may ignore.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractVersion {
    pub major: u16,
    pub minor: u16,
}

impl ContractVersion {
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    /// True if a peer advertising `self` can serve a client that *requires* at least
    /// `required`: same major, and at least the required minor.
    pub fn is_compatible_with(self, required: ContractVersion) -> bool {
        self.major == required.major && self.minor >= required.minor
    }
}

/// Why a handshake could not agree on a version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersionError {
    /// Different majors can never interoperate.
    MajorMismatch {
        host: ContractVersion,
        sidecar: ContractVersion,
    },
}

/// Negotiate the version both peers will speak. Requires equal majors; the agreed
/// minor is the lower of the two, since only features both understand may be used.
pub fn negotiate(
    host: ContractVersion,
    sidecar: ContractVersion,
) -> Result<ContractVersion, VersionError> {
    if host.major != sidecar.major {
        return Err(VersionError::MajorMismatch { host, sidecar });
    }
    Ok(ContractVersion::new(host.major, host.minor.min(sidecar.minor)))
}

// ---------------------------------------------------------------------------
// Capabilities (CPE-266) — a sidecar requests, the host grants scoped ones.
// ---------------------------------------------------------------------------

/// A brokered permission a sidecar may request. No capability = no access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Read the explorer's current folder/repo/selection (CPE-267).
    Context,
    /// Brokered access to the OS secret store, own namespace (CPE-268).
    Secrets,
    /// A private, host-assigned storage directory (CPE-269).
    Storage,
    /// Emit notifications / receive host lifecycle signals (CPE-270).
    Events,
    /// Make outbound network requests (installs, provider APIs).
    Network,
}

// ---------------------------------------------------------------------------
// Lifecycle (CPE-265)
// ---------------------------------------------------------------------------

/// The supervisor's view of a sidecar's lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    Starting,
    Ready,
    Draining,
    Stopped,
    Failed,
}

// ---------------------------------------------------------------------------
// Error taxonomy (CPE-299 seed)
// ---------------------------------------------------------------------------

/// Stable error categories carried across the boundary so failures propagate
/// predictably instead of as opaque strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Handshake,
    VersionIncompatible,
    CapabilityDenied,
    Transport,
    SidecarCrash,
    ToolFailure,
    Network,
    UserCancelled,
    Internal,
}

/// A structured error with a stable code, a human message, and whether retrying may
/// help. Never carries secret values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
}

impl ContractError {
    pub fn new(code: ErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
        }
    }
}

// ---------------------------------------------------------------------------
// Handshake (CPE-262)
// ---------------------------------------------------------------------------

/// sidecar → host: the opening message declaring identity, contract version, and the
/// capabilities it wants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub sidecar_id: String,
    pub sidecar_version: String,
    pub contract_version: ContractVersion,
    pub capabilities_requested: Vec<Capability>,
}

/// host → sidecar: handshake accepted, with the negotiated version and the subset of
/// capabilities actually granted (after consent, CPE-296).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Welcome {
    pub negotiated_version: ContractVersion,
    pub capabilities_granted: Vec<Capability>,
}

/// Why the host refused to mount a sidecar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectCode {
    IncompatibleVersion,
    Untrusted,
    PolicyDenied,
}

/// host → sidecar: handshake refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rejected {
    pub code: RejectCode,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Requests / responses / events
// ---------------------------------------------------------------------------

/// A method call over a granted capability, e.g. `secrets.get`. Params/results are
/// free-form JSON so capability providers evolve without changing this crate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub method: String,
    pub params: serde_json::Value,
}

/// The reply to a [`Request`]: either a JSON result or a structured error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub result: Result<serde_json::Value, ContractError>,
}

/// Severity for a [`Event::Notify`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Info,
    Warn,
    Error,
}

/// A fire-and-forget message a sidecar emits to the host (CPE-270).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Notify { level: Level, message: String },
    Progress { id: String, fraction: f32 },
    Status { state: String },
}

// ---------------------------------------------------------------------------
// The message union and framed envelope
// ---------------------------------------------------------------------------

/// Every logical message on the boundary. Tagged so it (de)serializes to
/// self-describing JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    Hello(Hello),
    Welcome(Welcome),
    Rejected(Rejected),
    Request(Request),
    Response(Response),
    Event(Event),
    Lifecycle(Lifecycle),
    Error(ContractError),
}

/// Correlates a [`Request`] with its [`Response`]. Events/lifecycle use `0`.
pub type CorrelationId = u64;

/// The single frame type on the wire. Carries its own schema version (CPE-300) so
/// the frame format can evolve independently of message payloads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub schema_version: u16,
    pub id: CorrelationId,
    pub message: Message,
}

impl Envelope {
    /// Build an envelope stamped with the current frame schema version.
    pub fn new(id: CorrelationId, message: Message) -> Self {
        Self {
            schema_version: ENVELOPE_SCHEMA_VERSION,
            id,
            message,
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

/// Length-prefixed framing for streaming transports (stdio pipe / socket). Newline
/// framing over a JSON-lines stream; the concrete transport is selected by the
/// runtime (CPE-294) but every transport agrees on this envelope shape.
pub mod codec {
    use super::Envelope;

    /// Encode an envelope as a single newline-terminated JSON line.
    pub fn encode_line(env: &Envelope) -> Result<String, serde_json::Error> {
        Ok(format!("{}\n", env.to_json()?))
    }

    /// Decode one JSON line (without the trailing newline) into an envelope.
    pub fn decode_line(line: &str) -> Result<Envelope, serde_json::Error> {
        Envelope::from_json(line.trim_end_matches(['\n', '\r']))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_major_higher_minor_is_compatible() {
        assert!(ContractVersion::new(1, 3).is_compatible_with(ContractVersion::new(1, 1)));
        assert!(!ContractVersion::new(1, 0).is_compatible_with(ContractVersion::new(1, 2)));
        assert!(!ContractVersion::new(2, 0).is_compatible_with(ContractVersion::new(1, 0)));
    }

    #[test]
    fn negotiate_picks_the_lower_minor() {
        let agreed = negotiate(ContractVersion::new(1, 5), ContractVersion::new(1, 2)).unwrap();
        assert_eq!(agreed, ContractVersion::new(1, 2));
    }

    #[test]
    fn negotiate_rejects_major_mismatch() {
        let err = negotiate(ContractVersion::new(2, 0), ContractVersion::new(1, 9)).unwrap_err();
        assert_eq!(
            err,
            VersionError::MajorMismatch {
                host: ContractVersion::new(2, 0),
                sidecar: ContractVersion::new(1, 9),
            }
        );
    }

    #[test]
    fn hello_round_trips_through_json() {
        let env = Envelope::new(
            7,
            Message::Hello(Hello {
                sidecar_id: "ai-console".into(),
                sidecar_version: "0.1.0".into(),
                contract_version: CONTRACT_VERSION,
                capabilities_requested: vec![Capability::Context, Capability::Secrets],
            }),
        );
        let line = codec::encode_line(&env).unwrap();
        let back = codec::decode_line(&line).unwrap();
        assert_eq!(env, back);
        assert_eq!(back.schema_version, ENVELOPE_SCHEMA_VERSION);
    }

    #[test]
    fn capabilities_serialize_as_snake_case() {
        let json = serde_json::to_string(&Capability::Secrets).unwrap();
        assert_eq!(json, "\"secrets\"");
    }

    #[test]
    fn response_carries_ok_or_error() {
        let ok = Response {
            result: Ok(serde_json::json!({ "value": 1 })),
        };
        let err = Response {
            result: Err(ContractError::new(ErrorCode::CapabilityDenied, "not granted", false)),
        };
        for r in [ok, err] {
            let env = Envelope::new(1, Message::Response(r));
            let back = Envelope::from_json(&env.to_json().unwrap()).unwrap();
            assert_eq!(env, back);
        }
    }

    #[test]
    fn every_message_variant_round_trips() {
        let msgs = vec![
            Message::Welcome(Welcome {
                negotiated_version: CONTRACT_VERSION,
                capabilities_granted: vec![Capability::Storage],
            }),
            Message::Rejected(Rejected {
                code: RejectCode::IncompatibleVersion,
                reason: "major 2 unsupported".into(),
            }),
            Message::Request(Request {
                method: "secrets.get".into(),
                params: serde_json::json!({ "name": "openrouter" }),
            }),
            Message::Event(Event::Progress {
                id: "install".into(),
                fraction: 0.5,
            }),
            Message::Lifecycle(Lifecycle::Ready),
            Message::Error(ContractError::new(ErrorCode::Transport, "pipe closed", true)),
        ];
        for m in msgs {
            let env = Envelope::new(42, m);
            let back = Envelope::from_json(&env.to_json().unwrap()).unwrap();
            assert_eq!(env, back);
        }
    }
}
