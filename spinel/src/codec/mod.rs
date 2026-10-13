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
    write_caps, Capability, CapabilityIter, PackedU32, RawRxFrame, RawTxFrame, ResetReason, Status,
};
pub use frame::{Frame, HdlcLiteFrame, Header};
pub use property::{Property, PropertyStream};
pub use vendor::{MaybeFormat, Never, NoVendor, Vendor, VendorValue};
