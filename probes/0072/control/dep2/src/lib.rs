//! Extraction control: only `only_this` is re-exported by the facade.
pub fn only_this() -> i64 { 1 }
pub fn not_this() -> i64 { 2 }
pub struct Unreached;
/// Same name as `ctrl_dep::Opts`, no fields.
pub struct Opts;
pub fn take_opts(o: Opts) -> Opts { o }
