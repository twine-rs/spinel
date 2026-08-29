//! Device-side (RCP) protocol dispatch.
//!
//! Everything under [`connection`](crate::connection) plays the *host* role: it issues
//! requests and awaits responses. [`RcpDevice`] is the mirror image, for firmware that
//! needs to answer them -- decode an incoming Host-to-RCP [`Frame`], dispatch it, and
//! produce the RCP-to-Host reply.
//!
//! [`RcpDevice`] has no transport or executor dependency: it works on decoded [`Frame`]s
//! in memory, so callers can drive it from anything (a `tokio`/`embassy` task, an
//! interrupt handler, a synchronous poll loop, or a unit test). Radio-facing property
//! access is dispatched through the [`Radio`] trait, which callers implement against
//! whatever hardware they target.

use crate::{Command, Frame, Header, PackedU32, Property, PropertyStream, ResetReason, Status};
use bytes::Bytes;

/// Hardware/radio operations [`RcpDevice`] dispatches PHY/MAC property access to.
///
/// `RcpDevice` understands exactly the properties below (`PhysicalEnabled`,
/// `PhysicalChannel`, `PhysicalTxPower`, `MacPromiscuousMode`, `HardwareAddress`, and the
/// raw stream) -- it has no other hardware or transport dependency. Implement this trait
/// against whatever 802.15.4 radio driver the target provides and hand it to
/// [`RcpDevice::new`].
pub trait Radio {
    /// Enable or disable the radio.
    fn set_enabled(&mut self, enabled: bool) -> Result<(), Status>;
    /// Whether the radio is currently enabled.
    fn enabled(&self) -> bool;

    /// Tune the radio to the given channel.
    fn set_channel(&mut self, channel: u8) -> Result<(), Status>;
    /// The channel the radio is currently tuned to.
    fn channel(&self) -> u8;

    /// Set the transmit power, in dBm.
    fn set_tx_power(&mut self, dbm: i8) -> Result<(), Status>;
    /// The radio's current transmit power, in dBm.
    fn tx_power(&self) -> i8;

    /// Enable or disable promiscuous (monitor) mode.
    fn set_promiscuous(&mut self, enabled: bool) -> Result<(), Status>;
    /// Whether promiscuous (monitor) mode is currently enabled.
    fn promiscuous(&self) -> bool;

    /// The radio's static EUI-64 hardware address.
    fn hardware_address(&self) -> [u8; 8];

    /// Queue a raw frame (e.g. an 802.15.4 PSDU) for transmission.
    fn transmit(&mut self, frame: &[u8]) -> Result<(), Status>;

    /// Non-blocking poll for a received raw frame.
    ///
    /// On success the frame is written into `buf` and its length returned; `None` if
    /// nothing is pending.
    fn try_receive(&mut self, buf: &mut [u8]) -> Option<usize>;
}

/// Dispatches Host-to-RCP [`Frame`]s to a [`Radio`] and produces the RCP-to-Host replies.
///
/// `RcpDevice` currently understands `Noop`, `Reset`, and `PropertyValueGet`/`Set` for the
/// PHY/MAC properties [`Radio`] exposes. Anything else (e.g. `ProtocolVersion`,
/// `NcpVersion`, `Capabilities`) is not yet handled and reports [`Status::PropertyNotFound`]
/// -- those are static/config properties unrelated to the radio, not a `Radio`
/// responsibility, and are left for a caller-supplied layer on top of this one.
pub struct RcpDevice<R: Radio> {
    radio: R,
    iid: u8,
}

impl<R: Radio> RcpDevice<R> {
    /// Create a new device dispatcher over `radio`, replying with the given Instance
    /// Identifier (IID) in unsolicited frames.
    pub fn new(radio: R, iid: u8) -> Self {
        Self { radio, iid }
    }

    /// Borrow the underlying [`Radio`].
    pub fn radio(&self) -> &R {
        &self.radio
    }

    /// Mutably borrow the underlying [`Radio`].
    pub fn radio_mut(&mut self) -> &mut R {
        &mut self.radio
    }

    /// Handle one Host-to-RCP [`Frame`] and produce the RCP-to-Host reply.
    ///
    /// The reply echoes the request's [`Header`] (TID and IID), per the Spinel wire
    /// format's request/response correlation.
    pub fn handle_frame(&mut self, frame: Frame) -> Frame {
        let header = frame.header();
        let command = match frame.command() {
            Command::Noop => status_command(Status::Ok),
            Command::Reset => reset_command(),
            Command::PropertyValueGet(prop) => self.get(prop),
            Command::PropertyValueSet(prop, value) => self.set(prop, value),
            _ => status_command(Status::InvalidCommand),
        };
        Frame::new(header, command)
    }

