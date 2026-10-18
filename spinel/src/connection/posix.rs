use core::fmt;
use std::collections::HashMap;
use std::time::Duration;

use bytes::Bytes;
use futures::{sink::SinkExt, stream::StreamExt};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    select,
    sync::{
        broadcast::{self, Receiver},
        mpsc, oneshot,
    },
    time::Instant,
};
use tokio_serial::SerialPortBuilderExt;
use tokio_util::codec::{Decoder, Framed};

use super::SpinelHostConnection;
use crate::{
    codec::{NoVendor, PackedU32, ResetReason, Status},
    Command, Error, Frame, HdlcCodec, Header, Property, PropertyStream,
};

type FrameResult = Result<Frame, Error>;
type OneshotFrameReply = oneshot::Sender<Result<oneshot::Receiver<FrameResult>, Error>>;
type BroadcastFrameReply = oneshot::Sender<Result<Receiver<Frame>, Error>>;

/// A TID with a value of zero is reserved for messages where a response is not expected.
///
/// Start the TID at 1 to avoid the reserved value.
const TID_START: u8 = 1;

/// A request that awaits its response frame.
struct PendingRequest {
    /// Channel that resolves the caller with the response or an error.
    sender: oneshot::Sender<FrameResult>,

    /// Time when the request expires with [`Error::RequestTimeout`].
    deadline: Instant,
}

#[derive(Debug)]
enum PosixSpinelHostMessage {
    Noop { reply: OneshotFrameReply },
    Reset { reply: OneshotFrameReply },
    LastStatus { reply: OneshotFrameReply },
    RadioFirmwareVersion { reply: OneshotFrameReply },
    SubscribeResetMessage { reply: BroadcastFrameReply },
    SubscribeDebugBroadcast { reply: BroadcastFrameReply },
    SubscribeNetBroadcast { reply: BroadcastFrameReply },
    SubscribeNetInsecureBroadcast { reply: BroadcastFrameReply },
    SubscribeLogBroadcast { reply: BroadcastFrameReply },
}

impl fmt::Display for PosixSpinelHostMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PosixSpinelHostMessage::Noop { .. } => write!(f, "Noop"),
            PosixSpinelHostMessage::Reset { .. } => write!(f, "Reset"),
            PosixSpinelHostMessage::LastStatus { .. } => write!(f, "LastStatus"),
            PosixSpinelHostMessage::RadioFirmwareVersion { .. } => {
                write!(f, "RadioFirmwareVersion")
            }
            PosixSpinelHostMessage::SubscribeResetMessage { .. } => {
                write!(f, "SubscribeResetMessage")
            }
            PosixSpinelHostMessage::SubscribeDebugBroadcast { .. } => {
                write!(f, "SubscribeDebugBroadcast")
            }
            PosixSpinelHostMessage::SubscribeNetBroadcast { .. } => {
                write!(f, "SubscribeNetBroadcast")
            }
            PosixSpinelHostMessage::SubscribeNetInsecureBroadcast { .. } => {
                write!(f, "SubscribeNetInsecureBroadcast")
            }
            PosixSpinelHostMessage::SubscribeLogBroadcast { .. } => {
                write!(f, "SubscribeLogBroadcast")
            }
        }
    }
}

/// Internal message to request a subscription to a specific message type.
enum SubscribeRequest {
    Reset,
    DebugBroadcast,
    NetBroadcast,
    NetInsecureBroadcast,
    LogBroadcast,
}

#[derive(Clone, Debug)]
pub struct PosixSpinelHostHandle {
    transaction: mpsc::UnboundedSender<PosixSpinelHostMessage>,
}

impl PosixSpinelHostHandle {
    const DEFAULT_BROADCAST_CAPACITY: usize = 16;

    /// Create a new [`PosixSpinelHostHandle`] from a Spinel URL
    pub fn new_from_url(_url: &str) -> Result<PosixSpinelHostHandle, Error> {
        // todo: parse URL and open with `new_from_serial`
        todo!()
    }

    pub fn new_from_serial(
        port_name: &str,
        baud: u32,
        iid: u8,
    ) -> Result<PosixSpinelHostHandle, Error> {
        let port = tokio_serial::new(port_name, baud)
            .open_native_async()
            .map_err(|e| {
                log::error!("Serial Config: {e}");
                Error::SerialConfig
            })?;

        Ok(Self::new_from_transport(port, iid))
    }

