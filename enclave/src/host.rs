//! Tools for interacting with host environment

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use cobs::CobsEncoder;
use ostd::arch::x86::device::serial::SerialPort;
use ostd::sync::Mutex;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use crate::host::MsgError::Utf8;

/// Messages to host environment
#[derive(Clone, Serialize, Deserialize)]
enum MsgToHost {
    Error(alloc::string::String)
}

/// Messages from host environment
#[derive(Clone, Serialize, Deserialize)]
enum MsgFromHost {

}

#[derive(Error, Debug)]
pub enum MsgError {
    #[error("COBS failed to decode message from COM 2 with: {0}")]
    Decode(cobs::DecodeError),
    #[error("Failed to deserialize CBOR with: {0}")]
    Deserialize(serde_cbor::Error),
    #[error("Input bytes were not valid utf-8")]
    Utf8,
}

/// A serial port for communicating with the host.
pub struct HostCom;

static HOST_COM: Mutex<SerialPort> = Mutex::new(
    // Serial port: COM 2
 unsafe { SerialPort::new(0x2F8) },
);

struct Frame {
    bytes: Vec<u8>,
}

impl Frame {
    fn deserialize<T: DeserializeOwned>(self) -> Result<T, MsgError> {
        serde_cbor::from_slice(&self.bytes)
            .map_err(MsgError::Deserialize)
    }
}

impl HostCom {
    /// Initialize the connection
    pub fn init()  {
        HOST_COM.lock().init();
    }

    /// Write a buffer of bytes to the serial port
    pub fn write_bytes(buf: &[u8]) {
        for b in buf.iter().copied() {
            HOST_COM.lock().send(b);
        }
    }

    /// Write a serializable message out to the serial port in CBOR,
    /// framed with COBS.
    pub fn write<T: Serialize>(msg: &T) {
        let data = serde_cbor::to_vec(&msg).unwrap();
        let buf_size = cobs::max_encoding_length(data.len());
        let mut frame_buf = vec![0u8; buf_size];
        let mut encoder = CobsEncoder::new(&mut frame_buf);
        encoder.push(&data).expect("Encoding cannot exceed maximum size computed for message");
        let size = encoder.finalize();
        Self::write_bytes(&frame_buf[..size]);
    }

    /// If data is available on the port, attempts to read it and
    /// deserialize it, which is blocking. If no data is available,
    /// it does not wait but returns immediately.
    pub fn try_read<T: DeserializeOwned>() -> Result<Option<T>, MsgError> {
        if let Some(frame) = Self::try_read_frame()? {
            frame.deserialize()
        } else {
            Ok(None)
        }
    }

    pub fn try_read_string() -> Result<Option<String>, MsgError> {
        let com = HOST_COM.lock();
        let mut bytes = vec![];
        while com.line_status() & 1 == 1 {
            bytes.push(com.recv());
        }
        if bytes.is_empty() {
            Ok(None)
        } else {
            Ok(Some(String::from_utf8(bytes).map_err(|_| Utf8)?))
        }
    }

    /// Block until a byte is read
    fn read_byte() -> u8 {
        let com = HOST_COM.lock();
        loop {
            if com.line_status() & 1 == 1 {
                break com.recv()
            }
            core::hint::spin_loop();
        }
    }

    /// Method to read framed bytes from the serial port.
    ///
    /// If there is no data from the port, returns nothing. Otherwise,
    /// blocks until an entire frame is read or error occurs.
    fn try_read_frame() -> Result<Option<Frame>, MsgError> {
        // check if data is available, otherwise return early
        if HOST_COM.lock().line_status() & 1 != 1 {
            Ok(None)
        } else {
            Ok(Some(Self::get_frame()?))
        }
    }

    /// Blocking method that reads a frame
    ///
    /// Uses an initial buffer with 1Kb in size. Dynamically increases the
    /// size of the frame buffer by 1Kb until either the message is decoded
    /// or an error occurs.
    ///
    /// Returns the raw framed bytes
    fn get_frame() -> Result<Frame, MsgError> {
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
            decoder.push(&read_bytes).expect("Previously read bytes should not produce a frame error.");
            loop {
                match decoder.feed(Self::read_byte()) {
                    Ok(None) => continue,
                    Ok(Some(len)) => {
                        let mut decoded = vec![];
                        decoded.copy_from_slice(&frame_buf[..len]);
                        break 'outer Ok(Frame { bytes: decoded })
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
}