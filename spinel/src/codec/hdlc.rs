use crate::{Frame, HdlcLiteFrame};
use bytes::BytesMut;
use std::io;
use tokio_util::codec::{Decoder, Encoder};

#[derive(Debug, Default)]
pub struct HdlcCodec;

impl Encoder<Frame> for HdlcCodec {
    type Error = std::io::Error;

    fn encode(&mut self, item: Frame, src: &mut BytesMut) -> Result<(), Self::Error> {
        let hdlc_frame = HdlcLiteFrame::new(item);
        match hdlc_frame.encode(src) {
            Ok(_) => Ok(()),
            Err(e) => {
                eprintln!("Frame encode error: {:?}", e);
                Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Encoder error: {e:?}"),
                ))
            }
        }
    }
}

impl Decoder for HdlcCodec {
    type Item = Frame;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() {
            return Ok(None);
        }

        if let Some(b) = HdlcLiteFrame::find_frame(&src.clone().freeze()) {
            // Split data from src so the buffer advances
            let frame = src.split_to(b.1 + 1).freeze().slice(b.0..);

            return match HdlcLiteFrame::decode(&frame) {
                Ok(f) => Ok(Some(f.into_inner())),
                Err(e) => {
                    eprintln!("Frame decode error: {:?}", e);
                    Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("Decoder error: {e:?}"),
                    ))
                }
            };
        }

        Ok(None)
    }
}
