//! A TCP based implementation of the Kassandra service communication protocol

use std::io::prelude::*;
use std::net::TcpStream;
use std::prelude::rust_2024::Vec;
use std::{io, vec};

use crate::tee::EnclaveComm;
use crate::{FramedBytes, MsgError, MsgFromHost, MsgToHost, ReadWriteByte};

const ENCLAVE_ADDRESS: &str = "0.0.0.0:12345";

pub struct Tcp {
    pub raw: TcpStream,
    buffered: Vec<u8>,
}

impl Clone for Tcp {
    fn clone(&self) -> Self {
        Self {
            raw: self.raw.try_clone().unwrap(),
            buffered: self.buffered.clone(),
        }
    }
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
        let frame = self.get_frame()?;
        frame.deserialize()
    }

    /// Read data from the stream into an internal buffer.
    /// The buffer is a stack, so the bytes are stored in
    /// reverse order that they are received.
    fn buffered_read(&mut self) -> io::Result<()> {
        let mut buffered = vec![0; 10];
        let len = self.raw.read(&mut buffered)?;
        buffered.truncate(len);
        self.buffered = buffered;
        Ok(())
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

impl EnclaveComm for Tcp {
    fn init() -> Self {
        Tcp::new(ENCLAVE_ADDRESS).unwrap()
    }
}
