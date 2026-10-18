use clap::{Parser, Subcommand};

/// A CLI tool for interacting with a networking device using the Spinel protocol.
#[derive(Debug, Parser)]
#[command(name = "spinel", version, about, long_about = None)]
pub struct Args {
    /// Baud rate
    #[clap(short('b'), long("baud"), default_value("115200"))]
    pub baud_rate: u32,

    /// Flow control
    #[clap(short('f'), long("flow-control"), default_value("None"))]
    pub flow_control: Option<String>,

    /// System port name
    #[clap(short('p'), long("port"))]
    pub port_name: String,

    #[command(subcommand)]
    pub command: Option<CliCommand>,
}

#[derive(Debug, Subcommand)]
pub enum CliCommand {
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
