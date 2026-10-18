use core::fmt;
use std::collections::HashMap;
use std::time::Duration;

use bytes::{Bytes, BytesMut};
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
    Capability, CapabilityIter, Command, Error, Frame, HdlcCodec, Header, PromiscuousMode,
    Property, PropertyStream, RawRxFrame, RawTxDone, RawTxFrame, SpinelBool, SpinelU8,
};

type FrameResult = Result<Frame, Error>;
type OneshotFrameReply = oneshot::Sender<Result<oneshot::Receiver<FrameResult>, Error>>;
type BroadcastFrameReply = oneshot::Sender<Result<Receiver<Frame>, Error>>;
type BroadcastRawRxReply = oneshot::Sender<Result<Receiver<RawRxFrame>, Error>>;

/// Timeouts for requests sent to the device.
#[derive(Clone, Debug, PartialEq)]
pub struct ConnectionConfig {
    /// Timeout for property requests.
    ///
    /// The default gives the device two seconds to answer a property request.
    pub property_timeout: Duration,

    /// Timeout for a raw transmit awaiting its transmit-done report.
    ///
    /// Longer than the property timeout, because a transmit can sit in CSMA
    /// backoffs and retries before it completes.
    pub tx_timeout: Duration,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            property_timeout: Duration::from_secs(2),
            tx_timeout: Duration::from_secs(5),
        }
    }
}

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
    Noop {
        reply: OneshotFrameReply,
    },
    Reset {
        reply: OneshotFrameReply,
    },
    LastStatus {
        reply: OneshotFrameReply,
    },
    RadioFirmwareVersion {
        reply: OneshotFrameReply,
    },
    SetProperty {
        prop: Property,
        value: Bytes,
        reply: OneshotFrameReply,
    },
    GetProperty {
        prop: Property,
        reply: OneshotFrameReply,
    },
    TransmitRaw {
        payload: Bytes,
        reply: OneshotFrameReply,
    },
    SubscribeRawRx {
        reply: BroadcastRawRxReply,
    },
    SubscribeResetMessage {
        reply: BroadcastFrameReply,
    },
    SubscribeDebugBroadcast {
        reply: BroadcastFrameReply,
    },
    SubscribeNetBroadcast {
        reply: BroadcastFrameReply,
    },
    SubscribeNetInsecureBroadcast {
        reply: BroadcastFrameReply,
    },
    SubscribeLogBroadcast {
        reply: BroadcastFrameReply,
    },
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
            PosixSpinelHostMessage::SetProperty { prop, .. } => {
                write!(f, "SetProperty({prop})")
            }
            PosixSpinelHostMessage::GetProperty { prop, .. } => {
                write!(f, "GetProperty({prop})")
            }
            PosixSpinelHostMessage::TransmitRaw { .. } => write!(f, "TransmitRaw"),
            PosixSpinelHostMessage::SubscribeRawRx { .. } => write!(f, "SubscribeRawRx"),
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

    /// Capacity of the raw receive broadcast channel.
    ///
    /// Promiscuous capture delivers frames in bursts far above the rate of
    /// the other broadcasts, so the raw stream gets a deeper ring. A
    /// receiver that falls further behind than this loses the oldest frames
    /// with a lag error.
    const RAW_RX_BROADCAST_CAPACITY: usize = 512;

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
        Self::new_from_transport_with_config(transport, iid, ConnectionConfig::default())
    }

    /// Create a handle from a byte transport with explicit timeouts.
    pub fn new_from_transport_with_config<T>(
        transport: T,
        iid: u8,
        config: ConnectionConfig,
    ) -> PosixSpinelHostHandle
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
            tx_radio_tid: None,
            config,
            lut: HashMap::new(),
            reset_broadcast: broadcast::channel(Self::DEFAULT_BROADCAST_CAPACITY).0,
            debug_broadcast: broadcast::channel(Self::DEFAULT_BROADCAST_CAPACITY).0,
            net_broadcast: broadcast::channel(Self::DEFAULT_BROADCAST_CAPACITY).0,
            net_insecure_broadcast: broadcast::channel(Self::DEFAULT_BROADCAST_CAPACITY).0,
            log_broadcast: broadcast::channel(Self::DEFAULT_BROADCAST_CAPACITY).0,
            raw_rx_broadcast: broadcast::channel(Self::RAW_RX_BROADCAST_CAPACITY).0,
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

    /// Set a property value on the device and await the outcome.
    ///
    /// Resolves with the echoed value when the device accepts the set. A
    /// device that replies with a `LastStatus` of `Ok` resolves with an
    /// empty value. An error `LastStatus` on the same transaction ID resolves
    /// as [`Error::Status`] with the reported code.
    pub async fn set_property(&self, prop: Property, value: Bytes) -> Result<Bytes, Error> {
        let (sender, receiver) = oneshot::channel();

        self.transaction
            .send(PosixSpinelHostMessage::SetProperty {
                prop: prop.clone(),
                value,
                reply: sender,
            })
            .map_err(|_| Error::HostConnectionSend)?;

        let response = receiver.await??.await.map_err(Error::from)??;

        let cmd_id = response.command().id();
        match response.command {
            Command::PropertyValueIs(p, v) if p == prop => Ok(v),
            Command::PropertyValueIs(Property::LastStatus, v) => {
                let (code, _) = PackedU32::decode_checked(&v)?;
                if Status::<NoVendor>::from(code) == Status::Ok {
                    Ok(Bytes::new())
                } else {
                    Err(Error::Status(code))
                }
            }
            _ => Err(Error::UnexpectedResponse(cmd_id)),
        }
    }

    /// Get a property value from the device.
    ///
    /// Resolves with the reported value. An error `LastStatus` on the same
    /// transaction ID resolves as [`Error::Status`] with the reported code.
    pub async fn get_property(&self, prop: Property) -> Result<Bytes, Error> {
        let (sender, receiver) = oneshot::channel();

        self.transaction
            .send(PosixSpinelHostMessage::GetProperty {
                prop: prop.clone(),
                reply: sender,
            })
            .map_err(|_| Error::HostConnectionSend)?;

        let response = receiver.await??.await.map_err(Error::from)??;

        let cmd_id = response.command().id();
        match response.command {
            Command::PropertyValueIs(p, v) if p == prop => Ok(v),
            Command::PropertyValueIs(Property::LastStatus, v) => {
                let (code, _) = PackedU32::decode_checked(&v)?;
                Err(Error::Status(code))
            }
            _ => Err(Error::UnexpectedResponse(cmd_id)),
        }
    }

    /// Get the capability list from the device.
    pub async fn capabilities(&self) -> Result<Vec<Capability>, Error> {
        let value = self.get_property(Property::Capabilities).await?;
        CapabilityIter::new(&value).collect()
    }

    /// Set whether the radio physical layer is enabled.
    ///
    /// Encodes the value as the Spinel `b` datatype, one byte 0x00 or 0x01.
    pub async fn set_physical_enabled(&self, enabled: bool) -> Result<(), Error> {
        let mut value = BytesMut::new();
        SpinelBool::write_to_buffer(enabled, &mut value);
        self.set_property(Property::PhysicalEnabled, value.freeze())
            .await
            .map(|_| ())
    }

    /// Set the 802.15.4 channel the radio uses.
    ///
    /// Encodes the value as the Spinel `C` datatype, one byte.
    pub async fn set_physical_channel(&self, channel: u8) -> Result<(), Error> {
        let mut value = BytesMut::new();
        SpinelU8::write_to_buffer(channel, &mut value);
        self.set_property(Property::PhysicalChannel, value.freeze())
            .await
            .map(|_| ())
    }

    /// Set whether the device forwards received raw frames to the host.
    ///
    /// Encodes the value as the Spinel `b` datatype, one byte 0x00 or 0x01.
    pub async fn set_mac_raw_stream_enabled(&self, enabled: bool) -> Result<(), Error> {
        let mut value = BytesMut::new();
        SpinelBool::write_to_buffer(enabled, &mut value);
        self.set_property(Property::MacRawStreamEnabled, value.freeze())
            .await
            .map(|_| ())
    }

    /// Set the MAC promiscuous mode of the radio.
    ///
    /// Encodes the mode as the Spinel `C` datatype, one byte with the mode's
    /// numeric value.
    pub async fn set_mac_promiscuous_mode(&self, mode: PromiscuousMode) -> Result<(), Error> {
        let mut value = BytesMut::new();
        SpinelU8::write_to_buffer(u8::from(mode), &mut value);
        self.set_property(Property::MacPromiscuousMode, value.freeze())
            .await
            .map(|_| ())
    }

    /// Transmit a raw 802.15.4 frame and await the transmit-done report.
    ///
    /// The PSDU in `frame` excludes the 2-byte FCS. The radio computes the
    /// FCS. One raw transmit is allowed in flight at a time, so a second
    /// transmit fails with [`Error::RequestBusy`] while one is pending.
    ///
    /// Resolves with the report only for the radio transmit outcomes `Ok`,
    /// `NoAck`, and `CcaFailure`. Any other status means the device rejected
    /// the request itself, for example `InvalidState` while raw mode is off,
    /// and resolves as [`Error::Status`] with the reported code.
    pub async fn transmit_raw(&self, frame: RawTxFrame) -> Result<RawTxDone, Error> {
        let mut payload = BytesMut::new();
        frame.encode(&mut payload)?;

        let (sender, receiver) = oneshot::channel();
        self.transaction
            .send(PosixSpinelHostMessage::TransmitRaw {
                payload: payload.freeze(),
                reply: sender,
            })
            .map_err(|_| Error::HostConnectionSend)?;

        let response = receiver.await??.await.map_err(Error::from)??;

        // The transmit-done arrives as a LastStatus value on the request TID.
        // A STREAM_RAW value is accepted too, because the Spinel header
        // documents the response under that property.
        let cmd_id = response.command().id();
        let done = match response.command {
            Command::PropertyValueIs(Property::LastStatus, v)
            | Command::PropertyValueIs(Property::Stream(PropertyStream::Raw), v) => {
                RawTxDone::decode(&v)?
            }
            _ => return Err(Error::UnexpectedResponse(cmd_id)),
        };

        // A status outside the radio transmit outcomes means the device
        // rejected the request, which resolves as an error like every other
        // rejected request.
        match done.status {
            Status::Ok | Status::NoAck | Status::CcaFailure => Ok(done),
            status => Err(Error::Status(u32::from(status))),
        }
    }

    /// Subscribe to received raw 802.15.4 frames.
    ///
    /// The channel carries decoded [`RawRxFrame`] values. Frames flow after
    /// raw mode is enabled with [`Self::enable_raw`].
    pub async fn subscribe_raw_rx(&self) -> Result<Receiver<RawRxFrame>, Error> {
        let (sender, receiver) = oneshot::channel();
        self.transaction
            .send(PosixSpinelHostMessage::SubscribeRawRx { reply: sender })
            .map_err(|_| Error::HostConnectionSend)?;
        receiver.await?
    }

    /// Enable raw link mode on the device.
    ///
    /// Runs the mode-enable sequence in order. The sequence verifies the
    /// `MacRaw` capability, enables the radio physical layer, selects the
    /// channel, enables raw frame delivery, and optionally sets promiscuous
    /// mode. A device that reports `InvalidState` has its network stack up,
    /// which resolves as [`Error::DeviceStackUp`]. This method never alters
    /// the device network state itself.
    pub async fn enable_raw(
        &self,
        channel: u8,
        promiscuous: Option<PromiscuousMode>,
    ) -> Result<(), Error> {
        let caps = self.capabilities().await?;
        if !caps.contains(&Capability::MacRaw) {
            return Err(Error::CapabilityMissing(
                Capability::<NoVendor>::MacRaw.id(),
            ));
        }

        self.set_physical_enabled(true)
            .await
            .map_err(Self::map_stack_up)?;
        self.set_physical_channel(channel)
            .await
            .map_err(Self::map_stack_up)?;
        self.set_mac_raw_stream_enabled(true)
            .await
            .map_err(Self::map_stack_up)?;

        if let Some(mode) = promiscuous {
            self.set_mac_promiscuous_mode(mode)
                .await
                .map_err(Self::map_stack_up)?;
        }

        Ok(())
    }

    /// Map an `InvalidState` status to the stack-up error.
    fn map_stack_up(e: Error) -> Error {
        if e == Error::Status(u32::from(Status::<NoVendor>::InvalidState)) {
            Error::DeviceStackUp
        } else {
            e
        }
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

    /// TID of the raw transmit in flight, when there is one.
    ///
    /// One raw transmit is allowed in flight at a time, so a second transmit
    /// fails with [`Error::RequestBusy`] while this is set.
    tx_radio_tid: Option<u8>,

    /// Request timeouts
    config: ConnectionConfig,

    /// Lookup table for transaction ID to pending request
    lut: HashMap<u8, PendingRequest>,

    reset_broadcast: broadcast::Sender<Frame>,
    debug_broadcast: broadcast::Sender<Frame>,
    net_broadcast: broadcast::Sender<Frame>,
    net_insecure_broadcast: broadcast::Sender<Frame>,
    log_broadcast: broadcast::Sender<Frame>,
    raw_rx_broadcast: broadcast::Sender<RawRxFrame>,
}

