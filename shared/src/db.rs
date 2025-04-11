//! Shared types to be stored in the host databases

use core::fmt::Formatter;

use chacha20poly1305::Key;
use serde::de::{Error, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A wrapper around a ChaCha key
///
/// Used to encrypted enclave responses for users
#[derive(Debug, Clone)]
pub struct EncKey(Key);

impl EncKey {
    /// Get the hash of this key
    pub fn hash(&self) -> alloc::string::String {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(self.0.as_slice());
        let hash: [u8; 32] = hasher.finalize().into();
        hex::encode(hash)
    }
}

impl From<Key> for EncKey {
    fn from(key: Key) -> Self {
        Self(key)
    }
}

impl<'a> From<&'a EncKey> for &'a Key {
    fn from(key: &'a EncKey) -> Self {
        &key.0
    }
}

impl Serialize for EncKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(self.0.as_slice())
    }
}

impl<'de> Deserialize<'de> for EncKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct EncKeyVisitor;
        impl Visitor<'_> for EncKeyVisitor {
            type Value = EncKey;

            fn expecting(&self, formatter: &mut Formatter) -> core::fmt::Result {
                formatter.write_str("32 bytes")
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: Error,
            {
                let bytes: [u8; 32] = v
                    .try_into()
                    .map_err(|_| Error::custom("Unexpected length of encryption key"))?;
                Ok(EncKey(*Key::from_slice(&bytes)))
            }
        }

        deserializer.deserialize_bytes(EncKeyVisitor)
    }
}

/// Simplified domain type for indexing a Tx on chain
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct Index {
    pub height: u64,
    pub tx: u32,
}

impl Index {
    pub fn as_bytes(&self) -> [u8; 12] {
        let mut bytes = [0u8; 12];
        let h_bytes = self.height.to_le_bytes();
        let tx_bytes = self.tx.to_le_bytes();
        for ix in 0..12 {
            if ix < 8 {
                bytes[ix] = h_bytes[ix];
            } else {
                bytes[ix] = tx_bytes[ix - 8];
            }
        }
        bytes
    }

    pub fn try_from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 12 {
            None
        } else {
            let mut h_bytes = [0u8; 8];
            let mut tx_bytes = [0u8; 4];
            for ix in 0..12 {
                if ix < 8 {
                    h_bytes[ix] = bytes[ix];
                } else {
                    tx_bytes[ix - 8] = bytes[ix];
                }
            }
            Some(Self {
                height: u64::from_le_bytes(h_bytes),
                tx: u32::from_le_bytes(tx_bytes),
            })
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexList(alloc::vec::Vec<Index>);

impl IndexList {
    /// Try to parse bytes into a list of indices
    pub fn try_from_bytes(bytes: &[u8]) -> Option<Self> {
        if 12 * (bytes.len() / 12) != bytes.len() {
            return None;
        }
        let len = bytes.len() / 12;
        let indices: alloc::vec::Vec<_> =
            bytes.chunks(12).filter_map(Index::try_from_bytes).collect();
        if indices.len() != len {
            None
        } else {
            Some(Self(indices))
        }
    }
}

/// The response from the enclave for performing
/// FMD for a particular uses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedResponse {
    /// Hash of user's encryption key used to identify
    /// when database entries belong to them
    pub owner: alloc::string::String,
    /// Nonce needed to decrypt the indices
    pub nonce: [u8; 12],
    /// encrypted indices
    pub indices: alloc::vec::Vec<u8>,
}