    /// Create a handle from any byte transport that carries HDLC frames.
    pub fn new_from_transport<T>(transport: T, iid: u8) -> PosixSpinelHostHandle
    where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let (handle_tx, handle_rx) = mpsc::unbounded_channel();
        let stream = HdlcCodec::<NoVendor>::default().framed(transport);

        let host_connection = PosixSpinelHost {
            msg: handle_rx,
            stream,
            iid,
            tid: TID_START,
            lut: HashMap::new(),
            reset_broadcast: broadcast::channel(Self::DEFAULT_BROADCAST_CAPACITY).0,
            debug_broadcast: broadcast::channel(Self::DEFAULT_BROADCAST_CAPACITY).0,
            net_broadcast: broadcast::channel(Self::DEFAULT_BROADCAST_CAPACITY).0,
            net_insecure_broadcast: broadcast::channel(Self::DEFAULT_BROADCAST_CAPACITY).0,
            log_broadcast: broadcast::channel(Self::DEFAULT_BROADCAST_CAPACITY).0,
        };

        host_connection.run();

        Self {
            transaction: handle_tx,
        }
    }

    /// Send a request to the connection actor to subscribe to a specific message type.
    async fn send_subscribe_request(
        &self,
        stream_request: SubscribeRequest,
    ) -> Result<Receiver<Frame>, Error> {
        let (sender, receiver) = oneshot::channel();
        let msg = match stream_request {
            SubscribeRequest::Reset => {
                PosixSpinelHostMessage::SubscribeResetMessage { reply: sender }
            }
            SubscribeRequest::DebugBroadcast => {
                PosixSpinelHostMessage::SubscribeDebugBroadcast { reply: sender }
            }
            SubscribeRequest::NetBroadcast => {
                PosixSpinelHostMessage::SubscribeNetBroadcast { reply: sender }
            }
            SubscribeRequest::NetInsecureBroadcast => {
                PosixSpinelHostMessage::SubscribeNetInsecureBroadcast { reply: sender }
            }
            SubscribeRequest::LogBroadcast => {
                PosixSpinelHostMessage::SubscribeLogBroadcast { reply: sender }
            }
        };
        self.transaction
            .send(msg)
            .map_err(|_| Error::HostConnectionSend)?;
        receiver.await?
    }

    pub async fn subscribe_reset_msg(&self) -> Result<Receiver<Frame>, Error> {
        self.send_subscribe_request(SubscribeRequest::Reset).await
    }

    /// Subscribe to debug broadcast messages
    pub async fn subscribe_debug_broadcast(&self) -> Result<Receiver<Frame>, Error> {
        self.send_subscribe_request(SubscribeRequest::DebugBroadcast)
            .await
    }

    /// Subscribe to network data broadcast messages
    pub async fn subscribe_net_broadcast(&self) -> Result<Receiver<Frame>, Error> {
        self.send_subscribe_request(SubscribeRequest::NetBroadcast)
            .await
    }

    /// Subscribe to insecure network data broadcast messages
    pub async fn subscribe_net_insecure_broadcast(&self) -> Result<Receiver<Frame>, Error> {
        self.send_subscribe_request(SubscribeRequest::NetInsecureBroadcast)
            .await
    }

    /// Subscribe to log broadcast messages
    pub async fn subscribe_log_broadcast(&self) -> Result<Receiver<Frame>, Error> {
        self.send_subscribe_request(SubscribeRequest::LogBroadcast)
            .await
    }

    async fn send_reset(&self) -> Result<(), Error> {
        // todo: switch reset to watch
        // then subscribe to watch point

        let (sender, _receiver) = oneshot::channel();
        let request: PosixSpinelHostMessage = PosixSpinelHostMessage::Reset { reply: sender };

        self.transaction
            .send(request)
            .map_err(|_| Error::HostConnectionSend)?;

        // Don't wait for the response as the device will respond with a [`Command::PropertyValueIs`](crate::Command::PropertyValueIs) message.
        Ok(())
    }

    /// Internal method to send a request to the host connection actor.
    async fn send_request(&self, cmd: Command) -> Result<Frame, Error> {
        let (sender, receiver) = oneshot::channel();

        let request = match cmd {
            Command::Noop => PosixSpinelHostMessage::Noop { reply: sender },
            Command::PropertyValueGet(Property::LastStatus) => {
                PosixSpinelHostMessage::LastStatus { reply: sender }
            }
            Command::PropertyValueGet(Property::NcpVersion) => {
                PosixSpinelHostMessage::RadioFirmwareVersion { reply: sender }
            }
            _ => {
                return Err(Error::Command(cmd.id()));
            }
        };

        self.transaction
            .send(request)
            .map_err(|_| Error::HostConnectionSend)?;

        receiver.await??.await.map_err(Error::from)?
    }
}

