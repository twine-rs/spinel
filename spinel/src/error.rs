use crate::{Frame, Status};
use platform_switch::thiserror;

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
    #[error("Incorrect packet length: {0}")]
    PacketLength(usize),
    #[error("Error configuring serial port")]
    SerialConfig,
    #[error("Target status: {0}")]
    Status(Status),
    #[error("Target sent unexpected response: {0:?}")]
    UnexpectedResponse(Frame),
}
