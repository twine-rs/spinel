use crate::{
    codec::{
        vendor::{NoVendor, Vendor, VendorValue},
        PackedU32, Property, Status,
    },
    error::Error,
};
use bytes::{BufMut, Bytes, BytesMut};
use core::fmt;

const CMD_NOOP: u32 = 0x00;
const CMD_RESET: u32 = 0x01;
const CMD_PROP_VALUE_GET: u32 = 0x02;
const CMD_PROP_VALUE_SET: u32 = 0x03;
const CMD_PROP_VALUE_IS: u32 = 0x06;

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Command<V: Vendor = NoVendor> {
    /// No Operation
    ///
    /// Induces the device to send a success status back to the host. This is primarily used for
    /// liveliness checks.
    #[default]
    Noop,

    /// Reset
    ///
    /// Perform a software reset on the target device. The device will reset and respond with a [`Status`](crate::Status)
    /// message containing the [`ResetReason`](crate::ResetReason).
    Reset,

    /// Get the value of a property
    ///
    /// The device will respond with [`Command::PropertyValueIs`](crate::Command::PropertyValueIs) containing the value
    /// of the property.
    PropertyValueGet(Property<V>),

    /// Set the value of a property
    PropertyValueSet(Property<V>, Bytes),

    /// Notification of the value of a property
    ///
    /// This command is typically sent in response to a [`Command::PropertyValueGet`](crate::Command::PropertyValueGet)
    /// command. However, it can also be sent by the device asynchronously to notify the host of a property value change.
    PropertyValueIs(Property<V>, Bytes),

    /// A vendor-defined command carrying an opaque payload.
    Vendor(V::Command, Bytes),
}

#[cfg(feature = "defmt")]
impl<V: Vendor> defmt::Format for Command<V> {
    fn format(&self, fmt: defmt::Formatter) {
        match self {
            Command::Noop => defmt::write!(fmt, "Noop"),
            Command::Reset => defmt::write!(fmt, "Reset"),
            Command::PropertyValueGet(prop) => defmt::write!(fmt, "PropertyValueGet({})", prop),
            Command::PropertyValueSet(prop, value) => {
                defmt::write!(fmt, "PropertyValueSet({}, {=[u8]})", prop, &value[..])
            }
            Command::PropertyValueIs(prop, value) => {
                defmt::write!(fmt, "PropertyValueIs({}, {=[u8]})", prop, &value[..])
            }
            Command::Vendor(vc, value) => {
                defmt::write!(fmt, "Vendor({}, {=[u8]})", vc, &value[..])
            }
        }
    }
}

impl<V: Vendor> fmt::Display for Command<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::Noop => write!(f, "Noop"),
            Command::Reset => write!(f, "Reset"),
            Command::PropertyValueGet(prop) => write!(f, "Get: {}", prop),
            Command::PropertyValueSet(prop, value) => write!(f, "Set: {} {:?}", prop, value),
            Command::PropertyValueIs(prop, value) => write!(f, "Is: {} {:?}", prop, value),
            Command::Vendor(vc, value) => write!(f, "Vendor: 0x{:x} {:?}", vc.id(), value),
        }
    }
}

impl<V: Vendor> Command<V> {
    /// Build a [`Command::PropertyValueIs`] carrying [`Property::LastStatus`]
    /// with `status` encoded as its packed value.
    pub fn last_status(status: Status<V>) -> Self {
        let mut value = BytesMut::new();
        PackedU32::write_to_buffer(u32::from(status), &mut value);
        Command::PropertyValueIs(Property::LastStatus, value.freeze())
    }

    /// Command identifier
    pub fn id(&self) -> u32 {
        match self {
            Command::Noop => CMD_NOOP,
            Command::Reset => CMD_RESET,
            Command::PropertyValueGet(_) => CMD_PROP_VALUE_GET,
            Command::PropertyValueSet(_, _) => CMD_PROP_VALUE_SET,
            Command::PropertyValueIs(_, _) => CMD_PROP_VALUE_IS,
            Command::Vendor(vc, _) => vc.id(),
        }
    }

