//! The Server runtime (CPE-825, epic CPE-810): accept a connection, run the CPE-811
//! Hello/Welcome handshake, then drive each [`Request`] through the CPE-824 [`Dispatcher`]
//! **behind the CPE-816 [`SecurityChain`]**.
//!
//! Security is enforced at this boundary, not inside the ~113 domain commands: every request
//! is evaluated Transport → AuthN → AuthZ before it can reach the dispatcher, and a denial
//! becomes a structured [`ContractError`] — the Server never dispatches a request the chain
//! rejected. The domain logic in `cpe-server` stays security-agnostic and receives an
//! already-authorized call.
//!
//! std-only, thread-per-connection: no async runtime. v1 is single-user (the handshake
//! establishes the implicit local [`Session`]); the per-request security evaluation is where
//! the multi-client principal will flow once AuthN providers land (CPE-817).

use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

use cpe_contract::{
    handshake, ContractError, Envelope, ErrorCode, Message, Principal, Rejected, RejectCode, Request,
    Response, Session, CONTRACT_VERSION,
};
use cpe_security::{Decision, Plane, SecurityChain, SecurityContext};
use cpe_server::ctx::ServerCtx;
use cpe_server::dispatch::Dispatcher;

use crate::wire::{read_envelope, write_envelope};
use crate::ws;

/// Reads/writes CPE-811 [`Envelope`]s over some framing, so the session loop is transport-agnostic
/// (CPE-819): raw length-prefixed TCP for a `Client(Rust)`, or WebSocket text frames for the browser.
trait EnvelopeIo {
    fn read_env(&mut self) -> io::Result<Option<Envelope>>;
    fn write_env(&mut self, env: &Envelope) -> io::Result<()>;
}

/// Raw length-prefixed wire (the historical `Client(Rust)` transport).
struct WireIo<R, W> {
    reader: R,
    writer: W,
}
impl<R: BufRead, W: Write> EnvelopeIo for WireIo<R, W> {
    fn read_env(&mut self) -> io::Result<Option<Envelope>> {
        read_envelope(&mut self.reader)
    }
    fn write_env(&mut self, env: &Envelope) -> io::Result<()> {
        write_envelope(&mut self.writer, env)
    }
}

/// WebSocket transport: the envelope JSON rides as a text frame's payload (CPE-819). Ping/pong and other
/// control frames are skipped; a close frame (or EOF) ends the session.
struct WsIo<R, W> {
    reader: R,
    writer: W,
}
impl<R: Read, W: Write> EnvelopeIo for WsIo<R, W> {
    fn read_env(&mut self) -> io::Result<Option<Envelope>> {
        loop {
            match ws::read_frame(&mut self.reader)? {
                None => return Ok(None),
                Some(f) if f.opcode == ws::op::CLOSE => return Ok(None),
                Some(f) if f.opcode == ws::op::TEXT || f.opcode == ws::op::BINARY => {
                    let s = String::from_utf8_lossy(&f.payload);
                    return Envelope::from_json(&s)
                        .map(Some)
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e));
                }
                // RFC 6455 §5.5.2: a Ping MUST be answered with a Pong echoing its payload — otherwise a
                // browser/proxy keepalive goes unanswered and the connection is dropped mid-session.
                Some(f) if f.opcode == ws::op::PING => {
                    ws::write_frame(&mut self.writer, ws::op::PONG, &f.payload)?;
                    continue;
                }
                Some(_) => continue, // pong/continuation — nothing to do, read the next frame
            }
        }
    }
    fn write_env(&mut self, env: &Envelope) -> io::Result<()> {
        let json = env.to_json().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        ws::write_text(&mut self.writer, &json)
    }
}

/// Read the WebSocket upgrade request's headers and return the `Sec-WebSocket-Key` (empty if absent).
/// The request line was already peeked; this consumes through the blank line that ends the headers.
fn read_ws_key(reader: &mut impl BufRead) -> io::Result<String> {
    let mut key = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break; // blank line ends the headers
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            if name.trim().eq_ignore_ascii_case("sec-websocket-key") {
                key = value.trim().to_string();
            }
        }
    }
    Ok(key)
}