impl SpinelHostConnection for PosixSpinelHostHandle {
    async fn noop(&self) -> Result<(), Error> {
        let response = self.send_request(Command::Noop).await?;
        if let Some(status) = response.last_status() {
            if status == Status::Ok {
                Ok(())
            } else {
                Err(Error::Status(u32::from(status)))
            }
        } else {
            Err(Error::UnexpectedResponse(response.command().id()))
        }
    }

    async fn reset(&self) -> Result<(), Error> {
        self.send_reset().await?;

        // todo: process response

        Ok(())
    }

    async fn last_reset_reason(&self) -> Result<(), Error> {
        // check cache value
        todo!();
    }

    async fn last_status(&self) -> Result<(), Error> {
        let _response = self
            .send_request(Command::PropertyValueGet(Property::LastStatus))
            .await?;

        // todo: process response

        Ok(())
    }

    async fn controller_version(&self) -> Result<Bytes, Error> {
        let response = self
            .send_request(Command::PropertyValueGet(Property::NcpVersion))
            .await?;

        let cmd_id = response.command().id();
        match response.command {
            Command::PropertyValueIs(Property::NcpVersion, value) => Ok(value),
            _ => Err(Error::UnexpectedResponse(cmd_id)),
        }
    }
}

struct PosixSpinelHost<T> {
    /// Message request channel from the host
    msg: mpsc::UnboundedReceiver<PosixSpinelHostMessage>,

    /// HDLC encoded stream of messages coming from a byte transport
    stream: Framed<T, HdlcCodec>,

    /// Instance ID
    iid: u8,

    /// Cursor for the next transaction ID to try
    tid: u8,

    /// Lookup table for transaction ID to pending request
    lut: HashMap<u8, PendingRequest>,

    reset_broadcast: broadcast::Sender<Frame>,
    debug_broadcast: broadcast::Sender<Frame>,
    net_broadcast: broadcast::Sender<Frame>,
    net_insecure_broadcast: broadcast::Sender<Frame>,
    log_broadcast: broadcast::Sender<Frame>,
}

