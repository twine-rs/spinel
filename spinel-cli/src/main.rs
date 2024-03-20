use clap::Parser;
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use spinel::{Command, Frame, HdlcCodec, Header, Property};
use tokio_serial::{SerialPortBuilderExt, SerialStream};
use tokio_util::codec::{Decoder, Framed};

struct SpinelHost {
    stream: Framed<SerialStream, HdlcCodec>,
}

impl SpinelHost {
    async fn send_frame(&mut self, frame: Frame) {
        self.stream.send(frame).await.unwrap();

        if let Some(resp) = self.stream.next().await {
            match resp {
                Ok(frame) => {
                    println!("{:?}", frame);
                }
                Err(e) => {
                    eprintln!("{:?}", e);
                }
            }
        }
    }

    async fn recv_loop(&mut self) {
        while let Some(frame) = self.stream.next().await {
            match frame {
                Ok(frame) => {
                    println!("{:?}", frame);
                }
                Err(e) => {
                    eprintln!("{:?}", e);
                }
            }
        }
    }
}

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
    let args = Args::parse();

    let port_name = args.port_name;
    let baud = args.baud_rate;

    let port = tokio_serial::new(&port_name, baud).open_native_async()?;
    let stream = HdlcCodec.framed(port);

    let mut host = SpinelHost { stream };

    println!("Receiving data on {port_name} ({baud} baud)");

    let reset_spinel_frame = spinel::Frame::new(Header::new(0, 0), Command::Reset);
    host.send_frame(reset_spinel_frame).await;

    let noop_spinel_frame = spinel::Frame::new(Header::new(0, 2), Command::Noop);
    host.send_frame(noop_spinel_frame.clone()).await;

    let version_frame = spinel::Frame::new(
        Header::new(0, 1),
        Command::PropertyValueGet(Property::NcpVersion),
    );
    host.send_frame(version_frame).await;

    host.recv_loop().await;

    Ok(())
}
