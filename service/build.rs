use std::env;

fn main() {
    println!("cargo:rerun-if-env-changed=SGX_MODE");
    println!("cargo:rerun-if-changed=build.rs");

    let sdk_dir = env::var("SGX_SDK").unwrap_or_else(|_| "/opt/intel/sgxsdk".to_string());
    let mode = env::var("SGX_MODE").unwrap_or_else(|_| "HW".to_string());

    println!("cargo:rustc-link-search=native=../lib");
    println!("cargo:rustc-link-lib=static=enclave_u");

    println!("cargo:rustc-link-search=native={}/lib64", sdk_dir);
    match mode.as_ref() {
        "SIM" | "SW" => println!("cargo:rustc-link-lib=dylib=sgx_urts_sim"),
        "HW" => println!("cargo:rustc-link-lib=dylib=sgx_urts"),
        _ => println!("cargo:rustc-link-lib=dylib=sgx_urts"),
    }
}