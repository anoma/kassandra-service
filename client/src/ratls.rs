use std::io::{BufReader, Read, Write};
use std::net::TcpStream;

use fmd::fmd2_compact::CompactSecretKey;
use rand_core::{OsRng, RngCore};
use shared::ratls::Connection;
use shared::{AckType, ClientMsg, ServerMsg};
use tdx_quote::Quote;

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
/// The handshake phase establishes a shared key via DHKE
/// as well as session id.
///
/// The client also validates the Remote Attestation report
/// provided by the enclave.
pub(crate) fn register_fmd_key(fmd_key: CompactSecretKey) {
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
    let quote = match Quote::from_bytes(&report) {
        Ok(q) => q,
        Err(e) => {
            abort_tls(stream, &format!("Could not parse RA report: {e}"));
        }
    };
    if !verify_quote(&quote, nonce) {
        abort_tls(
            stream,
            "Establishing RA-TLS connection failed: Invalid quote.",
        );
    }

    // Extract the signed ephemeral public key and session id
    let report_data = quote.report_input_data();
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

/// TODO: Replace with real values
const MRTD: &str = "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
const RTMR0: &str = "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
const RTMR1: &str = "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";

fn verify_quote(quote: &Quote, nonce: u64) -> bool {
    let mrtd = hex::encode(quote.mrtd());
    let rtmr0 = hex::encode(quote.rtmr0());
    let rtmr1 = hex::encode(quote.rtmr1());
    if mrtd != MRTD {
        tracing::error!(
            "Unexpected MRTD measurement. \n Wanted: {MRTD}\n Received: {}",
            mrtd
        );
        return false;
    }
    if rtmr0 != RTMR0 {
        tracing::error!(
            "Unexpected RTMR0 measurement. \n Wanted: {RTMR0}\n Received: {}",
            rtmr0
        );
        return false;
    }
    if rtmr1 != RTMR1 {
        tracing::error!(
            "Unexpected RTMR1 measurement. \n Wanted: {RTMR1}\n Received: {}",
            rtmr1
        );
        return false;
    }
    #[cfg(feature = "default")]
    if let Err(e) = quote.verify() {
        tracing::error!("RA quote verification failed: {}", e.to_string());
        return false;
    }

    let ra_nonce =
        u64::from_le_bytes(<[u8; 8]>::try_from(&quote.report_input_data()[32..40]).unwrap());
    if ra_nonce != nonce {
        tracing::error!("RA quote contained a nonce different that the provided one.");
        false
    } else {
        true
    }
}
