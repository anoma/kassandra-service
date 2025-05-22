//! Functions for querying the Kassandra service DB for data
//! relevant to a particular registered key.

use std::path::Path;

use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
use shared::db::{EncKey, IndexList};
use shared::{ClientMsg, ServerMsg};

use crate::com::OutgoingTcp;
use crate::config::{Config, Service};
use crate::get_host_uuid;

/// Query all services where a key is registered and combine the results.
pub fn query_fmd_key(base_dir: impl AsRef<Path>, key_hash: &String) -> IndexList {
    let config = match Config::load_or_new(base_dir) {
        Ok(config) => config,
        Err(e) => {
            tracing::error!("Error getting the associated services from the config file: {e}");
            panic!("Error getting the associated services from the config file: {e}");
        }
    };
    let services = config.get_services(key_hash);
    let mut indices = IndexList::default();
    for Service { url, enc_key, .. } in services {
        let uuid = get_host_uuid(&url);
        if let Some(list) = query_service(&url, &enc_key, &uuid) {
            indices.combine(list);
        }
    }
    indices
}

/// Query a particular service for data on a particular registered key.
pub fn query_service(url: &str, enc_key: &EncKey, uuid: &str) -> Option<IndexList> {
    let mut stream = OutgoingTcp::new(url);
    stream.write(ClientMsg::RequestIndices {
        key_hash: enc_key.hash(),
    });

    let encrypted = match stream.read() {
        Ok(ServerMsg::IndicesResponse(resp)) => resp,
        Ok(ServerMsg::Error(err)) => {
            tracing::error!("Service < {uuid} >: Error reported by server: {err}");
            return None;
        }
        _ => {
            tracing::error!("Service < {uuid} >: Unable to parse response from the service.");
            return None;
        }
    };

    if encrypted.owner != enc_key.hash() {
        tracing::error!("Service < {uuid} >: Received response for data owned by a different key");
        return None;
    }

    let cipher = ChaCha20Poly1305::new(enc_key.into());
    let nonce = Nonce::from(encrypted.nonce);
    let Ok(index_bytes) = cipher.decrypt(&nonce, encrypted.indices.as_ref()) else {
        tracing::error!("Service < {uuid} >: Failed to decrypt the response from the service");
        return None;
    };

    match IndexList::try_from_bytes(&index_bytes) {
        None => {
            tracing::error!(
                "Service < {uuid} >: Could not deserialize decrypted response as MASP indices"
            );
            None
        }
        Some(list) => {
            tracing::info!("Service < {uuid} >: Synced to height: {}", encrypted.height);
            Some(list)
        }
    }
}
