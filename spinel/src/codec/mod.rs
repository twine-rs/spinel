cfg_if::cfg_if! {
    if #[cfg(feature = "std")] {
        mod hdlc;
        pub use hdlc::HdlcCodec;
    }
}

mod command;
pub(crate) mod datatype;
mod frame;
mod property;
pub(crate) mod vendor;

pub use command::Command;
pub use datatype::{
    write_caps, Capability, CapabilityIter, PackedU32, PromiscuousMode, RawRxFrame, RawTxConfig,
    RawTxDone, RawTxFrame, ResetReason, SpinelBool, SpinelData, SpinelI8, SpinelU16, SpinelU32,
    SpinelU64, SpinelU8, Status, RAW_FRAME_FLAG_BAD_FCS, RAW_FRAME_FLAG_DUPE, RAW_FRAME_FLAG_TX,
};
pub use frame::{Frame, HdlcLiteFrame, Header};
pub use property::{Property, PropertyStream};
pub use vendor::{MaybeFormat, Never, NoVendor, Vendor, VendorValue};
