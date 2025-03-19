#![no_std]
extern crate alloc;
mod com;

use alloc::string::ToString;

use ostd::arch::x86::qemu::{exit_qemu, QemuExitCode};
use ostd::prelude::*;
use shared::{MsgFromHost, MsgToHost};
use shared::tee::{EnclaveRNG, RemoteAttestation};
use tdx_quote::{Quote, SigningKey};

use crate::com::HostCom;

#[ostd::main]
fn kernel_main() {
    println!("Enclave kernel initialized!");
    enclave::main::<Tdx, HostCom, Rng>();
    exit_qemu(QemuExitCode::Success);
}

#[derive(Copy,Clone)]
struct Tdx;

impl RemoteAttestation for Tdx {
    fn init() -> Self {
        Self
    }

    #[cfg(feature = "mock")]
    fn get_quote(&self, report_data: [u8; 64]) -> Quote {
        let attestation_key = SigningKey::from_slice(&[1; 32]).unwrap();
        let pck_key = SigningKey::from_slice(&[2; 32]).unwrap();
        Quote::mock(attestation_key, pck_key, report_data, alloc::vec![])
    }

    #[cfg(not(feature = "mock"))]
    fn get_quote(&self, report_data: [u8; 64]) -> Quote {
        todo!()
    }
}

use rand_core::{CryptoRng, Error, RngCore};


#[derive(Copy, Clone)]
struct Rng;

impl RngCore for Rng {
    fn next_u32(&mut self) -> u32 {
        let r = self.next_u64().to_le_bytes();
        u32::from_le_bytes([r[0], r[1], r[2], r[3]])
    }

    fn next_u64(&mut self) -> u64 {
        ostd::arch::x86::read_random().unwrap()
    }

    fn fill_bytes(&mut self, dst: &mut [u8]) {
        let mut ix = 0;
        let mut r = self.next_u64().to_le_bytes().to_vec();

        while let Some(b) = dst.get_mut(ix) {
            if r.is_empty() {
                r = self.next_u64().to_le_bytes().to_vec();
            }
            *b = r.pop().unwrap();
            ix += 1;
        }
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> core::result::Result<(), Error> {
        Ok(self.fill_bytes(dest))
    }
}

impl CryptoRng for Rng {}

impl EnclaveRNG for Rng {
    fn init() -> Self {
        Self
    }
}