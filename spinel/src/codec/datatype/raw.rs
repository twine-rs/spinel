use crate::{Error, PackedU32};
use bytes::{BufMut, Bytes, BytesMut};

/// Host-to-RCP raw 802.15.4 transmit request: the payload of a `PropertyValueSet` on
/// [`Property::Stream`](crate::Property::Stream)([`PropertyStream::Raw`](crate::PropertyStream::Raw)).
///
/// Matches OpenThread's `SPINEL_PROP_STREAM_RAW` wire format (and ziggurat's
/// `SpinelTxFrame`): a `u16` little-endian PSDU-length prefix, the PSDU, then a set of
/// *contiguous* optional transmit parameters -- if one is omitted, every field after it
/// must be omitted too, since there is nothing on the wire to mark which field a given
/// byte belongs to other than its position.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RawTxFrame {
    pub psdu: Bytes,
    pub channel: Option<u8>,
    pub max_csma_backoffs: Option<u8>,
    pub max_frame_retries: Option<u8>,
    pub enable_csma_ca: Option<bool>,
    pub is_header_updated: Option<bool>,
    pub is_a_retransmit: Option<bool>,
    pub is_security_processed: Option<bool>,
    pub tx_delay: Option<u32>,
    pub tx_delay_base_time: Option<u32>,
    pub rx_channel_after_tx: Option<u8>,
    pub tx_power: Option<i8>,
}

impl RawTxFrame {
    /// A bare PSDU with no transmit parameters -- the common case.
    pub fn new(psdu: Bytes) -> Self {
        Self {
            psdu,
            ..Default::default()
        }
    }

    /// Encode this request and append it to `buffer`.
    ///
    /// Stops at the first unset optional field, per the format's contiguity rule.
    pub fn encode(&self, buffer: &mut BytesMut) {
        buffer.put_u16_le(self.psdu.len() as u16);
        buffer.put_slice(&self.psdu);

        let Some(channel) = self.channel else {
            return;
        };
        buffer.put_u8(channel);

        let Some(max_csma_backoffs) = self.max_csma_backoffs else {
            return;
        };
        buffer.put_u8(max_csma_backoffs);

        let Some(max_frame_retries) = self.max_frame_retries else {
            return;
        };
        buffer.put_u8(max_frame_retries);

        let Some(enable_csma_ca) = self.enable_csma_ca else {
            return;
        };
        buffer.put_u8(enable_csma_ca as u8);

        let Some(is_header_updated) = self.is_header_updated else {
            return;
        };
        buffer.put_u8(is_header_updated as u8);

        let Some(is_a_retransmit) = self.is_a_retransmit else {
            return;
        };
        buffer.put_u8(is_a_retransmit as u8);

        let Some(is_security_processed) = self.is_security_processed else {
            return;
        };
        buffer.put_u8(is_security_processed as u8);

        let Some(tx_delay) = self.tx_delay else {
            return;
        };
        buffer.put_u32_le(tx_delay);

        let Some(tx_delay_base_time) = self.tx_delay_base_time else {
            return;
        };
        buffer.put_u32_le(tx_delay_base_time);

        let Some(rx_channel_after_tx) = self.rx_channel_after_tx else {
            return;
        };
        buffer.put_u8(rx_channel_after_tx);

        let Some(tx_power) = self.tx_power else {
            return;
        };
        buffer.put_u8(tx_power as u8);
    }

    pub fn to_bytes(&self) -> Bytes {
        let mut buffer = BytesMut::new();
        self.encode(&mut buffer);
        buffer.freeze()
    }

    /// Decode a request. Only the PSDU-length prefix and PSDU itself are required;
    /// parsing of the optional trailing parameters simply stops at the first one that
    /// doesn't fit; the format's contiguity rule means a short tail can only mean
    /// "these weren't sent", not "malformed".
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < 2 {
            return Err(Error::PacketLength(bytes.len()));
        }

        let psdu_len = u16::from_le_bytes([bytes[0], bytes[1]]) as usize;
        let mut offset = 2;

        if bytes.len() < offset + psdu_len {
            return Err(Error::PacketLength(bytes.len()));
        }

        let mut frame = Self::new(Bytes::copy_from_slice(&bytes[offset..offset + psdu_len]));
        offset += psdu_len;

        let Some(&channel) = bytes.get(offset) else {
            return Ok(frame);
        };
        frame.channel = Some(channel);
        offset += 1;

        let Some(&max_csma_backoffs) = bytes.get(offset) else {
            return Ok(frame);
        };
        frame.max_csma_backoffs = Some(max_csma_backoffs);
        offset += 1;

