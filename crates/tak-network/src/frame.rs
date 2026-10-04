//! Wire frames and stream framing.
//!
//! Two encodings share TAK stream connections:
//!
//! * **CoT XML stream** — documents are simply concatenated. A frame ends at
//!   the first `</event>`; whitespace (and the XML declaration) may precede
//!   the next document.
//! * **TAK Protocol v1 stream** — each message is `0xBF`, a protobuf varint
//!   payload length, then the protobuf `TakMessage`. Detection is trivial
//!   because `0xBF` can never begin a well-formed XML document (it is not
//!   valid UTF-8 on its own).
//!
//! For datagrams (UDP mesh), TAK Protocol v1 prefixes the protobuf payload
//! with the magic `0xBF 0x01 0xBF`; see [`Frame::encode_datagram`] /
//! [`Frame::decode_datagram`].
//!
//! [`StreamDecoder`] detects the encoding frame by frame, so it copes with the
//! switch from XML to protobuf that happens after TAK Protocol negotiation.

use bytes::{Buf, BufMut, Bytes, BytesMut};

use crate::error::NetworkError;

/// TAK Protocol stream frame marker.
pub const TAK_PROTO_MAGIC: u8 = 0xBF;
/// TAK Protocol mesh/datagram prefix.
pub const TAK_PROTO_MESH_MAGIC: [u8; 3] = [0xBF, 0x01, 0xBF];
const EVENT_END: &[u8] = b"</event>";

/// Which encoding a frame uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FrameKind {
    /// A CoT XML `<event>` document.
    CotXml,
    /// A TAK Protocol protobuf `TakMessage`.
    TakProtobuf,
}

/// One message as it appears on the wire, undecoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Frame {
    /// UTF-8 CoT XML (exactly one `<event>` document, possibly with a
    /// leading XML declaration).
    CotXml(Bytes),
    /// Serialised protobuf `TakMessage` (without any length prefix).
    TakProtobuf(Bytes),
}

impl Frame {
    /// Encoding of this frame.
    pub fn kind(&self) -> FrameKind {
        match self {
            Self::CotXml(_) => FrameKind::CotXml,
            Self::TakProtobuf(_) => FrameKind::TakProtobuf,
        }
    }

    /// The payload bytes.
    pub fn payload(&self) -> &Bytes {
        match self {
            Self::CotXml(b) | Self::TakProtobuf(b) => b,
        }
    }

    /// Payload length in bytes.
    pub fn len(&self) -> usize {
        self.payload().len()
    }

    /// Whether the payload is empty.
    pub fn is_empty(&self) -> bool {
        self.payload().is_empty()
    }

    /// A CoT XML frame from a string.
    pub fn xml(s: impl Into<String>) -> Self {
        Self::CotXml(Bytes::from(s.into()))
    }

    /// The XML payload as `&str`, if this is a well-formed UTF-8 XML frame.
    pub fn as_xml_str(&self) -> Option<&str> {
        match self {
            Self::CotXml(b) => std::str::from_utf8(b).ok(),
            Self::TakProtobuf(_) => None,
        }
    }

    /// Encode for a datagram transport (UDP mesh): protobuf frames get the
    /// `BF 01 BF` prefix, XML is sent as-is.
    pub fn encode_datagram(&self) -> Bytes {
        match self {
            Self::CotXml(b) => b.clone(),
            Self::TakProtobuf(b) => {
                let mut out = BytesMut::with_capacity(3 + b.len());
                out.put_slice(&TAK_PROTO_MESH_MAGIC);
                out.put_slice(b);
                out.freeze()
            }
        }
    }

    /// Decode a whole datagram.
    pub fn decode_datagram(mut datagram: Bytes, max_bytes: usize) -> Result<Self, NetworkError> {
        if datagram.len() > max_bytes {
            return Err(NetworkError::FrameTooLarge {
                max: max_bytes,
                actual: datagram.len(),
            });
        }
        if datagram.starts_with(&TAK_PROTO_MESH_MAGIC) {
            datagram.advance(3);
            return Ok(Self::TakProtobuf(datagram));
        }
        if datagram.first() == Some(&TAK_PROTO_MAGIC) {
            // Some senders use stream framing on UDP; tolerate it.
            let mut dec = StreamDecoder::new(max_bytes);
            dec.extend(&datagram);
            return dec.next_frame()?.ok_or_else(|| {
                NetworkError::InvalidFrame("truncated TAK Protocol datagram".into())
            });
        }
        let text = std::str::from_utf8(&datagram).map_err(|_| {
            NetworkError::InvalidFrame("datagram is neither UTF-8 XML nor TAK Protocol".into())
        })?;
        if text.trim_start().starts_with('<') {
            Ok(Self::CotXml(datagram))
        } else {
            Err(NetworkError::InvalidFrame(
                "datagram does not start with '<'".into(),
            ))
        }
    }
}

