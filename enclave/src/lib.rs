#![no_std]
extern crate alloc;
mod com;

use crate::com::HostCom;
use alloc::string::ToString;
use ostd::arch::x86::qemu::{exit_qemu, QemuExitCode};
use ostd::prelude::*;
use shared::MsgToHost;

#[ostd::main]
fn kernel_main() {
    println!("Enclave kernel initialized!");
    HostCom::init();
    loop {
        match HostCom::read_string() {
            Ok(msg) => {
                println!("Received msg: {:?}", msg);
                HostCom::write(MsgToHost::Basic(
                    "These are not the droids you're looking for.".to_string(),
                ));
            }
            Err(e) => {
                println!("Error reading message: {:?}", e);
                HostCom::write(MsgToHost::Error(e.to_string()));
            }
        }
        core::hint::spin_loop();
    }
    exit_qemu(QemuExitCode::Success);
}
