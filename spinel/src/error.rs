use crate::{Frame, Status};

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

impl From<core::str::Utf8Error> for Error {
    fn from(_: core::str::Utf8Error) -> Self {
        Error::DatatypeParseU8
    }
}

#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "thiserror", derive(thiserror::Error))]
pub enum Error {
    DatatypeParseU8,
    #[cfg_attr(feature = "thiserror", error("Invalid header: {0}"))]
    Header(u8),
    #[cfg_attr(feature = "thiserror", error("Incorrect HDLC checksum: {0}"))]
    HdlcChecksum(u16),
    #[cfg_attr(feature = "thiserror", error("Incorrect starting delimiter: {0}"))]
    HdlcStartDelimiter(u8),
    #[cfg_attr(feature = "thiserror", error("Incorrect ending delimiter: {0}"))]
    HdlcEndDelimiter(u8),
    #[cfg_attr(
        feature = "thiserror",
        error("Could not send message, host connection failure")
    )]
    HostConnectionSend,
    #[cfg_attr(
        feature = "thiserror",
        error("Could not receive message, host connection failure: {0}")
    )]
    HostConnectionRecv(HostConnectionRecvError),
    #[cfg_attr(feature = "thiserror", error("Unknown command: {0}"))]
    Command(u32),
    #[cfg_attr(feature = "thiserror", error("IO Error: {0}"))]
    Io(IoError),
    #[cfg_attr(feature = "thiserror", error("Unknown property: {0}"))]
    Property(u32),
    #[cfg_attr(
        feature = "thiserror",
        error("Invalid number of bytes for a packed integer")
    )]
    PackedU32ByteCount,
    #[cfg_attr(feature = "thiserror", error("Incorrect packet length: {0}"))]
    PacketLength(usize),
    #[cfg_attr(feature = "thiserror", error("Error configuring serial port"))]
    SerialConfig,
    #[cfg_attr(feature = "thiserror", error("Target status: {0}"))]
    Status(Status),
    #[cfg_attr(feature = "thiserror", error("Target sent unexpected response: {0}"))]
    UnexpectedResponse(Frame),
    #[cfg_attr(
        feature = "thiserror",
        error("Unable to parse UTF8 characters. Valid up to: {0}; Len: {1}")
    )]
    Utf8Parse(usize, Option<usize>),
}
