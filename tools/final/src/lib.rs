//! rnx's final: selection, concurrent stages under one budget, receipts,
//! release and revalidation.
pub mod bootstrap;
pub mod build_targets;
pub mod catalogue;
pub mod checkpoint;
pub mod child;
pub mod clean;
pub mod cli;
pub mod compare;
pub mod compare_oracle;
pub mod compare_route;
pub mod contracts;
pub mod controller;
pub mod cross;
pub mod evidence;
pub mod failure;
pub mod fixed_route;
mod fixed_route_images;
pub mod fixed_route_stage;
mod gate_inputs;
pub mod guardian;
pub mod jobs;
pub mod outputs;
pub mod owned;
pub mod producer;
pub mod pyjson;
pub mod pylist;
pub mod receipt;
pub mod routes;
pub mod schedule;
pub mod scheduler;
pub mod selection;
pub mod stage;
pub mod types;

mod space;
#[cfg(test)]
#[path = "../tests/common/witness.rs"]
mod witness;
