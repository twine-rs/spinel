use crate::{Error, Frame};
use bytes::{BufMut, Bytes, BytesMut};
use crc16::State;

#[derive(Debug, PartialEq)]
pub struct HdlcLiteFrame {
    spinel_frame: Frame,
}

impl HdlcLiteFrame {
    const FRAME_DELIMITER_FLAG: u8 = 0x7E;
    const ESCAPE_BYTE_FLAG: u8 = 0x7D;
    const XON: u8 = 0x11;
    const XOFF: u8 = 0x13;
    const VENDOR_SPECIFIC: u8 = 0xF8;

    /// Check if a byte requires escaping.
    fn requires_escape(byte: u8) -> bool {
        byte == Self::FRAME_DELIMITER_FLAG
            || byte == Self::ESCAPE_BYTE_FLAG
            || byte == Self::XON
            || byte == Self::XOFF
            || byte == Self::VENDOR_SPECIFIC
    }

    /// Find the [`HdlcLiteFrame`] delimiter.
    ///
    /// This is useful for syncronizing the HDLC frames before begining to
    /// process the frame (generally when decoding).
    ///
    /// Returns the positional index of the frame delimiter. The next index
    /// position is the start of the next [`HdlcLiteFrame`].
    #[inline]
    pub fn find_frame_delimiter(bytes: &Bytes) -> Option<usize> {
        bytes
            .iter()
            .position(|&byte| byte == Self::FRAME_DELIMITER_FLAG)
    }

    /// Find an [`HdlcLiteFrame`] that is surrounded by delimiters in a buffer.
    ///
    /// Returns the start and end positions in the buffer that mark the frame.
    /// Returns `None` if no full frame is found.
    #[inline]
    pub fn find_frame(bytes: &Bytes) -> Option<(usize, usize)> {
        // Find first frame delimiter
        let mut first_delimiter_pos = Self::find_frame_delimiter(bytes)?;
        let mut next_delimiter_pos: Option<usize> = None;

        // Search the buffer for the next frame delimiter, returning `None` if no other delimiter is found.
        for _ in 0..bytes.len() {
            // Track the position in the original buffer to pass back to the calling function by adding the first
            // position to the result. This is done because the buffer is being split and the position is relative
            // to the split.
            //
            // Add 1 to the position split to make sure any original frame delimiter is not included in the new
            // search when calculating the next position.
            let split_pos = first_delimiter_pos + 1;
            let next = Self::find_frame_delimiter(&bytes.clone().split_off(split_pos))? + split_pos;

            // Peek ahead to see if the next byte is also frame delimiter
            if next == split_pos {
                // Found a repeated frame delimiter, move the delimiter position forward
                first_delimiter_pos = next;
                continue;
            } else {
                // Otherwise, this is the full frame
                next_delimiter_pos = Some(next);
                break;
            }
        }

        // Determine if an end delimiter was found.
        // Note: `next` shouldn't necessarily end up returning `None` because the loop should always find a delimiter.
        let next = match next_delimiter_pos {
            Some(pos) => pos,
            None => return None,
        };

        Some((first_delimiter_pos, next))
    }

    /// Create a new [`HdlcLiteFrame`] from a standard Spinel [`Frame`].
    pub fn new(frame: Frame) -> Self {
        Self {
            spinel_frame: frame,
        }
    }

    /// Encode a [`HdlcLiteFrame`] into a mutable buffer of [`BytesMut`].
    /// todo: limit?
    pub fn encode(self, buffer: &mut BytesMut) -> Result<(), Error> {
        // todo: check for escape, new BytesMut first then write to input buffer

        buffer.put_u8(Self::FRAME_DELIMITER_FLAG);
        self.spinel_frame.encode(buffer)?;
        let crc = State::<crc16::X_25>::calculate(&buffer[1..]);
        buffer.put_u16_le(crc);
        buffer.put_u8(Self::FRAME_DELIMITER_FLAG);

        Ok(())
    }