        let Some(&max_frame_retries) = bytes.get(offset) else {
            return Ok(frame);
        };
        frame.max_frame_retries = Some(max_frame_retries);
        offset += 1;

        let Some(&enable_csma_ca) = bytes.get(offset) else {
            return Ok(frame);
        };
        frame.enable_csma_ca = Some(enable_csma_ca != 0);
        offset += 1;

        let Some(&is_header_updated) = bytes.get(offset) else {
            return Ok(frame);
        };
        frame.is_header_updated = Some(is_header_updated != 0);
        offset += 1;

        let Some(&is_a_retransmit) = bytes.get(offset) else {
            return Ok(frame);
        };
        frame.is_a_retransmit = Some(is_a_retransmit != 0);
        offset += 1;

        let Some(&is_security_processed) = bytes.get(offset) else {
            return Ok(frame);
        };
        frame.is_security_processed = Some(is_security_processed != 0);
        offset += 1;

        let Some(tx_delay_bytes) = bytes.get(offset..offset + 4) else {
            return Ok(frame);
        };
        frame.tx_delay = Some(u32::from_le_bytes(tx_delay_bytes.try_into().unwrap()));
        offset += 4;

        let Some(tx_delay_base_time_bytes) = bytes.get(offset..offset + 4) else {
            return Ok(frame);
        };
        frame.tx_delay_base_time = Some(u32::from_le_bytes(
            tx_delay_base_time_bytes.try_into().unwrap(),
        ));
        offset += 4;

        let Some(&rx_channel_after_tx) = bytes.get(offset) else {
            return Ok(frame);
        };
        frame.rx_channel_after_tx = Some(rx_channel_after_tx);
        offset += 1;

        let Some(&tx_power) = bytes.get(offset) else {
            return Ok(frame);
        };
        frame.tx_power = Some(tx_power as i8);

        Ok(frame)
    }
}

// Manual impl instead of `#[derive(defmt::Format)]`: `Bytes` doesn't implement
// `defmt::Format`, so `psdu` is formatted as a byte slice instead.
#[cfg(feature = "defmt")]
impl defmt::Format for RawTxFrame {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(
            fmt,
            "RawTxFrame {{ psdu: {=[u8]}, channel: {}, max_csma_backoffs: {}, max_frame_retries: {}, enable_csma_ca: {}, is_header_updated: {}, is_a_retransmit: {}, is_security_processed: {}, tx_delay: {}, tx_delay_base_time: {}, rx_channel_after_tx: {}, tx_power: {} }}",
            &self.psdu[..],
            self.channel,
            self.max_csma_backoffs,
            self.max_frame_retries,
            self.enable_csma_ca,
            self.is_header_updated,
            self.is_a_retransmit,
            self.is_security_processed,
            self.tx_delay,
            self.tx_delay_base_time,
            self.rx_channel_after_tx,
            self.tx_power,
        )
    }
}

/// RCP-to-Host raw 802.15.4 receive notification: the payload of a `PropertyValueIs` on
/// [`Property::Stream`](crate::Property::Stream)([`PropertyStream::Raw`](crate::PropertyStream::Raw)).
///
/// Matches OpenThread's `SPINEL_PROP_STREAM_RAW` receive format, as written by
/// `NcpBase::PackRadioFrame` and read by `RadioSpinel::ParseRadioFrame`:
///
/// ```text
/// u16 psdu_len, psdu
/// i8  rssi
/// i8  noise_floor
/// u16 flags
/// u16 len, { u8 channel, u8 lqi, u64 timestamp_us }   PHY-data
/// u16 len, { packed-uint receive_error }              Vendor-data
/// u16 len, { u8 ack_key_id, u32 ack_frame_counter }   MAC-data
/// ```
///
/// All integers are little-endian. Per `spinel.h`, metadata may be partially specified:
/// [`decode`](Self::decode) fills in defaults for any trailing fields that are absent,
/// and skips unknown bytes at the end of a length-prefixed struct.
#[derive(Clone, Debug, PartialEq)]
pub struct RawRxFrame {
    /// The received frame, including the FCS.
    pub psdu: Bytes,
    /// Received signal strength in dBm (default -128).
    pub rssi: i8,
    /// Noise floor in dBm (default -128). OpenThread's RCP always sends -128.
    pub noise_floor: i8,
    /// Metadata flags: 0x0001 transmitted rather than received, 0x0004 bad FCS, 0x0008 likely
    /// duplicate, 0x0010 acknowledged with frame pending set, 0x0020 acknowledged with a secure
    /// enhanced ACK.
    pub flags: u16,
    /// The 802.15.4 channel the frame was received on.
    pub channel: u8,
    /// The 802.15.4 link quality indicator (LQI).
    pub lqi: u8,
    /// Receive timestamp in microseconds.
    pub timestamp_us: u64,
    /// OpenThread `otError` the frame was received with; `0` means no error.
    pub receive_error: u32,
    /// Key ID from the security header of the ACK sent for this frame.
    pub ack_key_id: u8,
    /// Frame counter from the security header of the ACK sent for this frame.
    pub ack_frame_counter: u32,
}

