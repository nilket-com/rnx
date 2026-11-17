//! rnx's final: selection, concurrent stages under one budget, receipts,
//! release and revalidation.
pub mod bootstrap;
pub mod catalogue;
pub mod child;
pub mod cli;
pub mod compare;
pub mod compare_oracle;
pub mod compare_route;
pub mod controller;
pub mod evidence;
pub mod failure;
pub mod guardian;
pub mod outputs;
pub mod pyjson;
pub mod pylist;
pub mod receipt;
pub mod scheduler;
pub mod selection;
pub mod types;

#[cfg(test)]
#[path = "../tests/common/witness.rs"]
mod witness;
