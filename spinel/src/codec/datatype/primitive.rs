use bytes::{BufMut, BytesMut};

use crate::Error;

/// Codec for the Spinel `b` (boolean) datatype.
///
/// A boolean value occupies one byte on the wire. The protocol encodes
/// `false` as 0x00 and `true` as 0x01. Any other byte is a decode error.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct SpinelBool;

impl SpinelBool {
    /// Decode a boolean value from a byte slice.
    ///
    /// Returns the decoded value and the number of bytes that were read.
    #[inline]
    pub fn decode(bytes: &[u8]) -> Result<(bool, usize), Error> {
        match bytes.first() {
            Some(0x00) => Ok((false, 1)),
            Some(0x01) => Ok((true, 1)),
            Some(byte) => Err(Error::DatatypeBool(*byte)),
            None => Err(Error::DatatypeTruncated { needed: 1, got: 0 }),
        }
    }

    /// Encode the value and write it to a buffer.
    ///
    /// Returns the number of bytes that were written.
    #[inline]
    pub fn write_to_buffer(value: bool, buffer: &mut BytesMut) -> usize {
        buffer.put_u8(u8::from(value));
        1
    }
}

/// Codec for the Spinel `C` (unsigned 8-bit integer) datatype.
///
/// The value occupies one byte on the wire.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct SpinelU8;

impl SpinelU8 {
    /// Decode an unsigned 8-bit value from a byte slice.
    ///
    /// Returns the decoded value and the number of bytes that were read.
    #[inline]
    pub fn decode(bytes: &[u8]) -> Result<(u8, usize), Error> {
        match bytes.first() {
            Some(byte) => Ok((*byte, 1)),
            None => Err(Error::DatatypeTruncated { needed: 1, got: 0 }),
        }
    }

    /// Write the value to a buffer.
    ///
    /// Returns the number of bytes that were written.
    #[inline]
    pub fn write_to_buffer(value: u8, buffer: &mut BytesMut) -> usize {
        buffer.put_u8(value);
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bool_decode() {
        assert_eq!(SpinelBool::decode(&[0x00]), Ok((false, 1)));
        assert_eq!(SpinelBool::decode(&[0x01]), Ok((true, 1)));
        assert_eq!(SpinelBool::decode(&[0x01, 0xFF]), Ok((true, 1)));
    }

    #[test]
    fn bool_decode_rejects_other_bytes() {
        assert_eq!(SpinelBool::decode(&[0x02]), Err(Error::DatatypeBool(0x02)));
        assert_eq!(SpinelBool::decode(&[0xFF]), Err(Error::DatatypeBool(0xFF)));
    }

    #[test]
    fn bool_decode_rejects_empty_slice() {
        assert_eq!(
            SpinelBool::decode(&[]),
            Err(Error::DatatypeTruncated { needed: 1, got: 0 })
        );
    }

    #[test]
    fn bool_write_to_buffer() {
        let mut buffer = BytesMut::new();
        assert_eq!(SpinelBool::write_to_buffer(true, &mut buffer), 1);
        assert_eq!(SpinelBool::write_to_buffer(false, &mut buffer), 1);
        assert_eq!(&buffer[..], &[0x01, 0x00]);
    }

    #[test]
    fn u8_decode() {
        assert_eq!(SpinelU8::decode(&[0x00]), Ok((0x00, 1)));
        assert_eq!(SpinelU8::decode(&[0xAB]), Ok((0xAB, 1)));
        assert_eq!(SpinelU8::decode(&[0xAB, 0xCD]), Ok((0xAB, 1)));
    }

    #[test]
    fn u8_decode_rejects_empty_slice() {
        assert_eq!(
            SpinelU8::decode(&[]),
            Err(Error::DatatypeTruncated { needed: 1, got: 0 })
        );
    }

    #[test]
    fn u8_write_to_buffer() {
        let mut buffer = BytesMut::new();
        assert_eq!(SpinelU8::write_to_buffer(0x2A, &mut buffer), 1);
        assert_eq!(&buffer[..], &[0x2A]);
    }

    #[test]
    fn u8_round_trips() {
        for value in [0x00, 0x01, 0x7F, 0xFF] {
            let mut buffer = BytesMut::new();
            SpinelU8::write_to_buffer(value, &mut buffer);
            assert_eq!(SpinelU8::decode(&buffer), Ok((value, 1)));
        }
    }
}