impl RawRxFrame {
    /// `spinel.h`'s default for an unspecified RSSI or noise floor.
    pub const RSSI_UNKNOWN: i8 = -128;

    /// A received PSDU with every metadata field at its `spinel.h` default.
    pub fn new(psdu: Bytes) -> Self {
        Self {
            psdu,
            rssi: Self::RSSI_UNKNOWN,
            noise_floor: Self::RSSI_UNKNOWN,
            flags: 0,
            channel: 0,
            lqi: 0,
            timestamp_us: 0,
            receive_error: 0,
            ack_key_id: 0,
            ack_frame_counter: 0,
        }
    }

    pub fn encode(&self, buffer: &mut BytesMut) {
        buffer.put_u16_le(self.psdu.len() as u16);
        buffer.put_slice(&self.psdu);
        buffer.put_i8(self.rssi);
        buffer.put_i8(self.noise_floor);
        buffer.put_u16_le(self.flags);

        // PHY-data
        buffer.put_u16_le(1 + 1 + 8);
        buffer.put_u8(self.channel);
        buffer.put_u8(self.lqi);
        buffer.put_u64_le(self.timestamp_us);

        // Vendor-data
        buffer.put_u16_le(PackedU32::packed_len(self.receive_error) as u16);
        PackedU32::write_to_buffer(self.receive_error, buffer);

        // MAC-data
        buffer.put_u16_le(1 + 4);
        buffer.put_u8(self.ack_key_id);
        buffer.put_u32_le(self.ack_frame_counter);
    }

    pub fn to_bytes(&self) -> Bytes {
        let mut buffer = BytesMut::new();
        self.encode(&mut buffer);
        buffer.freeze()
    }

    /// Decode a notification. Only the PSDU-length prefix and PSDU are required; a
    /// metadata field that is cut off partway through, or a struct whose length prefix
    /// overruns the payload, is an error.
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let mut reader = Reader::new(bytes);

        let psdu_len = reader.u16()?.ok_or(Error::PacketLength(bytes.len()))? as usize;
        let psdu = reader
            .take(psdu_len)
            .ok_or(Error::PacketLength(bytes.len()))?;
        let mut frame = Self::new(Bytes::copy_from_slice(psdu));

        let Some(rssi) = reader.u8()? else {
            return Ok(frame);
        };
        frame.rssi = rssi as i8;

        let Some(noise_floor) = reader.u8()? else {
            return Ok(frame);
        };
        frame.noise_floor = noise_floor as i8;

        let Some(flags) = reader.u16()? else {
            return Ok(frame);
        };
        frame.flags = flags;

        let Some(mut phy) = reader.struct_()? else {
            return Ok(frame);
        };
        if let Some(channel) = phy.u8()? {
            frame.channel = channel;
            if let Some(lqi) = phy.u8()? {
                frame.lqi = lqi;
                if let Some(timestamp_us) = phy.u64()? {
                    frame.timestamp_us = timestamp_us;
                }
            }
        }

        let Some(vendor) = reader.struct_()? else {
            return Ok(frame);
        };
        if !vendor.is_empty() {
            frame.receive_error = PackedU32::decode_checked(vendor.rest())?.0;
        }

        let Some(mut mac) = reader.struct_()? else {
            return Ok(frame);
        };
        if let Some(ack_key_id) = mac.u8()? {
            frame.ack_key_id = ack_key_id;
            if let Some(ack_frame_counter) = mac.u32()? {
                frame.ack_frame_counter = ack_frame_counter;
            }
        }

        Ok(frame)
    }
}