/// Encodes frames for a stream connection.
#[derive(Clone, Copy, Debug, Default)]
pub struct StreamEncoder;

impl StreamEncoder {
    /// Produce the bytes to write for `frame`.
    pub fn encode(&self, frame: &Frame) -> Bytes {
        match frame {
            Frame::CotXml(b) if b.last() == Some(&b'\n') => b.clone(),
            Frame::CotXml(b) => {
                // TAK Server tolerates back-to-back documents, but a newline
                // separator keeps logs readable and matches ATAK's output.
                let mut out = BytesMut::with_capacity(b.len() + 1);
                out.put_slice(b);
                out.put_u8(b'\n');
                out.freeze()
            }
            Frame::TakProtobuf(b) => {
                let mut out = BytesMut::with_capacity(b.len() + 11);
                out.put_u8(TAK_PROTO_MAGIC);
                put_varint(&mut out, b.len() as u64);
                out.put_slice(b);
                out.freeze()
            }
        }
    }
}

/// Incrementally splits a byte stream into [`Frame`]s.
///
/// Feed bytes with [`StreamDecoder::extend`] and drain frames with
/// [`StreamDecoder::next_frame`] until it returns `Ok(None)`.
#[derive(Debug)]
pub struct StreamDecoder {
    buf: BytesMut,
    max_bytes: usize,
    xml_scan_from: usize,
}

impl StreamDecoder {
    /// A decoder that rejects frames larger than `max_bytes`.
    pub fn new(max_bytes: usize) -> Self {
        Self {
            buf: BytesMut::with_capacity(8 * 1024),
            max_bytes,
            xml_scan_from: 0,
        }
    }

