//! Traits to abstract away particular TEE implementations

use alloc::string::ToString;
use rand_core::{CryptoRng, RngCore};

use crate::{FramedBytes, MsgError, MsgFromHost, MsgToHost};

/// Logic for clients to verify enclave reports and extract data
/// from them
pub trait EnclaveClient {
    type Error: core::error::Error + core::fmt::Display;

    /// Verifies an attestation report and returns the user data
    /// if successful. The nonce is a challenge
    /// provided by the client to protect against replays.
    fn verify_quote(report: &[u8], nonce: u64) -> Result<[u8; 64], Self::Error>;
}

/// Logic for enclaves to generate remote attestation reports.
pub trait RemoteAttestation: Clone {
    fn init() -> Self;
    fn get_quote(&self, report_data: [u8; 64]) -> alloc::vec::Vec<u8>;
}

/// High level methods for the enclave to communicate with
/// its host and clients.
pub trait EnclaveComm: FramedBytes {
    /// Instantiate the communication channel
    fn init() -> Self;

    /// Read a message from the host
    fn read(&mut self) -> Result<MsgFromHost, MsgError> {
        let frame = self.get_frame()?;
        frame.deserialize()
    }

    /// Write a message to the host
    fn write(&mut self, msg: &MsgToHost) {
        self.write_frame(msg)
    }

    /// A factory function for writing errors back
    /// to the host.
    fn write_err(&mut self, err: &str) {
        self.write(&MsgToHost::Error(err.to_string()))
    }

    /// A factory function for writing errors back
    /// to a client.
    fn write_client_err(&mut self, err: &str) {
        self.write(&MsgToHost::ErrorForClient(err.to_string()))
    }
}

/// Stricter requirements on an RNG source
pub trait EnclaveRNG: RngCore + CryptoRng + Clone {
    fn init() -> Self;
}
