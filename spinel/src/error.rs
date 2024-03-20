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
    #[cfg_attr(feature = "thiserror", error("Unknown command: {0}"))]
    Command(u32),
    #[cfg_attr(feature = "thiserror", error("Unknown property: {0}"))]
    Property(u32),
    #[cfg_attr(
        feature = "thiserror",
        error("Invalid number of bytes for a packed integer")
    )]
    PackedU32ByteCount,
    #[cfg_attr(feature = "thiserror", error("Incorrect packet length: {0}"))]
    PacketLength(usize),
}
