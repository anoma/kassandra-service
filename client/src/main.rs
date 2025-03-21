use clap::{Parser, Subcommand};
use tracing_subscriber::fmt::SubscriberBuilder;

use crate::ratls::register_fmd_key;

mod ratls;

mod com;
#[cfg(feature = "tdx")]
mod tdx;
#[cfg(feature = "transparent")]
mod transparent;

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
            #[cfg(feature = "transparent")]
            register_fmd_key::<transparent::TClient>(&cli.url, fmd_key);
        }
    }
}

fn init_logging() {
    SubscriberBuilder::default().with_ansi(true).init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use fmd::FmdKeyGen;
    use fmd::fmd2_compact::MultiFmd2CompactScheme;

    #[test]
    fn generate_fmd_key() {
        let mut csprng = rand_core::OsRng;
        let mut compact_multi_fmd2 = MultiFmd2CompactScheme::new(12, 1);
        let (cmp_sk, _) = compact_multi_fmd2.generate_keys(&mut csprng);
        let sk = serde_json::to_string(&cmp_sk).unwrap();
        panic!("Secret key: {sk}");
    }
}