impl<T> PosixSpinelHost<T>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    /// Timeout for property requests.
    ///
    /// Matches the host driver wait time for property responses.
    const PROPERTY_TIMEOUT: Duration = Duration::from_secs(2);

    /// Period between sweeps for expired requests.
    const EXPIRY_SWEEP_PERIOD: Duration = Duration::from_millis(250);

    /// The IID that carries unsolicited notifications for every instance.
    const BROADCAST_IID: u8 = 0;

    /// Bound for one transport write.
    ///
    /// The device reads, the expiry sweep, and the writes share the actor
    /// task, so a blocked link must not stall the loop. A timed-out write
    /// resolves the request with [`Error::RequestTimeout`] and leaves the
    /// frame in the sink buffer, where the next write resumes flushing it.
    const SEND_TIMEOUT: Duration = Duration::from_secs(1);

    fn run(mut self) {
        tokio::spawn(async move {
            let mut expiry = tokio::time::interval(Self::EXPIRY_SWEEP_PERIOD);
            expiry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                select! {
                    msg = self.msg.recv() => {
                        match msg {
                            Some(msg) => {
                                log::trace!("Received host request: {msg}");
                                self.process_handle_msg(msg).await;
                            }
                            None => {
                                // Every handle has dropped, so no request
                                // can arrive. End the task to release the
                                // transport.
                                log::trace!("All handles dropped, actor stopping");
                                break;
                            }
                        }
                    }

                    Some(stream_msg) = self.stream.next() => {
                        log::trace!("Received raw frame from device: {stream_msg:?}");
                        match stream_msg {
                            Ok(frame) => {
                                self.process_frame(frame);
                            }
                            Err(e) => {
                                log::error!("Stream error: {e:?}");
                            }
                        }
                    }

                    _ = expiry.tick() => {
                        self.expire_requests();
                    }
                }
            }
        });
    }

    /// Route a received frame to a pending request or a broadcast channel.
    ///
    /// Frames for the configured IID and the broadcast IID pass. OpenThread
    /// devices emit unsolicited notifications, including resets, on the
    /// broadcast IID.
    fn process_frame(&mut self, frame: Frame) {
        let frame_iid = frame.header().iid();
        if frame_iid != self.iid && frame_iid != Self::BROADCAST_IID {
            log::trace!("Ignoring frame for IID: {frame_iid}");
            return;
        }

        let tid = frame.header().tid();
        if tid == 0 {
            self.process_unsolicited_frame(frame);
        } else {
            match self.lut.remove(&tid) {
                Some(pending) => {
                    let _ = pending.sender.send(Ok(frame));
                }
                None => {
                    log::error!("No response channel for TID: {tid}");
                }
            }
        }
    }

    /// Route an unsolicited frame (TID zero) to its broadcast channel.
    fn process_unsolicited_frame(&mut self, frame: Frame) {
        // todo: rework so that the payload is broadcast, not the frame
        match frame.command() {
            Command::PropertyValueIs(Property::LastStatus, bytes) => {
                match PackedU32::decode_checked(&bytes) {
                    Ok((status, _)) => match ResetReason::try_from(status) {
                        Ok(reason) => {
                            log::trace!("Reset reason: {reason:?}");
                            self.reset_tid();
                            let _ = self.reset_broadcast.send(frame);
                        }
                        Err(_) => {
                            // A status outside the reset-reason range is not a
                            // reset. Pending requests stay intact.
                            log::info!(
                                "Unsolicited status: {}",
                                Status::<NoVendor>::from(status)
                            );
                        }
                    },
                    Err(_) => {
                        log::error!("Malformed status payload: {bytes:?}");
                    }
                }
            }
            Command::PropertyValueIs(Property::Stream(PropertyStream::Debug), _) => {
                let _ = self.debug_broadcast.send(frame);
            }
            Command::PropertyValueIs(Property::Stream(PropertyStream::Net), _) => {
                let _ = self.net_broadcast.send(frame);
            }
            Command::PropertyValueIs(Property::Stream(PropertyStream::NetInsecure), _) => {
                let _ = self.net_insecure_broadcast.send(frame);
            }
            Command::PropertyValueIs(Property::Stream(PropertyStream::Log), _) => {
                let _ = self.log_broadcast.send(frame);
            }
            _ => {
                log::error!("Unknown broadcast message: {}", frame.command());
            }
        }
    }

    /// Process a request received from the host.
    async fn process_handle_msg(&mut self, message: PosixSpinelHostMessage) {
        match message {
            PosixSpinelHostMessage::Noop { reply } => {
                self.send_request(Command::Noop, reply).await;
            }
            PosixSpinelHostMessage::Reset { reply } => {
                self.send_request(Command::Reset, reply).await;
            }
            PosixSpinelHostMessage::LastStatus { reply } => {
                self.send_request(Command::PropertyValueGet(Property::LastStatus), reply)
                    .await;
            }
            PosixSpinelHostMessage::RadioFirmwareVersion { reply } => {
                self.send_request(Command::PropertyValueGet(Property::NcpVersion), reply)
                    .await;
            }
            PosixSpinelHostMessage::SubscribeResetMessage { reply } => {
                let rx = self.reset_broadcast.subscribe();
                let _send_frame_res = reply.send(Ok(rx));
            }
            PosixSpinelHostMessage::SubscribeDebugBroadcast { reply } => {
                let rx = self.debug_broadcast.subscribe();
                let _send_frame_res = reply.send(Ok(rx));
            }
            PosixSpinelHostMessage::SubscribeNetBroadcast { reply } => {
                let rx = self.net_broadcast.subscribe();
                let _send_frame_res = reply.send(Ok(rx));
            }
            PosixSpinelHostMessage::SubscribeNetInsecureBroadcast { reply } => {
                let rx = self.net_insecure_broadcast.subscribe();
                let _send_frame_res = reply.send(Ok(rx));
            }
            PosixSpinelHostMessage::SubscribeLogBroadcast { reply } => {
                let rx = self.log_broadcast.subscribe();
                let _send_frame_res = reply.send(Ok(rx));
            }
        };
    }

    /// Form and send a request to the target device.
    async fn send_request(&mut self, cmd: Command, reply: OneshotFrameReply) {
        log::trace!("Sending request: {cmd:?}");

        let Some(tid) = self.next_free_tid() else {
            log::warn!("No free TID for request: {cmd:?}");
            let _ = reply.send(Err(Error::RequestBusy));
            return;
        };

        let frame = Frame::new(Header::new(self.iid, tid), cmd);

        match self.send_frame(frame).await {
            Ok(_) => {
                let (send, recv) = oneshot::channel::<FrameResult>();
                let _ = reply.send(Ok(recv));
                self.lut.insert(
                    tid,
                    PendingRequest {
                        sender: send,
                        deadline: Instant::now() + Self::PROPERTY_TIMEOUT,
                    },
                );
                self.tid = Self::next_tid(tid);
            }
            Err(e) => {
                log::error!("Request error: {e:?}");
                let _ = reply.send(Err(e));
            }
        }
    }

    /// Send a [`Frame`] to the target device within [`Self::SEND_TIMEOUT`].
    async fn send_frame(&mut self, frame: Frame) -> Result<(), Error> {
        log::trace!("Sending frame: {frame:?}");
        match tokio::time::timeout(Self::SEND_TIMEOUT, self.stream.send(frame)).await {
            Ok(result) => result.map_err(|e| Error::Io(e.to_string())),
            Err(_) => {
                log::error!("Transport send timed out");
                Err(Error::RequestTimeout)
            }
        }
    }

    /// Find the next TID with no pending request, starting at the cursor.
    ///
    /// Returns `None` when every TID has a pending request, so the caller
    /// fails fast instead of replacing one.
    fn next_free_tid(&self) -> Option<u8> {
        let mut tid = self.tid;

        loop {
            if !self.lut.contains_key(&tid) {
                return Some(tid);
            }

            tid = Self::next_tid(tid);
            if tid == self.tid {
                return None;
            }
        }
    }

    /// The TID after the given one, wrapping around to 1 after the maximum.
    fn next_tid(tid: u8) -> u8 {
        if tid == 15 {
            TID_START
        } else {
            tid + 1
        }
    }

    /// Resolve every request whose deadline is past with a timeout error.
    fn expire_requests(&mut self) {
        let now = Instant::now();
        let expired: Vec<u8> = self
            .lut
            .iter()
            .filter(|(_, pending)| pending.deadline <= now)
            .map(|(tid, _)| *tid)
            .collect();

        for tid in expired {
            if let Some(pending) = self.lut.remove(&tid) {
                log::warn!("Request timed out for TID: {tid}");
                let _ = pending.sender.send(Err(Error::RequestTimeout));
            }
        }
    }

    /// Reset the TID cursor and clear out the lookup table.
    fn reset_tid(&mut self) {
        self.tid = TID_START;
        // Resolve every pending request so callers learn about the reset
        // instead of a dropped channel.
        for (_, pending) in self.lut.drain() {
            let _ = pending.sender.send(Err(Error::DeviceReset));
        }
    }
}