/// A streaming method handler (CPE-819/820): produces a series of JSON items to be sent as
/// `StreamItem`s over the wire, terminated by `StreamEnd`. This is the over-the-wire equivalent of the
/// app's `ipc::Channel` producers (directory listings, recursive search) — a request that yields many
/// results, correlated by the request's envelope id, so the client paints rows as they arrive.
/// The sink a [`StreamHandler`] pushes each item into — writing it to the socket as a `StreamItem`
/// immediately. Returns [`ControlFlow::Break`](std::ops::ControlFlow::Break) when the peer has gone
/// (or cancels), so the producer stops walking at the next batch boundary.
pub type StreamSink<'a> = dyn FnMut(serde_json::Value) -> std::ops::ControlFlow<()> + 'a;

pub type StreamHandler = Box<
    dyn Fn(&dyn ServerCtx, serde_json::Value, &mut StreamSink) -> Result<(), ContractError>
        + Send
        + Sync,
>;

/// Everything one Server needs: the method registry, the security chain guarding the
/// boundary, and the runtime context handed to domain logic. Cheap to share across
/// connection threads behind an [`Arc`] (all three parts are `Send + Sync`).
pub struct ServerRuntime {
    dispatcher: Dispatcher,
    chain: SecurityChain,
    ctx: Arc<dyn ServerCtx>,
    server_id: String,
    stream_handlers: std::collections::BTreeMap<String, StreamHandler>,
}

impl ServerRuntime {
    /// Build a runtime from a dispatcher, a security chain, and a runtime context.
    pub fn new(dispatcher: Dispatcher, chain: SecurityChain, ctx: Arc<dyn ServerCtx>) -> Self {
        Self {
            dispatcher,
            chain,
            ctx,
            server_id: "cpe-server-ref".to_string(),
            stream_handlers: std::collections::BTreeMap::new(),
        }
    }

    /// Override the `server_id` reported in the [`Welcome`] handshake.
    pub fn with_server_id(mut self, id: impl Into<String>) -> Self {
        self.server_id = id.into();
        self
    }

    /// Register a streaming method (CPE-819): a request for `method` runs `handler` and streams its
    /// items back as `StreamItem`s + a final `StreamEnd`, all security-checked exactly like a unary call.
    pub fn with_stream_handler<F>(mut self, method: impl Into<String>, handler: F) -> Self
    where
        F: Fn(&dyn ServerCtx, serde_json::Value, &mut StreamSink) -> Result<(), ContractError>
            + Send
            + Sync
            + 'static,
    {
        self.stream_handlers.insert(method.into(), Box::new(handler));
        self
    }

