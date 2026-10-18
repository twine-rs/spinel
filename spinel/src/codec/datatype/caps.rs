//! Device capability identifiers
//!
//! Capabilities are encoded as a Spinel list: a concatenation of packed
//! unsigned integers, one capability ID each, running to the end of the value.

use core::marker::PhantomData;

use bytes::BytesMut;

use crate::codec::vendor::{NoVendor, Vendor, VendorValue};
use crate::codec::PackedU32;
use crate::Error;

const CAP_MAC_RAW: u32 = 513;

/// A device capability.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Capability<V: Vendor = NoVendor> {
    /// The device supports raw 802.15.4 frame transmit and receive.
    ///
    /// Raw frames flow over [`PropertyStream::Raw`](crate::PropertyStream).
    MacRaw,

    /// A vendor-defined capability.
    Vendor(V::Capability),

    /// A capability that is not recognized.
    ///
    /// Retained rather than dropped so a host tolerates capabilities added by
    /// newer protocol version without losing the raw value.
    Unknown(u32),
}

impl<V: Vendor> Capability<V> {
    /// The ID for this capability on the wire.
    pub fn id(&self) -> u32 {
        match self {
            Capability::MacRaw => CAP_MAC_RAW,
            Capability::Vendor(v) => v.id(),
            Capability::Unknown(id) => *id,
        }
    }
}

impl<V: Vendor> From<u32> for Capability<V> {
    fn from(id: u32) -> Self {
        match id {
            CAP_MAC_RAW => Capability::MacRaw,
            // Any unassigned ID falls through to the vendor profile.
            other => match V::Capability::try_from_id(other) {
                Ok(v) => Capability::Vendor(v),
                Err(_) => Capability::Unknown(other),
            },
        }
    }
}

/// Encode a capability list into `buf` as a Spinel packed list.
pub fn write_caps<V: Vendor>(caps: &[Capability<V>], buf: &mut BytesMut) {
    for cap in caps {
        PackedU32::write_to_buffer(cap.id(), buf);
    }
}

/// Iterator that decodes a Spinel capability list into [`Capability`] values.
///
/// Unrecognized IDs decode to [`Capability::Unknown`]. If the value ends
/// without a valid packed integer, [`Error::CapsMalformed`] is returned. This
/// indicates a truncated value or one that is too long to be a valid packed
/// integer.
pub struct CapabilityIter<'a, V: Vendor = NoVendor> {
    bytes: &'a [u8],
    _marker: PhantomData<V>,
}

impl<'a, V: Vendor> CapabilityIter<'a, V> {
    /// Create an iterator over a raw [`Capability`] list.
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            _marker: PhantomData,
        }
    }
}

impl<V: Vendor> Iterator for CapabilityIter<'_, V> {
    type Item = Result<Capability<V>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.bytes.is_empty() {
            return None;
        }

        let (id, consumed) = match PackedU32::decode_checked(self.bytes) {
            Ok(decoded) => decoded,
            Err(_) => {
                self.bytes = &[];
                return Some(Err(Error::CapsMalformed));
            }
        };

        self.bytes = &self.bytes[consumed..];
        Some(Ok(Capability::from(id)))
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;

    #[test]
    fn vendor_capability_round_trips() {
        use crate::codec::vendor::test_vendor::{TestValue, TestVendor};

        // An unassigned ID decodes through the vendor profile and encodes
        // back to the same wire ID.
        let cap = Capability::<TestVendor>::from(900u32);
        assert_eq!(cap, Capability::Vendor(TestValue(900)));
        assert_eq!(cap.id(), 900);
    }

    #[test]
    fn unknown_caps_round_trip() {
        // 15360 packs to two bytes. With no vendor, it decodes as Unknown.
        let caps = [Capability::<NoVendor>::Unknown(15360)];
        let mut buf = BytesMut::new();
        write_caps(&caps, &mut buf);
        assert_eq!(&buf[..], &[0x80, 0x78]);

        let decoded = CapabilityIter::<NoVendor>::new(&buf)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(decoded, caps);
    }

    #[test]
    fn empty_value_yields_no_caps() {
        assert_eq!(CapabilityIter::<NoVendor>::new(&[]).count(), 0);
    }

    #[test]
    fn mac_raw_cap_decodes_from_list() {
        // 513 packs to [0x81, 0x04].
        let caps = [Capability::<NoVendor>::MacRaw];
        let mut buf = BytesMut::new();
        write_caps(&caps, &mut buf);
        assert_eq!(&buf[..], &[0x81, 0x04]);

        let decoded = CapabilityIter::<NoVendor>::new(&buf)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(decoded, caps);
    }

    #[test]
    fn truncated_value_errors() {
        let mut it = CapabilityIter::<NoVendor>::new(&[0x80]);
        assert_eq!(it.next(), Some(Err(Error::CapsMalformed)));
        assert_eq!(it.next(), None);
    }

    #[test]
    fn valid_cap_then_truncation_errors() {
        // Unknown(15360) (0x80 0x78) followed by a partial varint (0x80).
        let collected = CapabilityIter::<NoVendor>::new(&[0x80, 0x78, 0x80])
            .collect::<Result<Vec<Capability<NoVendor>>, _>>();
        assert_eq!(collected, Err(Error::CapsMalformed));
    }
}
