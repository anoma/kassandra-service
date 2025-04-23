mod manifest;
mod utils;

use std::path::PathBuf;
use std::process;
use std::process::Command;

use clap::{Parser, Subcommand, arg};
use toml::{Table, Value};

use crate::manifest::{
    add_feature_entries, add_manifest_dependency, copy_profile_configurations, get_cargo_metadata,
};
use crate::utils::{DirGuard, hard_link_or_copy};

const OSTD_VERSION: &str = "0.14.1";

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
    #[arg(short, long, value_name = "TRIPLE", help = "Build for target triple")]
    target: Option<String>,
    #[arg(
        long,
        short,
        help = " Build artifacts in release mode, with optimizations"
    )]
    release: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Compile the current package")]
    Build,
    #[command(about = "Run the current package")]
    Run,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Build => build(cli.features, cli.target, cli.release),
        Commands::Run => run(cli.features, cli.target, cli.release),
    }
}

fn build(features: Option<String>, target: Option<String>, release: bool) {
    let base_crate = do_new_base_crate();
    let target_dir = std::env::current_dir().unwrap().join("target");
    let _dir_guard = DirGuard::change_dir(&base_crate);
    run_cargo(features, target, target_dir, release);
    create_bootdev_image(release);
}

fn run_cargo(features: Option<String>, target: Option<String>, target_dir: PathBuf, release: bool) {
    let mut cargo = Command::new("cargo");
    let env_rustflags = std::env::var("RUSTFLAGS").unwrap_or_default();
    let rustflags = [
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
        cargo.arg("--features").arg(features);
    }
    if let Some(target) = target {
        cargo.arg("--target").arg(target);
    }
    cargo
        .arg("-Zbuild-std=core,alloc,compiler_builtins")
        .arg("-Zbuild-std-features=compiler-builtins-mem");
    if release {
        cargo.arg("--profile=release");
    }
    cargo.arg("--target-dir").arg(target_dir);
    println!("Running command:\n {:?}", cargo);
    let status = cargo.status().unwrap();
    if !status.success() {
        println!("Build failed: {status}");
        process::exit(1);
    }
}

fn do_new_base_crate() -> PathBuf {
    let base_crate_path = std::env::current_dir()
        .unwrap()
        .join("target")
        .join("osdk")
        .join("fmd-tdx-enclave-service-run-base");
    let dep_crate_name = "fmd-tdx-enclave-service";
    let dep_crate_path = std::env::current_dir().unwrap();
    use std::fs;
    let workspace_root = {
        let meta = get_cargo_metadata(None::<&str>, None::<&[&str]>).unwrap();
        std::path::PathBuf::from(meta.get("workspace_root").unwrap().as_str().unwrap())
    };

    if base_crate_path.exists() {
        fs::remove_dir_all(&base_crate_path).unwrap();
    }

    let (dep_crate_version, dep_crate_features) = {
        let cargo_toml = dep_crate_path.join("Cargo.toml");
        let cargo_toml = fs::read_to_string(cargo_toml).unwrap();
        let cargo_toml: toml::Value = toml::from_str(&cargo_toml).unwrap();
        let dep_version = cargo_toml
            .get("package")
            .unwrap()
            .as_table()
            .unwrap()
            .get("version")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        let dep_features = cargo_toml
            .get("features")
            .map(|f| f.as_table().unwrap().clone())
            .unwrap_or_default();
        (dep_version, dep_features)
    };

    // Create the directory
    fs::create_dir_all(&base_crate_path).unwrap();
    // Create the src directory
    fs::create_dir_all(base_crate_path.join("src")).unwrap();

    // Write Cargo.toml
    let cargo_toml = include_str!("Cargo.toml.template");
    let cargo_toml = cargo_toml.replace("#NAME#", &(dep_crate_name.to_string() + "-osdk-bin"));
    let cargo_toml = cargo_toml.replace("#VERSION#", &dep_crate_version);
    fs::write(base_crate_path.join("Cargo.toml"), cargo_toml).unwrap();

    // Set the current directory to the target osdk directory
    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(&base_crate_path).unwrap();

    // Add linker script files
    macro_rules! include_linker_script {
        ([$($linker_script:literal),+]) => {$(
            fs::write(
                base_crate_path.join($linker_script),
                include_str!(concat!($linker_script, ".template"))
            ).unwrap();
        )+};
    }
    // TODO: currently just x86_64 works; add support for other architectures
    // here when OSTD is ready
    include_linker_script!(["x86_64.ld", "riscv64.ld"]);

    // Overwrite the main.rs file
    let main_rs = include_str!("main.rs.template");
    // Replace all occurrence of `#TARGET_NAME#` with the `dep_crate_name`
    let main_rs = main_rs.replace("#TARGET_NAME#", &dep_crate_name.replace('-', "_"));
    fs::write("src/main.rs", main_rs).unwrap();

    // Add dependencies to the Cargo.toml
    add_manifest_dependency(dep_crate_name, dep_crate_path);

    // Copy the manifest configurations from the target crate to the base crate
    copy_profile_configurations(workspace_root);

    // Generate the features by copying the features from the target crate
    add_feature_entries(dep_crate_name, &dep_crate_features);

    // Get back to the original directory
    std::env::set_current_dir(original_dir).unwrap();
    base_crate_path
}

