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

use std::io::{BufReader, BufWriter};
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

/// Everything one Server needs: the method registry, the security chain guarding the
/// boundary, and the runtime context handed to domain logic. Cheap to share across
/// connection threads behind an [`Arc`] (all three parts are `Send + Sync`).
pub struct ServerRuntime {
    dispatcher: Dispatcher,
    chain: SecurityChain,
    ctx: Arc<dyn ServerCtx>,
    server_id: String,
}

impl ServerRuntime {
    /// Build a runtime from a dispatcher, a security chain, and a runtime context.
    pub fn new(dispatcher: Dispatcher, chain: SecurityChain, ctx: Arc<dyn ServerCtx>) -> Self {
        Self {
            dispatcher,
            chain,
            ctx,
            server_id: "cpe-server-ref".to_string(),
        }
    }

    /// Override the `server_id` reported in the [`Welcome`] handshake.
    pub fn with_server_id(mut self, id: impl Into<String>) -> Self {
        self.server_id = id.into();
        self
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

        // --- Handshake: expect a Hello, reply Welcome or Rejected. ---
        let first = match read_envelope(&mut reader)? {
            Some(env) => env,
            None => return Ok(()), // peer left before saying hello
        };
        let (reply, principal) = self.handshake(first.message);
        let rejected = matches!(reply, Message::Rejected(_));
        write_envelope(&mut writer, &Envelope::new(first.id, reply))?;
        if rejected {
            return Ok(());
        }

        // --- Request loop. ---
        while let Some(env) = read_envelope(&mut reader)? {
            match env.message {
                Message::Request(req) => {
                    let resp = self.dispatch_guarded(&principal, req);
                    write_envelope(&mut writer, &Envelope::new(env.id, Message::Response(resp)))?;
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

    /// Evaluate a request through the security chain, and only then dispatch it. A denial is
    /// mapped to a security [`ContractError`] and the request is *not* dispatched.
    fn dispatch_guarded(&self, principal: &Principal, req: Request) -> Response {
        let mut sctx = SecurityContext::new(principal.clone(), req.method.clone());
        // The resource an authorizer keys off, when the call names a filesystem path.
        if let Some(path) = req.params.get("path").and_then(|v| v.as_str()) {
            sctx = sctx.with_resource(path.to_string());
        }

        match self.chain.evaluate(&mut sctx) {
            Decision::Allow(_authorized) => self.dispatcher.dispatch(self.ctx.as_ref(), req),
            Decision::Deny(denial) => {
                // AuthZ failures are "authenticated but not permitted" (Unauthorized);
                // transport / authentication failures are "not admitted" (Unauthenticated).
                let code = match denial.plane {
                    Plane::Authorization => ErrorCode::Unauthorized,
                    Plane::Transport | Plane::Authentication => ErrorCode::Unauthenticated,
                };
                Response {
                    result: Err(ContractError::new(
                        code,
                        format!("{:?} denied: {}", denial.plane, denial.reason),
                        false,
                    )),
                }
            }
        }
    }
}
