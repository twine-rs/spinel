#![cfg_attr(not(feature = "std"), no_std)]

pub mod codec;
mod error;

#[cfg(feature = "std")]
pub use codec::HdlcCodec;
pub use codec::{Command, Frame, HdlcLiteFrame, Header, PackedU32, Property};
pub use error::Error;