    /// Poll the radio for a received raw frame and, if one is pending, produce the
    /// unsolicited RCP-to-Host [`Frame`] carrying it.
    ///
    /// Unsolicited frames use TID `0`, which the Spinel wire format reserves for
    /// messages that are not a response to a specific request.
    pub fn poll_raw_rx(&mut self, buf: &mut [u8]) -> Option<Frame> {
        let n = self.radio.try_receive(buf)?;
        Some(Frame::new(
            Header::new(self.iid, 0),
            Command::PropertyValueIs(
                Property::Stream(PropertyStream::Raw),
                Bytes::copy_from_slice(&buf[..n]),
            ),
        ))
    }

    fn get(&self, prop: Property) -> Command {
        match prop {
            Property::LastStatus => status_command(Status::Ok),
            Property::HardwareAddress => Command::PropertyValueIs(
                prop,
                Bytes::copy_from_slice(&self.radio.hardware_address()),
            ),
            Property::PhysicalEnabled => {
                Command::PropertyValueIs(prop, encode_bool(self.radio.enabled()))
            }
            Property::PhysicalChannel => {
                Command::PropertyValueIs(prop, encode_u8(self.radio.channel()))
            }
            Property::PhysicalTxPower => {
                Command::PropertyValueIs(prop, encode_i8(self.radio.tx_power()))
            }
            Property::MacPromiscuousMode => {
                Command::PropertyValueIs(prop, encode_bool(self.radio.promiscuous()))
            }
            _ => status_command(Status::PropertyNotFound),
        }
    }

    fn set(&mut self, prop: Property, value: Bytes) -> Command {
        let outcome: Result<(), Status> = match &prop {
            Property::PhysicalEnabled => {
                decode_bool(&value).and_then(|v| self.radio.set_enabled(v))
            }
            Property::PhysicalChannel => decode_u8(&value).and_then(|v| self.radio.set_channel(v)),
            Property::PhysicalTxPower => decode_i8(&value).and_then(|v| self.radio.set_tx_power(v)),
            Property::MacPromiscuousMode => {
                decode_bool(&value).and_then(|v| self.radio.set_promiscuous(v))
            }
            Property::Stream(PropertyStream::Raw) => self.radio.transmit(&value),
            _ => Err(Status::PropertyNotFound),
        };

        match outcome {
            // The raw stream is send-only from the host's perspective; a transmitted
            // frame is confirmed via LastStatus, not by echoing it back as a property.
            Ok(()) if matches!(prop, Property::Stream(PropertyStream::Raw)) => {
                status_command(Status::Ok)
            }
            // Read back the property rather than echoing `value` verbatim, in case the
            // radio clamped or normalized what was requested (e.g. an unsupported TX
            // power rounded to the nearest supported one).
            Ok(()) => self.get(prop),
            Err(status) => status_command(status),
        }
    }
}

fn status_command(status: Status) -> Command {
    let (bytes, len) = PackedU32::encode(u32::from(status));
    Command::PropertyValueIs(Property::LastStatus, Bytes::copy_from_slice(&bytes[..len]))
}

fn reset_command() -> Command {
    // `RcpDevice` doesn't own an actual reboot -- that's the firmware's/caller's
    // responsibility. This just reports the reset the way a real reset would.
    let (bytes, len) = PackedU32::encode(u32::from(ResetReason::Software));
    Command::PropertyValueIs(Property::LastStatus, Bytes::copy_from_slice(&bytes[..len]))
}

fn decode_bool(value: &Bytes) -> Result<bool, Status> {
    match (value.first(), value.len()) {
        (Some(&b), 1) => Ok(b != 0),
        _ => Err(Status::ParseError),
    }
}

fn decode_u8(value: &Bytes) -> Result<u8, Status> {
    match (value.first(), value.len()) {
        (Some(&b), 1) => Ok(b),
        _ => Err(Status::ParseError),
    }
}

fn decode_i8(value: &Bytes) -> Result<i8, Status> {
    match (value.first(), value.len()) {
        (Some(&b), 1) => Ok(b as i8),
        _ => Err(Status::ParseError),
    }
}

fn encode_bool(value: bool) -> Bytes {
    Bytes::copy_from_slice(&[value as u8])
}

fn encode_u8(value: u8) -> Bytes {
    Bytes::copy_from_slice(&[value])
}

