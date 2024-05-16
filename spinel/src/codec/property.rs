use crate::error::Error;

#[derive(Clone, Debug, PartialEq)]
pub enum PropertyStream {
    /// This stream provides the capability of sending human-readable debugging output which may be displayed in
    /// the host logs.
    ///
    /// The location of newline characters is not assumed by the host. It is the device's responsibility to insert
    /// newline characters where needed. To receive debug output, wait for [`Command::PropertyValueIs`](crate::Command::PropertyValueIs)
    /// to be sent with this property ID.
    Debug,

    /// This stream provides the capability of sending and receiving data packets to and from the currently attached
    /// network.
    ///
    /// The exact format of the frame metadata and data is dependent on the network protocol being used.
    ///
    /// This property is a streaming property, meaning that you cannot explicitly fetch the value of this property. To
    /// receive traffic, wait for [`Command::PropertyValueIs`](crate::Command::PropertyValueIs) to be sent from
    /// the device to the host with this property ID. To send network packets a call to
    /// [`Command::PropertyValueSet`](crate::Command::PropertyValueSet) is required.
    Net,
    NetInsecure,
    Log,
}

/// Spinel Properties
#[derive(Clone, Debug, PartialEq)]
pub enum Property {
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

    /// Special properties representing streams of data.
    ///
    /// All stream properties emit changes asynchronously using [`Command::PropertyValueIs`](crate::Command::PropertyValueIs)
    /// sent from the device to the host. Some properties may allow for sending traffic from the host to the device
    /// (for example IPv6 traffic).
    Stream(PropertyStream),
}

impl Property {
    const PROP_LAST_STATUS: u32 = 0x00;
    const PROP_PROTOCOL_VERSION: u32 = 0x01;
    const PROP_NCP_VERSION: u32 = 0x02;
    const PROP_INTERFACE_TYPE: u32 = 0x03;
    const PROP_STREAM_DEBUG: u32 = 0x70;
    const PROP_STREAM_NET: u32 = 0x71;
    const PROP_STREAM_NET_INSECURE: u32 = 0x73;
    const PROP_STREAM_LOG: u32 = 0x74;

    /// Byte representation of the [`Property`] on the wire
    pub fn id(&self) -> u32 {
        match self {
            Property::LastStatus => Self::PROP_LAST_STATUS,
            Property::ProtocolVersion => Self::PROP_PROTOCOL_VERSION,
            Property::NcpVersion => Self::PROP_NCP_VERSION,
            Property::InterfaceType => Self::PROP_INTERFACE_TYPE,
            Property::Stream(stream) => match stream {
                PropertyStream::Debug => Self::PROP_STREAM_DEBUG,
                PropertyStream::Net => Self::PROP_STREAM_NET,
                PropertyStream::NetInsecure => Self::PROP_STREAM_NET_INSECURE,
                PropertyStream::Log => Self::PROP_STREAM_LOG,
            },
        }
    }

    /// Length of the [`Property`] data when bit packed
    pub fn packed_len(&self) -> usize {
        crate::codec::PackedU32::packed_len(self.id())
    }
}

impl TryFrom<u32> for Property {
    type Error = Error;

    fn try_from(id: u32) -> Result<Self, Self::Error> {
        match id {
            Self::PROP_LAST_STATUS => Ok(Property::LastStatus),
            Self::PROP_PROTOCOL_VERSION => Ok(Property::ProtocolVersion),
            Self::PROP_NCP_VERSION => Ok(Property::NcpVersion),
            Self::PROP_INTERFACE_TYPE => Ok(Property::InterfaceType),
            Self::PROP_STREAM_DEBUG => Ok(Property::Stream(PropertyStream::Debug)),
            Self::PROP_STREAM_NET => Ok(Property::Stream(PropertyStream::Net)),
            Self::PROP_STREAM_NET_INSECURE => Ok(Property::Stream(PropertyStream::NetInsecure)),
            Self::PROP_STREAM_LOG => Ok(Property::Stream(PropertyStream::Log)),
            _ => Err(Error::Property(id)),
        }
    }
}

impl TryFrom<&[u8]> for Property {
    type Error = Error;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        use crate::codec::PackedU32;
        let len = PackedU32::count_bytes(bytes);
        let prop_id = PackedU32::decode(&bytes[..len]).0;
        Property::try_from(prop_id)
    }
}
