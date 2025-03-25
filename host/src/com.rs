//! Communication primitives for talking with enclavees and clients

use std::io;
use std::io::prelude::*;
use std::net::TcpStream;
use std::time::Duration;

use shared::{ClientMsg, FramedBytes, MsgError, MsgFromHost, MsgToHost, ReadWriteByte, ServerMsg};

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

#[derive(Clone)]
pub(crate) struct IncomingTcp {
    raw: shared::tcp::Tcp,
    timeout: Duration,
}

impl IncomingTcp {
    /// Create a new connection from a stream
    pub fn new(stream: TcpStream, timeout: Duration) -> Self {
        Self {
            raw: shared::tcp::Tcp::new(stream),
            timeout,
        }
    }

    /// Send a [`MsgFromHost`] into the enclave
    pub fn write(&mut self, msg: ServerMsg) {
        self.write_frame(&msg);
    }

    /// Read a message sent from the enclave
    pub fn read(&mut self) -> Result<ClientMsg, MsgError> {
        let frame = self.get_frame()?;
        frame.deserialize()
    }

    /// Try to read from a connection to a client. Times out if message is not
    /// received within time.
    pub async fn timed_read(&mut self) -> Option<Result<ClientMsg, MsgError>> {
        let mut conn = self.clone();
        let read = tokio::spawn(async move { conn.read() });
        tokio::select! {
            _ = tokio::time::sleep(self.timeout) => None,
            val = read => Some(val.ok()).flatten()
        }
    }
}

impl ReadWriteByte for IncomingTcp {
    fn read_byte(&mut self) -> u8 {
        self.raw.read_byte()
    }

    fn write_bytes(&mut self, buf: &[u8]) {
        self.raw.write_bytes(buf)
    }
}
