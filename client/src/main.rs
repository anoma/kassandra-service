use chacha20poly1305::Key;
use clap::{Parser, Subcommand};
use fmd::fmd2_compact::CompactSecretKey;
use hkdf::Hkdf;
use shared::db::EncKey;
use shared::{ClientMsg, ServerMsg};
use tracing_subscriber::fmt::SubscriberBuilder;

use crate::com::OutgoingTcp;
use crate::config::Config;
use crate::query::query_fmd_key;
use crate::ratls::register_fmd_key;

mod ratls;

mod com;
mod config;
mod query;
#[cfg(feature = "tdx")]
mod tdx;
#[cfg(feature = "transparent")]
mod transparent;

const GAMMA: usize = 12;

#[derive(Parser)]
#[command(version, about, long_about=None)]
struct Cli {
    #[arg(
        long,
        value_name = "PATH",
        help = "Path the directory storing client related files"
    )]
    base_dir: String,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Register a fuzzy message detection key with configured Kassandra services")]
    RegisterKey {
        #[arg(short, long, help = "JSON encoded FMD secret key")]
        key: String,
        #[arg(
            long,
            help = "A block height to start detecting after",
            value_name = "Integer"
        )]
        birthday: Option<u64>,
    },
    #[command(
        about = "Add a Kassandra service instance which a fuzzy message detection key will be registered to."
    )]
    AddService {
        #[arg(short, long, help = "JSON encoded FMD secret key")]
        key: String,
        #[arg(
            short,
            long,
            value_name = "URL",
            help = "URL of Kassandra service provider"
        )]
        url: String,
    },
    #[command(
        about = "Request the indices of MASP transactions that should be trial-decrypted by the provided key"
    )]
    QueryIndices {
        #[arg(short, long, help = "JSON encoded FMD secret key")]
        key: String,
    },
}

fn main() {
    init_logging();
    let cli = Cli::parse();
    match &cli.command {
        Commands::AddService { key, url } => {
            tracing::info!("Adding service to the config file...");
            let csk_key = serde_json::from_str(key).unwrap();
            Config::add_service(&cli.base_dir, csk_key, url).unwrap();
        }
        Commands::RegisterKey { key, birthday } => {
            tracing::info!("Registering FMD key...");
            let csk_key = serde_json::from_str(key).unwrap();
            #[cfg(feature = "tdx")]
            register_fmd_key::<tdx::TdxClient>(&cli.base_dir, csk_key, *birthday);
            #[cfg(feature = "transparent")]
            register_fmd_key::<transparent::TClient>(&cli.base_dir, csk_key, *birthday);
        }
        Commands::QueryIndices { key } => {
            let csk_key = serde_json::from_str(key).unwrap();
            query_fmd_key(&cli.base_dir, &csk_key);
        }
    }
}

fn init_logging() {
    SubscriberBuilder::default().with_ansi(true).init();
}

fn get_host_uuid(url: &str) -> String {
    let mut stream = OutgoingTcp::new(url);
    stream.write(ClientMsg::RequestUUID);
    match stream.read() {
        Ok(ServerMsg::UUID(uuid)) => uuid,
        Ok(ServerMsg::Error(err)) => panic!("{err}"),
        _ => panic!("Requesting UUID from host failed. Could not parse response."),
    }
}

fn encryption_key(csk_key: &CompactSecretKey, salt: &str) -> EncKey {
    let hk = Hkdf::<sha2::Sha256>::new(
        Some(salt.as_bytes()),
        serde_json::to_string(csk_key).unwrap().as_bytes(),
    );
    let mut encryption_key = [0u8; 32];
    hk.expand("Database encryption key".as_bytes(), &mut encryption_key)
        .unwrap();
    let enc_key: Key = encryption_key.into();
    enc_key.into()
}
