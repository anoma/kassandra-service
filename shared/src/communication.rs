use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::ratls::TlsCiphertext;
use serde::de::{DeserializeOwned, Error};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

#[derive(Debug, Copy, Clone)]
pub struct HexBytes<const N: usize>(pub [u8; N]);

impl<const N: usize> From<[u8; N]> for HexBytes<N> {
    fn from(value: [u8; N]) -> Self {
        Self(value)
    }
}

macro_rules! impl_serde {
    ($n:literal) => {
        impl Serialize for HexBytes<$n> {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&hex::encode(self.0))
            }
        }

        impl<'de> Deserialize<'de> for HexBytes<$n> {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let s = String::deserialize(deserializer)?;
                Ok(Self(
                    hex::decode(s.as_bytes())
                        .map_err(|_| Error::custom("Invalid hex"))?
                        .try_into()
                        .map_err(|_| Error::custom("Bytes were of wrong size"))?,
                ))
            }
        }
    };
}

impl_serde!(32);
impl_serde!(64);

/// Messages to host environment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MsgToHost {
    Basic(String),
    Error(String),
    ErrorForClient(String),
    RATLS { report: Vec<u8> },
    Report(Vec<u8>),
    KeyRegSuccess,
}

/// Messages from host environment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MsgFromHost {
    Basic(String),
    RegisterKey { nonce: u64, pk: HexBytes<32> },
    RequestReport { user_data: HexBytes<64> },
    RATLSAck(AckType),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMsg {
    /// Gives the clients public part of the shared key
    /// and requests the enclaves part.
    RegisterKey {
        nonce: u64,
        pk: HexBytes<32>,
    },
    RequestReport {
        user_data: HexBytes<64>,
    },
    RATLSAck(AckType),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AckType {
    Success(TlsCiphertext),
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMsg {
    /// The raw report bytes
    RATLS {
        report: Vec<u8>,
    },
    Error(String),
    KeyRegSuccess,
}

impl<'a> TryFrom<&'a ClientMsg> for MsgFromHost {
    type Error = &'static str;

    fn try_from(msg: &'a ClientMsg) -> Result<Self, Self::Error> {
        match msg {
            ClientMsg::RegisterKey { nonce, pk } => Ok(MsgFromHost::RegisterKey {
                nonce: *nonce,
                pk: *pk,
            }),
            ClientMsg::RequestReport { user_data } => Ok(MsgFromHost::RequestReport {
                user_data: *user_data,
            }),
            ClientMsg::RATLSAck(v) => Ok(MsgFromHost::RATLSAck(v.clone())),
        }
    }
}

impl TryFrom<MsgToHost> for ServerMsg {
    type Error = &'static str;

    fn try_from(msg: MsgToHost) -> Result<Self, &'static str> {
        match msg {
            MsgToHost::RATLS { report } => Ok(ServerMsg::RATLS { report }),
            MsgToHost::ErrorForClient(err) => Ok(ServerMsg::Error(err)),
            MsgToHost::KeyRegSuccess => Ok(ServerMsg::KeyRegSuccess),
            _ => Err("Message not intended for client"),
        }
    }
}

#[derive(Error, Debug)]
pub enum MsgError {
    #[error("COBS failed to decode message from COM 2 with: {0}")]
    Decode(cobs::DecodeError),
    #[error("Failed to deserialize CBOR with: {0}")]
    Deserialize(serde_cbor::Error),
    #[error("Input bytes were not valid utf-8: {0:?}")]
    Utf8(Vec<u8>),
}

pub struct Frame {
    pub bytes: Vec<u8>,
}

impl Frame {
    pub fn deserialize<T: DeserializeOwned>(self) -> Result<T, MsgError> {
        serde_cbor::from_slice(&self.bytes).map_err(MsgError::Deserialize)
    }
}

/// A trait for getting the next byte in a byte stream
pub trait ReadWriteByte {
    const FRAME_BUF_SIZE: usize = 1024;
    fn read_byte(&mut self) -> u8;

    fn write_bytes(&mut self, buf: &[u8]);
}

/// A trait for reading / writing framed data from a byte stream.
/// This trait should not be implemented directly, but rely on
/// the default implementation.
pub trait FramedBytes: ReadWriteByte {
    /// Blocking method that reads a frame
    ///
    /// Uses an initial buffer with 1Kb in size. Dynamically increases the
    /// size of the frame buffer by 1Kb until either the message is decoded
    /// or an error occurs.
    ///
    /// Returns the raw framed bytes
    fn get_frame(&mut self) -> Result<Frame, MsgError> {
        // initial buffer size for the frame
        let mut buf_size = Self::FRAME_BUF_SIZE;
        // keep track of bytes processed so far incase we need to increase
        // buffer size
        let mut read_bytes = Vec::<u8>::with_capacity(buf_size);
        // continue trying to populate the frame buffer until
        // a successful frame decoding or a decode error occurs.
        loop {
            // initial buffer
            let mut frame_buf = vec![0u8; buf_size];
            let mut decoder = cobs::CobsDecoder::new(&mut frame_buf);
            decoder
                .push(&read_bytes)
                .expect("Previously read bytes should not produce a frame error.");

            loop {
                let b = self.read_byte();
                read_bytes.push(b);
                match decoder.feed(b) {
                    Ok(None) => continue,
                    Ok(Some(len)) => {
                        frame_buf.truncate(len);
                        return Ok(Frame { bytes: frame_buf });
                    }
                    Err(cobs::DecodeError::TargetBufTooSmall) => {
                        // increase the buffer size ny 1Kb
                        buf_size += Self::FRAME_BUF_SIZE;
                        break;
                    }
                    Err(e) => return Err(MsgError::Decode(e)),
                }
            }
        }
    }

    /// Write a serializable message out to the serial port in CBOR,
    /// framed with COBS.
    fn write_frame<T: Serialize>(&mut self, msg: &T) {
        let data = serde_cbor::to_vec(&msg).unwrap();
        let mut encoded = cobs::encode_vec_with_sentinel(&data, 0);
        encoded.push(0);
        self.write_bytes(&encoded);
    }
}

impl<T: ReadWriteByte> FramedBytes for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    struct MockChannel(Vec<u8>);

    impl ReadWriteByte for MockChannel {
        const FRAME_BUF_SIZE: usize = 10;
        fn read_byte(&mut self) -> u8 {
            self.0.remove(0)
        }

        fn write_bytes(&mut self, buf: &[u8]) {
            self.0.extend_from_slice(buf);
        }
    }

    /// Test that if the data we are decoding does not initially
    /// fit into the frame buffer, we dynamically resize it until the
    /// data fits and decoding is successful.
    #[test]
    fn test_dynamic_frame_resizing() {
        let msg = MsgFromHost::Basic("Test".to_string());
        let data = serde_cbor::to_vec(&msg).expect("Test failed");
        let mut encoded = cobs::encode_vec_with_sentinel(&data, 0);
        encoded.push(0);
        let mut channel = MockChannel(encoded);
        let frame = channel.get_frame().expect("Test failed");
        let Ok(MsgFromHost::Basic(str)) = frame.deserialize() else {
            panic!("Test failed");
        };
        assert_eq!(str, "Test");
    }
}
