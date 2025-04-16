use std::process;
use std::process::Command;

use clap::{arg, Parser, Subcommand};
use toml::{Table, Value};

#[derive(Parser)]
#[command(version, about, long_about=None)]
struct Cli {
    #[arg(
        short,
        long,
        value_name = "FEATURES",
        help = "Space or comma separated list of features to activate"
    )]
    features: Option<String>,
    #[arg(
        short,
        long,
        value_name = "TRIPLE",
        help = "Build for target triple"
    )]
    target: Option<String>,
    #[arg(long, short, help=" Build artifacts in release mode, with optimizations")]
    release: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Compile the current package")]
    Build,
    #[command(about = "Run the current package")]
    Run
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Build => build(cli.features, cli.target, cli.release),
        Commands::Run => run(cli.features, cli.target, cli.release),
    }


}


fn build(features: Option<String>, target: Option<String>, release: bool) {
    let mut cargo = Command::new("cargo");
    let env_rustflags = std::env::var("RUSTFLAGS").unwrap_or_default();
    let rustflags = vec![
        &env_rustflags,
        "-C link-arg=-Tx86_64.ld",
        "-C relocation-model=static",
        "-C relro-level=off",
        "-C force-unwind-tables=yes",
        "--check-cfg cfg(ktest)",
        "-C no-redzone=y",
        "-C target-feature=+ermsb",
    ];

    cargo.env_remove("RUSTUP_TOOLCHAIN");
    cargo.env("RUSTFLAGS", rustflags.join(" "));
    cargo.arg("build");
    if let Some(features) = features {
        cargo.arg("--features")
            .arg(features);
    }
    if let Some(target) = target {
        cargo.arg("--target")
            .arg(target);
    }
    cargo.arg("-Zbuild-std=core,alloc,compiler_builtins")
        .arg("-Zbuild-std-features=compiler-builtins-mem");
    if release {
        cargo.arg("--profile=release");
    }

    let target_dir = std::env::current_dir()
        .unwrap()
        .join("target");
    cargo.arg("--target-dir")
        .arg(target_dir);
    println!("Running command:\n {:?}", cargo);
    let status = cargo.status().unwrap();
    if !status.success() {
        println!("Build failed: {status}");
        process::exit(1);
    }
}

fn run(features: Option<String>, target: Option<String>, release: bool) {
    if !std::fs::exists("target").unwrap() {
        build(features, target, release);
    }
    let config = std::fs::read_to_string("OSDK.toml").unwrap().parse::<Table>().unwrap();
    let Value::String(args) = &config["qemu"]["args"] else {
        panic!("Could not parse qemu args for OSDK.toml");
    };
    let args = args.split(' ').collect::<Vec<_>>();
    let mut command = Command::new("qemu-system-x86_64");
    command.current_dir(std::env::current_dir().unwrap().canonicalize().unwrap());
    let image = std::env::current_dir()
        .unwrap()
        .join("target")
        .join("osdk")
        .join("fmd-tdx-enclave-service")
        .join("fmd-tdx-enclave-service-osdk-bin.iso")
        .canonicalize()
        .unwrap();
    let image = image.to_string_lossy();
    command.arg("-drive");
    command.arg(format!("file={image},format=raw,index=2,media=cdrom"));
    command.args(args);

    println!("Running command:\n {:#?}", command);
    let status = command.status().unwrap();
    if !status.success() {
        println!("Build failed: {status}");
        process::exit(1);
    }
}