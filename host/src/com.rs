use std::io;
use std::io::prelude::*;
use std::net::TcpStream;

use shared::{FramedBytes, MsgError, MsgFromHost, MsgToHost, ReadWriteByte};

pub(crate) struct Tcp {
    pub raw: TcpStream,
    buffered: Vec<u8>,
}

impl Tcp {
    /// Create a new stream
    pub fn new(url: &str) -> io::Result<Self> {
        Ok(Self {
            raw: TcpStream::connect(url)?,
            buffered: Default::default(),
        })
    }

    /// Send a [`MsgFromHost`] into the enclave
    pub fn write(&mut self, msg: MsgFromHost) {
        self.write_frame(&msg);
    }

    /// Read a message sent from the enclave
    pub fn read(&mut self) -> Result<MsgToHost, MsgError> {
        let frame = self.get_ferm()?;
        frame.deserialize()
    }

    /// Read data from the stream into an internal buffer.
    /// The buffer is a stack, so the bytes are stored in
    /// reverse order that they are received.
    fn buffered_read(&mut self) -> io::Result<()> {
        let mut buffered = vec![0; 1024];
        let len = self.raw.read(&mut buffered)?;
        println!("{:?}, {len}", buffered);
        self.buffered = buffered[..len].to_vec();
        Ok(())
    }

    fn get_ferm(&mut self) -> Result<shared::Frame, MsgError> {
        // initial buffer size for the frame
        let mut buf_size = 1024;
        // initial buffer
        let mut frame_buf = Vec::<u8>::with_capacity(0);

        // continue trying to populate the frame buffer until
        // a successful frame decoding or a decode error occurs.
        loop {
            // dynamically resize the frame buffer if necessary
            let mut read_bytes = vec![0; buf_size];
            core::mem::swap(&mut read_bytes, &mut frame_buf);
            let mut decoder = cobs::CobsDecoder::new(&mut frame_buf);
            println!("Starting to read bytes");
            decoder
                .push(&read_bytes)
                .expect("Previously read bytes should not produce a frame error.");
            loop {
                println!("Feeding decoder");
                match decoder.feed(self.read_byte()) {
                    Ok(None) => continue,
                    Ok(Some(len)) => {
                        let mut decoded = vec![];
                        decoded.copy_from_slice(&frame_buf[..len]);
                        return  Ok(shared::Frame { bytes: decoded });
                    }
                    Err(cobs::DecodeError::TargetBufTooSmall) => {
                        println!("Uh oh, not enough bytes somehow");
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

impl ReadWriteByte for Tcp {
    fn read_byte(&mut self) -> u8 {
        // block until data is read into
        // internal buffer
        while self.buffered.is_empty() {
            self.buffered_read().unwrap();
            core::hint::spin_loop();
        }
        self.buffered.remove(0)
    }

    fn write_bytes(&mut self, buf: &[u8]) {
        self.raw.write_all(buf).unwrap();
        self.raw.flush().unwrap();
    }
}