    /// Register the standard streaming producers over the wire (CPE-819): the directory listing as
    /// `list_dir_stream`, emitting one `StreamItem` per [`DirEntry`]. The reference Server and its
    /// tests share this so the streaming path is exercised against real domain data, not a stub. (v1
    /// collects then emits; swapping in the shared incremental walker is a follow-up.)
    pub fn with_builtin_streams(self) -> Self {
        self.with_stream_handler("list_dir_stream", |_ctx, params, emit| {
            let path = params.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
                ContractError::new(ErrorCode::BadRequest, "list_dir_stream: missing 'path'", false)
            })?;
            // Emit each batch as the walker reads it (incremental — first rows land immediately),
            // stopping early if the peer has gone.
            cpe_server::listing::stream_dir_entries(path, 64, |batch| {
                for de in batch {
                    if let Ok(v) = serde_json::to_value(de) {
                        if emit(v).is_break() {
                            return std::ops::ControlFlow::Break(());
                        }
                    }
                }
                std::ops::ControlFlow::Continue(())
            })
            .map(|_total| ())
            .map_err(|e| ContractError::new(ErrorCode::Internal, e, false))
        })
        .with_stream_handler("name_search_stream", |_ctx, params, emit| {
            // Recursive filename search under `path` for `query`, streaming each batch of matches as the
            // walker finds them (the canonical streaming case — results arrive live).
            let path = params.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
                ContractError::new(ErrorCode::BadRequest, "name_search_stream: missing 'path'", false)
            })?;
            let query = params.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
                ContractError::new(ErrorCode::BadRequest, "name_search_stream: missing 'query'", false)
            })?;
            cpe_server::name_search::walk_name_matches(
                path,
                query,
                cpe_server::name_search::NAME_SEARCH_BATCH,
                |batch| {
                    for m in batch {
                        if let Ok(v) = serde_json::to_value(m) {
                            if emit(v).is_break() {
                                return std::ops::ControlFlow::Break(());
                            }
                        }
                    }
                    std::ops::ControlFlow::Continue(())
                },
            )
            .map(|_stats| ())
            .map_err(|e| ContractError::new(ErrorCode::Internal, e, false))
        })
        .with_stream_handler("content_search_stream", |_ctx, params, emit| {
            // Recursive text-content search under `path` for `query`, streaming each batch of line
            // matches as the walker finds them (the slowest producer — liveness matters most here).
            let path = params.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
                ContractError::new(ErrorCode::BadRequest, "content_search_stream: missing 'path'", false)
            })?;
            let query = params.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
                ContractError::new(ErrorCode::BadRequest, "content_search_stream: missing 'query'", false)
            })?;
            let case_sensitive = params.get("case_sensitive").and_then(|v| v.as_bool()).unwrap_or(false);
            cpe_server::content_search::stream_file_contents(
                path,
                query,
                case_sensitive,
                cpe_server::content_search::CONTENT_SEARCH_BATCH,
                |batch| {
                    for m in batch {
                        if let Ok(v) = serde_json::to_value(m) {
                            if emit(v).is_break() {
                                return std::ops::ControlFlow::Break(());
                            }
                        }
                    }
                    std::ops::ControlFlow::Continue(())
                },
            )
            .map(|_stats| ())
            .map_err(|e| ContractError::new(ErrorCode::Internal, e, false))
        })
    }

    /// Accept connections forever, handling each on its own thread. Blocks the caller.
    pub fn serve(self: Arc<Self>, listener: TcpListener) -> std::io::Result<()> {
        for stream in listener.incoming() {
            let stream = stream?;
            let me = Arc::clone(&self);
            std::thread::spawn(move || {
                // A per-connection I/O error just drops that client; it never brings the
                // Server down.
                let _ = me.handle(stream);
            });
        }
        Ok(())
    }

    /// Handle a single accepted connection: handshake, then the request loop until the peer
    /// closes. Public so a caller can drive one connection synchronously (tests, embedding).
    pub fn handle(&self, stream: TcpStream) -> std::io::Result<()> {
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut writer = BufWriter::new(stream);

        // Pick the transport from the first bytes without consuming them (CPE-819): a browser opens with
        // an HTTP `GET …` upgrade; a Client(Rust) sends a length-prefixed envelope. WebSocket clients get
        // the RFC 6455 101 handshake; then both speak CPE-811 envelopes through the shared session loop.
        if reader.fill_buf()?.starts_with(b"GET ") {
            let key = read_ws_key(&mut reader)?;
            let accept = ws::accept_key(&key);
            write!(
                writer,
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
                 Sec-WebSocket-Accept: {accept}\r\n\r\n"
            )?;
            writer.flush()?;
            self.run_session(&mut WsIo { reader, writer })
        } else {
            self.run_session(&mut WireIo { reader, writer })
        }
    }

    /// Drive one already-transported session: handshake, then the request/stream loop until the peer
    /// closes — over whatever [`EnvelopeIo`] framing (raw wire or WebSocket).
    fn run_session(&self, io: &mut dyn EnvelopeIo) -> std::io::Result<()> {
        // --- Handshake: expect a Hello, reply Welcome or Rejected. ---
        let first = match io.read_env()? {
            Some(env) => env,
            None => return Ok(()), // peer left before saying hello
        };
        let (reply, principal) = self.handshake(first.message);
        let rejected = matches!(reply, Message::Rejected(_));
        io.write_env(&Envelope::new(first.id, reply))?;
        if rejected {
            return Ok(());
        }

        // --- Request loop. ---
        while let Some(env) = io.read_env()? {
            match env.message {
                Message::Request(req) if self.stream_handlers.contains_key(&req.method) => {
                    // Streaming method (CPE-819): security-check, then let the handler push StreamItems
                    // to the peer *as produced* (incremental), correlated by the request's envelope id,
                    // terminated by StreamEnd. A denial/handler error is a single Response instead.
                    let id = env.id;
                    if let Err(e) = self.security_decision(&principal, &req) {
                        io.write_env(&Envelope::new(id, Message::Response(Response { result: Err(e) })))?;
                    } else {
                        let handler = &self.stream_handlers[&req.method];
                        let mut io_err: Option<std::io::Error> = None;
                        let outcome = {
                            let mut emit = |item: serde_json::Value| -> std::ops::ControlFlow<()> {
                                match io.write_env(&Envelope::new(id, Message::StreamItem(item))) {
                                    Ok(()) => std::ops::ControlFlow::Continue(()),
                                    Err(e) => {
                                        io_err = Some(e);
                                        std::ops::ControlFlow::Break(())
                                    }
                                }
                            };
                            handler(self.ctx.as_ref(), req.params, &mut emit)
                        };
                        if let Some(e) = io_err {
                            return Err(e); // the peer's write failed — drop the connection
                        }
                        match outcome {
                            Ok(()) => io.write_env(&Envelope::new(id, Message::StreamEnd))?,
                            Err(e) => io.write_env(&Envelope::new(id, Message::Response(Response { result: Err(e) })))?,
                        }
                    }
                }
                Message::Request(req) => {
                    let resp = self.dispatch_guarded(&principal, req);
                    io.write_env(&Envelope::new(env.id, Message::Response(resp)))?;
                }
                // A re-sent Hello is tolerated (no-op); anything else ends the session.
                Message::Hello(_) => {}
                _ => break,
            }
        }
        Ok(())
    }

    /// Decide the handshake reply and the session principal from the opening message. The version
    /// decision (negotiate → Welcome / Rejected) is the shared `cpe_contract::handshake` (CPE-820), so
    /// this crate and its conformance tests agree with one source of truth; here we only add the
    /// transport-side concerns: a non-Hello opener is a policy denial, and on accept the session acts as
    /// the client's proposed principal (v1 single-user; per-request AuthN lands in CPE-817).
    fn handshake(&self, opening: Message) -> (Message, Principal) {
        let Message::Hello(hello) = opening else {
            return (
                Message::Rejected(Rejected {
                    code: RejectCode::PolicyDenied,
                    reason: "handshake: expected Hello".to_string(),
                }),
                Principal::local(),
            );
        };
        match handshake(
            &hello,
            CONTRACT_VERSION,
            self.server_id.clone(),
            env!("CARGO_PKG_VERSION"),
            Session::local(),
        ) {
            Ok(welcome) => (Message::Welcome(welcome), hello.principal.unwrap_or_else(Principal::local)),
            Err(rejected) => (Message::Rejected(rejected), Principal::local()),
        }
    }

    /// Run a request through the security chain. `Ok(())` = admitted (proceed to dispatch/stream);
    /// `Err` = a denial mapped to the security [`ContractError`] (the request is never dispatched).
    /// Shared by the unary and streaming paths so both enforce the boundary identically.
    fn security_decision(&self, principal: &Principal, req: &Request) -> Result<(), ContractError> {
        let mut sctx = SecurityContext::new(principal.clone(), req.method.clone());
        // The resource an authorizer keys off, when the call names a filesystem path.
        if let Some(path) = req.params.get("path").and_then(|v| v.as_str()) {
            sctx = sctx.with_resource(path.to_string());
        }
        match self.chain.evaluate(&mut sctx) {
            Decision::Allow(_authorized) => Ok(()),
            Decision::Deny(denial) => {
                // AuthZ failures are "authenticated but not permitted" (Unauthorized);
                // transport / authentication failures are "not admitted" (Unauthenticated).
                let code = match denial.plane {
                    Plane::Authorization => ErrorCode::Unauthorized,
                    Plane::Transport | Plane::Authentication => ErrorCode::Unauthenticated,
                };
                Err(ContractError::new(
                    code,
                    format!("{:?} denied: {}", denial.plane, denial.reason),
                    false,
                ))
            }
        }
    }

    /// Evaluate a request through the security chain, and only then dispatch it. A denial is
    /// mapped to a security [`ContractError`] and the request is *not* dispatched.
    fn dispatch_guarded(&self, principal: &Principal, req: Request) -> Response {
        match self.security_decision(principal, &req) {
            Ok(()) => self.dispatcher.dispatch(self.ctx.as_ref(), req),
            Err(e) => Response { result: Err(e) },
        }
    }
}
