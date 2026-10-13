use crate::codec::vendor::{NoVendor, Vendor};
use crate::{Frame, HdlcLiteFrame};
use bytes::BytesMut;
use core::marker::PhantomData;
use std::io;
use tokio_util::codec::{Decoder, Encoder};

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

        if HdlcLiteFrame::<V>::resync_if_desynced(src) {
            log::warn!("HDLC stream desynced without a complete frame; resyncing");
        }

        Ok(None)
    }
}
