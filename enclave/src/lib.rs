#![no_std]
extern crate alloc;

use ::fmd::fmd2_compact::MultiFmd2CompactScheme;
use alloc::string::ToString;
use alloc::vec::Vec;
use shared::tee::{EnclaveComm, EnclaveRNG, RemoteAttestation};
use shared::{MsgFromHost, MsgToHost};

use crate::fmd::{IndexSet, check_flags};

const GAMMA: usize = 12;

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
    scheme: MultiFmd2CompactScheme,
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
            scheme: MultiFmd2CompactScheme::new(GAMMA, 1),
        }
    }
}

mod fmd;
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
                        let synced_height = key.birthday.unwrap_or(1);
                        registered_keys.push((key, IndexSet::from(synced_height)));
                    }
                }
                MsgFromHost::RequestReport { user_data } => {
                    let quote = ctx.ra.get_quote(user_data.0);
                    ctx.com.write(&MsgToHost::Report(quote));
                }
                MsgFromHost::RequiredBlocks => {
                    let heights = registered_keys.iter().map(|(_, ixs)| ixs.next()).collect();
                    ctx.com.write(&MsgToHost::BlockRequests(heights));
                }
                MsgFromHost::RequestedFlags { synced_to, flags } => {
                    let response = check_flags(&mut ctx, &mut registered_keys, synced_to, flags);
                    ctx.com.write(&response);
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