impl<T> PosixSpinelHost<T>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
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
            if self.tx_radio_tid == Some(tid) {
                self.tx_radio_tid = None;
            }

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
                            log::info!("Unsolicited status: {}", Status::<NoVendor>::from(status));
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
            Command::PropertyValueIs(Property::Stream(PropertyStream::Raw), bytes) => {
                match RawRxFrame::decode(&bytes) {
                    Ok(raw_frame) => {
                        let _ = self.raw_rx_broadcast.send(raw_frame);
                    }
                    Err(e) => {
                        log::warn!("Undecodable raw frame skipped: {e:?}");
                    }
                }
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
            PosixSpinelHostMessage::SetProperty { prop, value, reply } => {
                self.send_request(Command::PropertyValueSet(prop, value), reply)
                    .await;
            }
            PosixSpinelHostMessage::GetProperty { prop, reply } => {
                self.send_request(Command::PropertyValueGet(prop), reply)
                    .await;
            }
            PosixSpinelHostMessage::TransmitRaw { payload, reply } => {
                if self.tx_radio_tid.is_some() {
                    let _ = reply.send(Err(Error::RequestBusy));
                } else {
                    let cmd =
                        Command::PropertyValueSet(Property::Stream(PropertyStream::Raw), payload);
                    self.tx_radio_tid = self
                        .send_request_with_timeout(cmd, reply, self.config.tx_timeout)
                        .await;
                }
            }
            PosixSpinelHostMessage::SubscribeRawRx { reply } => {
                let rx = self.raw_rx_broadcast.subscribe();
                let _ = reply.send(Ok(rx));
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

    /// Form and send a request to the target device with the property timeout.
    async fn send_request(&mut self, cmd: Command, reply: OneshotFrameReply) {
        let timeout = self.config.property_timeout;
        self.send_request_with_timeout(cmd, reply, timeout).await;
    }

    /// Form and send a request to the target device.
    ///
    /// Returns the assigned TID when the request is in flight.
    async fn send_request_with_timeout(
        &mut self,
        cmd: Command,
        reply: OneshotFrameReply,
        timeout: Duration,
    ) -> Option<u8> {
        log::trace!("Sending request: {cmd:?}");

        let Some(tid) = self.next_free_tid() else {
            log::warn!("No free TID for request: {cmd:?}");
            let _ = reply.send(Err(Error::RequestBusy));
            return None;
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
                        deadline: Instant::now() + timeout,
                    },
                );
                self.tid = Self::next_tid(tid);
                Some(tid)
            }
            Err(e) => {
                log::error!("Request error: {e:?}");
                let _ = reply.send(Err(e));
                None
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
                if self.tx_radio_tid == Some(tid) {
                    self.tx_radio_tid = None;
                }
                let _ = pending.sender.send(Err(Error::RequestTimeout));
            }
        }
    }

    /// Reset the TID cursor and clear out the lookup table.
    fn reset_tid(&mut self) {
        self.tid = TID_START;
        self.tx_radio_tid = None;

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
    async fn property_set_resolves_on_echo() {
        let (handle, mut device) = setup();

        let set_handle = handle.clone();
        let set = tokio::spawn(async move { set_handle.set_physical_channel(11).await });

        let request = device.next().await.unwrap().unwrap();
        let tid = request.header().tid();
        assert_eq!(
            request.command(),
            Command::PropertyValueSet(Property::PhysicalChannel, Bytes::from_static(&[0x0B]))
        );

        device
            .send(Frame::new(
                Header::new(0, tid),
                Command::PropertyValueIs(Property::PhysicalChannel, Bytes::from_static(&[0x0B])),
            ))
            .await
            .unwrap();

        assert_eq!(set.await.unwrap(), Ok(()));
    }

    #[tokio::test(start_paused = true)]
    async fn property_set_resolves_error_status() {
        let (handle, mut device) = setup();

        let set_handle = handle.clone();
        let set = tokio::spawn(async move { set_handle.set_physical_enabled(true).await });

        let request = device.next().await.unwrap().unwrap();
        let tid = request.header().tid();
        assert_eq!(
            request.command(),
            Command::PropertyValueSet(Property::PhysicalEnabled, Bytes::from_static(&[0x01]))
        );

        device
            .send(Frame::new(
                Header::new(0, tid),
                Command::last_status(Status::InvalidState),
            ))
            .await
            .unwrap();

        assert_eq!(set.await.unwrap(), Err(Error::Status(4)));
    }

    fn raw_tx_frame() -> RawTxFrame {
        RawTxFrame {
            psdu: Bytes::from_static(&[0xAA, 0xBB]),
            channel: 11,
            config: Default::default(),
        }
    }

    /// The packed capability list containing MacRaw (513).
    const CAPS_WITH_MAC_RAW: &[u8] = &[0x81, 0x04];

    #[tokio::test(start_paused = true)]
    async fn transmit_raw_rejection_resolves_status_error() {
        let (handle, mut device) = setup();

        let tx_handle = handle.clone();
        let tx = tokio::spawn(async move { tx_handle.transmit_raw(raw_tx_frame()).await });

        let request = device.next().await.unwrap().unwrap();
        let tid = request.header().tid();

        // A device with raw mode off rejects the set with InvalidState,
        // which resolves as an error instead of a transmit report.
        device
            .send(Frame::new(
                Header::new(0, tid),
                Command::last_status(Status::InvalidState),
            ))
            .await
            .unwrap();

        assert_eq!(tx.await.unwrap(), Err(Error::Status(4)));
    }

    #[tokio::test(start_paused = true)]
    async fn transmit_raw_resolves_with_tx_done() {
        let (handle, mut device) = setup();

        let tx_handle = handle.clone();
        let tx = tokio::spawn(async move { tx_handle.transmit_raw(raw_tx_frame()).await });

        let request = device.next().await.unwrap().unwrap();
        let tid = request.header().tid();

        // The request payload matches the pinned codec encoding.
        let mut expected = BytesMut::new();
        raw_tx_frame().encode(&mut expected).unwrap();
        assert_eq!(
            request.command(),
            Command::PropertyValueSet(Property::Stream(PropertyStream::Raw), expected.freeze())
        );

        // Reply with a transmit-done report of Ok and no ack. The device
        // reports it as a LastStatus value on the request TID.
        let mut done = BytesMut::new();
        PackedU32::write_to_buffer(0, &mut done);
        SpinelBool::write_to_buffer(false, &mut done);
        SpinelBool::write_to_buffer(false, &mut done);
        device
            .send(Frame::new(
                Header::new(0, tid),
                Command::PropertyValueIs(Property::LastStatus, done.freeze()),
            ))
            .await
            .unwrap();

        let done = tx.await.unwrap().unwrap();
        assert_eq!(done.status, Status::Ok);
        assert!(!done.frame_pending);
        assert!(!done.header_updated);
        assert_eq!(done.ack, None);
    }

    #[tokio::test(start_paused = true)]
    async fn second_transmit_raw_is_busy() {
        let (handle, _device) = setup();

        let tx_handle = handle.clone();
        let _pending = tokio::spawn(async move { tx_handle.transmit_raw(raw_tx_frame()).await });
        tokio::time::sleep(Duration::from_millis(1)).await;

        assert_eq!(
            handle.transmit_raw(raw_tx_frame()).await,
            Err(Error::RequestBusy)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn raw_rx_frames_decode_and_skip_bad_payloads() {
        let (handle, mut device) = setup();
        let mut rx = handle.subscribe_raw_rx().await.unwrap();

        // An undecodable payload: the prefix claims 4 bytes, 1 follows.
        device
            .send(Frame::new(
                Header::new(0, 0),
                Command::PropertyValueIs(
                    Property::Stream(PropertyStream::Raw),
                    Bytes::from_static(&[0x04, 0x00, 0xAA]),
                ),
            ))
            .await
            .unwrap();

        // A valid raw frame follows and still arrives decoded.
        device
            .send(Frame::new(
                Header::new(0, 0),
                Command::PropertyValueIs(
                    Property::Stream(PropertyStream::Raw),
                    Bytes::from_static(&[0x02, 0x00, 0xCC, 0xDD]),
                ),
            ))
            .await
            .unwrap();

        let frame = rx.recv().await.unwrap();
        assert_eq!(&frame.psdu[..], &[0xCC, 0xDD]);
        assert_eq!(frame.rssi, -128);
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn enable_raw_fails_without_capability() {
        let (handle, mut device) = setup();

        let enable_handle = handle.clone();
        let enable = tokio::spawn(async move { enable_handle.enable_raw(11, None).await });

        let request = device.next().await.unwrap().unwrap();
        assert_eq!(
            request.command(),
            Command::PropertyValueGet(Property::Capabilities)
        );
        device
            .send(Frame::new(
                Header::new(0, request.header().tid()),
                Command::PropertyValueIs(Property::Capabilities, Bytes::new()),
            ))
            .await
            .unwrap();

        assert_eq!(enable.await.unwrap(), Err(Error::CapabilityMissing(513)));
    }

    #[tokio::test(start_paused = true)]
    async fn enable_raw_reports_stack_up() {
        let (handle, mut device) = setup();

        let enable_handle = handle.clone();
        let enable = tokio::spawn(async move { enable_handle.enable_raw(11, None).await });

        let caps_req = device.next().await.unwrap().unwrap();
        device
            .send(Frame::new(
                Header::new(0, caps_req.header().tid()),
                Command::PropertyValueIs(
                    Property::Capabilities,
                    Bytes::from_static(CAPS_WITH_MAC_RAW),
                ),
            ))
            .await
            .unwrap();

        // The stack is up, so enabling the physical layer is rejected.
        let set_req = device.next().await.unwrap().unwrap();
        assert_eq!(
            set_req.command(),
            Command::PropertyValueSet(Property::PhysicalEnabled, Bytes::from_static(&[0x01]))
        );
        device
            .send(Frame::new(
                Header::new(0, set_req.header().tid()),
                Command::last_status(Status::InvalidState),
            ))
            .await
            .unwrap();

        assert_eq!(enable.await.unwrap(), Err(Error::DeviceStackUp));
    }

    #[tokio::test(start_paused = true)]
    async fn property_set_accepts_ok_status_reply() {
        let (handle, mut device) = setup();

        let set_handle = handle.clone();
        let set = tokio::spawn(async move { set_handle.set_physical_channel(11).await });

        let request = device.next().await.unwrap().unwrap();
        let tid = request.header().tid();

        device
            .send(Frame::new(
                Header::new(0, tid),
                Command::last_status(Status::Ok),
            ))
            .await
            .unwrap();

        assert_eq!(set.await.unwrap(), Ok(()));
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

    /// Reply to the next request with an echo of the set value.
    async fn echo_next_set(device: &mut Framed<DuplexStream, HdlcCodec>) {
        let request = device.next().await.unwrap().unwrap();
        let tid = request.header().tid();
        let Command::PropertyValueSet(prop, value) = request.command() else {
            panic!("expected a property set, got {}", request.command());
        };
        device
            .send(Frame::new(
                Header::new(0, tid),
                Command::PropertyValueIs(prop, value),
            ))
            .await
            .unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn get_property_resolves_error_status() {
        let (handle, mut device) = setup();

        let get_handle = handle.clone();
        let get = tokio::spawn(async move { get_handle.get_property(Property::PhysicalChannel).await });

        let request = device.next().await.unwrap().unwrap();
        let tid = request.header().tid();
        assert_eq!(
            request.command(),
            Command::PropertyValueGet(Property::PhysicalChannel)
        );

        device
            .send(Frame::new(
                Header::new(0, tid),
                Command::last_status(Status::Failure),
            ))
            .await
            .unwrap();

        assert_eq!(get.await.unwrap(), Err(Error::Status(1)));
    }

    #[tokio::test(start_paused = true)]
    async fn noop_resolves_error_and_unexpected_responses() {
        let (handle, mut device) = setup();

        // An error status resolves as the status error.
        let noop_handle = handle.clone();
        let noop = tokio::spawn(async move { noop_handle.noop().await });
        let request = device.next().await.unwrap().unwrap();
        device
            .send(Frame::new(
                Header::new(0, request.header().tid()),
                Command::last_status(Status::Failure),
            ))
            .await
            .unwrap();
        assert_eq!(noop.await.unwrap(), Err(Error::Status(1)));

        // A response that is not a status resolves as unexpected.
        let noop_handle = handle.clone();
        let noop = tokio::spawn(async move { noop_handle.noop().await });
        let request = device.next().await.unwrap().unwrap();
        device
            .send(Frame::new(
                Header::new(0, request.header().tid()),
                Command::PropertyValueIs(Property::NcpVersion, Bytes::from_static(b"X")),
            ))
            .await
            .unwrap();
        assert_eq!(noop.await.unwrap(), Err(Error::UnexpectedResponse(6)));
    }

    #[tokio::test(start_paused = true)]
    async fn controller_version_resolves_the_value() {
        let (handle, mut device) = setup();

        let version_handle = handle.clone();
        let version = tokio::spawn(async move { version_handle.controller_version().await });

        let request = device.next().await.unwrap().unwrap();
        let tid = request.header().tid();
        assert_eq!(
            request.command(),
            Command::PropertyValueGet(Property::NcpVersion)
        );

        device
            .send(Frame::new(
                Header::new(0, tid),
                Command::PropertyValueIs(Property::NcpVersion, Bytes::from_static(b"VER")),
            ))
            .await
            .unwrap();

        assert_eq!(version.await.unwrap(), Ok(Bytes::from_static(b"VER")));
    }

    #[tokio::test(start_paused = true)]
    async fn controller_version_rejects_unexpected_response() {
        let (handle, mut device) = setup();

        let version_handle = handle.clone();
        let version = tokio::spawn(async move { version_handle.controller_version().await });

        let request = device.next().await.unwrap().unwrap();
        let tid = request.header().tid();
        device
            .send(Frame::new(
                Header::new(0, tid),
                Command::PropertyValueIs(Property::PhysicalChannel, Bytes::from_static(&[0x0B])),
            ))
            .await
            .unwrap();

        assert!(matches!(
            version.await.unwrap(),
            Err(Error::UnexpectedResponse(_))
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn last_status_resolves() {
        let (handle, mut device) = setup();

        let status_handle = handle.clone();
        let status = tokio::spawn(async move { status_handle.last_status().await });

        let request = device.next().await.unwrap().unwrap();
        let tid = request.header().tid();
        assert_eq!(
            request.command(),
            Command::PropertyValueGet(Property::LastStatus)
        );

        device
            .send(Frame::new(
                Header::new(0, tid),
                Command::last_status(Status::Ok),
            ))
            .await
            .unwrap();

        assert_eq!(status.await.unwrap(), Ok(()));
    }

    #[tokio::test(start_paused = true)]
    async fn reset_sends_the_command() {
        let (handle, mut device) = setup();

        let reset_handle = handle.clone();
        let reset = tokio::spawn(async move { reset_handle.reset().await });

        let request = device.next().await.unwrap().unwrap();
        assert_eq!(request.command(), Command::Reset);
        assert_eq!(reset.await.unwrap(), Ok(()));
    }

    #[tokio::test(start_paused = true)]
    async fn enable_raw_runs_the_full_sequence() {
        let (handle, mut device) = setup();

        let raw_handle = handle.clone();
        let enable = tokio::spawn(async move {
            raw_handle.enable_raw(11, Some(PromiscuousMode::Full)).await
        });

        // The capability check comes first.
        let request = device.next().await.unwrap().unwrap();
        let tid = request.header().tid();
        assert_eq!(
            request.command(),
            Command::PropertyValueGet(Property::Capabilities)
        );
        device
            .send(Frame::new(
                Header::new(0, tid),
                Command::PropertyValueIs(
                    Property::Capabilities,
                    Bytes::from_static(CAPS_WITH_MAC_RAW),
                ),
            ))
            .await
            .unwrap();

        // The four sets follow in order and echo back.
        for _ in 0..4 {
            echo_next_set(&mut device).await;
        }

        assert_eq!(enable.await.unwrap(), Ok(()));
    }

    #[tokio::test(start_paused = true)]
    async fn transmit_raw_accepts_stream_raw_reply() {
        let (handle, mut device) = setup();

        let tx_handle = handle.clone();
        let tx = tokio::spawn(async move { tx_handle.transmit_raw(raw_tx_frame()).await });

        let request = device.next().await.unwrap().unwrap();
        let tid = request.header().tid();

        // The transmit-done payload under the raw stream property decodes
        // with the same status-first layout.
        let mut done = BytesMut::new();
        PackedU32::write_to_buffer(0, &mut done);
        SpinelBool::write_to_buffer(true, &mut done);
        device
            .send(Frame::new(
                Header::new(0, tid),
                Command::PropertyValueIs(Property::Stream(PropertyStream::Raw), done.freeze()),
            ))
            .await
            .unwrap();

        let done = tx.await.unwrap().unwrap();
        assert_eq!(done.status, Status::Ok);
        assert!(done.frame_pending);
    }

    #[tokio::test(start_paused = true)]
    async fn reset_clears_a_pending_raw_transmit() {
        let (handle, mut device) = setup();

        let tx_handle = handle.clone();
        let tx = tokio::spawn(async move { tx_handle.transmit_raw(raw_tx_frame()).await });
        let _request = device.next().await.unwrap().unwrap();

        device
            .send(unsolicited_status(Status::Unknown(114)))
            .await
            .unwrap();
        assert_eq!(tx.await.unwrap(), Err(Error::DeviceReset));

        // The reset released the transmit slot, so the next transmit goes
        // out instead of failing busy.
        let tx_handle = handle.clone();
        let tx = tokio::spawn(async move { tx_handle.transmit_raw(raw_tx_frame()).await });
        let request = device.next().await.unwrap().unwrap();

        let mut done = BytesMut::new();
        PackedU32::write_to_buffer(0, &mut done);
        device
            .send(Frame::new(
                Header::new(0, request.header().tid()),
                Command::PropertyValueIs(Property::LastStatus, done.freeze()),
            ))
            .await
            .unwrap();
        assert_eq!(tx.await.unwrap().unwrap().status, Status::Ok);
    }

    #[tokio::test(start_paused = true)]
    async fn stream_broadcasts_route_to_their_subscribers() {
        let (handle, mut device) = setup();
        let mut debug_rx = handle.subscribe_debug_broadcast().await.unwrap();
        let mut insecure_rx = handle.subscribe_net_insecure_broadcast().await.unwrap();
        let mut log_rx = handle.subscribe_log_broadcast().await.unwrap();

        let stream_frame = |stream| {
            Frame::new(
                Header::new(0, 0),
                Command::PropertyValueIs(Property::Stream(stream), Bytes::from_static(b"S")),
            )
        };

        device
            .send(stream_frame(PropertyStream::Debug))
            .await
            .unwrap();
        device
            .send(stream_frame(PropertyStream::NetInsecure))
            .await
            .unwrap();
        device.send(stream_frame(PropertyStream::Log)).await.unwrap();

        assert!(debug_rx.recv().await.is_ok());
        assert!(insecure_rx.recv().await.is_ok());
        assert!(log_rx.recv().await.is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn malformed_broadcasts_do_not_stop_the_actor() {
        let (handle, mut device) = setup();

        // An unsolicited non-stream value, a status with an empty payload,
        // and a response with no pending request each log and move on.
        device
            .send(Frame::new(
                Header::new(0, 0),
                Command::PropertyValueIs(Property::NcpVersion, Bytes::from_static(b"X")),
            ))
            .await
            .unwrap();
        device
            .send(Frame::new(
                Header::new(0, 0),
                Command::PropertyValueIs(Property::LastStatus, Bytes::new()),
            ))
            .await
            .unwrap();
        device
            .send(Frame::new(
                Header::new(0, 5),
                Command::last_status(Status::Ok),
            ))
            .await
            .unwrap();

        // The actor still serves requests afterward.
        let noop_handle = handle.clone();
        let noop = tokio::spawn(async move { noop_handle.noop().await });
        let request = device.next().await.unwrap().unwrap();
        device
            .send(Frame::new(
                Header::new(0, request.header().tid()),
                Command::last_status(Status::Ok),
            ))
            .await
            .unwrap();
        assert_eq!(noop.await.unwrap(), Ok(()));
    }

    #[tokio::test(start_paused = true)]
    async fn corrupt_frame_does_not_stop_the_actor() {
        use tokio::io::AsyncWriteExt;

        let (host_side, mut device_side) = duplex(4096);
        let handle = PosixSpinelHostHandle::new_from_transport(host_side, 0);

        // A delimited frame with a bad checksum surfaces as a stream error,
        // which the actor logs without stopping.
        device_side
            .write_all(&[0x7E, 0x81, 0x00, 0x00, 0x00, 0x7E])
            .await
            .unwrap();

        // The actor still serves requests afterward.
        let mut device = HdlcCodec::<NoVendor>::default().framed(device_side);
        let noop_handle = handle.clone();
        let noop = tokio::spawn(async move { noop_handle.noop().await });
        let request = device.next().await.unwrap().unwrap();
        device
            .send(Frame::new(
                Header::new(0, request.header().tid()),
                Command::last_status(Status::Ok),
            ))
            .await
            .unwrap();
        assert_eq!(noop.await.unwrap(), Ok(()));
    }
}
