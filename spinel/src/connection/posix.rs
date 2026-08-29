use super::SpinelHostConnection;
use crate::{
    codec::{NoVendor, PackedU32, ResetReason, Status},
    Command, Error, Frame, HdlcCodec, Header, Property, PropertyStream, RawTxFrame,
};
use bytes::Bytes;
use core::fmt;
use futures::{sink::SinkExt, stream::StreamExt};
use std::collections::HashMap;
use tokio::{
    select,
    sync::{
        broadcast::{self, Receiver},
        mpsc, oneshot,
    },
};
use tokio_serial::{SerialPortBuilderExt, SerialStream};
use tokio_util::codec::{Decoder, Framed};

type OneshotFrameReply = oneshot::Sender<Result<oneshot::Receiver<Frame>, Error>>;
type BroadcastFrameReply = oneshot::Sender<Result<Receiver<Frame>, Error>>;

/// A TID with a value of zero is reserved for messages where a response is not expected.
/// Start the TID at 1 to avoid the reserved value.
const TID_START: u8 = 1;

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
    TransmitRaw {
        frame: RawTxFrame,
        reply: OneshotFrameReply,
    },
    SubscribeRawBroadcast { reply: BroadcastFrameReply },
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
            PosixSpinelHostMessage::TransmitRaw { .. } => write!(f, "TransmitRaw"),
            PosixSpinelHostMessage::SubscribeRawBroadcast { .. } => {
                write!(f, "SubscribeRawBroadcast")
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
    RawBroadcast,
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
        let (handle_tx, handle_rx) = mpsc::unbounded_channel();

        let port = tokio_serial::new(port_name, baud)
            .open_native_async()
            .map_err(|e| {
                log::error!("Serial Config: {e}");
                Error::SerialConfig
            })?;
        let stream = HdlcCodec::<NoVendor>::default().framed(port);

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
            raw_broadcast: broadcast::channel(Self::DEFAULT_BROADCAST_CAPACITY).0,
        };

        host_connection.run();

        Ok(Self {
            transaction: handle_tx,
        })
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
            SubscribeRequest::RawBroadcast => {
                PosixSpinelHostMessage::SubscribeRawBroadcast { reply: sender }
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

    /// Subscribe to raw 802.15.4 frames received by the radio.
    pub async fn subscribe_raw_broadcast(&self) -> Result<Receiver<Frame>, Error> {
        self.send_subscribe_request(SubscribeRequest::RawBroadcast)
            .await
    }

    /// Transmit a raw 802.15.4 frame through the radio.
    pub async fn transmit_raw(&self, frame: &RawTxFrame) -> Result<(), Error> {
        let (sender, receiver) = oneshot::channel();

        self.transaction
            .send(PosixSpinelHostMessage::TransmitRaw {
                frame: frame.clone(),
                reply: sender,
            })
            .map_err(|_| Error::HostConnectionSend)?;

        let response = receiver.await??.await.map_err(Error::from)?;

        match response.last_status() {
            Some(Status::Ok) => Ok(()),
            Some(status) => Err(Error::Status(u32::from(status))),
            None => Err(Error::UnexpectedResponse(response.command().id())),
        }
    }

    async fn send_reset(&self) -> Result<(), Error> {
        // todo: switch reset to watch
        // then subscribe to watch point

        let (sender, _receiver) = oneshot::channel();
        let request: PosixSpinelHostMessage = PosixSpinelHostMessage::Reset { reply: sender };

        self.transaction
            .send(request)
            .map_err(|_| Error::HostConnectionSend)?;

        // todo: timeout
        // wait on watchpoint

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

        // todo: add timeout
        // todo: this call is not that readable
        receiver.await??.await.map_err(Error::from)
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

struct PosixSpinelHost {
    /// Message request channel from the host
    msg: mpsc::UnboundedReceiver<PosixSpinelHostMessage>,

    /// HDLC encoded stream of messages coming from a serial device
    stream: Framed<SerialStream, HdlcCodec>,

    /// Instance ID
    iid: u8,

    /// Request transaction ID
    tid: u8,

    /// Lookup table for transaction ID to response channel
    lut: HashMap<u8, oneshot::Sender<Frame>>,

    reset_broadcast: broadcast::Sender<Frame>,
    debug_broadcast: broadcast::Sender<Frame>,
    net_broadcast: broadcast::Sender<Frame>,
    net_insecure_broadcast: broadcast::Sender<Frame>,
    log_broadcast: broadcast::Sender<Frame>,
    raw_broadcast: broadcast::Sender<Frame>,
}

impl PosixSpinelHost {
    fn run(mut self) {
        tokio::spawn(async move {
            loop {
                select! {
                    Some(msg) = self.msg.recv() => {
                        log::trace!("Received host request: {msg}");
                        self.process_handle_msg(msg).await;
                    }

                    Some(stream_msg) = self.stream.next() => {
                        log::trace!("Received raw frame from device: {stream_msg:?}");
                        match stream_msg {
                            Ok(frame) => {
                                let tid = frame.header().tid();
                                if tid == 0 {
                                    // todo: rework so that the payload is broadcast, not the frame
                                    match frame.command() {
                                        Command::PropertyValueIs(Property::LastStatus, bytes) => {
                                            match PackedU32::decode_checked(&bytes) {
                                                Ok((reset_reason, _)) => {
                                                    match ResetReason::try_from(reset_reason) {
                                                        Ok(reason) => {
                                                            log::trace!("Reset reason: {reason:?}");
                                                            self.reset_tid();
                                                            let _ = self.reset_broadcast.send(frame);
                                                        }
                                                        Err(e) => {
                                                            log::error!("Invalid reset reason: {e:?}");
                                                        }
                                                    }
                                                }
                                                Err(_) => {
                                                    log::error!("Malformed reset reason payload: {bytes:?}");
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
                                        Command::PropertyValueIs(Property::Stream(PropertyStream::Raw), _) => {
                                            let _ = self.raw_broadcast.send(frame);
                                        }
                                        _ => {
                                            log::error!("Unknown broadcast message: {}", frame.command());
                                        }
                                    }
                                } else {
                                    let response = self.lut.remove(&tid);
                                    match response {
                                        Some(sender) => {
                                            let _ = sender.send(frame);
                                        }
                                        None => {
                                            log::error!("No response channel for TID: {tid}");
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("Stream error: {e:?}");
                            }
                        }
                    }
                }
            }
        });
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
            PosixSpinelHostMessage::TransmitRaw { frame, reply } => {
                self.send_request(
                    Command::PropertyValueSet(
                        Property::Stream(PropertyStream::Raw),
                        frame.to_bytes(),
                    ),
                    reply,
                )
                .await;
            }
            PosixSpinelHostMessage::SubscribeRawBroadcast { reply } => {
                let rx = self.raw_broadcast.subscribe();
                let _send_frame_res = reply.send(Ok(rx));
            }
        };
    }

    /// Form and send a request to the target device.
    async fn send_request(
        &mut self,
        cmd: Command,
        reply: oneshot::Sender<Result<oneshot::Receiver<Frame>, Error>>,
    ) {
        log::trace!("Sending request: {cmd:?}");

        let tid = match Self::find_free_tid(self.tid, &self.lut) {
            Ok(tid) => tid,
            Err(e) => {
                log::error!("Request error: {e:?}");
                let _ = reply.send(Err(e));
                return;
            }
        };

        let frame = Frame::new(Header::new(self.iid, tid), cmd);

        match self.send_frame(frame).await {
            Ok(_) => {
                let (send, recv) = oneshot::channel::<Frame>();
                let _ = reply.send(Ok(recv));
                self.lut.insert(tid, send);
                self.tid = Self::next_tid(tid);
            }
            Err(e) => {
                log::error!("Request error: {e:?}");
                let _ = reply.send(Err(e));
            }
        }
    }

    /// Send a [`Frame`] to the target device.
    async fn send_frame(&mut self, frame: Frame) -> Result<(), Error> {
        log::trace!("Sending frame: {frame:?}");
        self.stream
            .send(frame)
            .await
            .map_err(|e| Error::Io(e.to_string()))
    }

    /// Wrap a TID to the next value in `1..=15`, the range of non-reserved TIDs.
    fn next_tid(tid: u8) -> u8 {
        if tid == 15 {
            TID_START
        } else {
            tid + 1
        }
    }

    /// Find the next TID, starting from `start`, that isn't a key of `in_flight`.
    ///
    /// Reusing a TID that is still awaiting a response would let a stale entry
    /// answer the new request (or vice versa), leaving one of the two callers
    /// waiting on a response that never comes.
    fn find_free_tid(
        start: u8,
        in_flight: &HashMap<u8, oneshot::Sender<Frame>>,
    ) -> Result<u8, Error> {
        let mut tid = start;

        for _ in 0..15 {
            if !in_flight.contains_key(&tid) {
                return Ok(tid);
            }
            tid = Self::next_tid(tid);
        }

        Err(Error::TransactionIdsExhausted)
    }

    /// Reset the TID and clear out the lookup table.
    fn reset_tid(&mut self) {
        self.tid = TID_START;
        self.lut.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_flight(tids: &[u8]) -> HashMap<u8, oneshot::Sender<Frame>> {
        tids.iter()
            .map(|&tid| {
                let (tx, _rx) = oneshot::channel();
                (tid, tx)
            })
            .collect()
    }

    #[test]
    fn find_free_tid_returns_start_when_free() {
        let lut = in_flight(&[]);
        assert_eq!(PosixSpinelHost::find_free_tid(1, &lut), Ok(1));
    }

    #[test]
    fn find_free_tid_skips_in_flight_tids() {
        let lut = in_flight(&[1, 2, 3]);
        assert_eq!(PosixSpinelHost::find_free_tid(1, &lut), Ok(4));
    }

    #[test]
    fn find_free_tid_wraps_around() {
        let lut = in_flight(&[15, 1, 2]);
        assert_eq!(PosixSpinelHost::find_free_tid(15, &lut), Ok(3));
    }

    #[test]
    fn find_free_tid_fails_when_all_in_flight() {
        let lut = in_flight(&(1..=15).collect::<Vec<_>>());
        assert_eq!(
            PosixSpinelHost::find_free_tid(1, &lut),
            Err(Error::TransactionIdsExhausted)
        );
    }

    // Integration tests below exercise `PosixSpinelHost` through a real (in-memory) PTY
    // pair instead of mocking the actor, so the encode/decode/dispatch loop is covered
    // end to end. One end is handed to the actor exactly as `new_from_serial` would; the
    // other end is wrapped as a "fake device" harness the test drives directly.
    use crate::RawRxFrame;

    fn spawn_test_host(port: SerialStream, iid: u8) -> PosixSpinelHostHandle {
        let (handle_tx, handle_rx) = mpsc::unbounded_channel();
        let stream = HdlcCodec::<NoVendor>::default().framed(port);

        PosixSpinelHost {
            msg: handle_rx,
            stream,
            iid,
            tid: TID_START,
            lut: HashMap::new(),
            reset_broadcast: broadcast::channel(PosixSpinelHostHandle::DEFAULT_BROADCAST_CAPACITY)
                .0,
            debug_broadcast: broadcast::channel(PosixSpinelHostHandle::DEFAULT_BROADCAST_CAPACITY)
                .0,
            net_broadcast: broadcast::channel(PosixSpinelHostHandle::DEFAULT_BROADCAST_CAPACITY).0,
            net_insecure_broadcast: broadcast::channel(
                PosixSpinelHostHandle::DEFAULT_BROADCAST_CAPACITY,
            )
            .0,
            log_broadcast: broadcast::channel(PosixSpinelHostHandle::DEFAULT_BROADCAST_CAPACITY).0,
            raw_broadcast: broadcast::channel(PosixSpinelHostHandle::DEFAULT_BROADCAST_CAPACITY).0,
        }
        .run();

        PosixSpinelHostHandle {
            transaction: handle_tx,
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn transmit_raw_returns_ok_on_last_status_ok() {
        let (host_port, device_port) = SerialStream::pair().unwrap();
        let handle = spawn_test_host(host_port, 0);
        let mut harness = HdlcCodec::<NoVendor>::default().framed(device_port);

        let tx_frame = RawTxFrame::new(Bytes::from_static(&[0xDE, 0xAD, 0xBE, 0xEF]));
        let expected_payload = tx_frame.to_bytes();

        let client = tokio::spawn(async move { handle.transmit_raw(&tx_frame).await });

        let request = harness.next().await.unwrap().unwrap();
        assert_eq!(
            request.command(),
            Command::PropertyValueSet(Property::Stream(PropertyStream::Raw), expected_payload)
        );

        harness
            .send(Frame::new(request.header(), Command::last_status(Status::Ok)))
            .await
            .unwrap();

        assert_eq!(client.await.unwrap(), Ok(()));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn transmit_raw_returns_status_error_on_non_ok_last_status() {
        let (host_port, device_port) = SerialStream::pair().unwrap();
        let handle = spawn_test_host(host_port, 0);
        let mut harness = HdlcCodec::<NoVendor>::default().framed(device_port);

        let tx_frame = RawTxFrame::new(Bytes::from_static(&[0xAA]));
        let client = tokio::spawn(async move { handle.transmit_raw(&tx_frame).await });

        let request = harness.next().await.unwrap().unwrap();
        harness
            .send(Frame::new(
                request.header(),
                Command::last_status(Status::Busy),
            ))
            .await
            .unwrap();

        assert_eq!(
            client.await.unwrap(),
            Err(Error::Status(u32::from(Status::<NoVendor>::Busy)))
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn subscribe_raw_broadcast_receives_unsolicited_frame() {
        let (host_port, device_port) = SerialStream::pair().unwrap();
        let handle = spawn_test_host(host_port, 0);
        let mut harness = HdlcCodec::<NoVendor>::default().framed(device_port);

        let mut raw_rx = handle.subscribe_raw_broadcast().await.unwrap();

        let rx_frame = RawRxFrame {
            psdu: Bytes::from_static(&[0x01, 0x02]),
            rssi: -50,
            noise_floor: -90,
            flags: 0,
            channel: 15,
            lqi: 100,
            timestamp_us: 12_345,
            receive_error: 0,
            manufacturer_specific: Bytes::new(),
        };

        harness
            .send(Frame::new(
                Header::new(0, 0),
                Command::PropertyValueIs(Property::Stream(PropertyStream::Raw), rx_frame.to_bytes()),
            ))
            .await
            .unwrap();

        let received = raw_rx.recv().await.unwrap();
        assert_eq!(received.stream_raw_frame(), Some(rx_frame));
    }
}
