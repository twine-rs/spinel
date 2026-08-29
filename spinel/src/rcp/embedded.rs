use super::{Radio, RcpDevice};
use crate::codec::NoVendor;
use crate::{Error, Frame, HdlcLiteFrame};
use bytes::BytesMut;
use embedded_io_async::{Read, Write};

/// Initial capacity of the scratch buffer used to accumulate bytes until a full
/// HDLC-Lite frame has been received. Grows on demand, so this is just a sizing hint.
const RX_CAPACITY_HINT: usize = 128;

/// Size of the chunks read from the transport on each poll of the underlying UART.
const READ_CHUNK: usize = 64;

/// Owns the UART transport for an [`RcpDevice`] and pumps HDLC-Lite framing over it,
/// the transport-bound counterpart to [`EmbeddedSpinelHostConnection`](crate::connection::EmbeddedSpinelHostConnection)
/// on the host side.
///
/// [`RcpDevice`] itself has no transport or executor dependency by design (see the
/// module docs); this type is for firmware that wants the UART pump handled instead of
/// hand-rolling it against [`HdlcLiteFrame`] directly.
///
/// There are two independent things to drive, matching the two ways a real RCP talks
/// on the wire:
///
/// - [`serve_request`](Self::serve_request) waits for one incoming Host-to-RCP frame,
///   dispatches it, and sends the reply. Call this in a loop from whatever task owns
///   the UART.
/// - [`flush_raw_rx`](Self::flush_raw_rx) checks the radio for a pending received
///   frame and, if there is one, sends the unsolicited RCP-to-Host notification. Call
///   this whenever the radio signals a pending receive (its own task, an interrupt
///   handler, or a periodic poll) -- independently of `serve_request`'s loop.
///
/// Both take `&mut self`, so a caller that needs to drive them from two separate tasks
/// has to share one connection the same way [`EmbeddedSpinelHostConnection`](crate::connection::EmbeddedSpinelHostConnection)
/// documents: wrap it in `embassy_sync::mutex::Mutex` (or similar) themselves.
pub struct EmbeddedRcpConnection<T, R>
where
    T: Read + Write,
    R: Radio,
{
    transport: T,
    device: RcpDevice<R>,
    rx: BytesMut,
}

