#![no_std]
extern crate alloc;
mod com;
mod ratls;
mod report;

use alloc::string::ToString;
use ostd::arch::x86::qemu::{exit_qemu, QemuExitCode};
use ostd::prelude::*;
use shared::{MsgFromHost, MsgToHost};

use crate::com::HostCom;

#[ostd::main]
fn kernel_main() {
    println!("Enclave kernel initialized!");
    HostCom::init();
    loop {
        match HostCom::read() {
            Ok(msg) => {
                println!("Received msg: {:?}", msg);
                match msg {
                    MsgFromHost::RegisterKey { nonce, pk } => ratls::register_key(pk.0, nonce),
                    MsgFromHost::RequestReport { user_data } => {
                        let quote = report::get_quote(user_data.0);
                        HostCom::write(MsgToHost::Report(quote.as_bytes()));
                    }
                    _ => {}
                }
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