fn create_bootdev_image(release: bool) {
    let iso_root = std::env::current_dir()
        .unwrap()
        .join("target")
        .join("osdk")
        .join("iso_root");
    if iso_root.exists() {
        std::fs::remove_dir_all(&iso_root).unwrap();
    }
    std::fs::create_dir_all(iso_root.join("boot").join("grub")).unwrap();
    let target_path = iso_root
        .join("boot")
        .join("fmd-tdx-enclave-service-osdk-bin");
    let bin_path = std::env::current_dir()
        .unwrap()
        .join("target")
        .join("x86_64-unknown-none")
        .join(if release { "release" } else { "debug" })
        .join("fmd-tdx-enclave-service-osdk-bin");
    println!("bin_path: {:?}", bin_path);
    println!("target_path: {:?}", target_path);
    hard_link_or_copy(&bin_path, &target_path).unwrap();
    let grub_cfg = r#"# AUTOMATICALLY GENERATED FILE, DO NOT EDIT UNLESS YOU KNOW WHAT YOU ARE DOING

    # set debug=linux,efi,linuxefi

    set timeout_style=hidden
    set timeout=0

    menuentry 'asterinas' {
        multiboot2 /boot/fmd-tdx-enclave-service-osdk-bin --

        boot
    }
    "#;
    let grub_cfg_path = iso_root.join("boot").join("grub").join("grub.cfg");
    std::fs::write(grub_cfg_path, grub_cfg).unwrap();
    let iso_path = std::env::current_dir()
        .unwrap()
        .join("target")
        .join("osdk")
        .join("fmd-tdx-enclave-service")
        .join("fmd-tdx-enclave-service-osdk-bin.iso");
    let mut grub_mkrescue_cmd = std::process::Command::new("grub-mkrescue");
    grub_mkrescue_cmd
        .arg(iso_root.as_os_str())
        .arg("-o")
        .arg(&iso_path);
    if !grub_mkrescue_cmd.status().unwrap().success() {
        panic!("Failed to run {:#?}.", grub_mkrescue_cmd);
    }

    hard_link_or_copy("fmd-tdx-enclave-service-osdk-bin.iso", iso_path).unwrap();
}

fn run(features: Option<String>, target: Option<String>, release: bool) {
    if !std::fs::exists("target").unwrap() {
        build(features, target, release);
    }
    let config = std::fs::read_to_string("OSDK.toml")
        .unwrap()
        .parse::<Table>()
        .unwrap();
    let Value::String(args) = &config["qemu"]["args"] else {
        panic!("Could not parse qemu args for OSDK.toml");
    };
    let mut args = args.split(' ').collect::<Vec<_>>();
    if Some("") == args.last().map(|v| &**v) {
        args.pop();
    }
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
