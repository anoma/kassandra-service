use std::net::TcpStream;

use shared::{ClientMsg, FramedBytes, MsgError, ReadWriteByte, ServerMsg};

pub(crate) struct OutgoingTcp(shared::tcp::Tcp);

impl OutgoingTcp {
    /// Create a new connection from a stream
    pub fn new(url: &str) -> Self {
        let stream = TcpStream::connect(url).unwrap();
        Self(shared::tcp::Tcp::new(stream))
    }

    /// Send a [`MsgFromHost`] into the enclave
    pub fn write(&mut self, msg: ClientMsg) {
        self.write_frame(&msg);
    }

    /// Read a message sent from the enclave
    pub fn read(&mut self) -> Result<ServerMsg, MsgError> {
        let frame = self.get_frame()?;
        frame.deserialize()
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
