//! Special whole-plan dispatch stays outside the shared invocation frame.
use super::*;

#[inline(never)]
pub(super) fn run(
	batch: PreparedBatch<'_, '_>,
	canceled: &mut impl FnMut() -> bool,
) -> core::result::Result<ColumnResult, (ColumnRefusal, Option<Fault>)> {
	match batch.plan.driver() {
		Driver::Boolean => boolean::run(batch, canceled),
		Driver::F64 => f64_plan::run(batch, canceled),
		Driver::Predicate => predicate::run(batch, canceled),
	}
}
