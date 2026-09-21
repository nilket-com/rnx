//! Extraction control: a dependency with every shape the extractor must see.
#[derive(Clone)]
pub struct Thing(i64);
impl Thing {
    pub fn new(v: i64) -> Self { Thing(v) }
    pub fn method(&self, k: i64) -> i64 { self.0 + k }
}
pub trait Tr {
    fn required(&self) -> i64;
    fn provided(&self) -> i64 { self.required() * 2 }
}
impl Tr for Thing { fn required(&self) -> i64 { self.0 } }
pub trait Blanket { fn blank(&self) -> u8 { 1 } }
impl<T: Clone> Blanket for T {}
pub mod inner {
    pub fn f() -> i64 { 1 }
    pub fn g(x: &str) -> String { x.to_owned() }
}
pub fn crate_fn(t: Thing) -> Thing { t }
pub unsafe fn danger(p: *const u8) -> u8 { unsafe { *p } }
#[doc(hidden)]
pub fn hidden() {}
pub const C: i64 = 7;
#[macro_export]
macro_rules! mac { () => { 1 } }
pub enum Kind { A, B(i64) }
/// Same name as `ctrl_dep2::Opts`, different identity: this one has a field.
pub struct Opts { pub flag: bool }
impl Default for Opts { fn default() -> Self { Opts { flag: false } } }
pub fn take_opts(o: Opts) -> bool { o.flag }
pub fn take_map(m: std::collections::HashMap<String, Opts>) -> usize { m.len() }
/// Generic struct, concrete alias, generic use.
pub struct Gen<T>(pub T);
pub type GenI = Gen<i64>;
impl<T> Gen<T> { pub fn get_ref(&self) -> &T { &self.0 } }
pub fn take_alias(g: GenI) -> i64 { g.0 }
pub fn take_generic<T>(g: Gen<T>) -> Gen<T> { g }
pub fn take_into<T: Into<Thing>>(t: T) -> Thing { t.into() }
pub fn take_into_extra<T: Into<Thing> + Iterator>(t: T) -> Thing { t.into() }
pub fn take_opt_vec(v: Option<Vec<&str>>) -> usize { v.map(|v| v.len()).unwrap_or(0) }
