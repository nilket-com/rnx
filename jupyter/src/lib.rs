//! Independent components of the rnx Jupyter kernel (record 0047).
pub mod connection;
#[cfg(target_os = "linux")]
pub mod containment;
#[cfg(target_os = "linux")]
pub mod kernel;
pub mod streams;
pub mod transport;
pub mod wire;
#[cfg(target_os = "linux")]
pub mod worker;

pub mod install;
