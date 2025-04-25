//! Module for handling the backing config file of the client. The
//! purpose of the config is to persist information about which
//! keys are registered to which services.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::io::ErrorKind;
use std::path::Path;

use fmd::fmd2_compact::CompactSecretKey;
use serde::{Deserialize, Serialize};
use sha2::Digest;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Config {
    /// A map from the hash of secret key to the services
    /// it is registered with
    pub services: BTreeMap<[u8; 32], Vec<Service>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// A service instance
pub struct Service {
    /// Address of the service
    pub url: String,
    /// An index indication which share of fmd keys it received
    pub index: usize,
}

impl Config {
    /// Load the config from the specified path
    pub fn load(path: impl AsRef<Path>) -> std::io::Result<Self> {
        toml::from_str(&std::fs::read_to_string(path)?).map_err(|e| {
            std::io::Error::new(
                ErrorKind::InvalidData,
                format!("Could not parse client config file: {e}"),
            )
        })
    }

    /// Save the config at the specified path
    pub fn save(mut self, path: impl AsRef<Path>) -> std::io::Result<()> {
        for (_, services) in self.services.iter_mut() {
            services.sort_by_key(|s| s.index);
            services.dedup_by_key(|s| s.index);
        }
        std::fs::write(path, toml::to_string(&self).unwrap())
    }

    /// Add a new service which a specified key will be registered to.
    pub fn add_service(
        path: impl AsRef<Path>,
        key: CompactSecretKey,
        url: &str,
    ) -> std::io::Result<()> {
        let path = path.as_ref();
        let mut config = if path.exists() {
            Self::load(path)?
        } else {
            Self::default()
        };
        let key = hash_key(&key);
        match config.services.entry(key) {
            Entry::Vacant(e) => {
                e.insert(vec![Service {
                    url: url.to_string(),
                    index: 1,
                }]);
            }
            Entry::Occupied(mut o) => {
                let ix = o.get().iter().map(|s| s.index).max().unwrap_or_default();
                o.get_mut().push(Service {
                    url: url.to_string(),
                    index: ix + 1,
                });
            }
        }
        config.save(path)
    }

    /// Get the services that the specified key is configured to be registered to
    pub fn get_services(
        path: impl AsRef<Path>,
        key: &CompactSecretKey,
    ) -> std::io::Result<Vec<Service>> {
        let path = path.as_ref();
        let config = if path.exists() {
            Self::load(path)?
        } else {
            Self::default()
        };
        let key = hash_key(key);
        Ok(config.services.get(&key).cloned().unwrap_or_default())
    }
}

fn hash_key(key: &CompactSecretKey) -> [u8; 32] {
    let mut hasher = sha2::Sha256::new();
    hasher.update(serde_json::to_string(key).unwrap().as_bytes());
    hasher.finalize().into()
}
