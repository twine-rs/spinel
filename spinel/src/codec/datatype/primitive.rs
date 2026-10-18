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

/// Define a codec for a fixed-width little-endian integer datatype.
macro_rules! fixed_int_codec {
    ($(#[$meta:meta])* $name:ident, $ty:ty, $len:expr) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq)]
        #[cfg_attr(feature = "defmt", derive(defmt::Format))]
        pub struct $name;

        impl $name {
            /// Decode a value from a byte slice.
            ///
            /// Returns the decoded value and the number of bytes that were read.
            #[inline]
            pub fn decode(bytes: &[u8]) -> Result<($ty, usize), Error> {
                match bytes.get(..$len) {
                    Some(slice) => {
                        let mut array = [0u8; $len];
                        array.copy_from_slice(slice);
                        Ok((<$ty>::from_le_bytes(array), $len))
                    }
                    None => Err(Error::DatatypeTruncated {
                        needed: $len,
                        got: bytes.len(),
                    }),
                }
            }

            /// Write the value to a buffer.
            ///
            /// Returns the number of bytes that were written.
            #[inline]
            pub fn write_to_buffer(value: $ty, buffer: &mut BytesMut) -> usize {
                buffer.put_slice(&value.to_le_bytes());
                $len
            }
        }
    };
}

fixed_int_codec!(
    /// Codec for the Spinel `C` (unsigned 8-bit integer) datatype.
    SpinelU8,
    u8,
    1
);

fixed_int_codec!(
    /// Codec for the Spinel `c` (signed 8-bit integer) datatype.
    SpinelI8,
    i8,
    1
);

fixed_int_codec!(
    /// Codec for the Spinel `S` (unsigned 16-bit integer) datatype.
    SpinelU16,
    u16,
    2
);

fixed_int_codec!(
    /// Codec for the Spinel `L` (unsigned 32-bit integer) datatype.
    SpinelU32,
    u32,
    4
);

fixed_int_codec!(
    /// Codec for the Spinel `X` (unsigned 64-bit integer) datatype.
    SpinelU64,
    u64,
    8
);

/// Codec for the Spinel `d` (length-prefixed data) datatype.
///
/// The payload follows an unsigned 16-bit little-endian length prefix.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct SpinelData;

impl SpinelData {
    /// Decode a data field from a byte slice.
    ///
    /// Returns the payload position within `bytes` and the total number of
    /// bytes that were read, including the length prefix. The caller slices
    /// its own buffer with the returned range, so the payload is not copied.
    #[inline]
    pub fn decode(bytes: &[u8]) -> Result<(core::ops::Range<usize>, usize), Error> {
        let (len, prefix) = SpinelU16::decode(bytes)?;
        let len = usize::from(len);
        let total = prefix + len;

        if bytes.len() < total {
            return Err(Error::DatatypeTruncated {
                needed: total,
                got: bytes.len(),
            });
        }

        Ok((prefix..total, total))
    }

    /// Write a length prefix and the payload to a buffer.
    ///
    /// Returns the number of bytes that were written.
    #[inline]
    pub fn write_to_buffer(payload: &[u8], buffer: &mut BytesMut) -> usize {
        let prefix = SpinelU16::write_to_buffer(payload.len() as u16, buffer);
        buffer.put_slice(payload);
        prefix + payload.len()
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
    fn u8_round_trips() {
        for value in [0x00, 0x01, 0x7F, 0xFF] {
            let mut buffer = BytesMut::new();
            SpinelU8::write_to_buffer(value, &mut buffer);
            assert_eq!(SpinelU8::decode(&buffer), Ok((value, 1)));
        }
    }

    #[test]
    fn i8_round_trips() {
        for value in [i8::MIN, -1, 0, 1, i8::MAX] {
            let mut buffer = BytesMut::new();
            SpinelI8::write_to_buffer(value, &mut buffer);
            assert_eq!(SpinelI8::decode(&buffer), Ok((value, 1)));
        }
    }

    #[test]
    fn u16_little_endian_layout() {
        let mut buffer = BytesMut::new();
        assert_eq!(SpinelU16::write_to_buffer(0x1234, &mut buffer), 2);
        assert_eq!(&buffer[..], &[0x34, 0x12]);
        assert_eq!(SpinelU16::decode(&buffer), Ok((0x1234, 2)));
    }

    #[test]
    fn u32_little_endian_layout() {
        let mut buffer = BytesMut::new();
        assert_eq!(SpinelU32::write_to_buffer(0x1234_5678, &mut buffer), 4);
        assert_eq!(&buffer[..], &[0x78, 0x56, 0x34, 0x12]);
        assert_eq!(SpinelU32::decode(&buffer), Ok((0x1234_5678, 4)));
    }

    #[test]
    fn u64_little_endian_layout() {
        let mut buffer = BytesMut::new();
        assert_eq!(
            SpinelU64::write_to_buffer(0x0102_0304_0506_0708, &mut buffer),
            8
        );
        assert_eq!(
            &buffer[..],
            &[0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]
        );
        assert_eq!(SpinelU64::decode(&buffer), Ok((0x0102_0304_0506_0708, 8)));
    }

    #[test]
    fn fixed_int_truncation_reports_needed_and_got() {
        assert_eq!(
            SpinelU32::decode(&[0x01, 0x02]),
            Err(Error::DatatypeTruncated { needed: 4, got: 2 })
        );
        assert_eq!(
            SpinelU64::decode(&[0x01, 0x02, 0x03]),
            Err(Error::DatatypeTruncated { needed: 8, got: 3 })
        );
    }

    #[test]
    fn data_write_count_adds_prefix_and_payload() {
        // A payload length other than two keeps the returned count honest:
        // the prefix adds to the length instead of multiplying it.
        let mut buffer = BytesMut::new();
        assert_eq!(
            SpinelData::write_to_buffer(&[0xAA, 0xBB, 0xCC], &mut buffer),
            5
        );
        assert_eq!(&buffer[..], &[0x03, 0x00, 0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn data_round_trips_without_copying() {
        let mut buffer = BytesMut::new();
        assert_eq!(SpinelData::write_to_buffer(&[0xAA, 0xBB], &mut buffer), 4);
        assert_eq!(&buffer[..], &[0x02, 0x00, 0xAA, 0xBB]);

        let (range, consumed) = SpinelData::decode(&buffer).unwrap();
        assert_eq!(consumed, 4);
        assert_eq!(&buffer[range], &[0xAA, 0xBB]);
    }

    #[test]
    fn data_decode_rejects_truncated_payload() {
        // The prefix claims 4 bytes, but only 2 follow.
        assert_eq!(
            SpinelData::decode(&[0x04, 0x00, 0xAA, 0xBB]),
            Err(Error::DatatypeTruncated { needed: 6, got: 4 })
        );
    }

    #[test]
    fn data_decode_rejects_missing_prefix() {
        assert_eq!(
            SpinelData::decode(&[0x02]),
            Err(Error::DatatypeTruncated { needed: 2, got: 1 })
        );
    }
}
