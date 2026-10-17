//! Payload codecs for the raw 802.15.4 stream property.
//!
//! The wire layouts follow the Spinel protocol documentation for the raw
//! stream property. Receive metadata MAY be empty or partially specified, so
//! the decoder treats every metadata field as optional.

use bytes::{Bytes, BytesMut};

use crate::codec::datatype::{
    PackedU32, SpinelBool, SpinelData, SpinelI8, SpinelU16, SpinelU32, SpinelU64, SpinelU8,
};
use crate::codec::vendor::{NoVendor, Vendor};
use crate::{Error, Status};

/// Number of FCS bytes the radio appends to a transmitted frame.
const FCS_LEN: usize = 2;

/// Maximum PSDU length without the FCS, per IEEE 802.15.4.
///
/// The standard caps a PHY packet at 127 bytes including the 2-byte FCS.
const MAX_PSDU_LEN: usize = 127 - FCS_LEN;

/// Default RSSI when the receive metadata omits it, in dBm.
const DEFAULT_RSSI: i8 = -128;

/// Default noise floor when the receive metadata omits it, in dBm.
const DEFAULT_NOISE_FLOOR: i8 = -128;

/// The frame was transmitted, not received.
pub const RAW_FRAME_FLAG_TX: u16 = 0x0001;

/// The frame was received with a bad FCS.
pub const RAW_FRAME_FLAG_BAD_FCS: u16 = 0x0004;

/// The frame appears to be a duplicate.
pub const RAW_FRAME_FLAG_DUPE: u16 = 0x0008;

/// Transmit options for a raw 802.15.4 frame.
///
/// The default value requests plain transmission. CSMA-CA is on, retries and
/// security processing are off, and the CSL delay fields encode as zero.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct RawTxConfig {
    /// Maximum number of backoff attempts before declaring CCA failure.
    ///
    /// The encoder always writes this field, so the device never applies its
    /// own default. The value 4 matches the usual device default for direct
    /// transmissions.
    pub max_csma_backoffs: u8,

    /// Maximum number of retries allowed after a transmission failure.
    pub max_frame_retries: u8,

    /// Whether CSMA-CA is enabled for this frame.
    pub csma_ca_enabled: bool,

    /// Whether the device may update the frame header.
    pub is_header_updated: bool,

    /// Whether this frame is a retransmission.
    pub is_a_retx: bool,

    /// Whether security was processed on the frame before submission.
    pub security_processed: bool,

    /// Channel the radio returns to after the transmit completes.
    ///
    /// Encodes the transmit channel when `None`.
    pub rx_channel_after_tx_done: Option<u8>,
}

impl Default for RawTxConfig {
    fn default() -> Self {
        Self {
            max_csma_backoffs: 4,
            max_frame_retries: 0,
            csma_ca_enabled: true,
            is_header_updated: false,
            is_a_retx: false,
            security_processed: false,
            rx_channel_after_tx_done: None,
        }
    }
}

/// A raw 802.15.4 frame to transmit.
///
/// The PSDU excludes the 2-byte FCS. The encoder reserves space for the FCS
/// and the radio computes it.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct RawTxFrame {
    /// The frame bytes, without the FCS.
    pub psdu: Bytes,

    /// The 802.15.4 channel to transmit on.
    pub channel: u8,

    /// Transmit options.
    pub config: RawTxConfig,
}

impl RawTxFrame {
    /// Encode the frame and metadata as a raw stream property value.
    ///
    /// Returns the number of bytes that were written. Errors when the PSDU is
    /// longer than an 802.15.4 frame allows.
    pub fn encode(&self, buffer: &mut BytesMut) -> Result<usize, Error> {
        if self.psdu.len() > MAX_PSDU_LEN {
            return Err(Error::PsduLength(self.psdu.len()));
        }

        let mut num = SpinelU16::write_to_buffer((self.psdu.len() + FCS_LEN) as u16, buffer);
        buffer.extend_from_slice(&self.psdu);
        buffer.extend_from_slice(&[0u8; FCS_LEN]);
        num += self.psdu.len() + FCS_LEN;

        num += SpinelU8::write_to_buffer(self.channel, buffer);
        num += SpinelU8::write_to_buffer(self.config.max_csma_backoffs, buffer);
        num += SpinelU8::write_to_buffer(self.config.max_frame_retries, buffer);
        num += SpinelBool::write_to_buffer(self.config.csma_ca_enabled, buffer);
        num += SpinelBool::write_to_buffer(self.config.is_header_updated, buffer);
        num += SpinelBool::write_to_buffer(self.config.is_a_retx, buffer);
        num += SpinelBool::write_to_buffer(self.config.security_processed, buffer);
        num += SpinelU32::write_to_buffer(0, buffer);
        num += SpinelU32::write_to_buffer(0, buffer);
        num += SpinelU8::write_to_buffer(
            self.config.rx_channel_after_tx_done.unwrap_or(self.channel),
            buffer,
        );

        Ok(num)
    }
}

