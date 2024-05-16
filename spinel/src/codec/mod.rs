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

pub use command::Command;
pub use datatype::{PackedU32, Status};
pub use frame::{Frame, HdlcLiteFrame, Header};
pub use property::Property;
