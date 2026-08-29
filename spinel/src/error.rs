cfg_if::cfg_if! {
    if #[cfg(feature = "std")] {
        type IoError = String;
        impl From<std::io::Error> for Error {
            fn from(e: std::io::Error) -> Self {
                Error::Io(e.to_string())
            }
        }

        type HostConnectionRecvError = String;
        impl From<tokio::sync::oneshot::error::RecvError> for Error {
            fn from(e: tokio::sync::oneshot::error::RecvError) -> Self {
                Error::HostConnectionRecv(e.to_string())
            }
        }
    } else {
        type IoError = ();
        type HostConnectionRecvError = ();
    }
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("Unable to parse UTF8 characters")]
    DatatypeParseU8(#[from] core::str::Utf8Error),
    #[error("Invalid header: {0}")]
    Header(u8),
    #[error("Incorrect HDLC checksum: {0}")]
    HdlcChecksum(u16),
    #[error("Incorrect starting delimiter: {0}")]
    HdlcStartDelimiter(u8),
    #[error("Incorrect ending delimiter: {0}")]
    HdlcEndDelimiter(u8),
    #[error("Could not send message, host connection failure")]
    HostConnectionSend,
    #[error("Could not receive message, host connection failure: {0:?}")]
    HostConnectionRecv(HostConnectionRecvError),
    #[error("Unknown command: {0}")]
    Command(u32),
    #[error("IO Error: {0:?}")]
    Io(IoError),
    #[error("Unknown property: {0}")]
    Property(u32),
    #[error("Invalid number of bytes for a packed integer")]
    PackedU32ByteCount,
    #[error("Capabilities list contains a malformed packed integer")]
    CapsMalformed,
    #[error("Incorrect packet length: {0}")]
    PacketLength(usize),
    #[error("Error configuring serial port")]
    SerialConfig,
    #[error("Target status: {0}")]
    Status(u32),
    #[error("Target sent an unexpected response for command: {0}")]
    UnexpectedResponse(u32),
}

// Manual impl instead of `#[derive(defmt::Format)]`: `core::str::Utf8Error` (inside
// `DatatypeParseU8`) and the std-only `IoError`/`HostConnectionRecvError` aliases don't
// implement `defmt::Format`, so their variants are named without their payload.
#[cfg(feature = "defmt")]
impl defmt::Format for Error {
    fn format(&self, fmt: defmt::Formatter) {
        match self {
            Error::DatatypeParseU8(_) => defmt::write!(fmt, "DatatypeParseU8"),
            Error::Header(v) => defmt::write!(fmt, "Header({})", v),
            Error::HdlcChecksum(v) => defmt::write!(fmt, "HdlcChecksum({})", v),
            Error::HdlcStartDelimiter(v) => defmt::write!(fmt, "HdlcStartDelimiter({})", v),
            Error::HdlcEndDelimiter(v) => defmt::write!(fmt, "HdlcEndDelimiter({})", v),
            Error::HostConnectionSend => defmt::write!(fmt, "HostConnectionSend"),
            Error::HostConnectionRecv(_) => defmt::write!(fmt, "HostConnectionRecv"),
            Error::Command(v) => defmt::write!(fmt, "Command({})", v),
            Error::Io(_) => defmt::write!(fmt, "Io"),
            Error::Property(v) => defmt::write!(fmt, "Property({})", v),
            Error::PackedU32ByteCount => defmt::write!(fmt, "PackedU32ByteCount"),
            Error::CapsMalformed => defmt::write!(fmt, "CapsMalformed"),
            Error::PacketLength(v) => defmt::write!(fmt, "PacketLength({})", v),
            Error::SerialConfig => defmt::write!(fmt, "SerialConfig"),
            Error::Status(v) => defmt::write!(fmt, "Status({})", v),
            Error::UnexpectedResponse(v) => defmt::write!(fmt, "UnexpectedResponse({})", v),
        }
    }
}
