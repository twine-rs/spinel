use bytes::Bytes;
use clap::{Parser, Subcommand};

use spinel::{PosixSpinelHostHandle, PromiscuousMode, RawTxFrame, SpinelHostConnection};

/// A CLI tool for interacting with a networking device using the Spinel protocol.
#[derive(Debug, Parser)]
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

    #[command(subcommand)]
    command: Option<CliCommand>,
}

#[derive(Debug, Subcommand)]
enum CliCommand {
    /// Enable raw link mode and print received raw frames.
    Raw {
        /// The 802.15.4 channel to use
        #[clap(short('c'), long("channel"), default_value("11"))]
        channel: u8,

        /// Promiscuous mode (off, network, or full)
        #[clap(long("promiscuous"), default_value("full"))]
        promiscuous: String,

        /// Hex encoded PSDU to transmit once, without the FCS
        #[clap(long("transmit"))]
        transmit: Option<String>,
    },
}

/// Parse a hex string into bytes.
///
/// Operates on characters, so non-ASCII input reports an error instead of
/// splitting a multi-byte character.
fn parse_hex(input: &str) -> Result<Vec<u8>, String> {
    let cleaned: Vec<char> = input.chars().filter(|c| !c.is_whitespace()).collect();

    if cleaned.len() % 2 != 0 {
        return Err("hex input needs an even number of digits".into());
    }

    cleaned
        .chunks(2)
        .enumerate()
        .map(|(i, pair)| match (pair[0].to_digit(16), pair[1].to_digit(16)) {
            (Some(hi), Some(lo)) => Ok((hi * 16 + lo) as u8),
            _ => Err(format!("invalid hex at offset {}", i * 2)),
        })
        .collect()
}

/// Parse a promiscuous mode name.
fn parse_promiscuous(input: &str) -> Result<Option<PromiscuousMode>, String> {
    match input {
        "off" => Ok(Some(PromiscuousMode::Off)),
        "network" => Ok(Some(PromiscuousMode::Network)),
        "full" => Ok(Some(PromiscuousMode::Full)),
        other => Err(format!("unknown promiscuous mode: {other}")),
    }
}

/// Enable raw link mode, optionally transmit one frame, and print raw frames.
async fn run_raw(
    actor: PosixSpinelHostHandle,
    channel: u8,
    promiscuous: String,
    transmit: Option<String>,
) -> Result<(), String> {
    let promiscuous = parse_promiscuous(&promiscuous)?;

    // Subscribe before enabling raw mode. The device starts forwarding
    // frames as soon as the mode is enabled, and a broadcast with no
    // receiver drops them.
    let mut raw_rx = actor
        .subscribe_raw_rx()
        .await
        .map_err(|e| format!("raw subscribe failed: {e}"))?;

    actor
        .enable_raw(channel, promiscuous)
        .await
        .map_err(|e| format!("enable raw failed: {e}"))?;
    println!("raw mode enabled on channel {channel}");

    if let Some(hex) = transmit {
        let psdu = parse_hex(&hex)?;
        let frame = RawTxFrame {
            psdu: Bytes::from(psdu),
            channel,
            config: Default::default(),
        };

        let done = actor
            .transmit_raw(frame)
            .await
            .map_err(|e| format!("transmit failed: {e}"))?;
        println!(
            "transmit done, status {}, frame pending {}, ack {:?}",
            done.status, done.frame_pending, done.ack
        );
    }

    loop {
        match raw_rx.recv().await {
            Ok(frame) => {
                let psdu: Vec<String> = frame.psdu.iter().map(|b| format!("{b:02x}")).collect();
                println!(
                    "rx {} bytes, rssi {}, lqi {:?}, channel {:?}, psdu {}",
                    frame.psdu.len(),
                    frame.rssi,
                    frame.lqi,
                    frame.channel,
                    psdu.join("")
                );
            }
            Err(e) => {
                log::warn!("raw receive lagged: {e}");
            }
        }
    }
}

/// Subscribe to every frame broadcast and log the traffic.
async fn run_monitor(actor: PosixSpinelHostHandle) {
    let mut reset_broadcast_rx = actor.subscribe_reset_msg().await.unwrap();
    let mut debug_broadcast_rx = actor.subscribe_debug_broadcast().await.unwrap();
    let mut net_broadcast_rx = actor.subscribe_net_broadcast().await.unwrap();
    let mut net_insecure_broadcast_rx = actor.subscribe_net_insecure_broadcast().await.unwrap();
    let mut log_broadcast_rx = actor.subscribe_log_broadcast().await.unwrap();

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
        }
    }
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
            if let Err(e) = run_raw(actor, channel, promiscuous, transmit).await {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        None => run_monitor(actor).await,
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_hex;

    #[test]
    fn parse_hex_decodes_bytes() {
        assert_eq!(parse_hex("0aFF 01"), Ok(vec![0x0A, 0xFF, 0x01]));
    }

    #[test]
    fn parse_hex_rejects_odd_length() {
        assert!(parse_hex("abc").is_err());
    }

    #[test]
    fn parse_hex_rejects_invalid_digits() {
        assert!(parse_hex("zz").is_err());
    }

    #[test]
    fn parse_hex_rejects_multi_byte_characters() {
        // A multi-byte character reports an error instead of panicking on a
        // character boundary.
        assert!(parse_hex("\u{e9}a").is_err());
    }
}
