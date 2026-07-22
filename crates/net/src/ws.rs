//! Minimal WebSocket (RFC 6455) codec for the reference Server (CPE-819) — just what the browser
//! transport needs: the `Sec-WebSocket-Accept` handshake reply, and read/write of a single data frame.
//!
//! WebSocket is the browser-native, bidirectional transport chosen for the frontend `RemoteTransport`
//! (it carries requests AND the `StreamItem` streaming). The CPE-811 envelope rides as the payload of a
//! **text** frame; this module is only the frame layer. `std`-only + two tiny crates (sha1/base64) for
//! the handshake — no async runtime, matching the rest of `cpe-net`. Server frames are unmasked; client
//! frames are masked (unmasked here). A declared payload over [`MAX_WS_PAYLOAD`] is refused before
//! allocating, so a hostile length can't drive an unbounded `vec![0u8; n]` (mirrors CPE-886).

use std::io::{self, Read, Write};

use base64::Engine as _;
use sha1::{Digest, Sha1};

/// Opcodes this codec handles.
pub mod op {
    pub const TEXT: u8 = 0x1;
    pub const BINARY: u8 = 0x2;
    pub const CLOSE: u8 = 0x8;
    pub const PING: u8 = 0x9;
    pub const PONG: u8 = 0xA;
}

/// Largest inbound frame payload accepted. The envelope carries small JSON requests + streamed items;
/// this bound stops an attacker-declared 64-bit length driving an unbounded allocation (CPE-886).
pub const MAX_WS_PAYLOAD: usize = 16 * 1024 * 1024;

/// The RFC 6455 GUID appended to the client key before hashing.
const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// `Sec-WebSocket-Accept` = base64(sha1(client-key + magic GUID)).
pub fn accept_key(client_key: &str) -> String {
    let mut h = Sha1::new();
    h.update(client_key.as_bytes());
    h.update(WS_GUID.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(h.finalize())
}

/// One decoded inbound frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub fin: bool,
    pub opcode: u8,
    pub payload: Vec<u8>,
}

/// Write one server frame (FIN=1, unmasked).
pub fn write_frame(w: &mut impl Write, opcode: u8, payload: &[u8]) -> io::Result<()> {
    let mut head = Vec::with_capacity(10);
    head.push(0x80 | opcode);
    let len = payload.len();
    if len < 126 {
        head.push(len as u8);
    } else if len <= 0xFFFF {
        head.push(126);
        head.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        head.push(127);
        head.extend_from_slice(&(len as u64).to_be_bytes());
    }
    w.write_all(&head)?;
    w.write_all(payload)?;
    w.flush()
}

/// Convenience: write a text frame carrying `s` (the envelope JSON).
pub fn write_text(w: &mut impl Write, s: &str) -> io::Result<()> {
    write_frame(w, op::TEXT, s.as_bytes())
}

/// Read one client frame (unmasking it). `Ok(None)` on clean end-of-stream. Rejects an over-large
/// declared length before allocating.
pub fn read_frame(r: &mut impl Read) -> io::Result<Option<Frame>> {
    let mut h = [0u8; 2];
    if fill(r, &mut h)? {
        return Ok(None);
    }
    let fin = h[0] & 0x80 != 0;
    let opcode = h[0] & 0x0F;
    let masked = h[1] & 0x80 != 0;
    let mut len = (h[1] & 0x7F) as usize;
    if len == 126 {
        let mut e = [0u8; 2];
        if fill(r, &mut e)? {
            return Ok(None);
        }
        len = u16::from_be_bytes(e) as usize;
    } else if len == 127 {
        let mut e = [0u8; 8];
        if fill(r, &mut e)? {
            return Ok(None);
        }
        len = u64::from_be_bytes(e) as usize;
    }
    if len > MAX_WS_PAYLOAD {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "websocket frame exceeds maximum size"));
    }
    let mut mask = [0u8; 4];
    if masked && fill(r, &mut mask)? {
        return Ok(None);
    }
    let mut payload = vec![0u8; len];
    if len > 0 && fill(r, &mut payload)? {
        return Ok(None);
    }
    if masked {
        for (i, b) in payload.iter_mut().enumerate() {
            *b ^= mask[i & 3];
        }
    }
    Ok(Some(Frame { fin, opcode, payload }))
}

/// Read exactly `buf.len()` bytes; `Ok(true)` if EOF was hit first (a clean close between frames).
fn fill(r: &mut impl Read, buf: &mut [u8]) -> io::Result<bool> {
    let mut filled = 0;
    while filled < buf.len() {
        match r.read(&mut buf[filled..]) {
            Ok(0) => return Ok(true),
            Ok(n) => filled += n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_key_matches_rfc6455_example() {
        // RFC 6455 §1.3 canonical example.
        assert_eq!(accept_key("dGhlIHNhbXBsZSBub25jZQ=="), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    #[test]
    fn write_then_read_a_masked_text_frame_round_trips() {
        // Server writes an unmasked text frame.
        let mut buf = Vec::new();
        write_text(&mut buf, "hello").unwrap();
        assert_eq!(buf[0], 0x80 | op::TEXT);
        assert_eq!(buf[1], 5); // unmasked, length 5
        assert_eq!(&buf[2..], b"hello");

        // A client frame is masked; build one and confirm we unmask it.
        let mask = [0x37u8, 0xfa, 0x21, 0x3d];
        let payload = b"type me";
        let mut frame = vec![0x80 | op::TEXT, 0x80 | payload.len() as u8];
        frame.extend_from_slice(&mask);
        for (i, b) in payload.iter().enumerate() {
            frame.push(b ^ mask[i & 3]);
        }
        let mut cur = std::io::Cursor::new(frame);
        let f = read_frame(&mut cur).unwrap().unwrap();
        assert!(f.fin && f.opcode == op::TEXT);
        assert_eq!(f.payload, payload);
    }

    #[test]
    fn read_frame_rejects_an_over_large_declared_length() {
        // A 127 + u64::MAX length header must be refused before allocating (CPE-886 class).
        let mut frame = vec![0x80 | op::BINARY, 127];
        frame.extend_from_slice(&u64::MAX.to_be_bytes());
        let mut cur = std::io::Cursor::new(frame);
        match read_frame(&mut cur) {
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::InvalidData),
            Ok(_) => panic!("over-large frame must be rejected"),
        }
    }

    #[test]
    fn clean_eof_between_frames_is_none() {
        let mut cur = std::io::Cursor::new(Vec::new());
        assert_eq!(read_frame(&mut cur).unwrap(), None);
    }
}
