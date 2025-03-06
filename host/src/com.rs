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
        let frame = self.get_frame()?;
        frame.deserialize()
    }

    /// Read data from the stream into an internal buffer.
    /// The buffer is a stack, so the bytes are stored in
    /// reverse order that they are received.
    fn buffered_read(&mut self) -> io::Result<()> {
        let mut buffered = vec![];
        self.raw.read_to_end(&mut buffered)?;
        buffered.reverse();
        std::mem::swap(&mut self.buffered, &mut buffered);
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
        let b = self.buffered.pop().unwrap();
        println!("Read byte {b}");
        b
    }

    fn write_bytes(&mut self, buf: &[u8]) {
        self.raw.write_all(buf).unwrap();
        self.raw.flush().unwrap();
    }
}
