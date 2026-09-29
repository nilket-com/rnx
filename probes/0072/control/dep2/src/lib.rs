//! Extraction control: only `only_this` is re-exported by the facade.
pub fn only_this() -> i64 { 1 }
pub fn not_this() -> i64 { 2 }
pub struct Unreached;
/// Same name as `ctrl_dep::Opts`, no fields.
pub struct Opts;
pub fn take_opts(o: Opts) -> Opts { o }
/// Record 0117: written in a third crate (neither the trait's nor the
/// type's, and not the root), so only the extractor's recovery pass sees
/// these impls; they differ from each other and from ctrl_dep's
/// `Cmp<i64>` only in the trait argument, and all three must survive.
pub struct Local2;
impl ctrl_dep::Cmp<Local2> for ctrl_dep::Thing { fn cmp_to(&self, _rhs: Local2) -> bool { true } }
impl<'a> ctrl_dep::Cmp<&'a Local2> for ctrl_dep::Thing { fn cmp_to(&self, _rhs: &'a Local2) -> bool { false } }
