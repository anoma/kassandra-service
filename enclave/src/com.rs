//! Tools for interacting with host environment

use alloc::vec;
use core::ops::Deref;
use ostd::arch::x86::device::serial::SerialPort;
use ostd::sync::Mutex;
use shared::{Frame, FramedBytes, MsgError, MsgFromHost, MsgToHost, ReadWriteByte};

/// A serial port for communicating with the host.
pub struct HostCom;

static HOST_COM: Mutex<SerialPort> = Mutex::new(
    // Serial port: COM 2
    unsafe { SerialPort::new(0x2F8) },
);

impl HostCom {
    /// Initialize the connection
    pub fn init() {
        HOST_COM.lock().init();
    }

    /// Write a buffer of bytes to the serial port
    pub fn write_bytes(buf: &[u8]) {
        let com = HOST_COM.lock();
        for b in buf.iter().copied() {
           Self::write_byte(com, b);
        }
    }

    /// Write to the host environment
    pub fn write(msg: MsgToHost) {
        let mut com = Self;
        com.write_frame(&msg);
    }

    /// If data is available on the port, attempts to read it and
    /// deserialize it, which is blocking. If no data is available,
    /// it does not wait but returns immediately.
    pub fn try_read() -> Result<Option<MsgFromHost>, MsgError> {
        if let Some(frame) = Self::try_read_frame()? {
            frame.deserialize()
        } else {
            Ok(None)
        }
    }

    /// Blocking read method to get a message form the host.
    pub fn read() -> Result<MsgFromHost, MsgError> {
        let frame = Self::get_frame()?;
        frame.deserialize()
    }

    pub fn read_string() -> Result<alloc::string::String, MsgError> {
        let mut bytes = vec![];
        while bytes.is_empty() {
            while HOST_COM.lock().line_status() & 1 == 1 {
                bytes.push(Self::read_byte());
            }
        }
        alloc::string::String::from_utf8(bytes.clone()).map_err(|_| MsgError::Utf8(bytes))
    }

    /// Block until a byte is read
    fn read_byte() -> u8 {
        let com = HOST_COM.lock();
        loop {
            if com.line_status() & 1 == 1 {
                break com.recv();
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

    fn get_frame() -> Result<Frame, MsgError> {
        let mut com = Self;
        com.get_frame()
    }

    fn write_byte(com: impl Deref<SerialPort>, data: u8)  {
        const OUTPUT_EMPTY: u8 = 1 << 5;
        match data {
            8 | 0x7F => {
                while com.line_status() & OUTPUT_EMPTY == 0 {}
                com.send(8);
                while com.line_status() & OUTPUT_EMPTY == 0 {}
                com.send(b' ');
                while com.line_status() & OUTPUT_EMPTY == 0 {}
                com.send(8);
            }
            _ => {
                while com.line_status() & OUTPUT_EMPTY == 0 {}
                com.send(data);
            }
        }
    }
}

impl ReadWriteByte for HostCom {
    fn read_byte(&mut self) -> u8 {
        Self::read_byte()
    }

    fn write_bytes(&mut self, buf: &[u8]) {
        Self::write_bytes(buf)
    }
}
