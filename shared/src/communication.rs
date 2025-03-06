use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use cobs::{CobsEncoder, encode_with_sentinel};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Messages to host environment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MsgToHost {
    Basic(String),
    Error(String),
}

/// Messages from host environment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MsgFromHost {
    Basic(String),
}

#[derive(Error, Debug)]
pub enum MsgError {
    #[error("COBS failed to decode message from COM 2 with: {0}")]
    Decode(cobs::DecodeError),
    #[error("Failed to deserialize CBOR with: {0}")]
    Deserialize(serde_cbor::Error),
    #[error("Input bytes were not valid utf-8: {0:?}")]
    Utf8(Vec<u8>),
}

pub struct Frame {
    pub bytes: Vec<u8>,
}

impl Frame {
    pub fn deserialize<T: DeserializeOwned>(self) -> Result<T, MsgError> {
        ostd::prelude::println!("CBOR bytes: {:?}", self.bytes);
        serde_cbor::from_slice(&self.bytes).map_err(MsgError::Deserialize)
    }
}

/// A trait for getting the next byte in a byte stream
pub trait ReadWriteByte {
    fn read_byte(&mut self) -> u8;

    fn write_bytes(&mut self, buf: &[u8]);
}

/// A trait for reading / writing framed data from a byte stream.
/// This trait should not be implemented directly, but rely on
/// the default implementation.
pub trait FramedBytes: ReadWriteByte {
    /// Blocking method that reads a frame
    ///
    /// Uses an initial buffer with 1Kb in size. Dynamically increases the
    /// size of the frame buffer by 1Kb until either the message is decoded
    /// or an error occurs.
    ///
    /// Returns the raw framed bytes
    fn get_frame(&mut self) -> Result<Frame, MsgError> {
        // initial buffer size for the frame
        let mut buf_size = 1024;
        // initial buffer
        let mut frame_buf = Vec::<u8>::with_capacity(0);

        // continue trying to populate the frame buffer until
        // a successful frame decoding or a decode error occurs.
        'outer: loop {
            // dynamically resize the frame buffer if necessary
            let mut read_bytes = vec![0; buf_size];
            core::mem::swap(&mut read_bytes, &mut frame_buf);
            let mut decoder = cobs::CobsDecoder::new(&mut frame_buf);
            decoder
                .push(&read_bytes)
                .expect("Previously read bytes should not produce a frame error.");
            loop {
                match decoder.feed(self.read_byte()) {
                    Ok(None) => continue,
                    Ok(Some(len)) => {
                        frame_buf.shrink_to(len);
                        return Ok(Frame { bytes: frame_buf });
                    }
                    Err(cobs::DecodeError::TargetBufTooSmall) => {
                        // increase the buffer size ny 1Kb
                        buf_size += 1024;
                        break;
                    }
                    Err(e) => return Err(MsgError::Decode(e)),
                }
            }
        }
    }

    /// Write a serializable message out to the serial port in CBOR,
    /// framed with COBS.
    fn write_frame<T: Serialize>(&mut self, msg: &T) {
        let data = serde_cbor::to_vec(&msg).unwrap();
        let mut encoded = cobs::encode_vec_with_sentinel(&data, 0);
        encoded.push(0);
        self.write_bytes(&encoded);
    }
}

impl<T: ReadWriteByte> FramedBytes for T {}
