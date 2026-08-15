use core::fmt;

use crate::codec::vendor::{NoVendor, Vendor, VendorValue};

const STATUS_OK: u32 = 0;
const STATUS_FAILURE: u32 = 1;
const STATUS_UNIMPLEMENTED: u32 = 2;
const STATUS_INVALID_ARGUMENT: u32 = 3;
const STATUS_INVALID_STATE: u32 = 4;
const STATUS_INVALID_COMMAND: u32 = 5;
const STATUS_INVALID_INTERFACE: u32 = 6;
const STATUS_INTERNAL_ERROR: u32 = 7;
const STATUS_SECURITY_ERROR: u32 = 8;
const STATUS_PARSE_ERROR: u32 = 9;
const STATUS_IN_PROGRESS: u32 = 10;
const STATUS_NO_MEMORY: u32 = 11;
const STATUS_BUSY: u32 = 12;
const STATUS_PROPERTY_NOT_FOUND: u32 = 13;
const STATUS_PACKET_DROPPED: u32 = 14;
const STATUS_EMPTY: u32 = 15;
const STATUS_COMMAND_TOO_BIG: u32 = 16;
const STATUS_NO_ACK: u32 = 17;
const STATUS_CCA_FAILURE: u32 = 18;
const STATUS_ALREADY: u32 = 19;
const STATUS_ITEM_NOT_FOUND: u32 = 20;
const STATUS_INVALID_COMMAND_FOR_PROPERTY: u32 = 21;
const STATUS_UNKNOWN_NEIGHBOR: u32 = 22;
const STATUS_NOT_CAPABLE: u32 = 23;
const STATUS_RESPONSE_TIMEOUT: u32 = 24;

/// Status codes for Spinel commands.
///
/// Status codes are sent from the device to the host via
/// [`Property::LastStatus`](crate::Property) and represent the result of the
/// last command executed by the device. Decoding is infallible: an unrecognized
/// core code that a vendor claims decodes to [`Status::Vendor`], and anything
/// else to [`Status::Unknown`], so a malformed or newer status never panics.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Status<V: Vendor = NoVendor> {
    /// The operation has completed successfully.
    Ok,

    /// The operation has failed generically.
    Failure,

    /// The operation has not been implemented.
    Unimplemented,

    /// An argument provided is invalid.
    InvalidArgument,

    /// The operation is not valid in the current state.
    InvalidState,

    /// The command is not recognized.
    InvalidCommand,

    /// The selected interface is not supported.
    InvalidInterface,

    /// An internal runtime error has occurred.
    InternalError,

    /// A security or authentication error has occurred.
    SecurityError,

    /// An error has occurred while parsing the command.
    ParseError,

    /// There is currently an operation in progress.
    InProgress,

    /// The operation has been prevented due to memory pressure.
    NoMemory,

    /// The device is currently performing another operation and cannot perform the request.
    Busy,

    /// The given property is not recognized.
    PropertyNotFound,

    /// The packet was dropped.
    PacketDropped,

    /// The result of the operation is empty.
    Empty,

    /// The command was too large to fit in the internal buffer.
    CommandTooBig,

    /// The packet was not acknowledged.
    NoAck,

    /// The packet was not sent due to CCA failure.
    CcaFailure,

    /// The operation is already in progress or the property was already set to the given value.
    Already,

    /// The given item could not be found in the property.
    ItemNotFound,

    /// The given command cannot be performed on this property.
    InvalidCommandForProperty,

    /// The neighbor is unknown.
    UnknownNeighbor,

    /// The target is not capable of performing the requested operation.
    NotCapable,

    /// No response received from the remote within the timeout period.
    ResponseTimeout,

    /// A vendor-defined status code.
    Vendor(V::Status),

    /// A status code that maps to neither a core nor a vendor status; carries
    /// the raw value.
    Unknown(u32),
}

impl<V: Vendor> fmt::Display for Status<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Status::Ok => write!(f, "Ok"),
            Status::Failure => write!(f, "Failure"),
            Status::Unimplemented => write!(f, "Unimplemented"),
            Status::InvalidArgument => write!(f, "InvalidArgument"),
            Status::InvalidState => write!(f, "InvalidState"),
            Status::InvalidCommand => write!(f, "InvalidCommand"),
            Status::InvalidInterface => write!(f, "InvalidInterface"),
            Status::InternalError => write!(f, "InternalError"),
            Status::SecurityError => write!(f, "SecurityError"),
            Status::ParseError => write!(f, "ParseError"),
            Status::InProgress => write!(f, "InProgress"),
            Status::NoMemory => write!(f, "NoMemory"),
            Status::Busy => write!(f, "Busy"),
            Status::PropertyNotFound => write!(f, "PropertyNotFound"),
            Status::PacketDropped => write!(f, "PacketDropped"),
            Status::Empty => write!(f, "Empty"),
            Status::CommandTooBig => write!(f, "CommandTooBig"),
            Status::NoAck => write!(f, "NoAck"),
            Status::CcaFailure => write!(f, "CcaFailure"),
            Status::Already => write!(f, "Already"),
            Status::ItemNotFound => write!(f, "ItemNotFound"),
            Status::InvalidCommandForProperty => write!(f, "InvalidCommandForProperty"),
            Status::UnknownNeighbor => write!(f, "UnknownNeighbor"),
            Status::NotCapable => write!(f, "NotCapable"),
            Status::ResponseTimeout => write!(f, "ResponseTimeout"),
            Status::Vendor(v) => write!(f, "Vendor(0x{:04x})", v.id()),
            Status::Unknown(value) => write!(f, "Unknown(0x{value:04x})"),
        }
    }
}

