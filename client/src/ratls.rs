//! All client methods calling the enclave directly. This first
//! requires establishing trust of the enclave (remote attestation),
//! and a Diffie-Hellman key exchange for secure communication.
//!
//! Currently, the only direct communication between enclaves and
//! clients is registering clients' FMD detection keys with the
//! enclave.

use std::io::{BufReader, Read, Write};
use std::net::TcpStream;

use fmd::fmd2_compact::CompactSecretKey;
use rand_core::{OsRng, RngCore};
use shared::ratls::Connection;
use shared::tee::EnclaveClient;
use shared::{AckType, ClientMsg, ServerMsg};

use crate::HOST_ADDRESS;

/// Read message from server
fn server_read(stream: &mut TcpStream) -> Option<ServerMsg> {
    let mut buf_reader = BufReader::new(stream);
    let mut req_bytes = vec![];
    buf_reader.read_to_end(&mut req_bytes).ok()?;
    serde_cbor::from_slice(&req_bytes).ok()
}

/// Write message to server
fn server_write(stream: &mut TcpStream, msg: &ClientMsg) {
    let msg = serde_cbor::to_vec(msg).unwrap();
    stream.write_all(&msg).unwrap();
}

/// Initialize a new TLS connection with the enclave.
/// The handshake phase establishes a shared key via DHKE.
///
/// The client also validates the Remote Attestation report
/// provided by the enclave.
pub(crate) fn register_fmd_key<C: EnclaveClient>(fmd_key: CompactSecretKey) {
    // create client side connection
    let mut stream = TcpStream::connect(HOST_ADDRESS).unwrap();
    let mut rng = OsRng;
    let conn = Connection::new(&mut rng);

    // create a nonce for replay protection
    let nonce = rng.next_u64();

    // initiate handshake with enclave
    server_write(&mut stream, &conn.client_send(nonce).unwrap());

    // validate remote attestation certificates
    let report = match server_read(&mut stream) {
        Some(ServerMsg::RATLS { report }) => report,
        Some(ServerMsg::Error(err)) => panic!("{err}"),
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
    server_write(&mut stream, &ClientMsg::RATLSAck(AckType::Success(cipher)));

    // wait for response from server if entire procedure was successful
    match server_read(&mut stream) {
        Some(ServerMsg::KeyRegSuccess) => tracing::info!("Key registered succesfully"),
        Some(ServerMsg::Error(msg)) => tracing::error!("Key registration failed: {msg}"),
        _ => tracing::error!("Received unexpected message from service"),
    }
}

fn abort_tls(mut stream: TcpStream, msg: &str) -> ! {
    server_write(&mut stream, &ClientMsg::RATLSAck(AckType::Fail));
    panic!("{}", msg);
}
