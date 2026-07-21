//! # cpe-contract
//!
//! The transport-neutral, versioned wire contract between the Cross-Platform Explorer
//! **GUI/Client** and its **Server** (epic CPE-810). One codebase serves two topologies
//! over the *same* contract — only the transport under it changes:
//!
//! ```text
//! Remote:  GUI ──(network)──► Client(Rust) ──(RPC)──► Server(Rust)
//! Local:   GUI ──(in-process IPC)────────────────────► Server(Rust)
//! ```
//!
//! This crate defines *what travels on the wire* — the framed [`Envelope`], the
//! [`Message`] union, the [`Hello`]/[`Welcome`] handshake, version [`negotiate`]ation,
//! and a structured error taxonomy. It is transport-agnostic and Tauri-free: the
//! concrete transport (in-process channel vs. a network socket) is chosen by the
//! Client/Server runtime, and simple line framing is provided by [`codec`].
//!
//! It reuses the proven `sidecar/contract` versioning pattern ([`ContractVersion`] +
//! [`negotiate`] + Hello/Welcome + a frame [`ENVELOPE_SCHEMA_VERSION`]), generalized
//! from the host↔sidecar boundary to a GUI↔Server boundary that can also cross a
//! network.
//!
//! **Multi-client from day one, single-user first (CPE-810 decision).** Every
//! [`Envelope`] carries a [`Session`]/[`Principal`] slot so the future multi-client
//! model is not precluded. In local single-user mode it defaults to
//! [`Session::local`] and is omitted from the serialized frame entirely, keeping the
//! local path fast and small (the hard tiebreaker).
//!
//! Covered ticket: CPE-811 (transport-neutral contract envelope).

use serde::{Deserialize, Serialize};

/// The contract version this build implements. Bump `minor` for additive changes a
/// peer may safely ignore, `major` for breaking ones (mirrors the sidecar precedent).
pub const CONTRACT_VERSION: ContractVersion = ContractVersion::new(1, 0);

/// Schema version of the [`Envelope`] frame format itself. Distinct from the contract
/// version so the outer frame can evolve independently of message payloads.
pub const ENVELOPE_SCHEMA_VERSION: u16 = 1;

// ---------------------------------------------------------------------------
// Versioning & negotiation
// ---------------------------------------------------------------------------

/// Semantic version of the wire contract. Same `major` = compatible; a higher `minor`
/// only adds messages a peer may ignore.
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

impl std::fmt::Display for ContractVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// Why a handshake could not agree on a version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersionError {
    /// Different majors can never interoperate.
    MajorMismatch {
        client: ContractVersion,
        server: ContractVersion,
    },
}

impl std::fmt::Display for VersionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionError::MajorMismatch { client, server } => write!(
                f,
                "incompatible contract major: client {client}, server {server}"
            ),
        }
    }
}

impl std::error::Error for VersionError {}

/// Negotiate the version both peers will speak. Requires equal majors; the agreed minor
/// is the lower of the two, since only features both understand may be used. Order of
/// arguments does not matter to the result, only to the error's labelling.
pub fn negotiate(
    client: ContractVersion,
    server: ContractVersion,
) -> Result<ContractVersion, VersionError> {
    if client.major != server.major {
        return Err(VersionError::MajorMismatch { client, server });
    }
    Ok(ContractVersion::new(
        client.major,
        client.minor.min(server.minor),
    ))
}

// ---------------------------------------------------------------------------
// Principal / session (CPE-810: reserve the multi-client slot; default local)
// ---------------------------------------------------------------------------

/// Who a request is made *as*. In local single-user mode this is the implicit
/// [`Principal::local`] identity; remote mode fills it from the authenticated client
/// (the security layer lands in CPE-816+, this only reserves the shape).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principal {
    /// Stable identifier. `"local"` for the in-process single user.
    pub id: String,
    /// Optional human-facing label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

impl Principal {
    /// The implicit, fully-trusted principal of the in-process local Server.
    pub fn local() -> Self {
        Self {
            id: "local".to_string(),
            display_name: None,
        }
    }

    /// True for the default local principal — used to keep local frames minimal.
    pub fn is_local(&self) -> bool {
        self.id == "local" && self.display_name.is_none()
    }
}

impl Default for Principal {
    fn default() -> Self {
        Self::local()
    }
}

