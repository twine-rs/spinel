use super::SpinelHostConnection;
use crate::{
    codec::{PackedU32, ResetReason, Status},
    Command, Error, Frame, HdlcCodec, Header, Property, PropertyStream,
};
use bytes::Bytes;
use core::fmt;
use futures::{sink::SinkExt, stream::StreamExt};
use platform_switch::log;
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

/// A TID with a value of zero is resevered for messages where a response is not expected.
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
        let (handle_tx, handle_rx) = mpsc::unbounded_channel();

        let port = tokio_serial::new(port_name, baud)
            .open_native_async()
            .map_err(|e| {
                log::error!("Serial Config: {e}");
                Error::SerialConfig
            })?;
        let stream = HdlcCodec.framed(port);

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
        };
        let _ = self.transaction.send(msg);
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
                Err(Error::Status(status))
            }
        } else {
            Err(Error::UnexpectedResponse(response))
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

        match response.command {
            Command::PropertyValueIs(Property::NcpVersion, value) => Ok(value),
            _ => Err(Error::UnexpectedResponse(response)),
        }
    }
}

struct PosixSpinelHost {
    /// Message request channel from the host
    msg: mpsc::UnboundedReceiver<PosixSpinelHostMessage>,

    /// HDLC encoded stream of messages comming from a serial device
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
                                            let reset_reason = PackedU32::decode(&bytes).0;
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
        };
    }

    /// Form and send a request to the target device.
    async fn send_request(
        &mut self,
        cmd: Command,
        reply: oneshot::Sender<Result<oneshot::Receiver<Frame>, Error>>,
    ) {
        log::trace!("Sending request: {cmd:?}");
        let frame = Frame::new(Header::new(self.iid, self.tid), cmd);

        match self.send_frame(frame).await {
            Ok(_) => {
                let (send, recv) = oneshot::channel::<Frame>();
                let _ = reply.send(Ok(recv));
                self.lut.insert(self.tid, send);
                self.increment_tid();
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

    /// Increase the TID by one, wrapping around to 1 if the maximum value is reached.
    fn increment_tid(&mut self) {
        if self.tid == 15 {
            self.tid = TID_START;
        } else {
            self.tid += 1;
        }
    }

    /// Reset the TID and clear out the lookup table.
    fn reset_tid(&mut self) {
        self.tid = TID_START;
        self.lut.clear();
    }
}
