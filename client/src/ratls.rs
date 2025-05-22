//! All client methods calling the enclave directly. This first
//! requires establishing trust of the enclave (remote attestation),
//! and a Diffie-Hellman key exchange for secure communication.
//!
//! Currently, the only direct communication between enclaves and
//! clients is registering clients' FMD detection keys with the
//! enclave.

use std::path::Path;

use fmd::fmd2_compact::{CompactSecretKey, MultiFmd2CompactScheme};
use fmd::{DetectionKey, KeyExpansion, MultiFmdScheme};
use rand_core::{OsRng, RngCore};
use shared::db::EncKey;
use shared::ratls::{Connection, FmdKeyRegistration};
use shared::tee::EnclaveClient;
use shared::{AckType, ClientMsg, ServerMsg};

use crate::GAMMA;
use crate::com::OutgoingTcp;
use crate::config::{Config, Service, hash_key};

/// Registers an fmd key to each service instance
/// specified in the config file.
pub fn register_fmd_key<C: EnclaveClient>(
    base_dir: impl AsRef<Path>,
    csk_key: CompactSecretKey,
    birthday: Option<u64>,
    gamma: usize,
) {
    let key_hash = hash_key(&csk_key, gamma);
    let services = match Config::get_services(base_dir, &key_hash) {
        Ok(services) => services,
        Err(e) => {
            tracing::error!("Error getting the associated services from the config file: {e}");
            panic!("Error getting the associated services from the config file: {e}");
        }
    };
    // Get the fmd key and encryption key
    let cpk_key = csk_key.master_public_key();
    let mut scheme = MultiFmd2CompactScheme::new(GAMMA, 1);
    let (fmd_key, _) = scheme.expand_keypair(&csk_key, &cpk_key);
    let detection_keys = scheme
        .multi_extract(&fmd_key, services.len(), 1, 1, services.len())
        .unwrap();
    for Service {
        url,
        index,
        enc_key,
    } in services
    {
        register_fmd_key_to_service::<C>(
            &url,
            enc_key,
            detection_keys[index - 1].clone(),
            birthday,
        );
    }
}

/// Initialize a new TLS connection with the enclave.
/// The handshake phase establishes a shared key via DHKE.
///
/// The client also validates the Remote Attestation report
/// provided by the enclave.
pub fn register_fmd_key_to_service<C: EnclaveClient>(
    url: &str,
    encryption_key: EncKey,
    detection_key: DetectionKey,
    birthday: Option<u64>,
) {
    let mut rng = OsRng;
    let mut stream = OutgoingTcp::new(url);
    let conn = Connection::new(&mut rng);

    // create a nonce for replay protection
    let nonce = rng.next_u64();

    // initiate handshake with enclave
    stream.write(conn.client_send(nonce).unwrap());

    // validate remote attestation certificates
    let report = match stream.read() {
        Ok(ServerMsg::RATLS { report }) => report,
        Ok(ServerMsg::Error(err)) => {
            tracing::error!("Error reported by server: {err}");
            panic!("{err}")
        }
        _ => {
            tracing::error!(
                "Establishing RA-TLS connection failed: Could not parse service response as RA report."
            );
            panic!(
                "Establishing RA-TLS connection failed: Could not parse service response as RA report."
            )
        }
    };

    let report_data = match C::verify_quote(&report, nonce) {
        Ok(d) => d,
        Err(e) => abort_tls(
            stream,
            &format!("Establishing RA-TLS connection failed: {e}"),
        ),
    };

    // Extract the signed ephemeral public key and session id
    let pk_bytes = <[u8; 32]>::try_from(&report_data[0..32]).unwrap();
    let pk = x25519_dalek::PublicKey::from(pk_bytes);

    // finish the handshake and initialize the connection
    let conn = match conn.initialize(pk) {
        Ok(conn) => conn,
        Err(e) => {
            abort_tls(
                stream,
                &format!("Could not initialize TLS connection to service: {e}"),
            );
        }
    };

    // encrypt the fmd key and send it to the enclave
    let key_reg = FmdKeyRegistration {
        fmd_key: detection_key,
        enc_key: encryption_key,
        birthday,
    };
    let cipher = conn
        .encrypt_msg(&serde_cbor::to_vec(&key_reg).unwrap(), &mut rng)
        .unwrap();
    stream.write(ClientMsg::RATLSAck(AckType::Success(cipher)));

    // wait for response from server if entire procedure was successful
    match stream.read() {
        Ok(ServerMsg::KeyRegSuccess) => tracing::info!("Key registered successfully"),
        Ok(ServerMsg::Error(msg)) => tracing::error!("Key registration failed: {msg}"),
        _ => tracing::error!("Received unexpected message from service"),
    }
}

fn abort_tls(mut stream: OutgoingTcp, msg: &str) -> ! {
    stream.write(ClientMsg::RATLSAck(AckType::Fail));
    tracing::error!(msg);
    panic!("{}", msg);
}