/// The session an [`Envelope`] belongs to. Reserved so the future multi-client model is
/// not precluded; single-user local mode leaves it defaulted ([`Session::local`]) and
/// the frame omits it entirely.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Session {
    /// Server-assigned session id. `None` for the implicit local session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Who this session acts as.
    #[serde(default, skip_serializing_if = "Principal::is_local")]
    pub principal: Principal,
}

impl Session {
    /// The implicit local single-user session: no id, local principal.
    pub fn local() -> Self {
        Self {
            id: None,
            principal: Principal::local(),
        }
    }

    /// True when this is the default local session — then it is omitted from the frame,
    /// keeping the local in-process path fast and small (the hard tiebreaker).
    pub fn is_local(&self) -> bool {
        self.id.is_none() && self.principal.is_local()
    }
}

// ---------------------------------------------------------------------------
// Error taxonomy
// ---------------------------------------------------------------------------

/// Stable error categories carried across the boundary so failures propagate
/// predictably instead of as opaque strings. `Unauthenticated`/`Unauthorized` are
/// reserved for the security layer (CPE-816+).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Handshake,
    VersionIncompatible,
    Unauthenticated,
    Unauthorized,
    BadRequest,
    NotFound,
    Transport,
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
// Handshake
// ---------------------------------------------------------------------------

/// client → server: the opening message declaring identity and contract version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub client_id: String,
    pub client_version: String,
    pub contract_version: ContractVersion,
    /// Principal the client proposes to act as. `None` = accept the server's implicit
    /// principal (local single-user). Reserved for the security layer; not yet
    /// authenticated here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal: Option<Principal>,
}

/// server → client: handshake accepted, with the negotiated version and the established
/// session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Welcome {
    pub server_id: String,
    pub server_version: String,
    pub negotiated_version: ContractVersion,
    /// The session the server established for this client (local default in single-user
    /// mode).
    #[serde(default)]
    pub session: Session,
}

/// Why the server refused the connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectCode {
    IncompatibleVersion,
    Unauthenticated,
    PolicyDenied,
}

/// server → client: handshake refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rejected {
    pub code: RejectCode,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Requests / responses / streaming
// ---------------------------------------------------------------------------

/// A method call on the Server, e.g. `list_dir`. Params/results are free-form JSON so
/// the command surface (the ~113 explorer commands) evolves without changing this
/// crate; the typed bindings on top are generated separately (CPE-812).
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

// ---------------------------------------------------------------------------
// The message union and framed envelope
// ---------------------------------------------------------------------------

/// Every logical message on the boundary. Tagged so it (de)serializes to
/// self-describing JSON.
///
/// `StreamItem`/`StreamEnd` reserve the streaming seam: the three `ipc::Channel`
/// producers (directory listings, recursive search, bulk thumbnails) map onto a series
/// of `StreamItem`s correlated by the [`Envelope`] `id`, terminated by `StreamEnd`
/// (wired up over the wire in CPE-819).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    Hello(Hello),
    Welcome(Welcome),
    Rejected(Rejected),
    Request(Request),
    Response(Response),
    /// server → client: one streamed item, correlated by the envelope `id`.
    StreamItem(serde_json::Value),
    /// server → client: the stream for this envelope `id` has completed.
    StreamEnd,
    Error(ContractError),
}

/// Correlates a [`Request`] with its [`Response`] (and its stream items). Unsolicited
/// messages use `0`.
pub type CorrelationId = u64;

/// The single frame type on the wire. Carries its own schema version so the frame
/// format can evolve independently of message payloads, and a [`Session`] slot so the
/// multi-client model is reachable without a frame break. In local single-user mode the
/// session is default and omitted, so the frame stays minimal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub schema_version: u16,
    pub id: CorrelationId,
    #[serde(default, skip_serializing_if = "Session::is_local")]
    pub session: Session,
    pub message: Message,
}

impl Envelope {
    /// Build an envelope stamped with the current frame schema version and the implicit
    /// local session (single-user mode).
    pub fn new(id: CorrelationId, message: Message) -> Self {
        Self {
            schema_version: ENVELOPE_SCHEMA_VERSION,
            id,
            session: Session::local(),
            message,
        }
    }

