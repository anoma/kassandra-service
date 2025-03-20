use crate::ratls::register_fmd_key;
use clap::{Parser, Subcommand};
use tracing_subscriber::fmt::SubscriberBuilder;

mod ratls;

#[cfg(feature = "tdx")]
mod tdx;

#[derive(Parser)]
#[command(version, about, long_about=None)]
struct Cli {
    #[arg(
        short,
        long,
        value_name = "URL",
        help = "URL of Kassandra service provider"
    )]
    url: String,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Register a fuzzy message detection key with a Kassandra service")]
    RegisterKey {
        #[arg(short, long, help = "JSON encoded FMD secret key")]
        key: String,
    },
}

fn main() {
    init_logging();
    let cli = Cli::parse();

    match &cli.command {
        Commands::RegisterKey { key } => {
            tracing::info!("Registering FMD key...");
            let fmd_key = serde_json::from_str(key).unwrap();
            #[cfg(feature = "tdx")]
            register_fmd_key::<tdx::TdxClient>(&cli.url, fmd_key);
        }
    }
}

fn init_logging() {
    SubscriberBuilder::default().with_ansi(true).init();
}
