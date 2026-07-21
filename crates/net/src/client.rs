//! The Client(Rust) proxy (CPE-825, epic CPE-810): implements the contract over a socket so a
//! GUI (or any Rust caller) can drive the Server *remotely* with the same request/response
//! shape it uses locally.
//!
//! Lifecycle: [`connect`](Client::connect) opens the socket and completes the CPE-811
//! Hello/Welcome handshake (returning a typed [`ConnectError`] if the server rejects the
//! version); [`call`](Client::call) issues one [`Request`] and awaits its [`Response`],
//! correlated by the envelope id. Single connection, one in-flight call at a time — enough to
//! prove `GUI → Client(Rust) → Server(Rust)`; pipelining/streaming ride the reserved
//! `StreamItem`/`StreamEnd` seam later (CPE-819/820).

use std::io::{BufReader, BufWriter};
use std::net::{TcpStream, ToSocketAddrs};

use cpe_contract::{
    ContractError, ContractVersion, Envelope, ErrorCode, Hello, Message, Principal, Rejected,
    Request, Session, CONTRACT_VERSION,
};

use crate::wire::{read_envelope, write_envelope};

/// Why a [`Client::connect`] could not establish a session.
#[derive(Debug)]
pub enum ConnectError {
    /// The socket itself failed (refused, reset, …).
    Io(std::io::Error),
    /// The server refused the handshake (e.g. incompatible contract version).
    Rejected(Rejected),
    /// The server spoke something other than a Welcome/Rejected at handshake.
    Protocol(String),
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectError::Io(e) => write!(f, "transport error: {e}"),
            ConnectError::Rejected(r) => write!(f, "server rejected connection ({:?}): {}", r.code, r.reason),
            ConnectError::Protocol(m) => write!(f, "protocol error: {m}"),
        }
    }
}

impl std::error::Error for ConnectError {}

/// A connected proxy to a Server. Holds the socket (split into a buffered reader/writer) plus
/// the negotiated version and session established at handshake.
pub struct Client {
    reader: BufReader<TcpStream>,
    writer: BufWriter<TcpStream>,
    next_id: u64,
    negotiated: ContractVersion,
    session: Session,
}

impl Client {
    /// Connect and handshake with this build's [`CONTRACT_VERSION`] as the implicit local
    /// principal.
    pub fn connect(addr: impl ToSocketAddrs) -> Result<Self, ConnectError> {
        Self::connect_as(addr, CONTRACT_VERSION, None)
    }

    /// Connect advertising a specific `client_version` and proposed `principal` — used to
    /// exercise version negotiation and (later) authentication.
    pub fn connect_as(
        addr: impl ToSocketAddrs,
        client_version: ContractVersion,
        principal: Option<Principal>,
    ) -> Result<Self, ConnectError> {
        let stream = TcpStream::connect(addr).map_err(ConnectError::Io)?;
        let mut reader = BufReader::new(stream.try_clone().map_err(ConnectError::Io)?);
        let mut writer = BufWriter::new(stream);

        let hello = Envelope::new(
            1,
            Message::Hello(Hello {
                client_id: "cpe-net-client".to_string(),
                client_version: env!("CARGO_PKG_VERSION").to_string(),
                contract_version: client_version,
                principal,
            }),
        );
        write_envelope(&mut writer, &hello).map_err(ConnectError::Io)?;

        match read_envelope(&mut reader).map_err(ConnectError::Io)? {
            Some(env) => match env.message {
                Message::Welcome(w) => Ok(Self {
                    reader,
                    writer,
                    next_id: 2,
                    negotiated: w.negotiated_version,
                    session: w.session,
                }),
                Message::Rejected(r) => Err(ConnectError::Rejected(r)),
                other => Err(ConnectError::Protocol(format!(
                    "expected Welcome or Rejected, got {other:?}"
                ))),
            },
            None => Err(ConnectError::Protocol(
                "server closed the connection during handshake".to_string(),
            )),
        }
    }

    /// The contract version both peers agreed to speak.
    pub fn negotiated_version(&self) -> ContractVersion {
        self.negotiated
    }

    /// The session the server established (local default in single-user mode).
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Call one method and await its result. A transport failure becomes a retryable
    /// [`ErrorCode::Transport`] error; a server-side failure is returned as its own
    /// structured [`ContractError`].
    pub fn call(
        &mut self,
        method: impl Into<String>,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, ContractError> {
        let id = self.next_id;
        self.next_id += 1;

        let req = Envelope::new(
            id,
            Message::Request(Request {
                method: method.into(),
                params,
            }),
        );
        write_envelope(&mut self.writer, &req).map_err(transport_err)?;

        match read_envelope(&mut self.reader).map_err(transport_err)? {
            Some(env) => match env.message {
                Message::Response(resp) => resp.result,
                other => Err(ContractError::new(
                    ErrorCode::Internal,
                    format!("expected a Response, got {other:?}"),
                    false,
                )),
            },
            None => Err(ContractError::new(
                ErrorCode::Transport,
                "server closed the connection before responding",
                true,
            )),
        }
    }
}

fn transport_err(e: std::io::Error) -> ContractError {
    ContractError::new(ErrorCode::Transport, e.to_string(), true)
}
