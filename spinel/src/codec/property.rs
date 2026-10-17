use core::fmt;

use crate::codec::vendor::{NoVendor, Vendor, VendorValue};
use crate::error::Error;

const PROP_LAST_STATUS: u32 = 0x00;
const PROP_PROTOCOL_VERSION: u32 = 0x01;
const PROP_NCP_VERSION: u32 = 0x02;
const PROP_INTERFACE_TYPE: u32 = 0x03;
const PROP_CAPS: u32 = 0x05;
const PROP_HWADDR: u32 = 0x08;
const PROP_PHY_ENABLED: u32 = 0x20;
const PROP_PHY_CHAN: u32 = 0x21;
const PROP_PHY_TX_POWER: u32 = 0x26;
const PROP_MAC_RAW_STREAM_ENABLED: u32 = 0x37;
const PROP_MAC_PROMISCUOUS_MODE: u32 = 0x38;
const PROP_STREAM_DEBUG: u32 = 0x70;
const PROP_STREAM_RAW: u32 = 0x71;
const PROP_STREAM_NET: u32 = 0x72;
const PROP_STREAM_NET_INSECURE: u32 = 0x73;
const PROP_STREAM_LOG: u32 = 0x74;

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum PropertyStream {
    /// This stream sends human-readable debugging output that the host can display in its logs.
    ///
    /// The location of newline characters is not assumed by the host. It is the device's responsibility to insert
    /// newline characters where needed. To receive debug output, wait for [`Command::PropertyValueIs`](crate::Command::PropertyValueIs)
    /// to be sent with this property ID.
    Debug,

    /// This stream sends and receives data packets to and from the currently attached network.
    ///
    /// The exact format of the frame metadata and data is dependent on the network protocol being used.
    ///
    /// This property is a streaming property, meaning that you cannot explicitly fetch the value of this property. To
    /// receive traffic, wait for [`Command::PropertyValueIs`](crate::Command::PropertyValueIs) to be sent from
    /// the device to the host with this property ID. To send network packets a call to
    /// [`Command::PropertyValueSet`](crate::Command::PropertyValueSet) is required.
    Net,

    /// This stream sends and receives raw 802.15.4 frames to and from the radio.
    ///
    /// Requires the `MAC_RAW` capability. To receive traffic, wait for
    /// [`Command::PropertyValueIs`](crate::Command::PropertyValueIs) to be sent with this property ID.
    /// A frame written to this stream with [`Command::PropertyValueSet`](crate::Command::PropertyValueSet)
    /// is transmitted over the radio.
    Raw,
    NetInsecure,
    Log,
}

impl fmt::Display for PropertyStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PropertyStream::Debug => write!(f, "Debug"),
            PropertyStream::Net => write!(f, "Net"),
            PropertyStream::Raw => write!(f, "Raw"),
            PropertyStream::NetInsecure => write!(f, "NetInsecure"),
            PropertyStream::Log => write!(f, "Log"),
        }
    }
}

