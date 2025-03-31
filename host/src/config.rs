use std::fmt::Formatter;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use serde::de::{Error, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::Cli;

const CLIENT_TIMEOUT: u64 = 1;
const CONFIG_FILE: &str = "config.toml";
const ENCLAVE_ADDRESS: &str = "0.0.0.0:12345";
const KASSANDRA_DIR: &str = ".kassandra";
const LISTENING_ADDRESS: &str = "0.0.0.0:666";
const MAX_WAL_SIZE: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub enclave_url: String,
    pub listen_url: String,
    pub listen_timeout: Duration,
    pub db: DbConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbConfig {
    #[serde(serialize_with = "serialize_url")]
    #[serde(deserialize_with = "deserialize_url")]
    pub indexer_url: reqwest::Url,
    pub max_wal_size: usize,
}

impl Config {
    /// Load a config from file
    pub fn load() -> std::io::Result<Self> {
        let config_file = kassandra_dir().join(CONFIG_FILE);
        toml::from_str(&std::fs::read_to_string(config_file)?).map_err(|e| {
            std::io::Error::new(
                ErrorKind::InvalidData,
                format!("Could not parse database config file: {e}"),
            )
        })
    }

    /// Parse a config from CLI arguments
    pub fn init(cli: Cli) -> Option<Self> {
        cli.indexer_url.as_ref().map(|ix_url| Self {
            enclave_url: cli.enclave.unwrap_or_else(|| ENCLAVE_ADDRESS.to_string()),
            listen_url: cli.listen.unwrap_or_else(|| LISTENING_ADDRESS.to_string()),
            listen_timeout: cli
                .listen_timeout
                .map(Duration::from_millis)
                .unwrap_or_else(|| Duration::from_secs(CLIENT_TIMEOUT)),
            db: DbConfig {
                indexer_url: reqwest::Url::from_str(ix_url).unwrap(),
                max_wal_size: cli.max_wal_size.unwrap_or(MAX_WAL_SIZE),
            },
        })
    }

    /// First try to load the config file. If that succeeds, overwrite the config
    /// with the CLI args present and persist it. If loading fails, create a config
    /// from the CLI args and persist it.
    ///
    /// Returns the final config.
    pub fn load_or_init(cli: Cli) -> Self {
        match Self::load() {
            Ok(mut conf) => {
                if let Some(e) = cli.enclave {
                    conf.enclave_url = e;
                }
                if let Some(l) = cli.listen {
                    conf.listen_url = l;
                }
                if let Some(t) = cli.listen_timeout {
                    conf.listen_timeout = Duration::from_millis(t);
                }
                if let Some(idx) = cli.indexer_url {
                    conf.db.indexer_url = reqwest::Url::from_str(&idx).unwrap();
                }
                if let Some(wal) = cli.max_wal_size {
                    conf.db.max_wal_size = wal;
                }
                conf.save().unwrap();
                conf
            }
            Err(e) => {
                tracing::warn!("Could not load config file: {e}");
                if let Some(conf) = Self::init(cli) {
                    tracing::info!("New config created.");
                    conf.save().unwrap();
                    conf
                } else {
                    panic!("Could load config from parsed arguments.");
                }
            }
        }
    }

    /// Save the config file
    pub fn save(&self) -> std::io::Result<()> {
        let k_dir = kassandra_dir();
        if !std::fs::exists(&k_dir).unwrap() {
            std::fs::create_dir(&k_dir).unwrap();
        }
        let config_file = k_dir.join(CONFIG_FILE);
        std::fs::write(config_file, toml::to_string(self).unwrap())
    }
}

pub fn kassandra_dir() -> PathBuf {
    home::home_dir().unwrap().join(KASSANDRA_DIR)
}

fn serialize_url<S>(url: &reqwest::Url, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let url = url.to_string();
    s.serialize_str(&url)
}

fn deserialize_url<'de, D>(des: D) -> Result<reqwest::Url, D::Error>
where
    D: Deserializer<'de>,
{
    struct UrlVisitor;
    impl Visitor<'_> for UrlVisitor {
        type Value = String;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("A valid URL")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(v.to_string())
        }
    }
    let string = des.deserialize_str(UrlVisitor)?;
    reqwest::Url::from_str(&string)
        .map_err(|e| D::Error::custom(format!("Could not parse url: {e}")))
}