    /// Length of the [`Command`] data when bit packed
    pub fn packed_len(&self) -> usize {
        PackedU32::packed_len(self.id())
    }

    /// Length of the [`Command`] payload (packed [`Property`] and optional data)
    pub fn payload_len(&self) -> usize {
        match self {
            Command::Noop | Command::Reset => 0,
            Command::PropertyValueGet(prop) => prop.packed_len(),
            Command::PropertyValueSet(prop, value) | Command::PropertyValueIs(prop, value) => {
                prop.packed_len() + value.len()
            }
            Command::Vendor(_, payload) => payload.len(),
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
            Command::PropertyValueSet(prop, value) | Command::PropertyValueIs(prop, value) => {
                let num = Self::write_to_buffer_with_property(id, prop, buffer);
                buffer.put_slice(value.as_ref());

                num + value.len()
            }
            Command::Vendor(_vc, payload) => {
                let num = PackedU32::write_to_buffer(id, buffer);
                buffer.put_slice(payload.as_ref());

                num + payload.len()
            }
        };

        Ok(())
    }

    /// Encode both the command and property IDs and write them to the buffer.
    fn write_to_buffer_with_property(cmd: u32, prop: Property<V>, buffer: &mut BytesMut) -> usize {
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

        let (id, cmd_id_len) = PackedU32::decode_checked(buffer.as_ref())?;
        let payload = &buffer[cmd_id_len..];

        match id {
            CMD_NOOP => Ok(Command::Noop),
            CMD_RESET => Ok(Command::Reset),
            CMD_PROP_VALUE_GET => {
                let prop = Property::try_from(payload)?;
                Ok(Command::PropertyValueGet(prop))
            }
            CMD_PROP_VALUE_SET => {
                let prop = Property::try_from(payload)?;
                let value = Bytes::copy_from_slice(&payload[prop.packed_len()..]);
                Ok(Command::PropertyValueSet(prop, value))
            }
            CMD_PROP_VALUE_IS => {
                let prop = Property::try_from(payload)?;
                let value = Bytes::copy_from_slice(&payload[prop.packed_len()..]);
                Ok(Command::PropertyValueIs(prop, value))
            }
            // Any unassigned command ID falls through to the vendor profile.
            other => match V::Command::try_from_id(other) {
                Ok(vc) => Ok(Command::Vendor(vc, Bytes::copy_from_slice(payload))),
                Err(_) => Err(Error::Command(other)),
            },
        }
    }
}

impl<V: Vendor> TryFrom<Command<V>> for Bytes {
    type Error = Error;

    fn try_from(cmd: Command<V>) -> Result<Self, Self::Error> {
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
        let cmd = Command::<NoVendor>::decode(&Bytes::new());
        assert_eq!(cmd, Err(Error::PacketLength(0)));
    }

    #[test]
    fn decode_fails_on_unknown_command() {
        let cmd = Command::<NoVendor>::decode(&Bytes::from_static(&[0xFF, 0xFF, 0x7F]));
        assert_eq!(cmd, Err(Error::Command(2_097_151)));
    }

    #[test]
    fn decode_fails_on_malformed_packed_command_id() {
        let cmd = Command::<NoVendor>::decode(&Bytes::from_static(&[0x80]));
        assert_eq!(cmd, Err(Error::PackedU32ByteCount));
    }

    #[test]
    fn property_value_set_round_trips() {
        // Set(HardwareAddress, [0xDE, 0xAD]) -> cmd 0x03, prop 0x08, payload.
        let cmd = Command::<NoVendor>::PropertyValueSet(
            Property::HardwareAddress,
            Bytes::from_static(&[0xDE, 0xAD]),
        );
        let bytes: Bytes = cmd.clone().try_into().unwrap();
        assert_eq!(&bytes[..], &[0x03, 0x08, 0xDE, 0xAD]);
        assert_eq!(Command::decode(&bytes).unwrap(), cmd);
    }
}