impl<V: Vendor> From<u32> for Status<V> {
    fn from(value: u32) -> Self {
        match value {
            STATUS_OK => Status::Ok,
            STATUS_FAILURE => Status::Failure,
            STATUS_UNIMPLEMENTED => Status::Unimplemented,
            STATUS_INVALID_ARGUMENT => Status::InvalidArgument,
            STATUS_INVALID_STATE => Status::InvalidState,
            STATUS_INVALID_COMMAND => Status::InvalidCommand,
            STATUS_INVALID_INTERFACE => Status::InvalidInterface,
            STATUS_INTERNAL_ERROR => Status::InternalError,
            STATUS_SECURITY_ERROR => Status::SecurityError,
            STATUS_PARSE_ERROR => Status::ParseError,
            STATUS_IN_PROGRESS => Status::InProgress,
            STATUS_NO_MEMORY => Status::NoMemory,
            STATUS_BUSY => Status::Busy,
            STATUS_PROPERTY_NOT_FOUND => Status::PropertyNotFound,
            STATUS_PACKET_DROPPED => Status::PacketDropped,
            STATUS_EMPTY => Status::Empty,
            STATUS_COMMAND_TOO_BIG => Status::CommandTooBig,
            STATUS_NO_ACK => Status::NoAck,
            STATUS_CCA_FAILURE => Status::CcaFailure,
            STATUS_ALREADY => Status::Already,
            STATUS_ITEM_NOT_FOUND => Status::ItemNotFound,
            STATUS_INVALID_COMMAND_FOR_PROPERTY => Status::InvalidCommandForProperty,
            STATUS_UNKNOWN_NEIGHBOR => Status::UnknownNeighbor,
            STATUS_NOT_CAPABLE => Status::NotCapable,
            STATUS_RESPONSE_TIMEOUT => Status::ResponseTimeout,
            // Any unassigned code falls through to the vendor, then to Unknown.
            other => match V::Status::try_from_id(other) {
                Ok(vendor) => Status::Vendor(vendor),
                Err(_) => Status::Unknown(other),
            },
        }
    }
}

impl<V: Vendor> From<Status<V>> for u32 {
    fn from(status: Status<V>) -> u32 {
        match status {
            Status::Ok => STATUS_OK,
            Status::Failure => STATUS_FAILURE,
            Status::Unimplemented => STATUS_UNIMPLEMENTED,
            Status::InvalidArgument => STATUS_INVALID_ARGUMENT,
            Status::InvalidState => STATUS_INVALID_STATE,
            Status::InvalidCommand => STATUS_INVALID_COMMAND,
            Status::InvalidInterface => STATUS_INVALID_INTERFACE,
            Status::InternalError => STATUS_INTERNAL_ERROR,
            Status::SecurityError => STATUS_SECURITY_ERROR,
            Status::ParseError => STATUS_PARSE_ERROR,
            Status::InProgress => STATUS_IN_PROGRESS,
            Status::NoMemory => STATUS_NO_MEMORY,
            Status::Busy => STATUS_BUSY,
            Status::PropertyNotFound => STATUS_PROPERTY_NOT_FOUND,
            Status::PacketDropped => STATUS_PACKET_DROPPED,
            Status::Empty => STATUS_EMPTY,
            Status::CommandTooBig => STATUS_COMMAND_TOO_BIG,
            Status::NoAck => STATUS_NO_ACK,
            Status::CcaFailure => STATUS_CCA_FAILURE,
            Status::Already => STATUS_ALREADY,
            Status::ItemNotFound => STATUS_ITEM_NOT_FOUND,
            Status::InvalidCommandForProperty => STATUS_INVALID_COMMAND_FOR_PROPERTY,
            Status::UnknownNeighbor => STATUS_UNKNOWN_NEIGHBOR,
            Status::NotCapable => STATUS_NOT_CAPABLE,
            Status::ResponseTimeout => STATUS_RESPONSE_TIMEOUT,
            Status::Vendor(vendor) => vendor.id(),
            Status::Unknown(value) => value,
        }
    }
}

/// Reasons that a device has reset.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ResetReason {
    PowerOn,
    External,
    Software,
    Fault,
    Crash,
    Assert,
    Other,
    Unknown,
    Watchdog,
}

impl ResetReason {
    const RESET_POWER_ON: u32 = 112;
    const RESET_EXTERNAL: u32 = 113;
    const RESET_SOFTWARE: u32 = 114;
    const RESET_FAULT: u32 = 115;
    const RESET_CRASH: u32 = 116;
    const RESET_ASSERT: u32 = 117;
    const RESET_OTHER: u32 = 118;
    const RESET_UNKNOWN: u32 = 119;
    const RESET_WATCHDOG: u32 = 120;
}

impl TryFrom<u32> for ResetReason {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            Self::RESET_POWER_ON => Ok(Self::PowerOn),
            Self::RESET_EXTERNAL => Ok(Self::External),
            Self::RESET_SOFTWARE => Ok(Self::Software),
            Self::RESET_FAULT => Ok(Self::Fault),
            Self::RESET_CRASH => Ok(Self::Crash),
            Self::RESET_ASSERT => Ok(Self::Assert),
            Self::RESET_OTHER => Ok(Self::Other),
            Self::RESET_UNKNOWN => Ok(Self::Unknown),
            Self::RESET_WATCHDOG => Ok(Self::Watchdog),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_status_round_trips() {
        for code in 0u32..=24 {
            let status = Status::<NoVendor>::from(code);
            assert_eq!(u32::from(status), code);
        }
    }

    #[test]
    fn unknown_status_is_infallible() {
        assert_eq!(Status::<NoVendor>::from(9999), Status::Unknown(9999));
        assert_eq!(u32::from(Status::<NoVendor>::Unknown(9999)), 9999);
    }
}
