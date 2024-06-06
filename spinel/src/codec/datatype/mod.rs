mod packed_u32;
mod status;

pub use packed_u32::PackedU32;
pub use status::{ResetReason, Status};

/// Type alias for `[u8]`.
/// Used to help clarify the intent of the type when used with packed types.
pub type PackedByteSlice = [u8];