/// Spinel Properties.
///
/// Covers the core protocol properties owned by `spinel`. Any wire ID the core
/// protocol does not assign is delegated to the [`Vendor`] profile via
/// [`Property::Vendor`]. With the default [`NoVendor`] profile such an ID is a
/// decode error.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Property<V: Vendor = NoVendor> {
    /// Describes the status of the last operation encoded as a packed unsigned integer.
    ///
    /// This property is emitted often to indicate the result status of pretty much any Host-to-Device operation.
    /// It is emitted automatically at device startup with a value indicating the reset reason.
    LastStatus,

    /// Describes the protocol version information.
    ProtocolVersion,

    /// Contains a string which describes the firmware currently running on the device.
    NcpVersion,

    /// Identifies the network protocol for the device.
    InterfaceType,

    /// The capabilities supported by the device.
    ///
    /// Read-only. Encoded as a Spinel list of packed unsigned integers,
    /// one capability ID each. Decode the value with
    /// [`CapabilityIter`](crate::CapabilityIter) into [`Capability`](crate::Capability)
    /// values.
    Capabilities,

    /// Special properties representing streams of data.
    ///
    /// All stream properties emit changes asynchronously using [`Command::PropertyValueIs`](crate::Command::PropertyValueIs)
    /// sent from the device to the host. Some properties may allow for sending traffic from the host to the device
    /// (for example IPv6 traffic).
    Stream(PropertyStream),

    /// The static EUI64 address of the device.
    ///
    /// Typically read-only, but may be writable for some vendor defined circumstances.
    HardwareAddress,

    /// Transmit power of the radio in dBm.
    PhysicalTxPower,

    /// Whether the radio physical layer is enabled.
    ///
    /// Encoded as a Spinel `b` (boolean). Setting this property to true
    /// enters raw link mode. A device that runs its own network stack rejects
    /// the set with [`Status::InvalidState`](crate::Status) while the device
    /// network interface is up.
    PhysicalEnabled,

    /// The 802.15.4 channel the radio uses.
    ///
    /// Encoded as a Spinel `C` (unsigned 8-bit integer).
    PhysicalChannel,

    /// Whether the device forwards received raw frames to the host.
    ///
    /// Encoded as a Spinel `b` (boolean). While true, the device delivers
    /// received 802.15.4 frames on [`PropertyStream::Raw`].
    MacRawStreamEnabled,

    /// The MAC promiscuous mode of the radio.
    ///
    /// Encoded as a Spinel `C` (unsigned 8-bit integer). Decode the value with
    /// [`PromiscuousMode`](crate::PromiscuousMode). Setting this property
    /// requires link-raw mode to be on already.
    MacPromiscuousMode,

    /// A vendor-defined property.
    Vendor(V::Property),
}

impl<V: Vendor> fmt::Display for Property<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Property::LastStatus => write!(f, "LastStatus"),
            Property::ProtocolVersion => write!(f, "ProtocolVersion"),
            Property::NcpVersion => write!(f, "NcpVersion"),
            Property::InterfaceType => write!(f, "InterfaceType"),
            Property::Capabilities => write!(f, "Capabilities"),
            Property::Stream(stream) => write!(f, "{}", stream),
            Property::HardwareAddress => write!(f, "HardwareAddress"),
            Property::PhysicalTxPower => write!(f, "PhysicalTxPower"),
            Property::PhysicalEnabled => write!(f, "PhysicalEnabled"),
            Property::PhysicalChannel => write!(f, "PhysicalChannel"),
            Property::MacRawStreamEnabled => write!(f, "MacRawStreamEnabled"),
            Property::MacPromiscuousMode => write!(f, "MacPromiscuousMode"),
            Property::Vendor(v) => write!(f, "Vendor(0x{:x})", v.id()),
        }
    }
}

impl<V: Vendor> Property<V> {
    /// Byte representation of the [`Property`] on the wire
    pub fn id(&self) -> u32 {
        match self {
            Property::LastStatus => PROP_LAST_STATUS,
            Property::ProtocolVersion => PROP_PROTOCOL_VERSION,
            Property::NcpVersion => PROP_NCP_VERSION,
            Property::InterfaceType => PROP_INTERFACE_TYPE,
            Property::Capabilities => PROP_CAPS,
            Property::Stream(stream) => match stream {
                PropertyStream::Debug => PROP_STREAM_DEBUG,
                PropertyStream::Net => PROP_STREAM_NET,
                PropertyStream::Raw => PROP_STREAM_RAW,
                PropertyStream::NetInsecure => PROP_STREAM_NET_INSECURE,
                PropertyStream::Log => PROP_STREAM_LOG,
            },
            Property::HardwareAddress => PROP_HWADDR,
            Property::PhysicalTxPower => PROP_PHY_TX_POWER,
            Property::PhysicalEnabled => PROP_PHY_ENABLED,
            Property::PhysicalChannel => PROP_PHY_CHAN,
            Property::MacRawStreamEnabled => PROP_MAC_RAW_STREAM_ENABLED,
            Property::MacPromiscuousMode => PROP_MAC_PROMISCUOUS_MODE,
            Property::Vendor(v) => v.id(),
        }
    }

