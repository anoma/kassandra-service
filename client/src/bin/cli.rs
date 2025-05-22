use clap::{Parser, Subcommand};
use fmd::KeyExpansion;
use fmd::fmd2_compact::{CompactSecretKey, MultiFmd2CompactScheme};
use kassandra_client::config::{Config, hash_key};
use kassandra_client::query::query_fmd_key;
use kassandra_client::ratls::register_fmd_key;
#[cfg(feature = "tdx")]
use kassandra_client::tdx;
#[cfg(feature = "transparent")]
use kassandra_client::transparent;
use kassandra_client::{GAMMA, encryption_key, get_host_uuid, init_logging};

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
            let uuid = get_host_uuid(url);
            let csk_key: CompactSecretKey = serde_json::from_str(key).unwrap();
            let cpk_key = csk_key.master_public_key();
            let scheme = MultiFmd2CompactScheme::new(GAMMA, 1);
            let (fmd_key, _) = scheme.expand_keypair(&csk_key, &cpk_key);
            let enc_key = encryption_key(&fmd_key, &uuid);
            let key_hash = hash_key(&csk_key, GAMMA);
            Config::add_service(&cli.base_dir, key_hash, url, enc_key).unwrap();
        }
        Commands::RegisterKey { key, birthday } => {
            tracing::info!("Registering FMD key...");
            let csk_key = serde_json::from_str(key).unwrap();
            #[cfg(feature = "tdx")]
            register_fmd_key::<tdx::TdxClient>(&cli.base_dir, csk_key, *birthday, GAMMA);
            #[cfg(feature = "transparent")]
            register_fmd_key::<transparent::TClient>(&cli.base_dir, csk_key, *birthday, GAMMA);
        }
        Commands::QueryIndices { key } => {
            let csk_key = serde_json::from_str(key).unwrap();
            let key_hash = hash_key(&csk_key, GAMMA);
            let indices = query_fmd_key(&cli.base_dir, &key_hash);
            let result = serde_json::to_string_pretty(&indices).unwrap();
            tracing::info!("{result}");
        }
    }
}
