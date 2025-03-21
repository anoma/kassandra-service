#![no_std]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod communication;
pub mod ratls;
pub mod tee;

pub use communication::*;
