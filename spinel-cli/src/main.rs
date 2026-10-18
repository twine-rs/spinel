mod args;
mod monitor;
mod raw;

use clap::Parser;

use crate::args::{Args, CliCommand};
use spinel::{PosixSpinelHostHandle, SpinelHostConnection};

#[tokio::main]
async fn main() -> tokio_serial::Result<()> {
    env_logger::init();

    let args = Args::parse();

    let port_name = args.port_name;
    let baud = args.baud_rate;

    let actor = PosixSpinelHostHandle::new_from_serial(&port_name, baud, 0).unwrap();

    actor.noop().await.unwrap();
    log::trace!("noop successful");

    // Wait for the reset notification. A request sent during the device
    // reboot is lost.
    let mut reset_rx = actor.subscribe_reset_msg().await.unwrap();
    actor.reset().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), reset_rx.recv())
        .await
        .expect("no reset notification from the device")
        .unwrap();
    log::trace!("reset successful");

    let version = actor.controller_version().await.unwrap();
    log::trace!("controller version: {:?}", version);

    match args.command {
        Some(CliCommand::Raw {
            channel,
            promiscuous,
            transmit,
        }) => {
            if let Err(e) = raw::run_raw(actor, channel, promiscuous, transmit).await {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        None => monitor::run_monitor(actor).await,
    }

    Ok(())
}
