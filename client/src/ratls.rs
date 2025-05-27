//! All client methods calling the enclave directly. This first
//! requires establishing trust of the enclave (remote attestation),
//! and a Diffie-Hellman key exchange for secure communication.
//!
//! Currently, the only direct communication between enclaves and
//! clients is registering clients' FMD detection keys with the
//! enclave.
use fmd::fmd2_compact::MultiFmd2CompactScheme;
use fmd::{DetectionKey, FmdSecretKey, MultiFmdScheme};
use rand_core::{OsRng, RngCore};
use shared::db::EncKey;
use shared::ratls::{Connection, FmdKeyRegistration};
use shared::tee::EnclaveClient;
use shared::{AckType, ClientMsg, ServerMsg};

use crate::GAMMA;
use crate::com::OutgoingTcp;
use crate::config::{Config, Service};
use crate::error::{self, Error};

/// Registers an fmd key to each service instance
/// specified in the config file.
pub(crate) fn register_fmd_key<C: EnclaveClient>(
    config: &Config,
    key_hash: String,
    fmd_key: &FmdSecretKey,
    birthday: Option<u64>,
) -> error::Result<()> {
    let services = config.get_services(&key_hash);
    // Get the encryption key
    let scheme = MultiFmd2CompactScheme::new(GAMMA, 1);
    let detection_keys = scheme
        .multi_extract(fmd_key, services.len(), 1, 1, services.len())
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
        )?;
    }
    Ok(())
}

/// Initialize a new TLS connection with the enclave.
/// The handshake phase establishes a shared key via DHKE.
///
/// The client also validates the Remote Attestation report
/// provided by the enclave.
fn register_fmd_key_to_service<C: EnclaveClient>(
    url: &str,
    encryption_key: EncKey,
    detection_key: DetectionKey,
    birthday: Option<u64>,
) -> error::Result<()> {
    let mut rng = OsRng;
    let mut stream = OutgoingTcp::new(url)?;
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
            return Err(Error::ServerError(err));
        }
        _ => {
            tracing::error!(
                "Establishing RA-TLS connection failed: Could not parse service response as RA report."
            );
            return Err(Error::ServerError(
                "Establishing RA-TLS connection failed: Could not parse service response as RA report.".to_string()
            ));
        }
    };

    let report_data =
        C::verify_quote(&report, nonce).map_err(|e| abort_tls(&mut stream, e.to_string()))?;

    // Extract the signed ephemeral public key and session id
    let pk_bytes = <[u8; 32]>::try_from(&report_data[0..32]).unwrap();
    let pk = x25519_dalek::PublicKey::from(pk_bytes);

    // finish the handshake and initialize the connection
    let conn = conn
        .initialize(pk)
        .map_err(|e| abort_tls(&mut stream, e.to_string()))?;

    // encrypt the fmd key and send it to the enclave
    let key_reg = FmdKeyRegistration {
        fmd_key: detection_key,
        enc_key: encryption_key,
        birthday,
    };
    let cipher = conn
        .encrypt_msg(&serde_cbor::to_vec(&key_reg).unwrap(), &mut rng)
        .expect("RA-TLS should already be initialized");
    stream.write(ClientMsg::RATLSAck(AckType::Success(cipher)));

    // wait for response from server if entire procedure was successful
    match stream.read() {
        Ok(ServerMsg::KeyRegSuccess) => {
            tracing::info!("Key registered successfully");
            Ok(())
        }
        Ok(ServerMsg::Error(msg)) => {
            tracing::error!("Key registration failed: {msg}");
            Err(Error::ServerError(msg))
        }
        _ => {
            tracing::error!("Received unexpected message from service");
            Err(Error::ServerError(
                "Received unexpected message from service".to_string(),
            ))
        }
    }
}

fn abort_tls(stream: &mut OutgoingTcp, msg: impl AsRef<str>) -> error::Error {
    let msg = msg.as_ref();
    stream.write(ClientMsg::RATLSAck(AckType::Fail));
    tracing::error!(msg);
    Error::RATLS(msg.to_string())
}
