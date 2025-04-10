use crate::com::OutgoingTcp;
use crate::ratls::register_fmd_key;
use chacha20poly1305::Key;
use clap::{Parser, Subcommand};
use fmd::fmd2_compact::CompactSecretKey;
use hkdf::Hkdf;
use shared::db::EncKey;
use shared::{ClientMsg, ServerMsg};
use tracing_subscriber::fmt::SubscriberBuilder;

mod ratls;

mod com;
#[cfg(feature = "tdx")]
mod tdx;
#[cfg(feature = "transparent")]
mod transparent;

const GAMMA: usize = 12;

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
        #[arg(
            long,
            help = "A block height to start detecting after",
            value_name = "Integer"
        )]
        birthday: Option<u64>,
    },
}

fn main() {
    init_logging();
    let cli = Cli::parse();
    let uuid = get_host_uuid(&cli.url);
    tracing::info!("Connected to host with uuid: {uuid}");
    match &cli.command {
        Commands::RegisterKey { key, birthday } => {
            tracing::info!("Registering FMD key...");
            let csk_key = serde_json::from_str(key).unwrap();
            let enc_key = encryption_key(&csk_key, &uuid);
            #[cfg(feature = "tdx")]
            register_fmd_key::<tdx::TdxClient>(&cli.url, csk_key, enc_key, *birthday);
            #[cfg(feature = "transparent")]
            register_fmd_key::<transparent::TClient>(&cli.url, csk_key, enc_key, *birthday);
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

#[cfg(test)]
mod tests {
    use crate::GAMMA;
    use fmd::FmdKeyGen;
    use fmd::fmd2_compact::MultiFmd2CompactScheme;

    #[test]
    fn generate_fmd_key() {
        let mut csprng = rand_core::OsRng;
        let mut compact_multi_fmd2 = MultiFmd2CompactScheme::new(GAMMA, 1);
        let (cmp_sk, cmp_pk) = compact_multi_fmd2.generate_keys(&mut csprng);
        panic!("Secret key: {cmp_sk}");
    }
}
