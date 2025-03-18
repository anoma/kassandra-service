//! A highly simplified version of RA-TLS. This performs a Diffie-Hellman
//! key exchange using a hardcoded cryptographic suits as well as remote
//! attestation. If successful, a single encrypted message containing an
//! FMD key is sent and the connection is terminated. This means that
//! we do not need to maintain a list of active sessions or session ids.

use alloc::vec::Vec;

use crate::{ClientMsg, MsgFromHost, MsgToHost};
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{AeadCore, ChaCha20Poly1305, Key, KeyInit, Nonce};
use rand_core::{CryptoRng, RngCore};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tdx_quote::QuoteParseError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RatlsError {
    #[error("Cannot perform Diffie-Hellman on a connection that is already initialized")]
    AlreadyInitialized,
    #[error("Shared Secret was non-contributory. This suggests a man-in-the-middle attack.")]
    NonContributory,
    #[error("Cannot encrypt to a non-initialized channel")]
    NotInitialized,
    #[error("Could not decrypt message")]
    Decryption,
    #[error("Failed to deserialize message with: {0}")]
    Deserialize(serde_cbor::Error),
}

/// A ChaCha20 encrypted payload with nonce
#[derive(Debug, Clone)]
pub struct TlsCiphertext {
    payload: Vec<u8>,
    nonce: Nonce,
}

impl Serialize for TlsCiphertext {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct SimplifiedCiphertext {
            payload: Vec<u8>,
            nonce: Vec<u8>,
        }
        let simplified = SimplifiedCiphertext {
            payload: self.payload.clone(),
            nonce: self.nonce.as_slice().to_vec(),
        };
        simplified.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TlsCiphertext {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct SimplifiedCiphertext {
            payload: Vec<u8>,
            nonce: Vec<u8>,
        }
        let simplified = SimplifiedCiphertext::deserialize(deserializer)?;
        Ok(Self {
            payload: simplified.payload,
            nonce: *Nonce::from_slice(&simplified.nonce),
        })
    }
}

/// A simplified, bespoke RA-TLS connection
/// It can be in two possible states:
///
///  * `Handshake` - initializing session between two parties
///  * `Initialized` - ready for communicating messages securely
pub enum Connection {
    Handshake {
        ephemeral_key: x25519_dalek::EphemeralSecret,
    },
    Initialized {
        shared_key: ChaCha20Poly1305,
    },
}

impl Connection {
    /// Create a new connection, which creates and ephemeral key for
    /// Diffie-Hellman
    pub fn new(rng: impl CryptoRng + RngCore) -> Self {
        Self::Handshake {
            ephemeral_key: x25519_dalek::EphemeralSecret::random_from_rng(rng),
        }
    }

    /// The client side sends its ephemeral public key
    pub fn client_send(&self, nonce: u64) -> Result<ClientMsg, RatlsError> {
        match &self {
            Self::Handshake { ephemeral_key } => Ok(ClientMsg::RegisterKey {
                nonce,
                pk: x25519_dalek::PublicKey::from(ephemeral_key).to_bytes().into(),
            }),
            Self::Initialized { .. } => Err(RatlsError::AlreadyInitialized),
        }
    }

    /// The enclave replies with its Attestation report, which contains
    /// its ephemeral public key and a session id.
    pub fn enclave_reply(&self, report: Vec<u8>) -> Result<MsgToHost, RatlsError> {
        match &self {
            Self::Handshake { .. } => Ok(MsgToHost::RATLS { report }),
            Self::Initialized { .. } => Err(RatlsError::AlreadyInitialized),
        }
    }

    /// Compute the shared ChaCha20 public key for the connection.
    pub fn initialize(self, pk: x25519_dalek::PublicKey) -> Result<Self, RatlsError> {
        let Self::Handshake { ephemeral_key } = self else {
            return Err(RatlsError::AlreadyInitialized);
        };
        let shared_secret = ephemeral_key.diffie_hellman(&pk);
        let shared_key = if shared_secret.was_contributory() {
            ChaCha20Poly1305::new(Key::from_slice(shared_secret.as_bytes()))
        } else {
            return Err(RatlsError::NonContributory);
        };
        Ok(Self::Initialized { shared_key })
    }

    /// Encrypt a message with the session key
    pub fn encrypt_msg<T: CryptoRng + RngCore>(
        &self,
        payload: &[u8],
        rng: &mut T,
    ) -> Result<TlsCiphertext, RatlsError> {
        if let Self::Initialized { shared_key } = &self {
            let nonce = ChaCha20Poly1305::generate_nonce(rng);
            Ok(TlsCiphertext {
                payload: shared_key.encrypt(&nonce, payload).unwrap(),
                nonce,
            })
        } else {
            Err(RatlsError::NotInitialized)
        }
    }

    /// Decrypt and deserialize  message
    pub fn decrypt_msg<T: DeserializeOwned>(&self, msg: &TlsCiphertext) -> Result<T, RatlsError> {
        if let Self::Initialized { shared_key } = &self {
            shared_key
                .decrypt(&msg.nonce, &*msg.payload)
                .or(Err(RatlsError::Decryption))
                .and_then(|p| serde_cbor::from_slice(p.as_slice()).map_err(RatlsError::Deserialize))
        } else {
            Err(RatlsError::NotInitialized)
        }
    }
}
