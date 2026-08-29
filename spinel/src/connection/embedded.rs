use crate::codec::NoVendor;
use crate::{Command, Error, Frame, HdlcLiteFrame, Header, Property};
use bytes::BytesMut;
use embedded_io_async::{Read, Write};

/// A TID with a value of zero is reserved for messages where a response is not expected.
/// Start the TID at 1 to avoid the reserved value.
const TID_START: u8 = 1;

/// Initial capacity of the scratch buffer used to accumulate bytes until a full
/// HDLC-Lite frame has been received. Grows on demand, so this is just a sizing hint.
const RX_CAPACITY_HINT: usize = 128;

/// Size of the chunks read from the transport on each poll of the underlying UART.
const READ_CHUNK: usize = 64;

/// A Spinel host connection built directly on [`embedded_io_async`], for `no_std` targets
/// (Embassy, RTIC, or any other executor) where `tokio`/`tokio-serial` are unavailable.
///
/// This is intentionally *not* an implementation of [`SpinelHostConnection`](super::SpinelHostConnection):
/// that trait takes `&self` because [`PosixSpinelHostHandle`](super::PosixSpinelHostHandle) is a cheap,
/// cloneable handle to a background `tokio` task that owns the real mutable state. There is no
/// equivalent background task here -- on most embedded setups a single task owns the UART and talks
/// to the radio directly, so [`EmbeddedSpinelHostConnection`] exposes plain `&mut self` methods
/// instead of paying for interior mutability (e.g. an `embassy-sync` mutex) nobody needs. A caller
/// that does need to share one connection across tasks can wrap it in `embassy_sync::mutex::Mutex`
/// (or similar) themselves.
///
/// Requests are sent and their matching response is awaited inline -- there is no background
/// read loop. Any frame received with a mismatched TID (e.g. an unsolicited `LAST_STATUS` reset,
/// or a `STREAM_DEBUG`/`STREAM_NET`/`STREAM_LOG` broadcast, which use TID `0`) is currently dropped;
/// unlike [`PosixSpinelHostHandle`], there is nowhere to broadcast it to yet.
pub struct EmbeddedSpinelHostConnection<T>
where
    T: Read + Write,
{
    transport: T,
    iid: u8,
    tid: u8,
    rx: BytesMut,
}

impl<T> EmbeddedSpinelHostConnection<T>
where
    T: Read + Write,
{
    /// Create a new connection over an already-configured [`embedded_io_async`] transport
    /// (typically a UART peripheral) and Spinel Instance Identifier (IID).
    pub fn new(transport: T, iid: u8) -> Self {
        Self {
            transport,
            iid,
            tid: TID_START,
            rx: BytesMut::with_capacity(RX_CAPACITY_HINT),
        }
    }

    /// Send a request, wait for the matching response, and return the raw [`Frame`].
    async fn request(&mut self, cmd: Command) -> Result<Frame, Error> {
        let tid = self.tid;
        self.send_frame(Frame::new(Header::new(self.iid, tid), cmd))
            .await?;
        self.increment_tid();

        loop {
            let response = self.recv_frame().await?;
            if response.header().tid() == tid {
                return Ok(response);
            }
            // todo: surface dropped frames (unsolicited resets/broadcasts) instead of discarding them
        }
    }

    /// HDLC-Lite encode and write a [`Frame`] to the transport.
    async fn send_frame(&mut self, frame: Frame) -> Result<(), Error> {
        let mut buf = BytesMut::new();
        HdlcLiteFrame::new(frame).encode(&mut buf)?;

        self.transport
            .write_all(&buf)
            .await
            .map_err(|_| Error::Io(()))?;
        self.transport.flush().await.map_err(|_| Error::Io(()))
    }

    /// Read from the transport, a chunk at a time, until a full HDLC-Lite frame has been
    /// accumulated, then decode and drain it (along with any leading noise) from `rx`.
    async fn recv_frame(&mut self) -> Result<Frame, Error> {
        loop {
            if let Some((start, end)) =
                HdlcLiteFrame::<NoVendor>::find_frame(&self.rx.clone().freeze())
            {
                let frame_bytes = self.rx.split_to(end + 1).freeze().slice(start..);
                return HdlcLiteFrame::<NoVendor>::decode(&frame_bytes)
                    .map(HdlcLiteFrame::into_inner);
            }

            let mut chunk = [0u8; READ_CHUNK];
            let n = self
                .transport
                .read(&mut chunk)
                .await
                .map_err(|_| Error::Io(()))?;
            if n == 0 {
                return Err(Error::Io(()));
            }
            self.rx.extend_from_slice(&chunk[..n]);
        }
    }

    /// Increase the TID by one, wrapping around to 1 if the maximum value is reached.
    fn increment_tid(&mut self) {
        if self.tid == 15 {
            self.tid = TID_START;
        } else {
            self.tid += 1;
        }
    }

    pub async fn noop(&mut self) -> Result<(), Error> {
        let response = self.request(Command::Noop).await?;
        match response.last_status() {
            Some(crate::Status::Ok) => Ok(()),
            Some(status) => Err(Error::Status(u32::from(status))),
            None => Err(Error::UnexpectedResponse(response.command().id())),
        }
    }

    pub async fn controller_version(&mut self) -> Result<bytes::Bytes, Error> {
        let response = self
            .request(Command::PropertyValueGet(Property::NcpVersion))
            .await?;

        let cmd_id = response.command().id();
        match response.command() {
            Command::PropertyValueIs(Property::NcpVersion, value) => Ok(value),
            _ => Err(Error::UnexpectedResponse(cmd_id)),
        }
    }
}
