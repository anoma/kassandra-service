use std::net::TcpStream;

use crate::error::{self, Error};
use shared::{ClientMsg, FramedBytes, ReadWriteByte, ServerMsg};

pub(crate) struct OutgoingTcp(shared::tcp::Tcp);

impl OutgoingTcp {
    /// Create a new connection from a stream
    pub fn new(url: &str) -> error::Result<Self> {
        let stream = TcpStream::connect(url).map_err(Error::Io)?;
        Ok(Self(shared::tcp::Tcp::new(stream)))
    }

    /// Send a message to a service
    pub fn write(&mut self, msg: ClientMsg) {
        self.write_frame(&msg);
    }

    /// Receive a message from a service
    pub fn read(&mut self) -> error::Result<ServerMsg> {
        let frame = self.get_frame().map_err(Error::MsgError)?;
        frame.deserialize().map_err(Error::MsgError)
    }
}

impl ReadWriteByte for OutgoingTcp {
    fn read_byte(&mut self) -> u8 {
        self.0.read_byte()
    }

    fn write_bytes(&mut self, buf: &[u8]) {
        self.0.write_bytes(buf)
    }
}
