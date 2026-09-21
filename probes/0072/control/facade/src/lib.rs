//! Extraction control facade, shaped like the polars crate root.
pub use ctrl_dep::Thing;              // single item re-export
pub mod alt { pub use ctrl_dep::Thing; } // same item, second path
pub use ctrl_dep::inner::*;           // glob re-export of a module
pub use ctrl_dep as dep;              // whole-crate re-export
pub use ctrl_dep::{Tr, Kind};
pub use ctrl_dep2::only_this;         // dep2 partially reachable
pub use ctrl_dep::{Opts, take_opts, take_map, Gen, GenI, take_alias, take_generic, take_into, take_into_extra, take_opt_vec};
pub mod two { pub use ctrl_dep2::{Opts, take_opts}; }
pub mod prelude { pub use ctrl_dep::Tr; pub use ctrl_dep::Blanket; }
#[cfg(feature = "extra")]
pub fn gated() -> i64 { 3 }
pub mod local {
    pub struct Local;
    impl Local { pub fn m(&self) -> i64 { 4 } }
}
