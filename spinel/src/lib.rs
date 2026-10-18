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
        pub use connection::{ConnectionConfig, SpinelHostConnection, PosixSpinelHostHandle};
    }
}

pub use codec::{
    write_caps, Capability, CapabilityIter, Command, Frame, HdlcLiteFrame, Header, MaybeFormat,
    Never, NoVendor, PackedU32, PromiscuousMode, Property, PropertyStream, RawRxFrame, RawTxConfig,
    RawTxDone, RawTxFrame, ResetReason, SpinelBool, SpinelData, SpinelI8, SpinelU16, SpinelU32,
    SpinelU64, SpinelU8, Status, Vendor, VendorValue, RAW_FRAME_FLAG_BAD_FCS, RAW_FRAME_FLAG_DUPE,
    RAW_FRAME_FLAG_TX,
};
pub use error::Error;
