use crate::{
    codec::{PackedU32, Property},
    error::Error,
};
use bytes::{BufMut, Bytes, BytesMut};
use core::fmt;

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Command {
    /// No Operation
    ///
    /// Induces the device to send a success status back to the host. This is primarily used for
    /// liveliness checks.
    #[default]
    Noop,

    /// Reset
    ///
    /// Perform a software reset on the target device. The device will reset and respond with a [`Status`] message
    /// containing the [`ResetReason`].
    Reset,

    /// Get the value of a property
    ///
    /// The device will respond with [`Command::PropertyValueIs`](crate::Command::PropertyValueIs) containing the value
    /// of the property.
    PropertyValueGet(Property),

    /// Notification of the value of a property
    ///
    /// This command is typically sent in response to a [`Command::PropertyValueGet`](crate::Command::PropertyValueGet)
    /// command. However, it can also be sent by the device asyncronously to notify the host of a property value change.
    PropertyValueIs(Property, Bytes),
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::Noop => write!(f, "Noop"),
            Command::Reset => write!(f, "Reset"),
            Command::PropertyValueGet(prop) => write!(f, "Get: {}", prop),
            Command::PropertyValueIs(prop, value) => write!(f, "Is: {} {:?}", prop, value),
        }
    }
}

impl Command {
    const CMD_NOOP: u32 = 0x00;
    const CMD_RESET: u32 = 0x01;
    const CMD_PROP_VALUE_GET: u32 = 0x02;
    const _CMD_PROP_VALUE_SET: u32 = 0x03;
    const CMD_PROP_VALUE_IS: u32 = 0x06;

    /// Command identifier
    pub fn id(&self) -> u32 {
        match self {
            Command::Noop => Self::CMD_NOOP,
            Command::Reset => Self::CMD_RESET,
            Command::PropertyValueGet(_) => Self::CMD_PROP_VALUE_GET,
            Command::PropertyValueIs(_, _) => Self::CMD_PROP_VALUE_IS,
        }
    }

    /// Length of the [`Command`] data when bit packed
    pub fn packed_len(&self) -> usize {
        crate::codec::PackedU32::packed_len(self.id())
    }

    /// Length of the [`Command`] payload (packed [`Property`] and optional data)
    pub fn payload_len(&self) -> usize {
        match self {
            Command::Noop => 0,
            Command::Reset => 0,
            Command::PropertyValueGet(prop) => prop.packed_len(),
            Command::PropertyValueIs(prop, value) => prop.packed_len() + value.len(),
        }
    }

    /// Total length of the [`Command`] data when bit packed and including the payload
    #[cfg(test)]
    fn total_packed_len(&self) -> usize {
        self.packed_len() + self.payload_len()
    }

    /// Encode the command and write it to the buffer.
    pub fn encode(self, buffer: &mut BytesMut) -> Result<(), Error> {
        let id = self.id();

        let _num = match self {
            Command::Noop | Command::Reset => PackedU32::write_to_buffer(id, buffer),
            Command::PropertyValueGet(prop) => {
                Self::write_to_buffer_with_property(id, prop, buffer)
            }
            Command::PropertyValueIs(prop, value) => {
                let num = Self::write_to_buffer_with_property(id, prop, buffer);
                buffer.put_slice(value.as_ref());

                num + value.len()
            }
        };

        Ok(())
    }

    /// Encode both the command and property IDs and write them to the buffer.
    fn write_to_buffer_with_property(cmd: u32, prop: Property, buffer: &mut BytesMut) -> usize {
        let (cmd_array, cmd_count) = PackedU32::encode(cmd);
        let (prop_array, prop_count) = PackedU32::encode(prop.id());

        buffer.put_slice(&cmd_array[..cmd_count]);
        buffer.put_slice(&prop_array[..prop_count]);

        cmd_count + prop_count
    }

