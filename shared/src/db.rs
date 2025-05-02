//! Shared types to be stored in the host databases

use borsh::{BorshDeserialize, BorshSerialize};
use chacha20poly1305::Key;
use core::fmt::Formatter;
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
#[derive(
    Debug,
    Copy,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
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

#[derive(
    Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
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

    /// Given two index sets, produce a new index set
    /// modifying self in-place.
    ///
    /// We assume `self` is synced ahead of `other`. The intersection of
    /// the indices up to the common block height is kept along with all
    /// indices of `self` with block height greater than `other`'s maximum
    /// block height.
    pub fn combine(&mut self, mut other: Self) {
        if self.0.is_empty() {
            *self = other;
            return;
        }
        if other.0.is_empty() {
            return;
        }
        self.0.sort();
        other.0.sort();
        let a_height = self.0.last().map(|ix| ix.height).unwrap_or_default();
        let b_height = other.0.last().map(|ix| ix.height).unwrap_or_default();
        // from here on out, we assume that `self` is synced further than `other`
        let height = if a_height < b_height {
            core::mem::swap(self, &mut other);
            a_height
        } else {
            b_height
        };
        self.0.retain(|ix| {
            if ix.height > height {
                true
            } else {
                other.contains(ix)
            }
        });
    }

    /// Create a union of two index sets
    pub fn union(&mut self, other: &Self) {
        self.0.extend_from_slice(&other.0[..]);
        self.0.sort();
        self.0.dedup();
    }

    /// Check if an index is contained in `self`
    /// Assumes `self` is sorted.
    pub fn contains(&self, index: &Index) -> bool {
        self.0.binary_search(index).is_ok()
    }

    /// Check if the index set contains a given height.
    /// Assumes `self` is sorted.
    pub fn contains_height(&self, height: u64) -> bool {
        self.0.binary_search_by_key(&height, |ix| ix.height).is_ok()
    }

    /// Return and iterator of references to the
    /// contained indices
    pub fn iter(&self) -> alloc::slice::Iter<Index> {
        self.0.iter()
    }

    /// Function for filtering out elements in-place
    pub fn retain<P>(&mut self, pred: P)
    where
        P: FnMut(&Index) -> bool,
    {
        self.0.retain(pred);
    }
}

impl IntoIterator for IndexList {
    type Item = Index;
    type IntoIter = alloc::vec::IntoIter<Index>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl FromIterator<Index> for IndexList {
    fn from_iter<T: IntoIterator<Item = Index>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
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
    /// The last height FMD was performed at
    pub height: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn test_combine_indices() {
        let a = IndexList(Vec::from([
            Index { height: 0, tx: 0 },
            Index { height: 0, tx: 1 },
            Index { height: 1, tx: 0 },
            Index { height: 3, tx: 0 },
        ]));
        let mut b = IndexList(Vec::from([
            Index { height: 0, tx: 1 },
            Index { height: 1, tx: 4 },
        ]));
        let expected = IndexList(Vec::from([
            Index { height: 0, tx: 1 },
            Index { height: 3, tx: 0 },
        ]));

        let mut first = a.clone();
        first.combine(b.clone());
        assert_eq!(first, expected);
        b.combine(a.clone());
        assert_eq!(b, expected);

        let mut new = IndexList::default();
        new.combine(a.clone());
        assert_eq!(new, a);
        let mut third = a.clone();
        third.combine(IndexList::default());
        assert_eq!(third, a);
    }
}