    /// Attach a non-local [`Session`] (remote / multi-client mode).
    pub fn with_session(mut self, session: Session) -> Self {
        self.session = session;
        self
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

/// Newline framing over a JSON-lines stream. The concrete transport (in-process channel
/// vs. socket) is selected by the Client/Server runtime, but every transport agrees on
/// this envelope shape.
pub mod codec {
    use super::Envelope;

    /// Encode an envelope as a single newline-terminated JSON line.
    pub fn encode_line(env: &Envelope) -> Result<String, serde_json::Error> {
        Ok(format!("{}\n", env.to_json()?))
    }

    /// Decode one JSON line (with or without a trailing newline) into an envelope.
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
        // Symmetric in value, whichever side is higher.
        let agreed = negotiate(ContractVersion::new(1, 2), ContractVersion::new(1, 5)).unwrap();
        assert_eq!(agreed, ContractVersion::new(1, 2));
    }

    #[test]
    fn negotiate_rejects_major_mismatch() {
        let err = negotiate(ContractVersion::new(2, 0), ContractVersion::new(1, 9)).unwrap_err();
        assert_eq!(
            err,
            VersionError::MajorMismatch {
                client: ContractVersion::new(2, 0),
                server: ContractVersion::new(1, 9),
            }
        );
        // The Display impl names both sides for a legible handshake failure.
        assert!(err.to_string().contains("2.0"));
        assert!(err.to_string().contains("1.9"));
    }

    #[test]
    fn local_envelope_omits_the_session_slot() {
        // The single-user local frame must not carry session bytes (fast/small path).
        let env = Envelope::new(1, Message::StreamEnd);
        let json = env.to_json().unwrap();
        assert!(!json.contains("session"), "local frame should omit session: {json}");
        // …but it still round-trips back to the default local session.
        let back = Envelope::from_json(&json).unwrap();
        assert!(back.session.is_local());
        assert_eq!(env, back);
    }

    #[test]
    fn remote_session_round_trips() {
        let session = Session {
            id: Some("sess-42".into()),
            principal: Principal {
                id: "alice".into(),
                display_name: Some("Alice".into()),
            },
        };
        let env = Envelope::new(
            7,
            Message::Request(Request {
                method: "list_dir".into(),
                params: serde_json::json!({ "path": "/home/alice" }),
            }),
        )
        .with_session(session.clone());
        let json = env.to_json().unwrap();
        assert!(json.contains("sess-42"));
        let back = Envelope::from_json(&json).unwrap();
        assert_eq!(back.session, session);
        assert_eq!(env, back);
    }

    #[test]
    fn hello_welcome_handshake_round_trips() {
        let hello = Envelope::new(
            1,
            Message::Hello(Hello {
                client_id: "explorer-gui".into(),
                client_version: "0.56.1".into(),
                contract_version: CONTRACT_VERSION,
                principal: None,
            }),
        );
        let welcome = Envelope::new(
            1,
            Message::Welcome(Welcome {
                server_id: "explorer-server".into(),
                server_version: "0.56.1".into(),
                negotiated_version: CONTRACT_VERSION,
                session: Session::local(),
            }),
        );
        for env in [hello, welcome] {
            let line = codec::encode_line(&env).unwrap();
            let back = codec::decode_line(&line).unwrap();
            assert_eq!(env, back);
            assert_eq!(back.schema_version, ENVELOPE_SCHEMA_VERSION);
        }
    }

    #[test]
    fn error_codes_serialize_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&ErrorCode::Unauthenticated).unwrap(),
            "\"unauthenticated\""
        );
        assert_eq!(
            serde_json::to_string(&RejectCode::IncompatibleVersion).unwrap(),
            "\"incompatible_version\""
        );
    }

    #[test]
    fn response_carries_ok_or_error() {
        let ok = Response {
            result: Ok(serde_json::json!({ "entries": [] })),
        };
        let err = Response {
            result: Err(ContractError::new(ErrorCode::NotFound, "no such path", false)),
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
            Message::Rejected(Rejected {
                code: RejectCode::IncompatibleVersion,
                reason: "major 2 unsupported".into(),
            }),
            Message::Request(Request {
                method: "read_file".into(),
                params: serde_json::json!({ "path": "a.txt" }),
            }),
            Message::StreamItem(serde_json::json!({ "name": "a.txt" })),
            Message::StreamEnd,
            Message::Error(ContractError::new(ErrorCode::Transport, "pipe closed", true)),
        ];
        for m in msgs {
            let env = Envelope::new(42, m);
            let back = Envelope::from_json(&env.to_json().unwrap()).unwrap();
            assert_eq!(env, back);
        }
    }
}
