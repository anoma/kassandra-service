#![no_std]
extern crate alloc;
mod host;

use ostd::arch::x86::qemu::{exit_qemu, QemuExitCode};
use ostd::prelude::*;

use crate::host::HostCom;

#[ostd::main]
fn kernel_main() {
    println!("Hello world from guest kernel!");
    HostCom::init();
    loop {
        if let Some(string) = HostCom::try_read_string().unwrap() {
            println!("Received: {string}");
        }
        core::hint::spin_loop();
    }
    exit_qemu(QemuExitCode::Sucess);
}
