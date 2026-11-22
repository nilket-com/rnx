use super::*;
use rune::host::OpaqueProducerLimit as Limit;
pub type Unary = fn(&Tensor) -> Result<Tensor, String>;
pub fn unary() -> impl Iterator<Item = (&'static str, Unary)> {
	let members = tensor_ops::IMAGE_UNARY.iter().copied();
	#[cfg(feature = "image-witness")]
	let members = members.chain(tensor_ops::fixture_member::IMAGE_UNARY.iter().copied());
	members
}
pub fn load(path: &str) -> Result<Mlp, String> {
	super::load(path)
}
pub fn dims(model: &Mlp) -> (i64, i64, i64) {
	mlp_dims(model)
}
pub fn retained(model: &Mlp) -> usize {
	dense_credit::model_bytes(model)
}
pub fn tensor_bytes(t: &Tensor) -> usize {
	dense_credit::tensor_bytes(t)
}
pub fn convert(d: &Dense, credit: usize) -> Result<Result<Tensor, String>, Limit> {
	dense_credit::convert(d, credit)
}
pub fn apply(model: &Mlp, x: &Tensor, credit: usize) -> Result<Result<Tensor, String>, Limit> {
	dense_credit::forward(model, x, credit)
}
pub fn display(t: &Tensor) -> String {
	display::render(&t.0)
}