    /// Decode a [`HdlcLiteFrame`] from a buffer of [`Bytes`].
    ///
    /// This function expects an aligned frame in the bytes buffer, including delimiters and CRC.
    /// It is the responsibility of the caller to ensure that the data stream is syncronized and
    /// the frame is complete before calling this function.
    pub fn decode(bytes: &Bytes) -> Result<Self, Error> {
        if let Some(f) = bytes.first() {
            if *f != Self::FRAME_DELIMITER_FLAG {
                return Err(Error::HdlcStartDelimiter(*f));
            }
        }

        // Starting delimiter has been checked, shadow define bytes to remove it from CRC
        let bytes = bytes.clone().split_off(1);

        if let Some(l) = bytes.last() {
            if *l != Self::FRAME_DELIMITER_FLAG {
                return Err(Error::HdlcEndDelimiter(*l));
            }
        }

        let mut packet = BytesMut::new();

        // Iterate over the bytes and escape any that require it
        let mut need_escape = false;
        for byte in bytes.iter() {
            if Self::requires_escape(*byte) {
                // Byte requires fixing an escape code or the end of the packet has been reached.
                // Note: The final delimiter is not included in the packet.
                need_escape = true;
                continue;
            }

            // CRC bytes are also escaped and need correction
            let mut byte_to_write = *byte;
            if need_escape {
                byte_to_write ^= 0x20;
                need_escape = false;
            }

            packet.put_u8(byte_to_write);
        }

        // Split the payload and end of frame data
        let pkt_len = packet.len();
        let end_frame_data = packet.split_off(pkt_len - 2);

        let pkt_crc = u16::from_le_bytes([end_frame_data[0], end_frame_data[1]]);
        let calculated_crc = State::<crc16::X_25>::calculate(&packet);

        if calculated_crc != pkt_crc {
            return Err(Error::HdlcChecksum(calculated_crc));
        }

        let frozen = packet.freeze();
        let spinel_frame = Frame::decode(&frozen)?;

        Ok(Self { spinel_frame })
    }

