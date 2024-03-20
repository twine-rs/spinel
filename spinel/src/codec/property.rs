use crate::error::Error;

#[derive(Clone, Debug, PartialEq)]
pub enum PropertyStream {
    Debug,
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
    ProtocolVersion,
    NcpVersion,
    InterfaceType,
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
