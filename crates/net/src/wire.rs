//! Transport-neutral envelope framing (CPE-825, epic CPE-810).
//!
//! Reads and writes the CPE-811 [`Envelope`] as newline-framed JSON (the contract's
//! [`codec`](cpe_contract::codec)) over *any* [`Read`]/[`Write`]. The concrete transport —
//! a loopback TCP socket here, a real remote socket later, or an in-process pipe — is chosen
//! by the runtime; every transport agrees on this one frame shape. Keeping the framing here
//! (not in the contract crate, which stays serde-only) lets the client and server share
//! exactly one read/write path.

use std::io::{BufRead, Write};

use cpe_contract::{codec, Envelope};

/// Write one envelope as a single newline-terminated JSON line and flush it, so the peer's
/// `read_line` sees a complete frame immediately (no half-buffered request hanging the loop).
pub fn write_envelope<W: Write>(w: &mut W, env: &Envelope) -> std::io::Result<()> {
    let line = codec::encode_line(env).map_err(to_io)?;
    w.write_all(line.as_bytes())?;
    w.flush()
}

/// Read one newline-framed envelope. `Ok(None)` signals a clean end-of-stream (the peer
/// closed the connection between frames), distinct from a malformed frame, which is an error.
pub fn read_envelope<R: BufRead>(r: &mut R) -> std::io::Result<Option<Envelope>> {
    let mut line = String::new();
    if r.read_line(&mut line)? == 0 {
        return Ok(None); // clean EOF
    }
    Ok(Some(codec::decode_line(&line).map_err(to_io)?))
}

fn to_io(e: serde_json::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cpe_contract::{Message, Request};
    use std::io::BufReader;

    #[test]
    fn round_trips_an_envelope_through_a_byte_buffer() {
        let env = Envelope::new(
            7,
            Message::Request(Request {
                method: "list_dir".into(),
                params: serde_json::json!({ "path": "/tmp" }),
            }),
        );
        let mut buf: Vec<u8> = Vec::new();
        write_envelope(&mut buf, &env).unwrap();
        // Exactly one newline-framed line.
        assert_eq!(buf.iter().filter(|&&b| b == b'\n').count(), 1);
        let mut reader = BufReader::new(&buf[..]);
        let back = read_envelope(&mut reader).unwrap().unwrap();
        assert_eq!(back, env);
        // A second read hits clean EOF.
        assert!(read_envelope(&mut reader).unwrap().is_none());
    }

    #[test]
    fn malformed_frame_is_an_error_not_eof() {
        let mut reader = BufReader::new(&b"{ not json }\n"[..]);
        assert!(read_envelope(&mut reader).is_err());
    }
}