#[cfg(test)]
mod tests {
    use bytes::BytesMut;
    use tokio::io::{duplex, DuplexStream};

    use super::*;

    fn setup() -> (PosixSpinelHostHandle, Framed<DuplexStream, HdlcCodec>) {
        setup_with_iid(0)
    }

    fn setup_with_iid(iid: u8) -> (PosixSpinelHostHandle, Framed<DuplexStream, HdlcCodec>) {
        let (host_side, device_side) = duplex(4096);
        let handle = PosixSpinelHostHandle::new_from_transport(host_side, iid);
        let device = HdlcCodec::<NoVendor>::default().framed(device_side);
        (handle, device)
    }

    fn unsolicited_status(status: Status) -> Frame {
        let mut value = BytesMut::new();
        PackedU32::write_to_buffer(u32::from(status), &mut value);
        Frame::new(
            Header::new(0, 0),
            Command::PropertyValueIs(Property::LastStatus, value.freeze()),
        )
    }

    #[tokio::test(start_paused = true)]
    async fn reset_resolves_pending_requests() {
        let (handle, mut device) = setup();

        let pending_handle = handle.clone();
        let pending = tokio::spawn(async move { pending_handle.noop().await });
        let _request = device.next().await.unwrap().unwrap();

        // A reset-range status resolves the pending request with the reset
        // error.
        device
            .send(unsolicited_status(Status::Unknown(114)))
            .await
            .unwrap();

        assert_eq!(pending.await.unwrap(), Err(Error::DeviceReset));
    }

