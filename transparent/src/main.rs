//! An implementation of the FMD detection portion of the Kassandra service that
//! does not run in a TEE.

mod com;

use crate::com::Tcp;
use rand_core::{CryptoRng, Error, OsRng, RngCore};
use shared::tee::{EnclaveRNG, RemoteAttestation};

fn main() {
    init_logging();
    tracing::info!("FMD service initialized, running transparently.");
    enclave::main::<Transparent, Tcp, TRng>();
}

#[derive(Copy, Clone)]
struct Transparent;

impl RemoteAttestation for Transparent {
    fn init() -> Self {
        Self
    }

    fn get_quote(&self, report_data: [u8; 64]) -> Vec<u8> {
        report_data.to_vec()
    }
}

fn init_logging() {
    tracing_subscriber::fmt::SubscriberBuilder::default()
        .with_ansi(true)
        .init();
}

#[derive(Copy, Clone)]
struct TRng(OsRng);

impl RngCore for TRng {
    fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }

    fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.0.fill_bytes(dest)
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Error> {
        self.0.try_fill_bytes(dest)
    }
}

impl CryptoRng for TRng {}

impl EnclaveRNG for TRng {
    fn init() -> Self {
        Self(OsRng)
    }
}