impl<T, R> EmbeddedRcpConnection<T, R>
where
    T: Read + Write,
    R: Radio,
{
    /// Create a new connection over an already-configured [`embedded_io_async`] transport
    /// (typically a UART peripheral), dispatching to `radio` with the given Instance
    /// Identifier (IID).
    pub fn new(transport: T, radio: R, iid: u8) -> Self {
        Self {
            transport,
            device: RcpDevice::new(radio, iid),
            rx: BytesMut::with_capacity(RX_CAPACITY_HINT),
        }
    }

    /// Borrow the underlying [`RcpDevice`].
    pub fn device(&self) -> &RcpDevice<R> {
        &self.device
    }

    /// Mutably borrow the underlying [`RcpDevice`].
    pub fn device_mut(&mut self) -> &mut RcpDevice<R> {
        &mut self.device
    }

    /// Wait for one incoming Host-to-RCP frame, dispatch it, and send the RCP-to-Host
    /// reply.
    pub async fn serve_request(&mut self) -> Result<(), Error> {
        let request = self.recv_frame().await?;
        let reply = self.device.handle_frame(request);
        self.send_frame(reply).await
    }

    /// Poll the radio for a received raw frame and, if one is pending, send the
    /// unsolicited RCP-to-Host notification. Returns whether a frame was sent.
    pub async fn flush_raw_rx(&mut self, psdu_buf: &mut [u8]) -> Result<bool, Error> {
        let Some(frame) = self.device.poll_raw_rx(psdu_buf) else {
            return Ok(false);
        };
        self.send_frame(frame).await?;
        Ok(true)
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
            HdlcLiteFrame::<NoVendor>::resync_if_desynced(&mut self.rx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Header, Status};
    use alloc::collections::VecDeque;
    use alloc::rc::Rc;
    use alloc::vec::Vec;
    use core::cell::RefCell;
    use core::convert::Infallible;
    use core::future::Future;
    use core::pin::pin;
    use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    /// Poll `fut` to completion without an executor. Every future in this test module
    /// resolves on its first poll (the mock transport never actually blocks), so a
    /// no-op waker is enough -- no need to pull in `tokio`/`futures`, which aren't even
    /// available as dependencies in the `not(std)` configuration this module compiles
    /// under.
    fn block_on<F: Future>(fut: F) -> F::Output {
        fn noop(_: *const ()) {}
        fn clone(_: *const ()) -> RawWaker {
            RawWaker::new(core::ptr::null(), &VTABLE)
        }
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);

        let waker = unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) };
        let mut cx = Context::from_waker(&waker);
        let mut fut = pin!(fut);

        loop {
            if let Poll::Ready(value) = fut.as_mut().poll(&mut cx) {
                return value;
            }
        }
    }

    /// An in-memory loopback transport: `Clone`s share the same queues, so a test can
    /// keep a handle to feed inbound bytes and inspect outbound bytes while the other
    /// clone is owned by the [`EmbeddedRcpConnection`] under test.
    #[derive(Clone, Default)]
    struct MockTransport {
        inbound: Rc<RefCell<VecDeque<u8>>>,
        outbound: Rc<RefCell<Vec<u8>>>,
    }

    impl embedded_io_async::ErrorType for MockTransport {
        type Error = Infallible;
    }

    impl Read for MockTransport {
        async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
            let mut inbound = self.inbound.borrow_mut();
            let n = buf.len().min(inbound.len());
            for slot in buf[..n].iter_mut() {
                *slot = inbound.pop_front().expect("checked above");
            }
            Ok(n)
        }
    }

    impl Write for MockTransport {
        async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
            self.outbound.borrow_mut().extend_from_slice(buf);
            Ok(buf.len())
        }

        async fn flush(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct MockRadio {
        enabled: bool,
        channel: u8,
        tx_power: i8,
        promiscuous: bool,
        pending_rx: Option<bytes::Bytes>,
    }

    impl Radio for MockRadio {
        fn set_enabled(&mut self, enabled: bool) -> Result<(), Status> {
            self.enabled = enabled;
            Ok(())
        }
        fn enabled(&self) -> bool {
            self.enabled
        }
        fn set_channel(&mut self, channel: u8) -> Result<(), Status> {
            self.channel = channel;
            Ok(())
        }
        fn channel(&self) -> u8 {
            self.channel
        }
        fn set_tx_power(&mut self, dbm: i8) -> Result<(), Status> {
            self.tx_power = dbm;
            Ok(())
        }
        fn tx_power(&self) -> i8 {
            self.tx_power
        }
        fn set_promiscuous(&mut self, enabled: bool) -> Result<(), Status> {
            self.promiscuous = enabled;
            Ok(())
        }
        fn promiscuous(&self) -> bool {
            self.promiscuous
        }
        fn hardware_address(&self) -> [u8; 8] {
            [0xAA; 8]
        }
        fn transmit(&mut self, _frame: &[u8]) -> Result<(), Status> {
            Ok(())
        }
        fn try_receive(&mut self, buf: &mut [u8]) -> Option<usize> {
            let frame = self.pending_rx.take()?;
            buf[..frame.len()].copy_from_slice(&frame);
            Some(frame.len())
        }
    }

    fn encode(frame: Frame) -> Vec<u8> {
        let mut buf = BytesMut::new();
        HdlcLiteFrame::new(frame).encode(&mut buf).unwrap();
        buf.to_vec()
    }

    #[test]
    fn serve_request_dispatches_and_replies() {
        let harness = MockTransport::default();
        let mut conn = EmbeddedRcpConnection::new(harness.clone(), MockRadio::default(), 0);

        let request = Frame::new(Header::new(0, 1), Command::Noop);
        harness.inbound.borrow_mut().extend(encode(request));

        block_on(conn.serve_request()).unwrap();

        let sent = harness.outbound.borrow().clone();
        let reply = HdlcLiteFrame::<NoVendor>::decode(&bytes::Bytes::from(sent))
            .unwrap()
            .into_inner();
        assert_eq!(reply.last_status(), Some(Status::Ok));
        assert_eq!(reply.header().tid(), 1);
    }

    #[test]
    fn flush_raw_rx_sends_pending_frame() {
        let harness = MockTransport::default();
        let mut conn = EmbeddedRcpConnection::new(harness.clone(), MockRadio::default(), 3);
        conn.device_mut().radio_mut().pending_rx = Some(bytes::Bytes::from_static(&[0xAB, 0xCD]));

        let mut buf = [0u8; 32];
        let sent = block_on(conn.flush_raw_rx(&mut buf)).unwrap();
        assert!(sent);

        let bytes = harness.outbound.borrow().clone();
        let frame = HdlcLiteFrame::<NoVendor>::decode(&bytes::Bytes::from(bytes))
            .unwrap()
            .into_inner();
        assert_eq!(frame.header().iid(), 3);
        assert_eq!(frame.header().tid(), 0);

        let rx_frame = frame.stream_raw_frame().expect("raw rx frame");
        assert_eq!(rx_frame.psdu, bytes::Bytes::from_static(&[0xAB, 0xCD]));

        assert!(!block_on(conn.flush_raw_rx(&mut buf)).unwrap());
    }
}
