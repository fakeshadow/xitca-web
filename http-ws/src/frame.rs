//! Copy from [actix-http](https://github.com/actix/actix-web)

use bytes::{Buf, BufMut, Bytes, BytesMut};
use tracing::debug;

use super::{
    error::ProtocolError,
    mask::apply_mask,
    proto::{CloseCode, CloseReason, OpCode},
};

/// A struct representing a WebSocket frame.
#[derive(Debug)]
pub struct Parser;

pub type MetaData = (usize, bool, OpCode, usize, Option<[u8; 4]>);

impl Parser {
    fn parse_metadata(src: &[u8], server: bool, max_size: usize) -> Result<Option<MetaData>, ProtocolError> {
        let chunk_len = src.len();

        let mut idx = 2;
        if chunk_len < 2 {
            return Ok(None);
        }

        let first = src[0];
        let second = src[1];
        let finished = first & 0x80 != 0;

        // check masking
        let masked = second & 0x80 != 0;
        if !masked && server {
            return Err(ProtocolError::UnmaskedFrame);
        } else if masked && !server {
            return Err(ProtocolError::MaskedFrame);
        }

        // Op code
        let opcode = OpCode::from(first & 0x0F);

        if let OpCode::Bad = opcode {
            return Err(ProtocolError::InvalidOpcode(first & 0x0F));
        }

        let len = second & 0x7F;
        let length = if len == 126 {
            if chunk_len < 4 {
                return Ok(None);
            }
            let len = usize::from(u16::from_be_bytes(TryFrom::try_from(&src[idx..idx + 2]).unwrap()));
            idx += 2;
            len
        } else if len == 127 {
            if chunk_len < 10 {
                return Ok(None);
            }
            let len = u64::from_be_bytes(TryFrom::try_from(&src[idx..idx + 8]).unwrap());
            if len > max_size as u64 {
                return Err(ProtocolError::Overflow);
            }
            idx += 8;
            len as usize
        } else {
            len as usize
        };

        // check for max allowed size
        if length > max_size {
            return Err(ProtocolError::Overflow);
        }

        let mask = if server {
            if chunk_len < idx + 4 {
                return Ok(None);
            }

            let mask = <[u8; 4]>::try_from(&src[idx..idx + 4]).unwrap();

            idx += 4;

            Some(mask)
        } else {
            None
        };

        Ok(Some((idx, finished, opcode, length, mask)))
    }

    /// Parse the input stream into a frame.
    pub fn parse(
        src: &mut BytesMut,
        server: bool,
        max_size: usize,
    ) -> Result<Option<(bool, OpCode, Option<Bytes>)>, ProtocolError> {
        // try to parse ws frame metadata
        let (idx, finished, opcode, length, mask) = match Parser::parse_metadata(src, server, max_size)? {
            None => return Ok(None),
            Some(res) => res,
        };

        // not enough data
        if src.len() < idx + length {
            return Ok(None);
        }

        // remove prefix
        src.advance(idx);

        // no need for body
        if length == 0 {
            return Ok(Some((finished, opcode, None)));
        }

        let mut data = src.split_to(length);

        // control frames must have length <= 125
        match opcode {
            OpCode::Ping | OpCode::Pong if length > 125 => Err(ProtocolError::InvalidLength(length)),
            OpCode::Close if length > 125 => {
                debug!("Received close frame with payload length exceeding 125. Morphing to protocol close frame.");
                Ok(Some((true, OpCode::Close, None)))
            }
            _ => {
                // unmask
                if let Some(mask) = mask {
                    apply_mask(&mut data, mask);
                }

                Ok(Some((finished, opcode, Some(data.freeze()))))
            }
        }
    }

    /// Parse the payload of a close frame.
    pub fn parse_close_payload(payload: &[u8]) -> Option<CloseReason> {
        (payload.len() >= 2).then(|| {
            let raw_code = u16::from_be_bytes(TryFrom::try_from(&payload[..2]).unwrap());
            let code = CloseCode::from(raw_code);
            let description = (payload.len() > 2).then(|| String::from_utf8_lossy(&payload[2..]).into());

            CloseReason { code, description }
        })
    }

    /// Generate binary representation
    pub fn write_message<B: AsRef<[u8]>>(dst: &mut BytesMut, pl: B, op: OpCode, fin: bool, mask: bool) {
        let payload = pl.as_ref();
        let one = if fin { 0x80 | u8::from(op) } else { u8::from(op) };
        let len = payload.len();
        let (two, len_maybe_mask) = if mask { (0x80, len + 4) } else { (0, len) };

        if len < 126 {
            dst.reserve(len_maybe_mask + 2);
            dst.put_slice(&[one, two | len as u8]);
        } else if len <= 65_535 {
            dst.reserve(len_maybe_mask + 4);
            dst.put_slice(&[one, two | 126]);
            dst.put_u16(len as u16);
        } else {
            dst.reserve(len_maybe_mask + 10);
            dst.put_slice(&[one, two | 127]);
            dst.put_u64(len as u64);
        };

        if mask {
            let mask = rand::random::<[u8; 4]>();
            dst.put_slice(&mask);
            dst.put_slice(payload);
            let pos = dst.len() - len;
            apply_mask(&mut dst[pos..], mask);
        } else {
            dst.put_slice(payload);
        }
    }

