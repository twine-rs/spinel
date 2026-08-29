use clap::Parser;
use spinel::{PosixSpinelHostHandle, RawTxFrame, SpinelHostConnection};

/// A CLI tool for interacting with a networking device using the Spinel protocol.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Baud rate
    #[clap(short('b'), long("baud"), default_value("115200"))]
    baud_rate: u32,

    /// Flow control
    #[clap(short('f'), long("flow-control"), default_value("None"))]
    flow_control: Option<String>,

    /// System port name
    #[clap(short('p'), long("port"))]
    port_name: String,
}

#[tokio::main]
async fn main() -> tokio_serial::Result<()> {
    env_logger::init();

    let args = Args::parse();

    let port_name = args.port_name;
    let baud = args.baud_rate;

    let actor = PosixSpinelHostHandle::new_from_serial(&port_name, baud, 0).unwrap();

    actor.noop().await.unwrap();
    log::trace!("noop successful");

    actor.reset().await.unwrap();
    log::trace!("reset successful");

    let version = actor.controller_version().await.unwrap();
    log::trace!("controller version: {:?}", version);

    let mut reset_broadcast_rx = actor.subscribe_reset_msg().await.unwrap();
    let mut debug_broadcast_rx = actor.subscribe_debug_broadcast().await.unwrap();
    let mut net_broadcast_rx = actor.subscribe_net_broadcast().await.unwrap();
    let mut net_insecure_broadcast_rx = actor.subscribe_net_insecure_broadcast().await.unwrap();
    let mut log_broadcast_rx = actor.subscribe_log_broadcast().await.unwrap();
    let mut raw_broadcast_rx = actor.subscribe_raw_broadcast().await.unwrap();

    // Manual verification: a bare Noop-length PSDU, just to confirm `transmit_raw` reaches
    // the radio. Errors are logged rather than unwrapped since a real radio may reject it
    // (e.g. no PAN joined yet).
    if let Err(e) = actor
        .transmit_raw(&RawTxFrame::new(bytes::Bytes::from_static(&[0x00])))
        .await
    {
        log::warn!("transmit_raw failed: {e:?}");
    }

    loop {
        tokio::select! {
            frame = reset_broadcast_rx.recv() => {
                log::trace!("reset broadcast received: {:?}", frame);
            }
            frame = debug_broadcast_rx.recv() => {
                log::trace!("debug broadcast received: {:?}", frame);
            }
            frame = net_broadcast_rx.recv() => {
                log::trace!("net broadcast received: {:?}", frame);
            }
            frame = net_insecure_broadcast_rx.recv() => {
                log::trace!("net insecure broadcast received: {:?}", frame);
            }
            frame = log_broadcast_rx.recv() => {
                log::trace!("log broadcast received: {:?}", frame);
            }
            frame = raw_broadcast_rx.recv() => {
                let raw_frame = frame.as_ref().ok().and_then(|f| f.stream_raw_frame());
                log::trace!("raw broadcast received: {:?}", raw_frame);
            }
        }
    }
}