fn encode_i8(value: i8) -> Bytes {
    Bytes::copy_from_slice(&[value as u8])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct MockRadio {
        enabled: bool,
        channel: u8,
        tx_power: i8,
        promiscuous: bool,
        transmitted: Option<Bytes>,
        pending_rx: Option<Bytes>,
    }

    impl Radio for MockRadio {
        fn set_enabled(&mut self, enabled: bool) -> Result<(), Status> {
            self.enabled = enabled;
            Ok(())
        }

        fn enabled(&self) -> bool {
            self.enabled
        }

        fn set_channel(&mut self, channel: u8) -> Result<(), Status> {
            if !(11..=26).contains(&channel) {
                return Err(Status::InvalidArgument);
            }
            self.channel = channel;
            Ok(())
        }

        fn channel(&self) -> u8 {
            self.channel
        }

        fn set_tx_power(&mut self, dbm: i8) -> Result<(), Status> {
            self.tx_power = dbm;
            Ok(())
        }

        fn tx_power(&self) -> i8 {
            self.tx_power
        }

        fn set_promiscuous(&mut self, enabled: bool) -> Result<(), Status> {
            self.promiscuous = enabled;
            Ok(())
        }

        fn promiscuous(&self) -> bool {
            self.promiscuous
        }

        fn hardware_address(&self) -> [u8; 8] {
            [0xAA; 8]
        }

        fn transmit(&mut self, frame: &[u8]) -> Result<(), Status> {
            self.transmitted = Some(Bytes::copy_from_slice(frame));
            Ok(())
        }

        fn try_receive(&mut self, buf: &mut [u8]) -> Option<usize> {
            let frame = self.pending_rx.take()?;
            buf[..frame.len()].copy_from_slice(&frame);
            Some(frame.len())
        }
    }

    fn request(cmd: Command) -> Frame {
        Frame::new(Header::new(0, 1), cmd)
    }

    #[test]
    fn noop_replies_ok() {
        let mut device = RcpDevice::new(MockRadio::default(), 0);
        let reply = device.handle_frame(request(Command::Noop));
        assert_eq!(reply.last_status(), Some(Status::Ok));
        assert_eq!(reply.header().tid(), 1);
    }

    #[test]
    fn set_then_get_physical_enabled_round_trips() {
        let mut device = RcpDevice::new(MockRadio::default(), 0);

        let set_reply = device.handle_frame(request(Command::PropertyValueSet(
            Property::PhysicalEnabled,
            encode_bool(true),
        )));
        assert_eq!(
            set_reply.command(),
            Command::PropertyValueIs(Property::PhysicalEnabled, encode_bool(true))
        );

        let get_reply = device.handle_frame(request(Command::PropertyValueGet(
            Property::PhysicalEnabled,
        )));
        assert_eq!(
            get_reply.command(),
            Command::PropertyValueIs(Property::PhysicalEnabled, encode_bool(true))
        );
    }

    #[test]
    fn invalid_channel_reports_status_instead_of_panicking() {
        let mut device = RcpDevice::new(MockRadio::default(), 0);
        let reply = device.handle_frame(request(Command::PropertyValueSet(
            Property::PhysicalChannel,
            encode_u8(99),
        )));
        assert_eq!(reply.last_status(), Some(Status::InvalidArgument));
    }

    #[test]
    fn unhandled_property_reports_not_found() {
        let mut device = RcpDevice::new(MockRadio::default(), 0);
        let reply = device.handle_frame(request(Command::PropertyValueGet(Property::NcpVersion)));
        assert_eq!(reply.last_status(), Some(Status::PropertyNotFound));
    }

    #[test]
    fn raw_stream_set_transmits_and_confirms_via_status() {
        let mut device = RcpDevice::new(MockRadio::default(), 0);
        let payload = Bytes::from_static(&[0x01, 0x02, 0x03]);

        let reply = device.handle_frame(request(Command::PropertyValueSet(
            Property::Stream(PropertyStream::Raw),
            payload.clone(),
        )));

        assert_eq!(reply.last_status(), Some(Status::Ok));
        assert_eq!(device.radio().transmitted, Some(payload));
    }

    #[test]
    fn poll_raw_rx_surfaces_pending_frame_as_unsolicited() {
        let mut device = RcpDevice::new(MockRadio::default(), 3);
        device.radio_mut().pending_rx = Some(Bytes::from_static(&[0xAB, 0xCD]));

        let mut buf = [0u8; 32];
        let frame = device.poll_raw_rx(&mut buf).expect("frame pending");

        assert_eq!(frame.header().iid(), 3);
        assert_eq!(frame.header().tid(), 0);
        assert_eq!(
            frame.command(),
            Command::PropertyValueIs(
                Property::Stream(PropertyStream::Raw),
                Bytes::from_static(&[0xAB, 0xCD])
            )
        );

        assert!(device.poll_raw_rx(&mut buf).is_none());
    }
}
