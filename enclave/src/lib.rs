#![no_std]
extern crate alloc;
mod host;

use ostd::prelude::*;

use crate::host::HostCom;

#[ostd::main]
fn kernel_main() {
    HostCom::init();
    println!("Hello world from guest kernel!");
}
