#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(any(feature = "alloc", test))]
extern crate alloc;

pub use bytes::{Bytes, BytesMut};

pub mod codec;
mod error;

cfg_if::cfg_if! {
    if #[cfg(feature = "std")] {
        mod connection;
        pub use codec::HdlcCodec;
        pub use connection::{SpinelHostConnection, PosixSpinelHostHandle};
    }
}

pub use codec::{
    write_caps, Capability, CapabilityIter, Command, Frame, HdlcLiteFrame, Header, MaybeFormat,
    Never, NoVendor, PackedU32, Property, PropertyStream, ResetReason, Status, Vendor, VendorValue,
};
pub use error::Error;
