//! Functions for querying the Kassandra service DB for data
//! relevant to a particular registered key.

use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
use shared::db::{EncKey, IndexList};
use shared::{ClientMsg, ServerMsg};

use crate::com::OutgoingTcp;
use crate::config::{Config, Service};
use crate::error::{self, Error};
use crate::get_host_uuid;

/// Query all services where a key is registered and combine the results.
pub fn query_fmd_key(config: &Config, key_hash: &String) -> error::Result<Vec<IndexList>> {
    let services = config.get_services(key_hash);
    let mut indices = vec![];
    for Service { url, enc_key, .. } in services {
        let uuid = get_host_uuid(&url)?;
        let list = query_service(&url, &enc_key, &uuid)?;
        indices.push(list);
    }
    Ok(indices)
}

/// Query a particular service for data on a particular registered key.
pub fn query_service(url: &str, enc_key: &EncKey, uuid: &str) -> error::Result<IndexList> {
    let mut stream = OutgoingTcp::new(url)?;
    stream.write(ClientMsg::RequestIndices {
        key_hash: enc_key.hash(),
    });

    let encrypted = match stream.read() {
        Ok(ServerMsg::IndicesResponse(resp)) => resp,
        Ok(ServerMsg::Error(err)) => {
            tracing::error!("Service < {uuid} >: Error reported by server: {err}");
            return Err(Error::ServerError(format!(
                "Service < {uuid} >: Error reported by server: {err}"
            )));
        }
        _ => {
            tracing::error!("Service < {uuid} >: Unable to parse response from the service.");
            return Err(Error::ServerError(format!(
                "Service < {uuid} >: Unable to parse response from the service."
            )));
        }
    };

    if encrypted.owner != enc_key.hash() {
        tracing::error!("Service < {uuid} >: Received response for data owned by a different key");
        return Err(Error::ServerError(format!(
            "Service < {uuid} >: Received response for data owned by a different key"
        )));
    }

    let cipher = ChaCha20Poly1305::new(enc_key.into());
    let nonce = Nonce::from(encrypted.nonce);
    let Ok(index_bytes) = cipher.decrypt(&nonce, encrypted.indices.as_ref()) else {
        tracing::error!("Service < {uuid} >: Failed to decrypt the response from the service");
        return Err(Error::ServerError(format!(
            "Service < {uuid} >: Failed to decrypt the response from the service"
        )));
    };

    match IndexList::try_from_bytes(&index_bytes) {
        None => {
            tracing::error!(
                "Service < {uuid} >: Could not deserialize decrypted response as MASP indices"
            );
            Err(Error::ServerError(format!(
                "Service < {uuid} >: Could not deserialize decrypted response as MASP indices"
            )))
        }
        Some(list) => {
            tracing::info!("Service < {uuid} >: Synced to height: {}", encrypted.height);
            Ok(list)
        }
    }
}
