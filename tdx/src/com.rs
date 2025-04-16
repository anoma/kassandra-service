//! Tools for interacting with host environment
use alloc::vec;

use ostd::arch::x86::device::serial::SerialPort;
use ostd::sync::Mutex;
use shared::{Frame, FramedBytes, MsgError, MsgFromHost, MsgToHost, ReadWriteByte};
use shared::tee::EnclaveComm;

/// A serial port for communicating with the host.
#[derive(Copy, Clone)]
pub struct HostCom;

static HOST_COM: Mutex<SerialPort> = Mutex::new(
    // Serial port: COM 2
    unsafe { SerialPort::new(0x2F8) },
);

impl HostCom {
    /// Write a buffer of bytes to the serial port
    pub fn write_bytes(buf: &[u8]) {
        let com = HOST_COM.lock();
        for b in buf.iter().copied() {
            Self::write_byte(&*com, b);
        }
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

    fn get_frame() -> Result<Frame, MsgError> {
        let mut com = Self;
        com.get_frame()
    }

    fn write_byte(com: &SerialPort, data: u8) {
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

impl EnclaveComm for HostCom {
    fn init() -> Self {
        HOST_COM.lock().init();
        Self
    }

    fn read(&mut self) -> Result<MsgFromHost, MsgError> {
        let frame = Self::get_frame()?;
        frame.deserialize()
    }

    fn write(&mut self, msg: &MsgToHost) {
        let mut com = Self;
        com.write_frame(&msg);
    }
}