/// Cursor over the optional, little-endian metadata fields of a [`RawRxFrame`].
///
/// Each read returns `Ok(None)` when the input is exhausted (the field was omitted) and
/// `Err` when only part of the field is present.
struct Reader<'a> {
    bytes: &'a [u8],
    total_len: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            total_len: bytes.len(),
        }
    }

    fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    fn rest(&self) -> &'a [u8] {
        self.bytes
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.bytes.len() < n {
            return None;
        }
        let (head, tail) = self.bytes.split_at(n);
        self.bytes = tail;
        Some(head)
    }

    fn array<const N: usize>(&mut self) -> Result<Option<[u8; N]>, Error> {
        if self.bytes.is_empty() {
            return Ok(None);
        }
        let head = self.take(N).ok_or(Error::PacketLength(self.total_len))?;
        Ok(Some(head.try_into().unwrap()))
    }

    fn u8(&mut self) -> Result<Option<u8>, Error> {
        Ok(self.array::<1>()?.map(|b| b[0]))
    }

    fn u16(&mut self) -> Result<Option<u16>, Error> {
        Ok(self.array()?.map(u16::from_le_bytes))
    }

    fn u32(&mut self) -> Result<Option<u32>, Error> {
        Ok(self.array()?.map(u32::from_le_bytes))
    }

    fn u64(&mut self) -> Result<Option<u64>, Error> {
        Ok(self.array()?.map(u64::from_le_bytes))
    }

    /// Read a `u16`-length-prefixed struct and return a reader over its contents.
    fn struct_(&mut self) -> Result<Option<Reader<'a>>, Error> {
        let Some(len) = self.u16()? else {
            return Ok(None);
        };
        let body = self
            .take(len as usize)
            .ok_or(Error::PacketLength(self.total_len))?;
        Ok(Some(Reader {
            bytes: body,
            total_len: self.total_len,
        }))
    }
}

