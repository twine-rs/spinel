use crate::{Command, Error};
use bytes::{BufMut, Bytes, BytesMut};

#[derive(Clone, Debug, PartialEq)]
pub struct Header {
    flag: u8,
    iid: u8,
    tid: u8,
}

impl Header {
    const HEADER_FLAG_MASK: u8 = 0b1100_0000;
    const HEADER_FLAG_SHIFT: u32 = 6;
    const HEADER_FLAG: u8 = 0b10;
    const HEADER_IID_MASK: u8 = 0b0011_0000;
    const HEADER_IID_SHIFT: u32 = 4;
    const HEADER_TID_MASK: u8 = 0b0000_1111;

    pub fn new(iid: u8, tid: u8) -> Self {
        Self {
            flag: Self::HEADER_FLAG,
            iid,
            tid,
        }
    }
}

impl From<Header> for u8 {
    fn from(header: Header) -> Self {
        (header.flag << Header::HEADER_FLAG_SHIFT)
            | (header.iid << Header::HEADER_IID_SHIFT)
            | header.tid
    }
}

impl TryFrom<u8> for Header {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        let flag = (value & Self::HEADER_FLAG_MASK).rotate_right(Self::HEADER_FLAG_SHIFT);
        let iid = (value & Self::HEADER_IID_MASK).rotate_right(Self::HEADER_IID_SHIFT);
        let tid = value & Self::HEADER_TID_MASK;

        if flag != Self::HEADER_FLAG {
            return Err(Error::Header(value));
        }

        Ok(Self { flag, iid, tid })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub(crate) header: Header,
    pub(crate) command: Command,
}

impl Frame {
    pub fn new(header: Header, command: Command) -> Self {
        Self { header, command }
    }

    pub fn encode(self, buffer: &mut BytesMut) -> Result<(), Error> {
        let header_byte = u8::from(self.header);
        let command = Bytes::try_from(self.command)?;

        buffer.put_u8(header_byte);
        buffer.put_slice(&command);

        Ok(())
    }

    pub fn decode(buffer: &Bytes) -> Result<Self, Error> {
        if buffer.len() < 2 {
            return Err(Error::PacketLength(buffer.len()));
        }

        Ok(Frame {
            header: Header::try_from(buffer[0])?,
            command: Command::decode(&buffer.clone().split_off(1))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER_IID_01_TID_02: Header = Header {
        flag: 0b10,
        iid: 0x01,
        tid: 0x02,
    };
    const HEADER_IID_01_IID_02_BYTE: u8 = 0b1001_0010;

    #[test]
    fn header_new() {
        let header = Header::new(0x1, 0x2);
        assert_eq!(header, HEADER_IID_01_TID_02);
    }

    #[test]
    fn header_try_from_u8() {
        let header_byte = HEADER_IID_01_IID_02_BYTE;
        let header = Header::try_from(header_byte).unwrap();
        assert_eq!(header, HEADER_IID_01_TID_02);
    }

    #[test]
    fn header_into_u8() {
        let header_byte: u8 = HEADER_IID_01_TID_02.into();
        assert_eq!(header_byte, HEADER_IID_01_IID_02_BYTE);
    }

    #[test]
    fn header_missing_flag() {
        let header_byte = 0b0001_0010;
        let header = Header::try_from(header_byte);
        assert_eq!(header, Err(Error::Header(header_byte)));
    }

    #[test]
    fn frame_decode_at_least_two_bytes() {
        let buffer = Bytes::from_static(&[0x01]);
        let frame = Frame::decode(&buffer);
        assert_eq!(frame, Err(Error::PacketLength(1)));
    }
}
