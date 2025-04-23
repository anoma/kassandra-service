use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;

use crate::OSTD_VERSION;

/// Get the Cargo metadata parsed from the standard output
/// of the invocation of Cargo. Return `None` if the command
/// fails or the `current_dir` is not in a Cargo workspace.
pub fn get_cargo_metadata<S1: AsRef<Path>, S2: AsRef<std::ffi::OsStr>>(
    current_dir: Option<S1>,
    cargo_args: Option<&[S2]>,
) -> Option<serde_json::Value> {
    let mut command = Command::new("cargo");
    command.args(["metadata", "--no-deps", "--format-version", "1"]);

    if let Some(current_dir) = current_dir {
        command.current_dir(current_dir);
    }

    if let Some(cargo_args) = cargo_args {
        command.args(cargo_args);
    }

    let output = command.output().unwrap();

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Some(serde_json::from_str(&stdout).unwrap())
}

pub fn add_manifest_dependency(crate_name: &str, crate_path: impl AsRef<Path>) {
    let manifest_path = "Cargo.toml";

    let mut manifest: toml::Table = {
        let content = std::fs::read_to_string(manifest_path).unwrap();
        toml::from_str(&content).unwrap()
    };

    // Check if "dependencies" key exists, create it if it doesn't
    if !manifest.contains_key("dependencies") {
        manifest.insert(
            "dependencies".to_string(),
            toml::Value::Table(toml::Table::new()),
        );
    }

    let dependencies = manifest.get_mut("dependencies").unwrap();

    let target_dep = toml::Table::from_str(&format!(
        "{} = {{ path = \"{}\", default-features = false }}",
        crate_name,
        crate_path.as_ref().display()
    ))
    .unwrap();
    dependencies.as_table_mut().unwrap().extend(target_dep);
    add_manifest_dependency_to(
        dependencies,
        "osdk-frame-allocator",
        Path::new("deps").join("frame-allocator"),
    );

    add_manifest_dependency_to(
        dependencies,
        "osdk-heap-allocator",
        Path::new("deps").join("heap-allocator"),
    );

    add_manifest_dependency_to(dependencies, "ostd", Path::new("..").join("ostd"));

    let content = toml::to_string(&manifest).unwrap();
    std::fs::write(manifest_path, content).unwrap();
}
pub fn add_manifest_dependency_to(manifest: &mut toml::Value, dep_name: &str, path: PathBuf) {
    let dep_str = match option_env!("OSDK_LOCAL_DEV") {
        Some("1") => {
            let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let dep_crate_dir = crate_dir.join(path);
            format!(
                "{} = {{ path = \"{}\" }}",
                dep_name,
                dep_crate_dir.display()
            )
        }
        _ => format!("{} = {{ version = \"{}\" }}", dep_name, OSTD_VERSION,),
    };
    let dep_val = toml::Table::from_str(&dep_str).unwrap();
    manifest.as_table_mut().unwrap().extend(dep_val);
}

pub fn copy_profile_configurations(workspace_root: impl AsRef<Path>) {
    let target_manifest_path = workspace_root.as_ref().join("Cargo.toml");
    let manifest_path = "Cargo.toml";

    let target_manifest: toml::Table = {
        let content = std::fs::read_to_string(target_manifest_path).unwrap();
        toml::from_str(&content).unwrap()
    };

    let mut manifest: toml::Table = {
        let content = std::fs::read_to_string(manifest_path).unwrap();
        toml::from_str(&content).unwrap()
    };

    // Copy the profile configurations
    let profile = target_manifest.get("profile");
    if let Some(profile) = profile {
        manifest.insert(
            "profile".to_string(),
            toml::Value::Table(profile.as_table().unwrap().clone()),
        );
    }

    let content = toml::to_string(&manifest).unwrap();
    std::fs::write(manifest_path, content).unwrap();
}

pub fn add_feature_entries(dep_crate_name: &str, features: &toml::Table) {
    let manifest_path = "Cargo.toml";
    let mut manifest: toml::Table = {
        let content = std::fs::read_to_string(manifest_path).unwrap();
        toml::from_str(&content).unwrap()
    };

    let mut table = toml::Table::new();
    for (feature, value) in features.iter() {
        let value = if feature != &"default".to_string() {
            vec![toml::Value::String(format!(
                "{}/{}",
                dep_crate_name, feature
            ))]
        } else {
            value.as_array().unwrap().clone()
        };
        table.insert(feature.clone(), toml::Value::Array(value));
    }

    manifest.insert("features".to_string(), toml::Value::Table(table));

    let content = toml::to_string(&manifest).unwrap();
    std::fs::write(manifest_path, content).unwrap();
}