// Manual impl instead of `#[derive(defmt::Format)]`: `Bytes` doesn't implement
// `defmt::Format`, so `psdu` is formatted as a byte slice instead.
#[cfg(feature = "defmt")]
impl defmt::Format for RawRxFrame {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(
            fmt,
            "RawRxFrame {{ psdu: {=[u8]}, rssi: {}, noise_floor: {}, flags: {}, channel: {}, lqi: {}, timestamp_us: {}, receive_error: {}, ack_key_id: {}, ack_frame_counter: {} }}",
            &self.psdu[..],
            self.rssi,
            self.noise_floor,
            self.flags,
            self.channel,
            self.lqi,
            self.timestamp_us,
            self.receive_error,
            self.ack_key_id,
            self.ack_frame_counter,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tx_frame_bare_psdu_round_trips() {
        let frame = RawTxFrame::new(Bytes::from_static(&[0xDE, 0xAD, 0xBE, 0xEF]));
        let bytes = frame.to_bytes();

        assert_eq!(&bytes[..], &[0x04, 0x00, 0xDE, 0xAD, 0xBE, 0xEF]);
        assert_eq!(RawTxFrame::decode(&bytes), Ok(frame));
    }

    #[test]
    fn tx_frame_all_params_round_trips() {
        let frame = RawTxFrame {
            psdu: Bytes::from_static(&[0xAA]),
            channel: Some(11),
            max_csma_backoffs: Some(4),
            max_frame_retries: Some(3),
            enable_csma_ca: Some(true),
            is_header_updated: Some(false),
            is_a_retransmit: Some(true),
            is_security_processed: Some(false),
            tx_delay: Some(1_000),
            tx_delay_base_time: Some(2_000),
            rx_channel_after_tx: Some(15),
            tx_power: Some(-4),
        };
        let bytes = frame.to_bytes();

        assert_eq!(RawTxFrame::decode(&bytes), Ok(frame));
    }

    #[test]
    fn tx_frame_partial_contiguous_params_round_trips() {
        let frame = RawTxFrame {
            psdu: Bytes::from_static(&[0xAA]),
            channel: Some(11),
            max_csma_backoffs: Some(4),
            ..Default::default()
        };
        let bytes = frame.to_bytes();

        assert_eq!(RawTxFrame::decode(&bytes), Ok(frame));
    }

    #[test]
    fn tx_frame_decode_fails_on_truncated_length_prefix() {
        assert_eq!(RawTxFrame::decode(&[0x01]), Err(Error::PacketLength(1)));
    }

    #[test]
    fn tx_frame_decode_fails_on_truncated_psdu() {
        // Claims a 4-byte PSDU but only provides 2.
        let bytes = [0x04, 0x00, 0xDE, 0xAD];
        assert_eq!(RawTxFrame::decode(&bytes), Err(Error::PacketLength(4)));
    }

    #[test]
    fn rx_frame_round_trips() {
        let frame = RawRxFrame {
            psdu: Bytes::from_static(&[0x01, 0x02, 0x03]),
            rssi: -70,
            noise_floor: -95,
            flags: 0x1234,
            channel: 15,
            lqi: 200,
            timestamp_us: 0x0102_0304_0506_0708,
            receive_error: 300,
            ack_key_id: 2,
            ack_frame_counter: 0xAABB_CCDD,
        };
        let bytes = frame.to_bytes();

        assert_eq!(RawRxFrame::decode(&bytes), Ok(frame));
    }

    #[test]
    fn rx_frame_pinned_byte_layout() {
        // Pins the wire layout against OpenThread's `NcpBase::PackRadioFrame` /
        // `RadioSpinel::ParseRadioFrame`, since a silent drift here breaks interop with
        // real Spinel hosts/RCPs without any local round-trip test noticing.
        let frame = RawRxFrame {
            psdu: Bytes::from_static(&[0xAB, 0xCD]),
            rssi: -1,
            noise_floor: -128,
            flags: 0x0001,
            channel: 20,
            lqi: 255,
            timestamp_us: 1,
            receive_error: 0,
            ack_key_id: 3,
            ack_frame_counter: 7,
        };

        #[rustfmt::skip]
        let expected: [u8; 30] = [
            0x02, 0x00, // psdu_len = 2 (LE)
            0xAB, 0xCD, // psdu
            0xFF, // rssi = -1
            0x80, // noise_floor = -128
            0x01, 0x00, // flags = 1 (LE)
            0x0A, 0x00, // PHY-data struct length = 10
            0x14, // channel = 20
            0xFF, // lqi = 255
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // timestamp_us = 1 (LE)
            0x01, 0x00, // Vendor-data struct length = 1
            0x00, // receive_error = 0 (packed)
            0x05, 0x00, // MAC-data struct length = 5
            0x03, // ack_key_id = 3
            0x07, 0x00, 0x00, 0x00, // ack_frame_counter = 7 (LE)
        ];
        assert_eq!(&frame.to_bytes()[..], &expected);
        assert_eq!(RawRxFrame::decode(&expected), Ok(frame));
    }

    #[test]
    fn rx_frame_decodes_without_mac_data() {
        // What a host sees from an RCP that omits the MAC-data struct, which OpenThread's
        // host only reads when the radio has `OT_RADIO_CAPS_TRANSMIT_SEC`.
        #[rustfmt::skip]
        let bytes = [
            0x01, 0x00, 0xAA,
            0xC4, 0x80,
            0x00, 0x00,
            0x0A, 0x00, 0x0B, 0x64, 0x2A, 0, 0, 0, 0, 0, 0, 0,
            0x01, 0x00, 0x00,
        ];
        let frame = RawRxFrame::decode(&bytes).unwrap();

        assert_eq!(frame.rssi, -60);
        assert_eq!(frame.channel, 11);
        assert_eq!(frame.lqi, 100);
        assert_eq!(frame.timestamp_us, 42);
        assert_eq!(frame.receive_error, 0);
        assert_eq!(frame.ack_key_id, 0);
        assert_eq!(frame.ack_frame_counter, 0);
    }

    #[test]
    fn rx_frame_bare_psdu_uses_defaults() {
        let bytes = [0x01, 0x00, 0xAA];
        assert_eq!(
            RawRxFrame::decode(&bytes),
            Ok(RawRxFrame::new(Bytes::from_static(&[0xAA])))
        );
    }

    #[test]
    fn rx_frame_skips_unknown_struct_tail() {
        // A PHY-data struct with two extra bytes beyond the fields this crate knows.
        #[rustfmt::skip]
        let bytes = [
            0x00, 0x00,
            0x00, 0x00,
            0x00, 0x00,
            0x0C, 0x00, 0x0F, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0xEE, 0xEE,
            0x01, 0x00, 0x05,
        ];
        let frame = RawRxFrame::decode(&bytes).unwrap();

        assert_eq!(frame.channel, 15);
        assert_eq!(frame.receive_error, 5);
    }

    #[test]
    fn rx_frame_decode_fails_on_truncated_field() {
        // Flags cut off after one byte.
        let bytes = [0x01, 0x00, 0xAA, 0x00, 0x00, 0x01];
        assert_eq!(RawRxFrame::decode(&bytes), Err(Error::PacketLength(6)));
    }

    #[test]
    fn rx_frame_decode_fails_on_overlong_struct() {
        // PHY-data struct claims 10 bytes but only 2 follow.
        let bytes = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x0B, 0x64];
        assert_eq!(RawRxFrame::decode(&bytes), Err(Error::PacketLength(10)));
    }

    #[test]
    fn rx_frame_decode_fails_on_truncated_psdu() {
        let bytes = [0x04, 0x00, 0xAA];
        assert_eq!(RawRxFrame::decode(&bytes), Err(Error::PacketLength(3)));
    }
}
