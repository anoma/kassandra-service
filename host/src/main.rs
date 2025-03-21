mod com;

use std::io::{BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use shared::{AckType, ClientMsg, MsgFromHost, MsgToHost, ServerMsg};
use tracing::{error, info};

use crate::com::Tcp;

const ENCLAVE_ADDRESS: &str = "0.0.0.0:12345";
const LISTENING_ADDRESS: &str = "0.0.0.0:666";

fn client_read(client_conn: &mut TcpStream) -> Option<ClientMsg> {
    let mut buf_reader = BufReader::new(client_conn);
    let mut req_bytes = vec![];
    buf_reader.read_to_end(&mut req_bytes).ok()?;
    if let Ok(msg) = serde_cbor::from_slice::<ClientMsg>(&req_bytes) {
        Some(msg)
    } else {
        error!("Error deserializing client request: {:?}", req_bytes);
        None
    }
}

/// Handle a client request and issue a response.
fn handle_connection(mut client_conn: TcpStream, enclave_conn: &mut Tcp) -> std::io::Result<()> {
    let Some(req) = client_read(&mut client_conn) else {
        return Ok(());
    };
    if let Ok(msg) = MsgFromHost::try_from(&req) {
        if let MsgFromHost::RegisterKey { .. } = msg {
            handle_key_registration(client_conn, enclave_conn, msg);
        } else {
            enclave_conn.write(msg);
            match enclave_conn.read() {
                Ok(msg) => {
                    if let MsgToHost::Error(err) = &msg {
                        error!("Received error from enclave: {err}");
                    } else {
                        info!("Received message: {:?}", msg);
                        if let Ok(resp) = ServerMsg::try_from(msg) {
                            let resp = serde_cbor::to_vec(&resp).unwrap();
                            client_conn.write_all(&resp)?;
                        }
                    }
                }
                Err(e) => error!("Error receiving message from enclave: {e}"),
            }
        }
    }

    Ok(())
}

/// A simplified TLS designed to send an encrypted secret FMD detection key from
/// a client to the enclave. It is a multi-round protocol as follows:
///
/// * Client initiates with public DH key and challenge nonce
/// * Enclave replies with a signed Attestation Report whose user data contains the
///   challenge nonce and its public DH key.
/// * The client verifies the report and sends back an FMD key encrypted with the shared
///   key
/// * The enclave sends and acknowledgement of receipt
fn handle_key_registration(mut client_conn: TcpStream, enclave_conn: &mut Tcp, msg: MsgFromHost) {
    // if we cannot complete the TLS setup for any reason, send a
    // failing acknowledgement to the enclave so that it can drop the
    // connection.
    macro_rules! abort_tls {
        () => {
            error!("Encountered unexpected error, aborting TLS connection setup.");
            enclave_conn.write(MsgFromHost::RATLSAck(AckType::Fail));
            return
        };
    }
    // The first communication round (RA and DHKE)
    enclave_conn.write(msg);
    match enclave_conn.read() {
        Ok(msg) => {
            info!("Received message: {:?}", msg);
            // This should be the attestation report or an enclave error
            // intended for the client.
            if let Ok(resp) = ServerMsg::try_from(msg) {
                let resp = serde_cbor::to_vec(&resp).unwrap();
                if client_conn.write_all(&resp).is_err() {
                    abort_tls!();
                }
            } else {
                error!("Received an unexpected message from the enclave");
                abort_tls!();
            }

            // read the client's response
            let Some(req) = client_read(&mut client_conn) else {
                abort_tls!();
            };

            // send an acknowledgement back to the enclave
            if let ClientMsg::RATLSAck(val) = req {
                enclave_conn.write(MsgFromHost::RATLSAck(val));
            } else {
                error!("Received an unexpected message from the client");
                abort_tls!();
            }
        }
        Err(e) => error!("Error receiving message from enclave: {e}"),
    }
    // Handle the final acknowledgement round
    match enclave_conn.read() {
        Ok(msg) => {
            info!("Received message: {:?}", msg);
            // This should be an success message or an enclave error
            // intended for the client.
            if let Ok(resp) = ServerMsg::try_from(msg) {
                let resp = serde_cbor::to_vec(&resp).unwrap();
                if client_conn.write_all(&resp).is_err() {
                    abort_tls!();
                }
            } else {
                error!("Received an unexpected message from the enclave");
                abort_tls!();
            }
        }
        Err(e) => error!("Error receiving message from enclave: {e}"),
    }
}

fn main() -> std::io::Result<()> {
    init_logging();
    info!("Kassandra service started.");
    let mut enclave_connection = Tcp::new(ENCLAVE_ADDRESS)?;
    let listener = TcpListener::bind(LISTENING_ADDRESS)?;
    for stream in listener.incoming().flatten() {
        stream.set_read_timeout(Some(Duration::new(5, 0))).unwrap();
        if let Err(e) = handle_connection(stream, &mut enclave_connection) {
            error!("Error handling client request: {e}");
        }
    }

    Ok(())
}

fn init_logging() {
    tracing_subscriber::fmt::SubscriberBuilder::default()
        .with_ansi(true)
        .init();
}
