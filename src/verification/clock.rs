//! Shared live clock predicates for API and actual CLI observations.
pub(crate) fn clock_check(
	clock: &str,
	value: Option<i64>,
	before: i64,
	after: i64,
	elapsed: u128,
) -> bool {
	match (clock, value) {
		("", _) => true,
		("wall", Some(value)) => before <= value && value <= after.saturating_add(1),
		("monotonic", Some(value)) => value >= 0 && value as u128 <= elapsed.saturating_add(1),
		_ => false,
	}
}
