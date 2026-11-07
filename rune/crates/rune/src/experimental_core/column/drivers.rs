//! Special whole-plan dispatch stays outside the shared invocation frame.
use super::*;

#[inline(never)]
pub(super) fn run(
	batch: PreparedBatch<'_, '_>,
	canceled: &mut impl FnMut() -> bool,
) -> core::result::Result<ColumnResult, (ColumnRefusal, Option<Fault>)> {
	if batch.plan.boolean_plan {
		boolean::run(batch, canceled)
	} else {
		f64_plan::run(batch, canceled)
	}
}
