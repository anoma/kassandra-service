//! The fuzzy message detection logic the enclave must perform

use alloc::string::ToString;
use alloc::vec::Vec;

use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
use fmd::MultiFmdScheme;
use fmd::fmd2_compact::FlagCiphertexts;
use shared::MsgToHost;
use shared::db::{EncKey, EncryptedResponse, Index};
use shared::ratls::FmdKeyRegistration;
use shared::tee::{EnclaveComm, EnclaveRNG, RemoteAttestation};

use crate::Ctx;

/// The current status of which MASP txs a user
/// should trial decrypt
pub struct IndexSet {
    /// The last block height that FMD has been done
    /// for this user
    pub synced_to: u64,
    /// The indexed txs corresponding the MASP txs that
    /// the user should trial decrypt. Stored here opaquely
    pub indices: Vec<Index>,
}

impl Default for IndexSet {
    fn default() -> Self {
        Self {
            synced_to: 1,
            indices: Vec::new(),
        }
    }
}

impl From<u64> for IndexSet {
    fn from(value: u64) -> Self {
        Self {
            synced_to: value,
            indices: Vec::new(),
        }
    }
}

impl IndexSet {
    /// The next block height to perform FMD on.
    pub(crate) fn next(&self) -> u64 {
        self.synced_to + 1
    }

    /// Advance the block height synced to by one
    fn advance(&mut self) {
        self.synced_to += 1;
    }

    /// Add the encryption of `self` to the
    /// results contained in the response to the host.
    fn add_result(&self, enc_key: &EncKey, nonce: Nonce, msg: MsgToHost) -> MsgToHost {
        let cipher = ChaCha20Poly1305::new(enc_key.into());
        match cipher.encrypt(&nonce, self.index_bytes().as_slice()) {
            Err(e) => MsgToHost::Error(e.to_string()),
            Ok(indices) => match msg {
                msg @ MsgToHost::Error(_) => msg,
                MsgToHost::FmdResults(mut results) => {
                    results.push(EncryptedResponse {
                        owner: enc_key.hash(),
                        nonce: *nonce.as_ref(),
                        indices,
                        height: self.synced_to,
                    });
                    MsgToHost::FmdResults(results)
                }
                _ => unreachable!(),
            },
        }
    }

    fn index_bytes(&self) -> Vec<u8> {
        self.indices.iter().flat_map(|ix| ix.as_bytes()).collect()
    }
}

/// Check the input flags against all registered keys.
///
/// On success, add this flag's index to the registered key's data.
/// Creates a message for the host with encrypted versions of each
/// key's updated index sets.
pub fn check_flags<RA, COM, RNG>(
    ctx: &mut Ctx<RA, COM, RNG>,
    registered_keys: &mut [(FmdKeyRegistration, IndexSet)],
    synced_to: u64,
    flags: Vec<(Index, Option<FlagCiphertexts>)>,
) -> MsgToHost
where
    RA: RemoteAttestation,
    COM: EnclaveComm,
    RNG: EnclaveRNG,
{
    let mut response = MsgToHost::FmdResults(Vec::new());
    for (key, indices) in registered_keys
        .iter_mut()
        .filter(|(_, ix)| ix.synced_to < synced_to)
    {
        for (ix, flag) in flags
            .iter()
            .filter(|(ix, _)| ix.height == indices.synced_to + 1)
        {
            if match flag {
                None => true,
                Some(flag) => ctx.scheme.detect(&key.fmd_key, flag),
            } {
                indices.indices.push(*ix);
            }
        }
        let mut nonce_bytes = [0u8; 12];
        ctx.rng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from(nonce_bytes);
        response = indices.add_result(&key.enc_key, nonce, response);
        indices.advance();
    }
    response
}
