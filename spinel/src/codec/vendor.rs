//! Vendor extension traits.
//!
//! `spinel` owns the entire core Spinel protocol and all framing/encoding. A
//! vendor extends it by implementing the [`Vendor`] trait bundle on a type and
//! supplying their own [`Property`](crate::Property), [`Command`](crate::Command),
//! [`Capability`](crate::Capability), and [`Status`](crate::Status) enums as
//! the associated [`VendorValue`] types.
//!
//! Vendor values may map to *any* wire ID the core protocol has not assigned:
//! the codec matches its own assigned IDs first and falls through to the vendor
//! trait for everything else. A [`VendorValue::try_from_id`] implementation must
//! reject IDs it does not own so decoding surfaces an error rather than silently
//! failing to decode.

use crate::codec::PackedU32;
use crate::Error;

#[cfg(feature = "defmt")]
pub trait MaybeFormat: defmt::Format {}
#[cfg(feature = "defmt")]
impl<T: defmt::Format + ?Sized> MaybeFormat for T {}

#[cfg(not(feature = "defmt"))]
pub trait MaybeFormat {}
#[cfg(not(feature = "defmt"))]
impl<T: ?Sized> MaybeFormat for T {}

/// A value that maps to and from a Spinel wire ID (a packed `u32`).
///
/// Implemented by a vendor for each of their extension enums.
pub trait VendorValue: Clone + core::fmt::Debug + PartialEq + MaybeFormat {
    /// The wire ID for this value.
    ///
    /// Must be an ID the core protocol does not assign.
    fn id(&self) -> u32;

    /// Decode a value from a wire ID.
    ///
    /// Return an [`Error`] for any ID this vendor does not own so the codec surfaces a decode error instead of decoding
    /// an unrelated ID.
    fn try_from_id(id: u32) -> Result<Self, Error>;

    /// Number of bytes this value occupies once packed.
    fn packed_len(&self) -> usize {
        PackedU32::packed_len(self.id())
    }
}

/// A bundle of vendor extensions for the Spinel codec.
///
/// This should be implemented on a zero-sized marker type. For a pure Spinel implementation with no vendor extensions,
/// use [`NoVendor`].
pub trait Vendor: Clone + core::fmt::Debug + PartialEq + MaybeFormat {
    /// Vendor-defined properties
    type Property: VendorValue;

    /// Vendor-defined commands
    type Command: VendorValue;

    /// Vendor-defined capabilities
    type Capability: VendorValue;

    /// Vendor-defined status codes
    type Status: VendorValue;
}

/// An empty [`VendorValue`], the associated type used by [`NoVendor`].
///
/// It can never be constructed, so a [`Vendor`] generic type instantiated with
/// [`NoVendor`] has no reachable vendor variants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Never {}

#[cfg(feature = "defmt")]
impl defmt::Format for Never {
    fn format(&self, _: defmt::Formatter) {
        match *self {}
    }
}

impl VendorValue for Never {
    fn id(&self) -> u32 {
        match *self {}
    }

    fn try_from_id(id: u32) -> Result<Self, Error> {
        Err(Error::Property(id))
    }
}

/// The default [`Vendor`]: pure core Spinel with no vendor extensions.
///
/// All codec types default to `NoVendor`, so a standard protocol code needs no type
/// parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct NoVendor;

impl Vendor for NoVendor {
    type Property = Never;
    type Command = Never;
    type Capability = Never;
    type Status = Never;
}
