//! All client methods calling the enclave directly. This first
//! requires establishing trust of the enclave (remote attestation),
//! and a Diffie-Hellman key exchange for secure communication.
//!
//! Currently, the only direct communication between enclaves and
//! clients is registering clients' FMD detection keys with the
//! enclave.
use crate::com::OutgoingTcp;
use fmd::fmd2_compact::CompactSecretKey;
use rand_core::{OsRng, RngCore};
use shared::ratls::Connection;
use shared::tee::EnclaveClient;
use shared::{AckType, ClientMsg, ServerMsg};

/// Initialize a new TLS connection with the enclave.
/// The handshake phase establishes a shared key via DHKE.
///
/// The client also validates the Remote Attestation report
/// provided by the enclave.
pub(crate) fn register_fmd_key<C: EnclaveClient>(url: &str, fmd_key: CompactSecretKey) {
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
        Ok(ServerMsg::Error(err)) => panic!("{err}"),
        _ => panic!(
            "Establishing RA-TLS connection failed: Could not parse service response as RA report."
        ),
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
    let cipher = conn
        .encrypt_msg(&serde_cbor::to_vec(&fmd_key).unwrap(), &mut rng)
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
    panic!("{}", msg);
}
