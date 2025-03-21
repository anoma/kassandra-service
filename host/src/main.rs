mod com;

use std::net::TcpListener;
use std::time::Duration;

use shared::{AckType, ClientMsg, MsgFromHost, MsgToHost, ServerMsg};
use tracing::{error, info};

use crate::com::{IncomingTcp, Tcp};

const ENCLAVE_ADDRESS: &str = "0.0.0.0:12345";
const LISTENING_ADDRESS: &str = "0.0.0.0:666";

fn client_read(client_conn: &mut IncomingTcp) -> Option<ClientMsg> {
    client_conn.read().ok()
}

/// Handle a client request and issue a response.
fn handle_connection(mut client_conn: IncomingTcp, enclave_conn: &mut Tcp) -> std::io::Result<()> {
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
                            client_conn.write(resp);
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
fn handle_key_registration(mut client_conn: IncomingTcp, enclave_conn: &mut Tcp, msg: MsgFromHost) {
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
                client_conn.write(resp);
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
            // This should be a success message or an enclave error
            // intended for the client.
            if let Ok(resp) = ServerMsg::try_from(msg) {
                client_conn.write(resp);
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
    info!("Connected to enclave");
    let listener = TcpListener::bind(LISTENING_ADDRESS)?;
    for stream in listener.incoming().flatten() {
        info!("Received connection...");
        let mut incoming = IncomingTcp::new(stream);
        incoming.set_read_timeout(Duration::new(5, 0));
        if let Err(e) = handle_connection(incoming, &mut enclave_connection) {
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
