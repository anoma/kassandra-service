//! Module for handling the backing config file of the client. The
//! purpose of the config is to persist information about which
//! keys are registered to which services.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::io::ErrorKind;
use std::path::Path;

use fmd::KeyExpansion;
use fmd::fmd2_compact::{CompactSecretKey, MultiFmd2CompactScheme};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use shared::db::EncKey;

/// The name of the config file
pub const CLIENT_FILE_NAME: &str = "kassandra-client.toml";

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Config {
    /// A map from the hash of FMD secret key to the services
    /// it is registered with
    pub services: BTreeMap<String, Vec<Service>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// A service instance
pub struct Service {
    /// Address of the service
    pub url: String,
    /// An index indication which share of fmd keys it received
    pub index: usize,
    /// The key used to decrypt responses from the service
    pub enc_key: EncKey,
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

    /// Load a config if it exists, otherwise create a new one
    pub fn load_or_new(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref().join(CLIENT_FILE_NAME);
        if path.exists() {
            Self::load(path)
        } else {
            Ok(Self::default())
        }
    }

    /// Save the config at the specified path
    pub fn save(&mut self, path: impl AsRef<Path>) -> std::io::Result<()> {
        for (_, services) in self.services.iter_mut() {
            services.sort_by_key(|s| s.index);
            services.dedup_by_key(|s| s.index);
        }
        let dest = path.as_ref().join(CLIENT_FILE_NAME);
        std::fs::write(dest, toml::to_string(&self).unwrap())
    }

    /// Add a new service which a specified key will be registered to.
    pub fn add_service(
        path: impl AsRef<Path>,
        key: String,
        url: &str,
        enc_key: EncKey,
    ) -> std::io::Result<()> {
        let mut config = Self::load_or_new(path.as_ref())?;
        match config.services.entry(key) {
            Entry::Vacant(e) => {
                e.insert(vec![Service {
                    url: url.to_string(),
                    index: 1,
                    enc_key,
                }]);
            }
            Entry::Occupied(mut o) => {
                let ix = o.get().iter().map(|s| s.index).max().unwrap_or_default();
                o.get_mut().push(Service {
                    url: url.to_string(),
                    index: ix + 1,
                    enc_key,
                });
            }
        }
        config.save(path)
    }

    /// Get the services that the specified key is configured to be registered to
    pub fn get_services(path: impl AsRef<Path>, key: &String) -> std::io::Result<Vec<Service>> {
        let config = Self::load_or_new(path)?;
        Ok(config.services.get(key).cloned().unwrap_or_default())
    }
}

/// Get a hash of an FMD key from a Compact secret key and choice of gamma.
pub fn hash_key(csk_key: &CompactSecretKey, gamma: usize) -> String {
    let mut hasher = sha2::Sha256::new();

    let cpk_key = csk_key.master_public_key();
    let scheme = MultiFmd2CompactScheme::new(gamma, 1);
    let (fmd_key, _) = scheme.expand_keypair(csk_key, &cpk_key);

    hasher.update(serde_json::to_string(&fmd_key).unwrap().as_bytes());
    let bytes: [u8; 32] = hasher.finalize().into();
    hex::encode(bytes)
}