    #[tokio::test(start_paused = true)]
    async fn broadcast_iid_reset_reaches_other_iid() {
        let (handle, mut device) = setup_with_iid(1);

        let pending_handle = handle.clone();
        let pending = tokio::spawn(async move { pending_handle.noop().await });
        let _request = device.next().await.unwrap().unwrap();

        // The device announces the reset on the broadcast IID zero, which
        // resolves requests pending on every instance.
        device
            .send(unsolicited_status(Status::Unknown(114)))
            .await
            .unwrap();

        assert_eq!(pending.await.unwrap(), Err(Error::DeviceReset));
    }

    #[tokio::test(start_paused = true)]
    async fn actor_stops_when_handles_drop() {
        let (handle, mut device) = setup();

        // With every handle gone the actor ends and releases the transport,
        // which the device observes as the end of the stream.
        drop(handle);
        assert!(device.next().await.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn blocked_transport_write_times_out() {
        // A one-byte transport stalls the first frame write. The bounded
        // write resolves the request instead of stalling the actor.
        let (host_side, device_side) = duplex(1);
        let handle = PosixSpinelHostHandle::new_from_transport(host_side, 0);
        let _device = device_side;

        assert_eq!(handle.noop().await, Err(Error::RequestTimeout));
    }

    #[tokio::test(start_paused = true)]
    async fn request_without_response_times_out() {
        let (handle, _device) = setup();
        assert_eq!(handle.noop().await, Err(Error::RequestTimeout));
    }

    #[tokio::test(start_paused = true)]
    async fn request_with_no_free_tid_is_busy() {
        let (handle, _device) = setup();

        // Fill all fifteen TIDs with requests that never get a response.
        let mut pending = Vec::new();
        for _ in 0..15 {
            let h = handle.clone();
            pending.push(tokio::spawn(async move { h.noop().await }));
        }
        tokio::time::sleep(Duration::from_millis(1)).await;

        assert_eq!(handle.noop().await, Err(Error::RequestBusy));

        for task in pending {
            assert_eq!(task.await.unwrap(), Err(Error::RequestTimeout));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn frames_for_another_iid_are_ignored() {
        let (handle, mut device) = setup();
        let mut rx = handle.subscribe_net_broadcast().await.unwrap();

        let net_frame = |iid| {
            Frame::new(
                Header::new(iid, 0),
                Command::PropertyValueIs(Property::Stream(PropertyStream::Net), Bytes::new()),
            )
        };

        device.send(net_frame(1)).await.unwrap();
        device.send(net_frame(0)).await.unwrap();

        let frame = rx.recv().await.unwrap();
        assert_eq!(frame.header().iid(), 0);
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn non_reset_status_leaves_requests_intact() {
        let (handle, mut device) = setup();
        let mut reset_rx = handle.subscribe_reset_msg().await.unwrap();

        // Hold a request open across the unsolicited status.
        let pending_handle = handle.clone();
        let pending = tokio::spawn(async move { pending_handle.noop().await });
        let request = device.next().await.unwrap().unwrap();
        let tid = request.header().tid();

        // A non-reset status must not resolve as a reset notification and
        // must not clear the pending request.
        device
            .send(unsolicited_status(Status::InvalidState))
            .await
            .unwrap();

        // The pending request still resolves with its response.
        device
            .send(Frame::new(
                Header::new(0, tid),
                Command::last_status(Status::Ok),
            ))
            .await
            .unwrap();
        assert_eq!(pending.await.unwrap(), Ok(()));

        // A status in the reset-reason range must broadcast a reset.
        device
            .send(unsolicited_status(Status::Unknown(114)))
            .await
            .unwrap();

        let frame = reset_rx.recv().await.unwrap();
        assert_eq!(frame.last_status(), Some(Status::Unknown(114)));
        assert!(reset_rx.try_recv().is_err());
    }
}