    /// Decode the command from the buffer.
    pub fn decode(buffer: &Bytes) -> Result<Self, Error> {
        if buffer.is_empty() {
            return Err(Error::PacketLength(0));
        }

        let cmd_id_len = PackedU32::count_bytes(buffer.as_ref());
        let id = PackedU32::decode(&buffer[..cmd_id_len]).0;
        let payload = &buffer[cmd_id_len..];

        match id {
            Self::CMD_NOOP => Ok(Command::Noop),
            Self::CMD_RESET => Ok(Command::Reset),
            Self::CMD_PROP_VALUE_GET => {
                let prop = Property::try_from(payload)?;
                Ok(Command::PropertyValueGet(prop))
            }
            Self::CMD_PROP_VALUE_IS => {
                let prop = Property::try_from(payload)?;
                let value = Bytes::copy_from_slice(&payload[prop.packed_len()..]);
                Ok(Command::PropertyValueIs(prop, value))
            }
            _ => Err(Error::Command(id)),
        }
    }
}

impl TryFrom<Command> for Bytes {
    type Error = Error;

    fn try_from(cmd: Command) -> Result<Self, Self::Error> {
        let id = cmd.id();
        let mut bytes = BytesMut::with_capacity(PackedU32::packed_len(id));
        cmd.encode(&mut bytes)?;
        Ok(bytes.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_CMD_NOOP_WIRE_FMT: [u8; 1] = [0x00];
    const TEST_CMD_RESET_WIRE_FMT: [u8; 1] = [0x01];
    const TEST_CMD_PROP_VALUE_GET_LAST_STATUS_WIRE_FMT: [u8; 2] = [0x02, 0x00];

    struct TestCmdArrayItem {
        /// Command enumeration
        cmd: Command,

        /// Length of the command + property + payload
        len: usize,

        /// Wire format of the command
        bytes: &'static [u8],
    }

    const TEST_CMD_NOOP: TestCmdArrayItem = TestCmdArrayItem {
        cmd: Command::Noop,
        len: 1,
        bytes: &TEST_CMD_NOOP_WIRE_FMT,
    };

    const TEST_CMD_RESET: TestCmdArrayItem = TestCmdArrayItem {
        cmd: Command::Reset,
        len: 1,
        bytes: &TEST_CMD_RESET_WIRE_FMT,
    };

    const TEST_CMD_PROP_VALUE_GET_LAST_STATUS: TestCmdArrayItem = TestCmdArrayItem {
        cmd: Command::PropertyValueGet(Property::LastStatus),
        len: 2,
        bytes: &TEST_CMD_PROP_VALUE_GET_LAST_STATUS_WIRE_FMT,
    };

    static TEST_CMD_ARRAY: [TestCmdArrayItem; 3] = [
        TEST_CMD_NOOP,
        TEST_CMD_RESET,
        TEST_CMD_PROP_VALUE_GET_LAST_STATUS,
    ];

    /// Test all command lengths and byte arrays
    #[test]
    fn try_from_cmd_all_commands() {
        for item in TEST_CMD_ARRAY.iter() {
            let bytes: Bytes = item.cmd.clone().try_into().unwrap();
            assert_eq!(bytes.len(), item.len);
            assert_eq!(bytes, Bytes::from_static(item.bytes));
        }
    }

    #[test]
    fn payload_len() {
        for item in TEST_CMD_ARRAY.iter() {
            println!("Command: {:?}", item.cmd);
            assert_eq!(item.cmd.total_packed_len(), item.len);
        }
    }

    #[test]
    fn decode_all_commands() {
        for item in TEST_CMD_ARRAY.iter() {
            let cmd = Command::decode(&Bytes::from_static(item.bytes)).unwrap();
            assert_eq!(cmd, item.cmd);
        }
    }

    #[test]
    fn decode_fails_on_empty_buffer() {
        let cmd = Command::decode(&Bytes::new());
        assert_eq!(cmd, Err(Error::PacketLength(0)));
    }

    #[test]
    fn decode_fails_on_unknown_command() {
        let cmd = Command::decode(&Bytes::from_static(&[0xFF, 0xFF, 0x7F]));
        assert_eq!(cmd, Err(Error::Command(2_097_151)));
    }
}
