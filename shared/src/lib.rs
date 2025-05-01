#![no_std]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod communication;
pub mod db;
pub mod ratls;
pub mod tee;

pub use communication::*;
pub use db::{Index, IndexList};
