use crate::Error;
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
/// Matches OpenThread's `SPINEL_PROP_STREAM_RAW` wire format (and ziggurat's
/// `SpinelRxFrame`): a `u16` little-endian PSDU-length prefix, the PSDU, a fixed
/// 17-byte metadata trailer, then an optional vendor/manufacturer-specific tail.
#[derive(Clone, Debug, PartialEq)]
pub struct RawRxFrame {
    pub psdu: Bytes,
    pub rssi: i8,
    pub noise_floor: i8,
    pub flags: u32,
    pub channel: u8,
    pub lqi: u8,
    pub timestamp_us: u64,
    pub receive_error: u8,
    pub manufacturer_specific: Bytes,
}

impl RawRxFrame {
    /// Length, in bytes, of the fixed metadata trailer following the PSDU: rssi(1) +
    /// noise_floor(1) + flags(4) + channel(1) + lqi(1) + timestamp_us(8) + receive_error(1).
    const TRAILER_LEN: usize = 1 + 1 + 4 + 1 + 1 + 8 + 1;

    pub fn encode(&self, buffer: &mut BytesMut) {
        buffer.put_u16_le(self.psdu.len() as u16);
        buffer.put_slice(&self.psdu);
        buffer.put_i8(self.rssi);
        buffer.put_i8(self.noise_floor);
        buffer.put_u32_le(self.flags);
        buffer.put_u8(self.channel);
        buffer.put_u8(self.lqi);
        buffer.put_u64_le(self.timestamp_us);
        buffer.put_u8(self.receive_error);
        buffer.put_slice(&self.manufacturer_specific);
    }

    pub fn to_bytes(&self) -> Bytes {
        let mut buffer = BytesMut::new();
        self.encode(&mut buffer);
        buffer.freeze()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < 2 {
            return Err(Error::PacketLength(bytes.len()));
        }

        let psdu_len = u16::from_le_bytes([bytes[0], bytes[1]]) as usize;
        let mut offset = 2;

        if bytes.len() < offset + psdu_len + Self::TRAILER_LEN {
            return Err(Error::PacketLength(bytes.len()));
        }

        let psdu = Bytes::copy_from_slice(&bytes[offset..offset + psdu_len]);
        offset += psdu_len;

        let rssi = bytes[offset] as i8;
        offset += 1;

        let noise_floor = bytes[offset] as i8;
        offset += 1;

        let flags = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        offset += 4;

        let channel = bytes[offset];
        offset += 1;

        let lqi = bytes[offset];
        offset += 1;

        let timestamp_us = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
        offset += 8;

        let receive_error = bytes[offset];
        offset += 1;

        let manufacturer_specific = Bytes::copy_from_slice(&bytes[offset..]);

        Ok(Self {
            psdu,
            rssi,
            noise_floor,
            flags,
            channel,
            lqi,
            timestamp_us,
            receive_error,
            manufacturer_specific,
        })
    }
}

// Manual impl instead of `#[derive(defmt::Format)]`: `Bytes` doesn't implement
// `defmt::Format`, so `psdu`/`manufacturer_specific` are formatted as byte slices instead.
#[cfg(feature = "defmt")]
impl defmt::Format for RawRxFrame {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(
            fmt,
            "RawRxFrame {{ psdu: {=[u8]}, rssi: {}, noise_floor: {}, flags: {}, channel: {}, lqi: {}, timestamp_us: {}, receive_error: {}, manufacturer_specific: {=[u8]} }}",
            &self.psdu[..],
            self.rssi,
            self.noise_floor,
            self.flags,
            self.channel,
            self.lqi,
            self.timestamp_us,
            self.receive_error,
            &self.manufacturer_specific[..],
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
            flags: 0x1234_5678,
            channel: 15,
            lqi: 200,
            timestamp_us: 0x0102_0304_0506_0708,
            receive_error: 0,
            manufacturer_specific: Bytes::new(),
        };
        let bytes = frame.to_bytes();

        assert_eq!(RawRxFrame::decode(&bytes), Ok(frame));
    }

    #[test]
    fn rx_frame_pinned_byte_layout() {
        // Pins the wire layout against the values defined in OpenThread's `spinel.h`
        // rx-frame format, since a silent drift here breaks interop with real Spinel
        // hosts/RCPs without any local round-trip test noticing.
        let frame = RawRxFrame {
            psdu: Bytes::from_static(&[0xAB, 0xCD]),
            rssi: -1,
            noise_floor: -2,
            flags: 0x0000_0001,
            channel: 20,
            lqi: 255,
            timestamp_us: 1,
            receive_error: 0,
            manufacturer_specific: Bytes::from_static(&[0x99]),
        };

        #[rustfmt::skip]
        let expected: [u8; 22] = [
            0x02, 0x00, // psdu_len = 2 (LE)
            0xAB, 0xCD, // psdu
            0xFF, // rssi = -1
            0xFE, // noise_floor = -2
            0x01, 0x00, 0x00, 0x00, // flags = 1 (LE)
            0x14, // channel = 20
            0xFF, // lqi = 255
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // timestamp_us = 1 (LE)
            0x00, // receive_error = 0
            0x99, // manufacturer_specific
        ];
        assert_eq!(&frame.to_bytes()[..], &expected);
    }

    #[test]
    fn rx_frame_round_trips_with_manufacturer_specific_tail() {
        let frame = RawRxFrame {
            psdu: Bytes::from_static(&[0x01]),
            rssi: 0,
            noise_floor: 0,
            flags: 0,
            channel: 11,
            lqi: 0,
            timestamp_us: 0,
            receive_error: 0,
            manufacturer_specific: Bytes::from_static(&[0xDE, 0xAD, 0xBE, 0xEF]),
        };
        let bytes = frame.to_bytes();

        assert_eq!(RawRxFrame::decode(&bytes), Ok(frame));
    }

    #[test]
    fn rx_frame_decode_fails_on_missing_trailer() {
        // A PSDU with no metadata trailer at all.
        let bytes = [0x01, 0x00, 0xAA];
        assert_eq!(RawRxFrame::decode(&bytes), Err(Error::PacketLength(3)));
    }
}
