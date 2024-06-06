#![cfg_attr(not(feature = "std"), no_std)]

pub mod codec;
mod error;

cfg_if::cfg_if! {
    if #[cfg(feature = "std")] {
        pub use codec::HdlcCodec;
        pub use connection::{SpinelHostConnection, PosixSpinelHostHandle};
    }
}

pub use codec::{
    Command, Frame, HdlcLiteFrame, Header, PackedU32, Property, PropertyStream, Status,
};
mod connection;
pub use error::Error;