/// A raw 802.15.4 frame received from the radio.
///
/// Decoded from an unsolicited raw stream property value. The PSDU is a
/// zero-copy slice of the received buffer and contains the frame bytes exactly
/// as the device delivered them.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct RawRxFrame {
    /// The received frame bytes.
    pub psdu: Bytes,

    /// Received signal strength in dBm.
    ///
    /// Defaults to -128 when the metadata omits it.
    pub rssi: i8,

    /// Noise floor in dBm.
    ///
    /// Defaults to -128 when the metadata omits it.
    pub noise_floor: i8,

    /// Metadata flags matching the `RAW_FRAME_FLAG_*` constants.
    pub flags: u16,

    /// The 802.15.4 channel the frame arrived on.
    pub channel: Option<u8>,

    /// Link quality indicator for the frame.
    pub lqi: Option<u8>,

    /// Receive timestamp in microseconds.
    pub timestamp_us: Option<u64>,
}

impl RawRxFrame {
    /// Decode a received raw stream property value.
    ///
    /// Metadata fields that are absent take their documented defaults. The
    /// PSDU shares `buffer` rather than copying it.
    pub fn decode(buffer: &Bytes) -> Result<Self, Error> {
        let (range, consumed) = SpinelData::decode(buffer)?;
        let psdu = buffer.slice(range);
        let mut rest = &buffer[consumed..];

        let rssi = Self::next_field(&mut rest, SpinelI8::decode).unwrap_or(DEFAULT_RSSI);
        let noise_floor =
            Self::next_field(&mut rest, SpinelI8::decode).unwrap_or(DEFAULT_NOISE_FLOOR);
        let flags = Self::next_field(&mut rest, SpinelU16::decode).unwrap_or(0);

        let mut channel = None;
        let mut lqi = None;
        let mut timestamp_us = None;

        if let Ok((phy_range, _)) = SpinelData::decode(rest) {
            let mut phy = &rest[phy_range];
            channel = Self::next_field(&mut phy, SpinelU8::decode);
            lqi = Self::next_field(&mut phy, SpinelU8::decode);
            timestamp_us = Self::next_field(&mut phy, SpinelU64::decode);
        }

        Ok(Self {
            psdu,
            rssi,
            noise_floor,
            flags,
            channel,
            lqi,
            timestamp_us,
        })
    }

    /// Decode one optional metadata field and advance the cursor.
    ///
    /// Returns `None` when the field is absent or truncated, so a partially
    /// specified metadata block decodes to defaults instead of an error.
    fn next_field<T>(
        rest: &mut &[u8],
        decode: impl Fn(&[u8]) -> Result<(T, usize), Error>,
    ) -> Option<T> {
        match decode(rest) {
            Ok((value, consumed)) => {
                *rest = &rest[consumed..];
                Some(value)
            }
            Err(_) => None,
        }
    }
}

/// The result of a raw 802.15.4 transmit.
///
/// The device reports the transmit outcome as a `LastStatus` value on the
/// transaction ID of the transmit request. The payload carries the status,
/// the frame-pending flag, the header-updated flag, and the acknowledgment
/// frame when the transmit requested one.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct RawTxDone<V: Vendor = NoVendor> {
    /// The radio's transmit outcome.
    pub status: Status<V>,

    /// Whether the acknowledgment had the frame-pending bit set.
    pub frame_pending: bool,

    /// Whether the device updated the frame header before the transmit.
    pub header_updated: bool,

    /// The received acknowledgment frame, when the transmit requested one.
    pub ack: Option<RawRxFrame>,
}

