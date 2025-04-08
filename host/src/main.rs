mod com;
mod config;
mod db;

use clap::Parser;
use eyre::WrapErr;
use once_cell::sync::OnceCell;
use shared::{AckType, ClientMsg, MsgFromHost, ServerMsg};
use tokio::net::TcpListener;
use tokio::select;
use tracing::{error, info};
use uuid::Uuid;

use crate::com::{IncomingTcp, Tcp};
use crate::config::Config;
use crate::db::{DB, InterruptFlag};

/// The UUID for this host instances
static HOST_UUID: OnceCell<Uuid> = OnceCell::new();

#[derive(Parser, Clone)]
#[command(version, about, long_about=None)]
struct Cli {
    #[arg(short, long, value_name = "URL", help = "URL to talk the enclave")]
    enclave: Option<String>,
    #[arg(
        short,
        long,
        value_name = "Port",
        help = "Port on which to list for client requests"
    )]
    listen: Option<String>,
    #[arg(
        long,
        value_name = "Millisecond",
        help = "How long to wait on client responses before timing out"
    )]
    listen_timeout: Option<u64>,
    #[arg(long, value_name = "URL", help = "URL of a masp indexer.")]
    indexer_url: Option<String>,
    #[arg(
        long,
        value_name = "Size",
        help = "Maximum number of entries in the fetching write-ahead log before flushing to disk."
    )]
    max_wal_size: Option<usize>,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    init_logging();
    let cli = Cli::parse();
    let config = Config::load_or_init(cli);

    // open the DB and spawn the fetch job in the background
    let (mut db, uuid) = DB::new()?;
    info!("Loaded databases; this instance has a UUID of {uuid}");
    HOST_UUID.set(uuid).unwrap();

    // start the job of fetching MASP txs from an indexer
    let mut interrupt_flag = InterruptFlag::new();
    db.start_updates(
        config.db.indexer_url,
        config.db.max_wal_size,
        interrupt_flag.clone(),
    )?;

    info!("Kassandra service started.");
    let mut enclave_connection =
        Tcp::new(&config.enclave_url).wrap_err("Could not establish connection to the enclave")?;
    info!("Connected to enclave");
    let listener = TcpListener::bind(&config.listen_url)
        .await
        .wrap_err("Could not bind to port to listen for incoming connections")?;

    loop {
        select! {
            incoming = listener.accept() => {
                match incoming {
                    Ok((stream, _)) => {
                        info!("Received connection...");
                        let incoming = IncomingTcp::new(
                            stream.into_std().unwrap(),
                            config.listen_timeout
                        );
                        handle_connection(incoming, &mut enclave_connection).await;
                    }
                    Err(e) => {
                        error!("Encountered unexpected error while listening for new connections: {e}");
                    }
                }
            }
            _ = interrupt_flag.dropped() => {
                db.close().await;
                return Ok(());
            }
        }
    }
}

/// Handle a client request and issue a response.
async fn handle_connection(mut client_conn: IncomingTcp, enclave_conn: &mut Tcp) {
    let req = match client_conn.timed_read().await {
        Some(Ok(req)) => req,
        Some(Err(e)) => {
            error!("Error receiving message from client: {e}");
            return;
        }
        None => return,
    };

    match &req {
        msg @ ClientMsg::RegisterKey { .. } => {
            handle_key_registration(
                client_conn,
                enclave_conn,
                MsgFromHost::try_from(msg).unwrap(),
            )
            .await;
        }
        ClientMsg::RequestReport { .. } | ClientMsg::RATLSAck(_) => {
            // These messages should have been preceded by a `RegisterKey`
            // call and then these would be handled inside the
            // `handle_key_registration` function.
            error!("Unexpect message from client, ignoring...");
        }
        ClientMsg::RequestUUID => {
            client_conn.write(ServerMsg::UUID(HOST_UUID.get().unwrap().to_string()));
        }
    }
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
async fn handle_key_registration(
    mut client_conn: IncomingTcp,
    enclave_conn: &mut Tcp,
    msg: MsgFromHost,
) {
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
            let req = match client_conn.timed_read().await {
                Some(Ok(req)) => req,
                Some(Err(e)) => {
                    error!("Error receiving message from client: {e}");
                    abort_tls!();
                }
                None => {
                    abort_tls!();
                }
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

fn init_logging() {
    tracing_subscriber::fmt::SubscriberBuilder::default()
        .with_ansi(true)
        .init();
}