    /// Append received bytes.
    pub fn extend(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// Mutable access to the internal buffer for zero-copy reads
    /// (`AsyncReadExt::read_buf`).
    pub fn buffer_mut(&mut self) -> &mut BytesMut {
        &mut self.buf
    }

    /// Bytes buffered but not yet framed.
    pub fn pending(&self) -> usize {
        self.buf.len()
    }

    /// Try to extract the next complete frame.
    ///
    /// Returns `Ok(None)` when more bytes are needed. Errors are sticky in the
    /// sense that the offending bytes stay in the buffer; callers should drop
    /// the connection.
    pub fn next_frame(&mut self) -> Result<Option<Frame>, NetworkError> {
        // Skip inter-document whitespace / NULs that some senders emit.
        let skip = self
            .buf
            .iter()
            .take_while(|b| b.is_ascii_whitespace() || **b == 0)
            .count();
        if skip > 0 {
            self.buf.advance(skip);
            self.xml_scan_from = 0;
        }
        let Some(&first) = self.buf.first() else {
            return Ok(None);
        };
        if first == TAK_PROTO_MAGIC {
            return self.next_proto_frame();
        }
        if first == b'<' || first == 0xEF {
            // '<' or a UTF-8 BOM (EF BB BF)
            return self.next_xml_frame();
        }
        Err(NetworkError::InvalidFrame(format!(
            "unexpected leading byte 0x{first:02X}; expected '<' or 0xBF"
        )))
    }

    fn next_proto_frame(&mut self) -> Result<Option<Frame>, NetworkError> {
        let Some((len, header)) = get_varint(&self.buf[1..])? else {
            if self.buf.len() > 11 {
                return Err(NetworkError::InvalidFrame(
                    "TAK Protocol varint too long".into(),
                ));
            }
            return Ok(None);
        };
        let len = usize::try_from(len).map_err(|_| NetworkError::FrameTooLarge {
            max: self.max_bytes,
            actual: usize::MAX,
        })?;
        if len > self.max_bytes {
            return Err(NetworkError::FrameTooLarge {
                max: self.max_bytes,
                actual: len,
            });
        }
        let total = 1 + header + len;
        if self.buf.len() < total {
            return Ok(None);
        }
        self.buf.advance(1 + header);
        let payload = self.buf.split_to(len).freeze();
        Ok(Some(Frame::TakProtobuf(payload)))
    }

    fn next_xml_frame(&mut self) -> Result<Option<Frame>, NetworkError> {
        let start = self.xml_scan_from.min(self.buf.len());
        let Some(pos) = find(&self.buf[start..], EVENT_END) else {
            if self.buf.len() > self.max_bytes {
                return Err(NetworkError::FrameTooLarge {
                    max: self.max_bytes,
                    actual: self.buf.len(),
                });
            }
            // Resume scanning just before the tail so a terminator split
            // across reads is still found.
            self.xml_scan_from = self.buf.len().saturating_sub(EVENT_END.len());
            return Ok(None);
        };
        let end = start + pos + EVENT_END.len();
        if end > self.max_bytes {
            return Err(NetworkError::FrameTooLarge {
                max: self.max_bytes,
                actual: end,
            });
        }
        let doc = self.buf.split_to(end).freeze();
        self.xml_scan_from = 0;
        Ok(Some(Frame::CotXml(doc)))
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Write a protobuf base-128 varint.
pub fn put_varint(out: &mut BytesMut, mut v: u64) {
    loop {
        let byte = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.put_u8(byte);
            return;
        }
        out.put_u8(byte | 0x80);
    }
}

/// Read a protobuf varint. Returns `(value, bytes_consumed)`, or `None` if
/// more input is needed.
pub fn get_varint(buf: &[u8]) -> Result<Option<(u64, usize)>, NetworkError> {
    let mut value: u64 = 0;
    for (i, byte) in buf.iter().enumerate() {
        if i >= 10 {
            return Err(NetworkError::InvalidFrame(
                "varint longer than 10 bytes".into(),
            ));
        }
        value |= u64::from(byte & 0x7F) << (7 * i);
        if byte & 0x80 == 0 {
            return Ok(Some((value, i + 1)));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EV1: &str = r#"<event version="2.0" uid="a" type="t-x-c-t" time="2024-05-01T12:00:00Z" start="2024-05-01T12:00:00Z" stale="2024-05-01T12:01:00Z"><point lat="0" lon="0" hae="0" ce="0" le="0"/><detail/></event>"#;
    const EV2: &str = r#"<?xml version="1.0"?><event version="2.0" uid="b" type="t-x-c-t" time="2024-05-01T12:00:00Z" start="2024-05-01T12:00:00Z" stale="2024-05-01T12:01:00Z"><point lat="0" lon="0" hae="0" ce="0" le="0"/><detail><remarks>&lt;/event&gt; is escaped so it does not terminate</remarks></detail></event>"#;

    fn drain(dec: &mut StreamDecoder) -> Vec<Frame> {
        let mut out = Vec::new();
        while let Some(f) = dec.next_frame().unwrap() {
            out.push(f);
        }
        out
    }

    #[test]
    fn splits_concatenated_xml_documents() {
        let mut dec = StreamDecoder::new(1 << 20);
        dec.extend(format!("{EV1}\n{EV2}\r\n\0").as_bytes());
        let frames = drain(&mut dec);
        assert_eq!(frames, vec![Frame::xml(EV1), Frame::xml(EV2)]);
        assert_eq!(dec.pending(), 0);
    }

    #[test]
    fn reassembles_xml_split_at_every_byte() {
        let stream = format!("{EV1}{EV2}");
        for cut in 1..stream.len() {
            let mut dec = StreamDecoder::new(1 << 20);
            dec.extend(&stream.as_bytes()[..cut]);
            let mut frames = drain(&mut dec);
            dec.extend(&stream.as_bytes()[cut..]);
            frames.extend(drain(&mut dec));
            assert_eq!(frames.len(), 2, "cut at {cut}");
            assert_eq!(frames[0], Frame::xml(EV1));
            assert_eq!(frames[1], Frame::xml(EV2));
        }
    }

    #[test]
    fn proto_frames_round_trip_and_interleave_with_xml() {
        let enc = StreamEncoder;
        let payload = Bytes::from_static(&[1, 2, 3, 4, 5]);
        let big = Bytes::from(vec![0xAB; 300]);
        let mut stream = BytesMut::new();
        stream.extend_from_slice(&enc.encode(&Frame::xml(EV1)));
        stream.extend_from_slice(&enc.encode(&Frame::TakProtobuf(payload.clone())));
        stream.extend_from_slice(&enc.encode(&Frame::TakProtobuf(big.clone())));
        stream.extend_from_slice(&enc.encode(&Frame::xml(EV2)));
        // 300 needs a two-byte varint
        assert_eq!(
            &stream[EV1.len() + 1 + 7..EV1.len() + 1 + 7 + 3],
            &[0xBF, 0xAC, 0x02]
        );

        for cut in [
            1usize,
            5,
            50,
            EV1.len() + 3,
            EV1.len() + 9,
            stream.len() - 1,
        ] {
            let mut dec = StreamDecoder::new(1 << 20);
            dec.extend(&stream[..cut]);
            let mut frames = drain(&mut dec);
            dec.extend(&stream[cut..]);
            frames.extend(drain(&mut dec));
            assert_eq!(
                frames,
                vec![
                    Frame::xml(EV1),
                    Frame::TakProtobuf(payload.clone()),
                    Frame::TakProtobuf(big.clone()),
                    Frame::xml(EV2)
                ],
                "cut at {cut}"
            );
        }
    }

    #[test]
    fn enforces_size_limit() {
        let mut dec = StreamDecoder::new(64);
        dec.extend(EV1.as_bytes());
        assert!(matches!(
            dec.next_frame(),
            Err(NetworkError::FrameTooLarge { .. })
        ));

        let mut dec = StreamDecoder::new(64);
        dec.extend(&[0xBF, 0x80, 0x01]); // declares 128 bytes
        assert!(matches!(
            dec.next_frame(),
            Err(NetworkError::FrameTooLarge {
                max: 64,
                actual: 128
            })
        ));

        // unterminated XML that keeps growing is rejected once over the limit
        let mut dec = StreamDecoder::new(64);
        dec.extend(&[b'<'; 65]);
        assert!(matches!(
            dec.next_frame(),
            Err(NetworkError::FrameTooLarge { .. })
        ));
    }

    #[test]
    fn rejects_garbage_leading_bytes() {
        let mut dec = StreamDecoder::new(1 << 20);
        dec.extend(b"GET / HTTP/1.1\r\n");
        assert!(matches!(
            dec.next_frame(),
            Err(NetworkError::InvalidFrame(_))
        ));
        let mut dec = StreamDecoder::new(1 << 20);
        dec.extend(&[
            0xBF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        ]);
        assert!(matches!(
            dec.next_frame(),
            Err(NetworkError::InvalidFrame(_))
        ));
    }

    #[test]
    fn partial_input_waits() {
        let mut dec = StreamDecoder::new(1 << 20);
        assert!(dec.next_frame().unwrap().is_none());
        dec.extend(&[0xBF]);
        assert!(dec.next_frame().unwrap().is_none());
        dec.extend(&[0x05, 1, 2]);
        assert!(dec.next_frame().unwrap().is_none());
        dec.extend(&[3, 4, 5]);
        assert_eq!(
            dec.next_frame().unwrap(),
            Some(Frame::TakProtobuf(Bytes::from_static(&[1, 2, 3, 4, 5])))
        );
    }

    #[test]
    fn datagram_framing() {
        let p = Frame::TakProtobuf(Bytes::from_static(b"pb"));
        let d = p.encode_datagram();
        assert_eq!(&d[..], &[0xBF, 0x01, 0xBF, b'p', b'b']);
        assert_eq!(Frame::decode_datagram(d, 1024).unwrap(), p);
        let x = Frame::xml(EV1);
        assert_eq!(
            Frame::decode_datagram(x.encode_datagram(), 1024).unwrap(),
            x
        );
        assert!(Frame::decode_datagram(Bytes::from_static(b"nope"), 1024).is_err());
        assert!(Frame::decode_datagram(Bytes::from_static(&[0xFF, 0xFE]), 1024).is_err());
        assert!(matches!(
            Frame::decode_datagram(Bytes::from(vec![b'<'; 2000]), 1024),
            Err(NetworkError::FrameTooLarge { .. })
        ));
    }

    #[test]
    fn varint_round_trip() {
        for v in [
            0u64,
            1,
            127,
            128,
            300,
            16_383,
            16_384,
            u64::from(u32::MAX),
            u64::MAX,
        ] {
            let mut b = BytesMut::new();
            put_varint(&mut b, v);
            assert_eq!(get_varint(&b).unwrap(), Some((v, b.len())));
        }
        assert_eq!(get_varint(&[0x80]).unwrap(), None);
    }
}
