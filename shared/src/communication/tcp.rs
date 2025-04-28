//! Communication primitives for talking with hosts

use std::io::ErrorKind;
use std::io::prelude::*;
use std::net::{TcpListener, TcpStream};
use std::prelude::rust_2024::{String, Vec};
use std::{io, vec};

use crate::ReadWriteByte;
use crate::tee::EnclaveComm;
use once_cell::sync::OnceCell;

pub const DEFAULT_ENCLAVE_ADDRESS: &str = "0.0.0.0:12345";

/// The TCP address for the host-enclave channel
pub static ENCLAVE_ADDRESS: OnceCell<String> = OnceCell::new();

/// A TCP stream connected with the host
/// **NOT THREAD SAFE**
pub struct Tcp {
    pub raw: TcpStream,
    buffered: Vec<u8>,
}

impl Tcp {
    /// Create a new connection from a stream
    pub fn new(stream: TcpStream) -> Self {
        Self {
            raw: stream,
            buffered: vec![],
        }
    }
    /// Listen for a connection request from the host. Once
    /// received, return the stream.
    pub fn connect(url: &str) -> io::Result<Self> {
        let listener = TcpListener::bind(url)?;
        loop {
            if let Some(Ok(stream)) = listener.incoming().next() {
                break Ok(Self {
                    raw: stream,
                    buffered: Default::default(),
                });
            }
        }
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
            match self.buffered_read() {
                Err(err) if err.kind() == ErrorKind::WouldBlock => continue,
                Ok(()) => {}
                Err(e) => panic!("{e}"),
            }
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
        Self::connect(ENCLAVE_ADDRESS.get().unwrap()).unwrap()
    }
}
