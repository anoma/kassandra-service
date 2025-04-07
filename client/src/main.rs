use crate::com::OutgoingTcp;
use crate::ratls::register_fmd_key;
use clap::{Parser, Subcommand};
use shared::{ClientMsg, ServerMsg};
use tracing_subscriber::fmt::SubscriberBuilder;

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
    let uuid = get_host_uuid(&cli.url);
    tracing::info!("Connected to host with uuid: {uuid}");
    match &cli.command {
        Commands::RegisterKey { key } => {
            tracing::info!("Registering FMD key...");
            let csk_key = serde_json::from_str(key).unwrap();
            #[cfg(feature = "tdx")]
            register_fmd_key::<tdx::TdxClient>(&cli.url, csk_key);
            #[cfg(feature = "transparent")]
            register_fmd_key::<transparent::TClient>(&cli.url, csk_key);
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

#[cfg(test)]
mod tests {
    use fmd::FmdKeyGen;
    use fmd::fmd2_compact::MultiFmd2CompactScheme;

    #[test]
    fn generate_fmd_key() {
        let mut csprng = rand_core::OsRng;
        let mut compact_multi_fmd2 = MultiFmd2CompactScheme::new(12, 1);
        let (cmp_sk, cmp_pk) = compact_multi_fmd2.generate_keys(&mut csprng);
        panic!("Secret key: {cmp_sk}");
    }
}
