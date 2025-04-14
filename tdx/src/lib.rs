//! A TDX implementation of the FMD detection portion of the Kassandra service

#![no_std]
extern crate alloc;
mod com;

use alloc::vec::Vec;
use drbg::ctr::{CtrBuilder, CtrDrbg};
use drbg::entropy::Entropy;
use ostd::arch::x86::qemu::{exit_qemu, QemuExitCode};
use ostd::prelude::*;
use rand_core::{CryptoRng, Error, RngCore};
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
    fn get_quote(&self, report_data: [u8; 64]) -> Vec<u8> {
        let attestation_key = SigningKey::from_slice(&[1; 32]).unwrap();
        let pck_key = SigningKey::from_slice(&[2; 32]).unwrap();
        Quote::mock(attestation_key, pck_key, report_data, alloc::vec![]).as_bytes()
    }

    #[cfg(not(feature = "mock"))]
    fn get_quote(&self, report_data: [u8; 64]) -> Vec<u8> {
        todo!()
    }
}

struct Rng {
    inner: CtrDrbg<Seed>,
}

impl RngCore for Rng {
    fn next_u32(&mut self) -> u32 {
        let mut bytes = [0u8; 4];
        self.fill_bytes(&mut bytes);
        u32::from_le_bytes(bytes)
    }

    fn next_u64(&mut self) -> u64 {
        let mut bytes = [0u8; 8];
        self.fill_bytes(&mut bytes);
        u64::from_le_bytes(bytes)
    }

    fn fill_bytes(&mut self, dst: &mut [u8]) {
        self.inner.fill_bytes(dst, None).unwrap()
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> core::result::Result<(), Error> {
        Ok(self.fill_bytes(dest))
    }
}

impl CryptoRng for Rng {}

impl Clone for Rng {
    fn clone(&self) -> Self {
        Self::init()
    }
}

impl EnclaveRNG for Rng {
    fn init() -> Self {
        Self {
            inner: CtrBuilder::new(Seed)
                .personal("TDX pseudo-random number generator".as_bytes())
                .build()
                .unwrap()
        }
    }
}

#[derive(Copy, Clone)]
struct Seed;

impl Entropy for Seed {
    fn fill_bytes(&mut self, bytes: &mut [u8]) -> core::result::Result<(), drbg::entropy::Error> {
        let mut seed = 0u64;
        for ix in 0..bytes.len() {
            if ix.rem_euclid(4) == 0 {
                unsafe { while core::arch::x86_64::_rdseed64_step(&mut seed) != 1 {} }
                core::hint::spin_loop();
            }
            bytes[ix] = seed.to_le_bytes()[ix.rem_euclid(4)];
        }
        Ok(())
    }
}
