#![no_std]
extern crate alloc;
mod host_channel;

use ostd::prelude::*;

use crate::host_channel::HostCom;

#[ostd::main]
fn kernel_main() {
    HostCom::init();
    println!("Hello world from guest kernel!");
}
