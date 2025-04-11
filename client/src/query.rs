//! Functions for querying the Kassandra service DB for data
//! relevant to a particular registered key.

use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
use shared::db::{EncKey, IndexList};
use shared::{ClientMsg, ServerMsg};

use crate::com::OutgoingTcp;

pub fn query_fmd_key(url: &str, enc_key: &EncKey) {
    let mut stream = OutgoingTcp::new(url);
    stream.write(ClientMsg::RequestIndices {
        key_hash: enc_key.hash(),
    });

    let encrypted = match stream.read() {
        Ok(ServerMsg::IndicesResponse(resp)) => resp,
        Ok(ServerMsg::Error(err)) => {
            tracing::error!("Error reported by server: {err}");
            panic!("{err}")
        }
        _ => {
            tracing::error!("Unable to parse response from the service.");
            panic!("Unable to parse response from the service.");
        }
    };

    if encrypted.owner != enc_key.hash() {
        tracing::error!("Received response for data owned by a different key");
        panic!("Received response for data owned by a different key");
    }

    let cipher = ChaCha20Poly1305::new(enc_key.into());
    let nonce = Nonce::from(encrypted.nonce);
    let Ok(index_bytes) = cipher.decrypt(&nonce, encrypted.indices.as_ref()) else {
        tracing::error!("Failed to decrypt the response from the service");
        panic!("Failed to decrypt the response from the service");
    };

    match IndexList::try_from_bytes(&index_bytes) {
        None => {
            tracing::error!("Could not deserialize decrypted response as MASP indices");
            panic!("Could not deserialize decrypted response as MASP indices");
        }
        Some(list) => {
            let indices = serde_json::to_string_pretty(&list).unwrap();
            tracing::info!("{}", indices);
        }
    }
}
