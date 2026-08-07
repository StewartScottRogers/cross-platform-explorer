//! Transport-neutral envelope framing (CPE-825, epic CPE-810).
//!
//! Reads and writes the CPE-811 [`Envelope`] as newline-framed JSON (the contract's
//! [`codec`](cpe_contract::codec)) over *any* [`Read`]/[`Write`]. The concrete transport —
//! a loopback TCP socket here, a real remote socket later, or an in-process pipe — is chosen
//! by the runtime; every transport agrees on this one frame shape. Keeping the framing here
//! (not in the contract crate, which stays serde-only) lets the client and server share
//! exactly one read/write path.

use std::io::{BufRead, Read, Write};

use cpe_contract::{codec, Envelope};

/// Hard ceiling on a single framed envelope while reading it off the wire (CPE-1416). Envelopes
/// are JSON control messages (requests/responses/events), not payload carriers, so this is
/// generously above any legitimate frame — it only exists to stop an unbounded `read_line` from
/// letting a malicious or buggy peer that sends bytes but never a `\n` grow the buffer without
/// limit (a memory-exhaustion DoS on the `cpe-net` loop).
const MAX_ENVELOPE_BYTES: u64 = 16 * 1024 * 1024; // 16 MiB

/// Write one envelope as a single newline-terminated JSON line and flush it, so the peer's
/// `read_line` sees a complete frame immediately (no half-buffered request hanging the loop).
pub fn write_envelope<W: Write>(w: &mut W, env: &Envelope) -> std::io::Result<()> {
    let line = codec::encode_line(env).map_err(to_io)?;
    w.write_all(line.as_bytes())?;
    w.flush()
}

/// Read one newline-framed envelope. `Ok(None)` signals a clean end-of-stream (the peer
/// closed the connection between frames), distinct from a malformed frame, which is an error.
///
/// The read is capped at [`MAX_ENVELOPE_BYTES`] (CPE-1416): a peer that streams bytes without
/// ever sending a `\n` gets a prompt `Err` once the cap is hit, instead of growing the line
/// buffer without limit.
pub fn read_envelope<R: BufRead>(r: &mut R) -> std::io::Result<Option<Envelope>> {
    read_envelope_capped(r, MAX_ENVELOPE_BYTES)
}

/// The guts of [`read_envelope`], parameterized on the cap so tests can exercise the boundary
/// with a small `max_bytes` instead of allocating [`MAX_ENVELOPE_BYTES`] worth of buffer.
fn read_envelope_capped<R: BufRead>(r: &mut R, max_bytes: u64) -> std::io::Result<Option<Envelope>> {
    let mut line = String::new();
    // `Take` bounds how many bytes `read_line` will ever pull from `r`, so the buffer can't
    // grow past the cap no matter how long the peer keeps sending without a newline.
    let mut limited = r.take(max_bytes);
    let n = limited.read_line(&mut line)?;
    if n == 0 {
        return Ok(None); // clean EOF
    }
    if limited.limit() == 0 && !line.ends_with('\n') {
        // Consumed the entire cap without finding a frame terminator: either a malicious/buggy
        // peer withholding the newline, or a legitimate frame larger than any envelope should
        // ever be. Either way, bail cleanly rather than reading further.
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("envelope exceeds max size of {max_bytes} bytes"),
        ));
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

    /// CPE-1416: a peer that streams bytes but never sends a `\n` must not grow the read
    /// buffer without limit. `std::io::repeat` is a genuinely infinite byte source — if the
    /// cap in `read_envelope` didn't work, this test would hang (and OOM) instead of failing
    /// fast, so its mere termination is the assertion that matters as much as the `Err`.
    #[test]
    fn unbounded_peer_with_no_newline_errors_promptly_at_the_cap() {
        let mut reader = BufReader::new(std::io::repeat(b'x'));
        let err = read_envelope(&mut reader).expect_err("unbounded stream with no newline");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(
            err.to_string().contains("exceeds max size"),
            "unexpected error: {err}"
        );
    }

    /// Same shape as the test above but against a small cap, so the assertion covers the exact
    /// byte where the cap kicks in without allocating `MAX_ENVELOPE_BYTES` of scratch space.
    #[test]
    fn reader_with_no_newline_errors_right_at_a_small_cap() {
        let mut reader = BufReader::new(std::io::repeat(b'x'));
        let err = read_envelope_capped(&mut reader, 64).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn envelope_exactly_at_the_cap_still_round_trips() {
        let env = Envelope::new(
            1,
            Message::Request(Request {
                method: "ping".into(),
                params: serde_json::json!({}),
            }),
        );
        let line = codec::encode_line(&env).unwrap(); // includes the trailing '\n'
        let cap = line.len() as u64; // exactly enough for this one frame
        let mut reader = BufReader::new(line.as_bytes());
        let back = read_envelope_capped(&mut reader, cap).unwrap().unwrap();
        assert_eq!(back, env);
    }

    #[test]
    fn envelope_one_byte_over_the_cap_errors() {
        let env = Envelope::new(
            1,
            Message::Request(Request {
                method: "ping".into(),
                params: serde_json::json!({}),
            }),
        );
        let line = codec::encode_line(&env).unwrap();
        let cap = (line.len() - 1) as u64; // one byte short of the full frame
        let mut reader = BufReader::new(line.as_bytes());
        let err = read_envelope_capped(&mut reader, cap).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }
}
