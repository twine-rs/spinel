#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(any(feature = "alloc", test))]
extern crate alloc;

pub use bytes::{Bytes, BytesMut};

pub mod codec;
mod error;

mod connection;
pub use connection::SpinelHostConnection;

pub mod rcp;
pub use rcp::{Radio, RcpDevice};

cfg_if::cfg_if! {
    if #[cfg(feature = "std")] {
        pub use codec::HdlcCodec;
        pub use connection::PosixSpinelHostHandle;
    } else {
        pub use connection::EmbeddedSpinelHostConnection;
    }
}

pub use codec::{
    write_caps, Capability, CapabilityIter, Command, Frame, HdlcLiteFrame, Header, MaybeFormat,
    Never, NoVendor, PackedU32, Property, PropertyStream, RawRxFrame, RawTxFrame, ResetReason,
    Status, Vendor, VendorValue,
};
pub use error::Error;