impl<V: Vendor> RawTxDone<V> {
    /// Decode a transmit-done value.
    ///
    /// The status is required. The flags and the acknowledgment frame are
    /// optional, so a plain error status decodes too.
    pub fn decode(buffer: &Bytes) -> Result<Self, Error> {
        let (status, consumed) = PackedU32::decode_checked(buffer)?;
        let status = Status::from(status);
        let mut offset = consumed;
        let mut rest = &buffer[consumed..];

        let frame_pending = match SpinelBool::decode(rest) {
            Ok((value, len)) => {
                rest = &rest[len..];
                offset += len;
                value
            }
            Err(_) => false,
        };

        let header_updated = match SpinelBool::decode(rest) {
            Ok((value, len)) => {
                offset += len;
                value
            }
            Err(_) => false,
        };

        // The device writes an ack frame only on a successful transmit, and
        // a security key index and frame counter can trail the flags in both
        // outcomes. Decoding the ack only on success keeps the tail of a
        // failed transmit from being mistaken for one. The device packs an
        // empty frame structure when no acknowledgment arrived, so an empty
        // PSDU also means no ack.
        let ack_bytes = buffer.slice(offset..);
        let ack = if status != Status::Ok || ack_bytes.is_empty() {
            None
        } else {
            Some(RawRxFrame::decode(&ack_bytes)?).filter(|a| !a.psdu.is_empty())
        };

        Ok(Self {
            status,
            frame_pending,
            header_updated,
            ack,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TX encoding of a two-byte PSDU on channel 11 with the default config.
    ///
    /// Layout: length 4 (PSDU + FCS), PSDU, FCS placeholder, channel,
    /// backoffs, retries, csma-ca, header-updated, is-a-retx,
    /// security-processed, tx delay, tx delay base time, rx channel.
    const TX_DEFAULT_WIRE_FMT: &[u8] = &[
        0x04, 0x00, 0xAA, 0xBB, 0x00, 0x00, 0x0B, 0x04, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0B,
    ];

    #[test]
    fn tx_default_config_pins_wire_layout() {
        let frame = RawTxFrame {
            psdu: Bytes::from_static(&[0xAA, 0xBB]),
            channel: 11,
            config: RawTxConfig::default(),
        };

        let mut buffer = BytesMut::new();
        let written = frame.encode(&mut buffer).unwrap();
        assert_eq!(written, buffer.len());
        assert_eq!(&buffer[..], TX_DEFAULT_WIRE_FMT);
    }

    #[test]
    fn tx_oversized_psdu_errors() {
        let frame = RawTxFrame {
            psdu: Bytes::from(alloc::vec![0u8; 126]),
            channel: 11,
            config: RawTxConfig::default(),
        };

        let mut buffer = BytesMut::new();
        assert_eq!(frame.encode(&mut buffer), Err(Error::PsduLength(126)));
        assert!(buffer.is_empty());
    }

    #[test]
    fn tx_explicit_config_pins_wire_layout() {
        let frame = RawTxFrame {
            psdu: Bytes::from_static(&[0xAA]),
            channel: 26,
            config: RawTxConfig {
                max_csma_backoffs: 2,
                max_frame_retries: 3,
                csma_ca_enabled: false,
                is_header_updated: true,
                is_a_retx: true,
                security_processed: true,
                rx_channel_after_tx_done: Some(15),
            },
        };

        let mut buffer = BytesMut::new();
        frame.encode(&mut buffer).unwrap();
        assert_eq!(
            &buffer[..],
            &[
                0x03, 0x00, 0xAA, 0x00, 0x00, 0x1A, 0x02, 0x03, 0x00, 0x01, 0x01, 0x01, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0F,
            ]
        );
    }

    #[test]
    fn rx_full_metadata_decodes() {
        // Frame [0xAA, 0xBB], RSSI -40, noise floor -95, flags 0x0004,
        // PHY struct with channel 11, LQI 200, timestamp 0x0102030405060708.
        let mut buffer = BytesMut::new();
        SpinelData::write_to_buffer(&[0xAA, 0xBB], &mut buffer);
        SpinelI8::write_to_buffer(-40, &mut buffer);
        SpinelI8::write_to_buffer(-95, &mut buffer);
        SpinelU16::write_to_buffer(RAW_FRAME_FLAG_BAD_FCS, &mut buffer);
        let mut phy = BytesMut::new();
        SpinelU8::write_to_buffer(11, &mut phy);
        SpinelU8::write_to_buffer(200, &mut phy);
        SpinelU64::write_to_buffer(0x0102_0304_0506_0708, &mut phy);
        SpinelData::write_to_buffer(&phy, &mut buffer);

        let frame = RawRxFrame::decode(&buffer.freeze()).unwrap();
        assert_eq!(&frame.psdu[..], &[0xAA, 0xBB]);
        assert_eq!(frame.rssi, -40);
        assert_eq!(frame.noise_floor, -95);
        assert_eq!(frame.flags, RAW_FRAME_FLAG_BAD_FCS);
        assert_eq!(frame.channel, Some(11));
        assert_eq!(frame.lqi, Some(200));
        assert_eq!(frame.timestamp_us, Some(0x0102_0304_0506_0708));
    }

    #[test]
    fn rx_decodes_at_every_truncation_boundary() {
        // Full value, truncated after each metadata field boundary.
        let mut full = BytesMut::new();
        SpinelData::write_to_buffer(&[0xAA], &mut full);
        SpinelI8::write_to_buffer(-40, &mut full);
        SpinelI8::write_to_buffer(-95, &mut full);
        SpinelU16::write_to_buffer(0, &mut full);
        let full = full.freeze();

        // Metadata lengths after the 3-byte frame field: 0, 1, 2, 4 bytes.
        for boundary in [3usize, 4, 5, 7] {
            let frame = RawRxFrame::decode(&full.slice(..boundary)).unwrap();
            assert_eq!(&frame.psdu[..], &[0xAA]);
            assert_eq!(frame.rssi, if boundary > 3 { -40 } else { -128 });
            assert_eq!(frame.noise_floor, if boundary > 4 { -95 } else { -128 });
            assert_eq!(frame.channel, None);
            assert_eq!(frame.lqi, None);
            assert_eq!(frame.timestamp_us, None);
        }
    }

    #[test]
    fn rx_truncated_frame_field_errors() {
        // The prefix claims 4 bytes, but only 1 follows.
        let buffer = Bytes::from_static(&[0x04, 0x00, 0xAA]);
        assert_eq!(
            RawRxFrame::decode(&buffer),
            Err(Error::DatatypeTruncated { needed: 6, got: 3 })
        );
    }

    #[test]
    fn rx_psdu_shares_source_buffer() {
        let mut buffer = BytesMut::new();
        SpinelData::write_to_buffer(&[0xAA, 0xBB], &mut buffer);
        let buffer = buffer.freeze();

        let frame = RawRxFrame::decode(&buffer).unwrap();
        // A zero-copy slice points into the source allocation.
        assert_eq!(frame.psdu.as_ptr(), buffer[2..].as_ptr());
    }

    #[test]
    fn tx_done_with_ack_decodes() {
        let mut buffer = BytesMut::new();
        PackedU32::write_to_buffer(0, &mut buffer);
        SpinelBool::write_to_buffer(true, &mut buffer);
        SpinelBool::write_to_buffer(false, &mut buffer);
        SpinelData::write_to_buffer(&[0x02, 0x00, 0xCC], &mut buffer);

        let done = RawTxDone::<NoVendor>::decode(&buffer.freeze()).unwrap();
        assert_eq!(done.status, Status::Ok);
        assert!(done.frame_pending);
        assert!(!done.header_updated);
        assert_eq!(&done.ack.unwrap().psdu[..], &[0x02, 0x00, 0xCC]);
    }

    #[test]
    fn tx_done_without_ack_decodes() {
        let mut buffer = BytesMut::new();
        PackedU32::write_to_buffer(0, &mut buffer);
        SpinelBool::write_to_buffer(false, &mut buffer);
        SpinelBool::write_to_buffer(true, &mut buffer);

        let done = RawTxDone::<NoVendor>::decode(&buffer.freeze()).unwrap();
        assert_eq!(done.status, Status::Ok);
        assert!(!done.frame_pending);
        assert!(done.header_updated);
        assert_eq!(done.ack, None);
    }

    #[test]
    fn tx_done_error_status_decodes() {
        // CCA failure (18) with no further fields.
        let mut buffer = BytesMut::new();
        PackedU32::write_to_buffer(18, &mut buffer);

        let done = RawTxDone::<NoVendor>::decode(&buffer.freeze()).unwrap();
        assert_eq!(done.status, Status::CcaFailure);
        assert!(!done.frame_pending);
        assert!(!done.header_updated);
        assert_eq!(done.ack, None);
    }

    #[test]
    fn tx_done_error_with_security_tail_decodes() {
        // A failed security-processed transmit trails a key index and a
        // frame counter with no ack frame before them.
        let mut buffer = BytesMut::new();
        PackedU32::write_to_buffer(17, &mut buffer);
        SpinelBool::write_to_buffer(false, &mut buffer);
        SpinelBool::write_to_buffer(true, &mut buffer);
        SpinelU8::write_to_buffer(0x01, &mut buffer);
        SpinelU32::write_to_buffer(0x0000_1234, &mut buffer);

        let done = RawTxDone::<NoVendor>::decode(&buffer.freeze()).unwrap();
        assert_eq!(done.status, Status::NoAck);
        assert!(done.header_updated);
        assert_eq!(done.ack, None);
    }
}
