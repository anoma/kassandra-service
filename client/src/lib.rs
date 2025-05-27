use chacha20poly1305::Key;
use fmd::FmdSecretKey;
use hkdf::Hkdf;
use shared::db::EncKey;
use shared::{ClientMsg, ServerMsg};
use tracing_subscriber::fmt::SubscriberBuilder;

use crate::com::OutgoingTcp;
use crate::config::Config;
use crate::error::Error;

mod ratls;

pub mod com;
pub mod config;
pub mod error;
pub mod query;
#[cfg(feature = "tdx")]
pub mod tdx;
#[cfg(feature = "transparent")]
pub mod transparent;

pub const GAMMA: usize = 20;

pub fn init_logging() {
    SubscriberBuilder::default().with_ansi(true).init();
}

pub fn get_host_uuid(url: &str) -> error::Result<String> {
    let mut stream = OutgoingTcp::new(url)?;
    stream.write(ClientMsg::RequestUUID);
    match stream.read() {
        Ok(ServerMsg::UUID(uuid)) => Ok(uuid),
        Ok(ServerMsg::Error(err)) => Err(Error::ServerError(err)),
        _ => Err(Error::ServerError(format!(
            "Requesting UUID from host at {url} failed. Could not parse response."
        ))),
    }
}

pub fn encryption_key(fmd_key: &FmdSecretKey, salt: &str) -> EncKey {
    let hk = Hkdf::<sha2::Sha256>::new(
        Some(salt.as_bytes()),
        serde_json::to_string(fmd_key).unwrap().as_bytes(),
    );
    let mut encryption_key = [0u8; 32];
    hk.expand("Database encryption key".as_bytes(), &mut encryption_key)
        .expect("This operation should not fail.");
    let enc_key: Key = encryption_key.into();
    enc_key.into()
}

#[cfg(feature = "tdx")]
pub fn register_fmd_key(
    config: &Config,
    key_hash: String,
    fmd_key: &FmdSecretKey,
    birthday: Option<u64>,
) -> error::Result<()> {
    ratls::register_fmd_key::<tdx::TdxClient>(config, key_hash, fmd_key, birthday)
}
#[cfg(feature = "transparent")]
pub fn register_fmd_key(
    config: &Config,
    key_hash: String,
    fmd_key: &FmdSecretKey,
    birthday: Option<u64>,
) -> error::Result<()> {
    ratls::register_fmd_key::<transparent::TClient>(config, key_hash, fmd_key, birthday)
}