    /// Create a new Close control frame.
    #[inline]
    pub fn write_close(dst: &mut BytesMut, reason: Option<CloseReason>, mask: bool) {
        let payload = reason
            .map(|reason| {
                let mut payload = u16::from(reason.code).to_be_bytes().to_vec();
                if let Some(description) = reason.description {
                    payload.extend(description.as_bytes());
                }
                payload
            })
            .unwrap_or_else(Vec::new);

        Parser::write_message(dst, payload, OpCode::Close, true, mask)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct F {
        finished: bool,
        opcode: OpCode,
        payload: Bytes,
    }

    type Extract = (bool, OpCode, Option<Bytes>);

    fn is_none(frm: &Result<Option<Extract>, ProtocolError>) -> bool {
        matches!(*frm, Ok(None))
    }

    fn extract(frm: Result<Option<Extract>, ProtocolError>) -> F {
        match frm {
            Ok(Some((finished, opcode, payload))) => F {
                finished,
                opcode,
                payload: payload.unwrap_or_else(|| Bytes::from("")),
            },
            _ => unreachable!("error"),
        }
    }

    #[test]
    fn test_parse() {
        let mut buf = BytesMut::from(&[0b0000_0001u8, 0b0000_0001u8][..]);
        assert!(is_none(&Parser::parse(&mut buf, false, 1024)));

        let mut buf = BytesMut::from(&[0b0000_0001u8, 0b0000_0001u8][..]);
        buf.extend(b"1");

        let frame = extract(Parser::parse(&mut buf, false, 1024));
        assert!(!frame.finished);
        assert_eq!(frame.opcode, OpCode::Text);
        assert_eq!(frame.payload.as_ref(), &b"1"[..]);
    }

    #[test]
    fn test_parse_length0() {
        let mut buf = BytesMut::from(&[0b0000_0001u8, 0b0000_0000u8][..]);
        let frame = extract(Parser::parse(&mut buf, false, 1024));
        assert!(!frame.finished);
        assert_eq!(frame.opcode, OpCode::Text);
        assert!(frame.payload.is_empty());
    }

    #[test]
    fn test_parse_length2() {
        let mut buf = BytesMut::from(&[0b0000_0001u8, 126u8][..]);
        assert!(is_none(&Parser::parse(&mut buf, false, 1024)));

        let mut buf = BytesMut::from(&[0b0000_0001u8, 126u8][..]);
        buf.extend(&[0u8, 4u8][..]);
        buf.extend(b"1234");

        let frame = extract(Parser::parse(&mut buf, false, 1024));
        assert!(!frame.finished);
        assert_eq!(frame.opcode, OpCode::Text);
        assert_eq!(frame.payload.as_ref(), &b"1234"[..]);
    }

    #[test]
    fn test_parse_length4() {
        let mut buf = BytesMut::from(&[0b0000_0001u8, 127u8][..]);
        assert!(is_none(&Parser::parse(&mut buf, false, 1024)));

        let mut buf = BytesMut::from(&[0b0000_0001u8, 127u8][..]);
        buf.extend(&[0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 4u8][..]);
        buf.extend(b"1234");

        let frame = extract(Parser::parse(&mut buf, false, 1024));
        assert!(!frame.finished);
        assert_eq!(frame.opcode, OpCode::Text);
        assert_eq!(frame.payload.as_ref(), &b"1234"[..]);
    }

    #[test]
    fn test_parse_frame_mask() {
        let mut buf = BytesMut::from(&[0b0000_0001u8, 0b1000_0001u8][..]);
        buf.extend(b"0001");
        buf.extend(b"1");

        assert!(Parser::parse(&mut buf, false, 1024).is_err());

        let frame = extract(Parser::parse(&mut buf, true, 1024));
        assert!(!frame.finished);
        assert_eq!(frame.opcode, OpCode::Text);
        assert_eq!(frame.payload, Bytes::from(vec![1u8]));
    }

    #[test]
    fn test_parse_frame_no_mask() {
        let mut buf = BytesMut::from(&[0b0000_0001u8, 0b0000_0001u8][..]);
        buf.extend([1u8]);

        assert!(Parser::parse(&mut buf, true, 1024).is_err());

        let frame = extract(Parser::parse(&mut buf, false, 1024));
        assert!(!frame.finished);
        assert_eq!(frame.opcode, OpCode::Text);
        assert_eq!(frame.payload, Bytes::from(vec![1u8]));
    }

    #[test]
    fn test_parse_frame_max_size() {
        let mut buf = BytesMut::from(&[0b0000_0001u8, 0b0000_0010u8][..]);
        buf.extend([1u8, 1u8]);

        assert!(Parser::parse(&mut buf, true, 1).is_err());

        if let Err(ProtocolError::Overflow) = Parser::parse(&mut buf, false, 0) {
        } else {
            unreachable!("error");
        }
    }

    #[test]
    fn test_ping_frame() {
        let mut buf = BytesMut::new();
        Parser::write_message(&mut buf, Vec::from("data"), OpCode::Ping, true, false);

        let mut v = vec![137u8, 4u8];
        v.extend(b"data");
        assert_eq!(&buf[..], &v[..]);
    }

    #[test]
    fn test_pong_frame() {
        let mut buf = BytesMut::new();
        Parser::write_message(&mut buf, Vec::from("data"), OpCode::Pong, true, false);

        let mut v = vec![138u8, 4u8];
        v.extend(b"data");
        assert_eq!(&buf[..], &v[..]);
    }

    #[test]
    fn test_close_frame() {
        let mut buf = BytesMut::new();
        let reason = (CloseCode::Normal, "data");
        Parser::write_close(&mut buf, Some(reason.into()), false);

        let mut v = vec![136u8, 6u8, 3u8, 232u8];
        v.extend(b"data");
        assert_eq!(&buf[..], &v[..]);
    }

    #[test]
    fn test_empty_close_frame() {
        let mut buf = BytesMut::new();
        Parser::write_close(&mut buf, None, false);
        assert_eq!(&buf[..], &vec![0x88, 0x00][..]);
    }
}
