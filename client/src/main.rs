use crate::ratls::register_fmd_key;

mod ratls;

#[cfg(feature = "tdx")]
mod tdx;

const HOST_ADDRESS: &str = "0.0.0.0:666";

fn main() {}
