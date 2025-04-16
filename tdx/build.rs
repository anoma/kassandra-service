
fn main() {
    println!("cargo::rerun-if-changed=src/lib.rs");
    println!("cargo::rerun-if-changed=src/com.rs");
    if cfg!(feature = "mock") {
        panic!("HOOPY POOPY");
    }
}