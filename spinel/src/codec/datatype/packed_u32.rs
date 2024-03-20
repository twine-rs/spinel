use super::PackedByteSlice;
use crate::Error;
use bytes::{BufMut, BytesMut};

/// A packed representation of a `u32` value used in the Spinel protocol.
///
/// Represent a [`u32`] value using a variable number of bytes according to the
/// EXI representation of unsigned integers [1].
///
/// [1] https://www.w3.org/TR/exi/#encodingUnsignedInteger
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PackedU32 {
    /// The packed [`u32`] value.
    pub(crate) array: [u8; 3],
}

impl PackedU32 {
    /// Count the number of bytes used to represent the [`u32`] value
    #[inline]
    pub(crate) fn count_bytes(value: &PackedByteSlice) -> usize {
        let mut count = 0;

        for (i, byte) in value.iter().enumerate() {
            if (byte & 0x80) == 0 {
                count = i + 1;
                break;
            }
        }

        count
    }

    /// Encode a [`u32`] value into a packed representation
    ///
    /// Returns the packed value and number of bytes that were used
    #[inline]
    pub fn encode(value: u32) -> ([u8; 3], usize) {
        let mut result = [0; 3];

        // The encode will always return at least one byte
        let mut count = 1;

        let mut value = value;
        for item in &mut result {
            let mut byte = (value & 0x7F) as u8;

            value >>= 7;

            if value != 0 {
                byte |= 0x80;
                count += 1;
            }

            *item = byte;
        }

        (result, count)
    }

    /// Decode a packed [`u32`] value from a byte slice
    ///
    /// Returns the decoded value and number of bytes that were read
    #[inline]
    pub fn decode(bytes: &PackedByteSlice) -> (u32, usize) {
        let mut value = 0;
        let mut multiplier = 1;
        let mut count = 0;

        for (i, byte) in bytes.iter().enumerate() {
            // The spinel protocol uses a maximum of 3 bytes to represent a u32
            // Bail if we've read more than 3 bytes.
            if i >= 3 {
                break;
            }

            // 2. Read next octet
            // 3. Muliply value of unsigned number represented by the 7 lsb of the
            //    octet by the multiplier and add to the value
            value += (byte & 0x7F) as u32 * multiplier;

            // 4. Multiply the multiplier by 128
            multiplier *= 128;

            // 5. If the msb of the octet was 1, go back to step 2
            //    Otherwise, we're done
            if byte & 0x80 == 0 {
                count = i + 1;
                break;
            }
        }
        (value, count)
    }

    /// Get the expected length of the packed [`u32`] value
    #[inline]
    pub fn packed_len(value: u32) -> usize {
        match value {
            0..=127 => 1,
            128..=16_383 => 2,
            _ => 3,
        }
    }

    /// Pack the value and write the inner [`u32`] value to a buffer.
    #[inline]
    pub fn write_to_buffer(value: u32, buffer: &mut BytesMut) -> usize {
        let (array, count) = PackedU32::encode(value);
        buffer.put_slice(&array[..count]);
        count
    }

    /// Get the length of the packed [`u32`] value
    #[cfg(test)]
    pub fn len(&self) -> usize {
        Self::count_bytes(&self.array)
    }
}

impl From<PackedU32> for u32 {
    fn from(value: PackedU32) -> Self {
        PackedU32::decode(&value.array).0
    }
}

impl From<u32> for PackedU32 {
    fn from(value: u32) -> Self {
        let (array, _) = Self::encode(value);
        PackedU32 { array }
    }
}

impl TryFrom<&PackedByteSlice> for PackedU32 {
    type Error = Error;

    fn try_from(bytes: &PackedByteSlice) -> Result<Self, Self::Error> {
        let count = Self::count_bytes(bytes);

        if count > 3 {
            return Err(Error::PackedU32ByteCount);
        }

        let mut array = [0; 3];
        array.copy_from_slice(&bytes[..count]);

        Ok(PackedU32 { array })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestItem {
        packed: [u8; 3],
        unpacked: u32,
        count: usize,
    }

    const TEST_PACK_ARRAY: [TestItem; 10] = [
        TestItem {
            packed: [0x00, 0x00, 0x00],
            unpacked: 0,
            count: 1,
        },
        TestItem {
            packed: [0x01, 0x00, 0x00],
            unpacked: 1,
            count: 1,
        },
        TestItem {
            packed: [0x7F, 0x00, 0x00],
            unpacked: 127,
            count: 1,
        },
        TestItem {
            packed: [0x80, 0x01, 0x00],
            unpacked: 128,
            count: 2,
        },
        TestItem {
            packed: [0x81, 0x01, 0x00],
            unpacked: 129,
            count: 2,
        },
        TestItem {
            packed: [0xB9, 0x0A, 0x00],
            unpacked: 1_337,
            count: 2,
        },
        TestItem {
            packed: [0xFF, 0x7F, 0x00],
            unpacked: 16_383,
            count: 2,
        },
        TestItem {
            packed: [0x80, 0x80, 0x01],
            unpacked: 16_384,
            count: 3,
        },
        TestItem {
            packed: [0x81, 0x80, 0x01],
            unpacked: 16_385,
            count: 3,
        },
        TestItem {
            packed: [0xFF, 0xFF, 0x7F],
            unpacked: 2_097_151,
            count: 3,
        },
    ];

    #[test]
    fn decode_too_long() {
        let array = [0xFF, 0xFF, 0xFF, 0x0F];
        let packed = &array[..];
        let result = PackedU32::try_from(packed);
        assert_eq!(result, Err(Error::PackedU32ByteCount));
    }

    #[test]
    fn decode_u32() {
        for item in TEST_PACK_ARRAY.iter() {
            let test = PackedU32 { array: item.packed };

            let result: u32 = test.try_into().unwrap();
            assert_eq!(result, item.unpacked);
            assert_eq!(test.len(), item.count);

            let result = PackedU32::decode(&item.packed);
            assert_eq!(result.0, item.unpacked);
            assert_eq!(result.1, item.count);
        }
    }

    #[test]
    fn encode_u32() {
        for item in TEST_PACK_ARRAY.iter() {
            let (result, count) = PackedU32::encode(item.unpacked);
            assert_eq!(result, item.packed);
            assert_eq!(count, item.count);
        }
    }
}
