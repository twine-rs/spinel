mod caps;
mod packed_u32;
mod primitive;
mod promiscuous;
mod status;
mod stream_raw;

pub use caps::{write_caps, Capability, CapabilityIter};
pub use packed_u32::PackedU32;
pub use primitive::{
    SpinelBool, SpinelData, SpinelI8, SpinelU16, SpinelU32, SpinelU64, SpinelU8,
};
pub use promiscuous::PromiscuousMode;
pub use status::{ResetReason, Status};
pub use stream_raw::{
    RawRxFrame, RawTxConfig, RawTxDone, RawTxFrame, RAW_FRAME_FLAG_BAD_FCS, RAW_FRAME_FLAG_DUPE,
    RAW_FRAME_FLAG_TX,
};

/// Type alias for `[u8]`.
///
/// Used to help clarify the intent of the type when used with packed types.
pub type PackedByteSlice = [u8];