    /// Length of the [`Property`] data when bit packed
    pub fn packed_len(&self) -> usize {
        crate::codec::PackedU32::packed_len(self.id())
    }
}

impl<V: Vendor> TryFrom<u32> for Property<V> {
    type Error = Error;

    fn try_from(id: u32) -> Result<Self, Self::Error> {
        match id {
            PROP_LAST_STATUS => Ok(Property::LastStatus),
            PROP_PROTOCOL_VERSION => Ok(Property::ProtocolVersion),
            PROP_NCP_VERSION => Ok(Property::NcpVersion),
            PROP_INTERFACE_TYPE => Ok(Property::InterfaceType),
            PROP_CAPS => Ok(Property::Capabilities),
            PROP_STREAM_DEBUG => Ok(Property::Stream(PropertyStream::Debug)),
            PROP_STREAM_NET => Ok(Property::Stream(PropertyStream::Net)),
            PROP_STREAM_RAW => Ok(Property::Stream(PropertyStream::Raw)),
            PROP_STREAM_NET_INSECURE => Ok(Property::Stream(PropertyStream::NetInsecure)),
            PROP_STREAM_LOG => Ok(Property::Stream(PropertyStream::Log)),
            PROP_HWADDR => Ok(Property::HardwareAddress),
            PROP_PHY_TX_POWER => Ok(Property::PhysicalTxPower),
            PROP_PHY_ENABLED => Ok(Property::PhysicalEnabled),
            PROP_PHY_CHAN => Ok(Property::PhysicalChannel),
            PROP_MAC_RAW_STREAM_ENABLED => Ok(Property::MacRawStreamEnabled),
            PROP_MAC_PROMISCUOUS_MODE => Ok(Property::MacPromiscuousMode),
            // Any unassigned ID falls through to the vendor profile.
            _ => V::Property::try_from_id(id).map(Property::Vendor),
        }
    }
}

impl<V: Vendor> TryFrom<&[u8]> for Property<V> {
    type Error = Error;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        use crate::codec::PackedU32;
        let (prop_id, _) = PackedU32::decode_checked(bytes)?;
        Property::try_from(prop_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_fails_on_malformed_packed_property_id() {
        let prop = Property::<NoVendor>::try_from(&[0x80][..]);
        assert_eq!(prop, Err(Error::PackedU32ByteCount));
    }

    #[test]
    fn core_property_ids_round_trip() {
        let cases: [(Property, u32); 16] = [
            (Property::LastStatus, 0x00),
            (Property::ProtocolVersion, 0x01),
            (Property::NcpVersion, 0x02),
            (Property::InterfaceType, 0x03),
            (Property::Capabilities, 0x05),
            (Property::HardwareAddress, 0x08),
            (Property::PhysicalEnabled, 0x20),
            (Property::PhysicalChannel, 0x21),
            (Property::PhysicalTxPower, 0x26),
            (Property::MacRawStreamEnabled, 0x37),
            (Property::MacPromiscuousMode, 0x38),
            (Property::Stream(PropertyStream::Debug), 0x70),
            (Property::Stream(PropertyStream::Raw), 0x71),
            (Property::Stream(PropertyStream::Net), 0x72),
            (Property::Stream(PropertyStream::NetInsecure), 0x73),
            (Property::Stream(PropertyStream::Log), 0x74),
        ];

        for (prop, id) in cases {
            assert_eq!(prop.id(), id);
            assert_eq!(Property::<NoVendor>::try_from(id), Ok(prop));
        }
    }

    #[test]
    fn vendor_property_packed_len_matches_its_id() {
        use crate::codec::vendor::test_vendor::{TestValue, TestVendor};

        let prop = Property::<TestVendor>::Vendor(TestValue(513));
        assert_eq!(prop.packed_len(), 2);
    }
}
