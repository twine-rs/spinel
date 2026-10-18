use core::marker::PhantomData;
use std::io;

use bytes::BytesMut;
use tokio_util::codec::{Decoder, Encoder};

use crate::codec::vendor::{NoVendor, Vendor};
use crate::{Frame, HdlcLiteFrame};

#[derive(Debug)]
pub struct HdlcCodec<V: Vendor = NoVendor> {
    _marker: PhantomData<V>,
}

impl<V: Vendor> Default for HdlcCodec<V> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<V: Vendor> Encoder<Frame<V>> for HdlcCodec<V> {
    type Error = std::io::Error;

    fn encode(&mut self, item: Frame<V>, src: &mut BytesMut) -> Result<(), Self::Error> {
        let hdlc_frame = HdlcLiteFrame::new(item);
        match hdlc_frame.encode(src) {
            Ok(_) => Ok(()),
            Err(e) => {
                eprintln!("Frame encode error: {:?}", e);
                Err(io::Error::other(format!("Encoder error: {e:?}")))
            }
        }
    }
}

impl<V: Vendor> Decoder for HdlcCodec<V> {
    type Item = Frame<V>;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() {
            return Ok(None);
        }

        if let Some(b) = HdlcLiteFrame::<V>::find_frame(&src.clone().freeze()) {
            // Split data from src so the buffer advances
            let frame = src.split_to(b.1 + 1).freeze().slice(b.0..);

            return match HdlcLiteFrame::<V>::decode(&frame) {
                Ok(f) => Ok(Some(f.into_inner())),
                Err(e) => {
                    eprintln!("Frame decode error: {:?}", e);
                    Err(io::Error::other(format!("Decoder error: {e:?}")))
                }
            };
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use bytes::BufMut;

    use super::*;
    use crate::{Command, Header};

    /// Encode one noop frame into a buffer.
    fn encoded_noop() -> BytesMut {
        let mut buffer = BytesMut::new();
        let mut codec = HdlcCodec::<NoVendor>::default();
        codec
            .encode(Frame::new(Header::new(0, 1), Command::Noop), &mut buffer)
            .unwrap();
        buffer
    }

    #[test]
    fn decoder_waits_for_a_complete_frame() {
        let mut codec = HdlcCodec::<NoVendor>::default();
        let full = encoded_noop();

        // An empty buffer and a partial frame both decode to none.
        let mut src = BytesMut::new();
        assert!(codec.decode(&mut src).unwrap().is_none());

        src.put_slice(&full[..3]);
        assert!(codec.decode(&mut src).unwrap().is_none());

        // The rest of the frame completes the decode.
        src.put_slice(&full[3..]);
        let frame = codec.decode(&mut src).unwrap().unwrap();
        assert_eq!(frame.command(), Command::Noop);
    }

    #[test]
    fn decoder_reports_a_corrupt_frame_as_an_error() {
        let mut codec = HdlcCodec::<NoVendor>::default();

        // A delimited frame with a broken checksum errors instead of
        // decoding.
        let mut src = BytesMut::new();
        src.put_slice(&[0x7E, 0x81, 0x00, 0x00, 0x00, 0x7E]);
        assert!(codec.decode(&mut src).is_err());
    }
}
