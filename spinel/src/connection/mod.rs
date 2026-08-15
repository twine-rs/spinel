use bytes::Bytes;

use crate::Error;

cfg_if::cfg_if! {
    if #[cfg(feature = "std")] {
        mod posix;
        pub use posix::PosixSpinelHostHandle;
    }
}

pub trait SpinelHostConnection {
    fn noop(&self) -> impl core::future::Future<Output = Result<(), Error>> + Send;
    fn reset(&self) -> impl core::future::Future<Output = Result<(), Error>> + Send;
    fn last_reset_reason(&self) -> impl core::future::Future<Output = Result<(), Error>> + Send;
    fn last_status(&self) -> impl core::future::Future<Output = Result<(), Error>> + Send;
    fn controller_version(&self)
        -> impl core::future::Future<Output = Result<Bytes, Error>> + Send;
}