    pub fn into_inner(self) -> Frame {
        self.spinel_frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Property;
    use crate::{Command, Header};
    use bytes::Bytes;
    use rand::distributions::Uniform;

    pub(crate) const TEST_DESYNC_STR: [u8; 24] = [
        0xc2, 0x5a, 0xa7, 0xaf, 0x97, 0xb1, 0x54, 0x99, 0x2b, 0xf5, 0x06, 0xe5, 0x7b, 0x5d, 0xdc,
        0x8d, 0x24, 0x81, 0x3f, 0x7e, 0x7e, 0x80, 0x06, 0x73,
    ];

    // Noop
    pub(crate) const TEST_REQ_NOOP_ARRAY: [u8; 6] = [0x7e, 0x81, 0x00, 0x53, 0x9a, 0x7e];
    pub(crate) const TEST_REQ_NCP_VERSION_ARRAY: [u8; 7] =
        [0x7e, 0x81, 0x02, 0x02, 0x5e, 0x80, 0x7e];

    // NCP Version
    pub(crate) const TEST_RESP_NCP_VERSION_ARRAY: [u8; 91] = [
        0x7e, 0x81, 0x06, 0x02, 0x4f, 0x50, 0x45, 0x4e, 0x54, 0x48, 0x52, 0x45, 0x41, 0x44, 0x2f,
        0x74, 0x68, 0x72, 0x65, 0x61, 0x64, 0x2d, 0x72, 0x65, 0x66, 0x65, 0x72, 0x65, 0x6e, 0x63,
        0x65, 0x2d, 0x32, 0x30, 0x32, 0x33, 0x30, 0x37, 0x30, 0x36, 0x2d, 0x33, 0x38, 0x30, 0x2d,
        0x67, 0x62, 0x39, 0x64, 0x63, 0x64, 0x62, 0x63, 0x61, 0x34, 0x3b, 0x20, 0x4e, 0x52, 0x46,
        0x35, 0x32, 0x38, 0x34, 0x30, 0x3b, 0x20, 0x4d, 0x61, 0x72, 0x20, 0x20, 0x31, 0x20, 0x32,
        0x30, 0x32, 0x34, 0x20, 0x31, 0x36, 0x3a, 0x31, 0x32, 0x3a, 0x32, 0x38, 0x00, 0x05, 0xc4,
        0x7e,
    ];
    pub(crate) const TEST_RESP_NCP_VERSION_STR: &str =
        "OPENTHREAD/thread-reference-20230706-380-gb9dcdbca4; NRF52840; Mar  1 2024 16:12:28\0";

    // Stream
    const TEST_HDLC_DECODE_STREAM: [u8; 96] = [
        0x7e, 0x80, 0x06, 0x73, 0x54, 0x00, 0x60, 0x00, 0x00, 0x00, 0x00, 0x2c, 0x7d, 0x31, 0xff,
        0xfe, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xb4, 0x0f, 0x00, 0xb3, 0x98, 0x60, 0x22,
        0x52, 0xff, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x02, 0x4d, 0x4c, 0x4d, 0x4c, 0x00, 0x2c, 0x1a, 0x25, 0x00, 0x15, 0x10, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x65, 0x7d, 0x5d, 0x91, 0xac, 0x2d, 0x26, 0x35, 0x78,
        0x62, 0x34, 0x7d, 0x31, 0xce, 0xb6, 0x0a, 0x4c, 0x88, 0x41, 0xd8, 0xfa, 0xe3, 0xd6, 0x03,
        0xab, 0xae, 0x3a, 0x68, 0xb3, 0x7e,
    ];

    #[test]
    fn find_frame_delimiter() {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let range = Uniform::new(0, 255);

        // Generate a some what random vector and make sure it does not contain
        // the frame delimiter
        let mut test_vector: Vec<u8> = (0..512)
            .map(|_| rng.sample(&range))
            .map(|b| if b == 0x7E { b + 1 } else { b })
            .collect();

        // Insert a frame delimiter at a random position
        let random_index = rng.gen_range(0..test_vector.len());
        test_vector[random_index] = 0x7E;

        // Determine if the frame delimiter is found in the correct position
        let bytes = Bytes::from(test_vector);
        let index = HdlcLiteFrame::find_frame_delimiter(&bytes);
        assert_eq!(index, Some(random_index));
    }

    #[test]
    fn finds_frame_in_misaligned_buffer() {
        let bytes = [0x09, 0x27, 0x7e, 0x81, 0x00, 0x53, 0x9a, 0x7e, 0x11, 0x23];
        let result = HdlcLiteFrame::find_frame(&Bytes::from_iter(bytes.iter().cloned()));
        assert_eq!(result, Some((2, 7)));
    }

    #[test]
    fn errors_on_incorrect_checksum() {
        let mut bytes = BytesMut::from_iter(TEST_REQ_NOOP_ARRAY.iter().cloned());
        let test = bytes.get_mut(4).unwrap();
        *test = 0x00;

        assert_eq!(
            HdlcLiteFrame::decode(&bytes.freeze()),
            Err(Error::HdlcChecksum(0x9A53))
        );
    }

    #[test]
    fn errors_on_missing_delimiter() {
        let bytes = [0x7E, 0x7D, 0x11, 0x13, 0xF8, 0x7E];
        let missing_start = Bytes::copy_from_slice(&bytes[1..]);
        let test = HdlcLiteFrame::decode(&missing_start);
        assert_eq!(test, Err(Error::HdlcStartDelimiter(0x7D)));

        let missing_end = Bytes::copy_from_slice(&bytes[..5]);
        let test = HdlcLiteFrame::decode(&missing_end);
        assert_eq!(test, Err(Error::HdlcEndDelimiter(0xF8)));
    }

    #[test]
    fn requires_escape() {
        let escape_bytes = [0x7E, 0x7D, 0x11, 0x13, 0xF8];
        for byte in escape_bytes.iter() {
            let escape = HdlcLiteFrame::requires_escape(*byte);
            assert_eq!(escape, true);
        }
    }

    #[test]
    fn find_frame_returns_none_on_desync() {
        let bytes = Bytes::from_static(&TEST_DESYNC_STR);
        let frame = HdlcLiteFrame::find_frame(&bytes);
        assert_eq!(frame, None);
    }

    #[test]
    fn encode_noop() {
        let header = Header::new(0x00, 0x01);
        let cmd = Command::Noop;
        let spinel_frame = Frame::new(header, cmd);
        let hdlc_frame = HdlcLiteFrame::new(spinel_frame);

        let mut buffer = BytesMut::with_capacity(32);
        hdlc_frame.encode(&mut buffer).unwrap();
        assert_eq!(buffer, Bytes::from_static(&TEST_REQ_NOOP_ARRAY));
    }

    #[test]
    fn decode_noop() {
        let bytes = Bytes::from_static(&TEST_REQ_NOOP_ARRAY);
        let frame = HdlcLiteFrame::decode(&bytes);
        let expected = Frame::new(Header::new(0x00, 0x01), Command::Noop);
        assert_eq!(frame, Ok(HdlcLiteFrame::new(expected)));
    }

    #[test]
    fn encode_property_get_ncp_version() {
        let header = Header::new(0x00, 0x01);
        let cmd = Command::PropertyValueGet(Property::NcpVersion);
        let spinel_frame = Frame::new(header, cmd);

        let hdlc_frame = HdlcLiteFrame::new(spinel_frame);
        let mut buffer = BytesMut::with_capacity(4096);
        hdlc_frame.encode(&mut buffer).unwrap();
        println!("{buffer:02x?}");
        // assert_eq!(encoded, Ok(HdlcEncodedBytes::new([0x7e, 0x01, 0x02, 0x7e])))
        assert_eq!(buffer, Bytes::from_static(&TEST_REQ_NCP_VERSION_ARRAY));
    }

    #[test]
    fn decode_property_get_ncp_version() {
        let bytes = Bytes::from_static(&TEST_REQ_NCP_VERSION_ARRAY);
        let frame = HdlcLiteFrame::decode(&bytes);
        let expected = HdlcLiteFrame::new(Frame::new(
            Header::new(0x00, 0x01),
            Command::PropertyValueGet(Property::NcpVersion),
        ));
        assert_eq!(frame, Ok(expected));
    }

    #[test]
    fn decode_ncp_version_property_is() {
        let bytes = Bytes::from_static(&TEST_RESP_NCP_VERSION_ARRAY);
        let frame = HdlcLiteFrame::decode(&bytes);
        let expected = HdlcLiteFrame::new(Frame::new(
            Header::new(0x00, 0x01),
            Command::PropertyValueIs(
                Property::NcpVersion,
                Bytes::from_static(TEST_RESP_NCP_VERSION_STR.as_bytes()),
            ),
        ));
        assert_eq!(frame, Ok(expected));
    }

    #[test]
    fn encode_ncp_version_property_is() {
        let header = Header::new(0x00, 0x01);
        let cmd = Command::PropertyValueIs(
            Property::NcpVersion,
            Bytes::from_static(TEST_RESP_NCP_VERSION_STR.as_bytes()),
        );
        let spinel_frame = Frame::new(header, cmd);
        let hdlc_frame = HdlcLiteFrame::new(spinel_frame);
        let mut buffer = BytesMut::with_capacity(4096);
        hdlc_frame.encode(&mut buffer).unwrap();
        assert_eq!(buffer, Bytes::from_static(&TEST_RESP_NCP_VERSION_ARRAY));
    }

    #[test]
    fn decode_stream() {
        let bytes = Bytes::from_static(&TEST_HDLC_DECODE_STREAM);
        println!("bytes: {:02x?}", &bytes[..]);
        let frame = HdlcLiteFrame::decode(&bytes);
        assert!(frame.is_ok());
        // todo: assert frame is stream
    }
}
