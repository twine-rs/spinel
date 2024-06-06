use core::fmt;

/// Status codes for Spinel commands.
///
/// Status codes sent from the device to the host via [`Property::LastStatus`](crate::codec::Property). Status codes
/// represent the result of the last command executed by the device.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
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
}

impl Status {
    const STATUS_OK: u8 = 0;
    const STATUS_FAILURE: u8 = 1;
    const STATUS_UNIMPLEMENTED: u8 = 2;
    const STATUS_INVALID_ARGUMENT: u8 = 3;
    const STATUS_INVALID_STATE: u8 = 4;
    const STATUS_INVALID_COMMAND: u8 = 5;
    const STATUS_INVALID_INTERFACE: u8 = 6;
    const STATUS_INTERNAL_ERROR: u8 = 7;
    const STATUS_SECURITY_ERROR: u8 = 8;
    const STATUS_PARSE_ERROR: u8 = 9;
    const STATUS_IN_PROGRESS: u8 = 10;
    const STATUS_NO_MEMORY: u8 = 11;
    const STATUS_BUSY: u8 = 12;
    const STATUS_PROPERTY_NOT_FOUND: u8 = 13;
    const STATUS_PACKET_DROPPED: u8 = 14;
    const STATUS_EMPTY: u8 = 15;
    const STATUS_COMMAND_TOO_BIG: u8 = 16;
    const STATUS_NO_ACK: u8 = 17;
    const STATUS_CCA_FAILURE: u8 = 18;
    const STATUS_ALREADY: u8 = 19;
    const STATUS_ITEM_NOT_FOUND: u8 = 20;
    const STATUS_INVALID_COMMAND_FOR_PROPERTY: u8 = 21;
    const STATUS_UNKNOWN_NEIGHBOR: u8 = 22;
    const STATUS_NOT_CAPABLE: u8 = 23;
    const STATUS_RESPONSE_TIMEOUT: u8 = 24;
}

impl fmt::Display for Status {
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
        }
    }
}

impl TryFrom<u8> for Status {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            Self::STATUS_OK => Ok(Self::Ok),
            Self::STATUS_FAILURE => Ok(Self::Failure),
            Self::STATUS_UNIMPLEMENTED => Ok(Self::Unimplemented),
            Self::STATUS_INVALID_ARGUMENT => Ok(Self::InvalidArgument),
            Self::STATUS_INVALID_STATE => Ok(Self::InvalidState),
            Self::STATUS_INVALID_COMMAND => Ok(Self::InvalidCommand),
            Self::STATUS_INVALID_INTERFACE => Ok(Self::InvalidInterface),
            Self::STATUS_INTERNAL_ERROR => Ok(Self::InternalError),
            Self::STATUS_SECURITY_ERROR => Ok(Self::SecurityError),
            Self::STATUS_PARSE_ERROR => Ok(Self::ParseError),
            Self::STATUS_IN_PROGRESS => Ok(Self::InProgress),
            Self::STATUS_NO_MEMORY => Ok(Self::NoMemory),
            Self::STATUS_BUSY => Ok(Self::Busy),
            Self::STATUS_PROPERTY_NOT_FOUND => Ok(Self::PropertyNotFound),
            Self::STATUS_PACKET_DROPPED => Ok(Self::PacketDropped),
            Self::STATUS_EMPTY => Ok(Self::Empty),
            Self::STATUS_COMMAND_TOO_BIG => Ok(Self::CommandTooBig),
            Self::STATUS_NO_ACK => Ok(Self::NoAck),
            Self::STATUS_CCA_FAILURE => Ok(Self::CcaFailure),
            Self::STATUS_ALREADY => Ok(Self::Already),
            Self::STATUS_ITEM_NOT_FOUND => Ok(Self::ItemNotFound),
            Self::STATUS_INVALID_COMMAND_FOR_PROPERTY => Ok(Self::InvalidCommandForProperty),
            Self::STATUS_UNKNOWN_NEIGHBOR => Ok(Self::UnknownNeighbor),
            Self::STATUS_NOT_CAPABLE => Ok(Self::NotCapable),
            Self::STATUS_RESPONSE_TIMEOUT => Ok(Self::ResponseTimeout),
            _ => Err(()),
        }
    }
}

impl From<Status> for u8 {
    fn from(status: Status) -> u8 {
        match status {
            Status::Ok => Status::STATUS_OK,
            Status::Failure => Status::STATUS_FAILURE,
            Status::Unimplemented => Status::STATUS_UNIMPLEMENTED,
            Status::InvalidArgument => Status::STATUS_INVALID_ARGUMENT,
            Status::InvalidState => Status::STATUS_INVALID_STATE,
            Status::InvalidCommand => Status::STATUS_INVALID_COMMAND,
            Status::InvalidInterface => Status::STATUS_INVALID_INTERFACE,
            Status::InternalError => Status::STATUS_INTERNAL_ERROR,
            Status::SecurityError => Status::STATUS_SECURITY_ERROR,
            Status::ParseError => Status::STATUS_PARSE_ERROR,
            Status::InProgress => Status::STATUS_IN_PROGRESS,
            Status::NoMemory => Status::STATUS_NO_MEMORY,
            Status::Busy => Status::STATUS_BUSY,
            Status::PropertyNotFound => Status::STATUS_PROPERTY_NOT_FOUND,
            Status::PacketDropped => Status::STATUS_PACKET_DROPPED,
            Status::Empty => Status::STATUS_EMPTY,
            Status::CommandTooBig => Status::STATUS_COMMAND_TOO_BIG,
            Status::NoAck => Status::STATUS_NO_ACK,
            Status::CcaFailure => Status::STATUS_CCA_FAILURE,
            Status::Already => Status::STATUS_ALREADY,
            Status::ItemNotFound => Status::STATUS_ITEM_NOT_FOUND,
            Status::InvalidCommandForProperty => Status::STATUS_INVALID_COMMAND_FOR_PROPERTY,
            Status::UnknownNeighbor => Status::STATUS_UNKNOWN_NEIGHBOR,
            Status::NotCapable => Status::STATUS_NOT_CAPABLE,
            Status::ResponseTimeout => Status::STATUS_RESPONSE_TIMEOUT,
        }
    }
}

/// Reasons that a device has reset.
#[derive(Clone, Debug, PartialEq)]
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
