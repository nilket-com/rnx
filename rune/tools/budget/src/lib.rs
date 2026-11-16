//! One CPU and memory budget for a command, Cargo, rustc and their children.
//!
//! [`pipe`] is an anonymous-pipe jobserver a command's own children inherit;
//! [`fifo`] is a named one that independent processes join through
//! `MAKEFLAGS`; [`memory`] derives how many jobs either may hold.
pub mod fifo;
pub mod memory;
pub mod pipe;
