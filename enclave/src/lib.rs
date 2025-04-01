#![no_std]
extern crate alloc;

use alloc::string::ToString;
use alloc::vec::Vec;
use shared::tee::{EnclaveComm, EnclaveRNG, RemoteAttestation};
use shared::{MsgFromHost, MsgToHost};

#[derive(Clone)]
struct Ctx<RA, COM, RNG>
where
    RA: RemoteAttestation,
    COM: EnclaveComm,
    RNG: EnclaveRNG,
{
    ra: RA,
    com: COM,
    rng: RNG,
}

impl<RA, COM, RNG> Ctx<RA, COM, RNG>
where
    RA: RemoteAttestation,
    COM: EnclaveComm,
    RNG: EnclaveRNG,
{
    pub fn init() -> Self {
        Self {
            ra: RA::init(),
            com: COM::init(),
            rng: RNG::init(),
        }
    }
}

pub mod ratls;

pub fn main<RA, COM, RNG>()
where
    RA: RemoteAttestation,
    COM: EnclaveComm,
    RNG: EnclaveRNG,
{
    let mut ctx = Ctx::<RA, COM, RNG>::init();
    let mut registered_keys = Vec::new();

    loop {
        match ctx.com.read() {
            Ok(msg) => match msg {
                MsgFromHost::RegisterKey { nonce, pk } => {
                    if let Some(key) =
                        ratls::register_key(&mut ctx, x25519_dalek::PublicKey::from(pk.0), nonce)
                    {
                        registered_keys.push(key);
                    }
                }
                MsgFromHost::RequestReport { user_data } => {
                    let quote = ctx.ra.get_quote(user_data.0);
                    ctx.com.write(&MsgToHost::Report(quote));
                }
                _ => {}
            },
            Err(e) => {
                ctx.com.write(&MsgToHost::Error(e.to_string()));
            }
        }
        core::hint::spin_loop();
    }
}
