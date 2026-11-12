//! One set of live clock predicates for census and session observations.
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(tag = "name", rename_all = "kebab-case")]
pub(crate) enum Property {
	WallEnvelope {
		tolerance_ms: u64,
	},
	MonotonicPairEnvelope {
		units_per_ms: u64,
		pause_ms: u64,
		tolerance_ms: u64,
	},
}
impl Property {
	pub(crate) fn for_function(function: &str) -> Option<Self> {
		match function {
			"time::now_ms" => Some(Self::WallEnvelope { tolerance_ms: 1 }),
			"time::monotonic_ms" | "time::monotonic_ns" => Some(Self::MonotonicPairEnvelope {
				units_per_ms: if function.ends_with("_ns") {
					1_000_000
				} else {
					1
				},
				pause_ms: 20,
				tolerance_ms: 1,
			}),
			_ => None,
		}
	}
	pub(crate) fn check(
		self,
		values: &[Option<i64>],
		wall: [i64; 2],
		gap_ns: u128,
		span_ns: u128,
	) -> bool {
		match (self, values) {
			(Self::WallEnvelope { tolerance_ms }, [Some(value)]) => {
				wall_contains(*value, wall[0], wall[1], tolerance_ms)
			}
			(
				Self::MonotonicPairEnvelope {
					units_per_ms,
					pause_ms,
					tolerance_ms,
				},
				[Some(a), Some(b)],
			) => {
				if *a < 0 || b < a || gap_ns > span_ns || gap_ns < u128::from(pause_ms) * 1_000_000
				{
					return false;
				}
				let units = u128::from(units_per_ms);
				let lower = gap_ns
					.saturating_sub(u128::from(tolerance_ms) * 1_000_000)
					.saturating_mul(units)
					/ 1_000_000;
				let upper = span_ns
					.saturating_add(u128::from(tolerance_ms) * 1_000_000)
					.saturating_mul(units)
					/ 1_000_000;
				let delta = (*b - *a) as u128;
				lower <= delta && delta <= upper
			}
			_ => false,
		}
	}
}
fn wall_contains(value: i64, before: i64, after: i64, tolerance_ms: u64) -> bool {
	before <= value && value <= after.saturating_add(tolerance_ms.min(i64::MAX as u64) as i64)
}
// The census keeps its original single-reading contract and units.
pub(crate) fn clock_check(
	clock: &str,
	value: Option<i64>,
	before: i64,
	after: i64,
	elapsed: u128,
) -> bool {
	match (clock, value) {
		("", _) => true,
		("wall", Some(value)) => wall_contains(value, before, after, 1),
		("monotonic", Some(value)) => value >= 0 && value as u128 <= elapsed.saturating_add(1),
		_ => false,
	}
}
#[test]
fn census_contract_unchanged() {
	assert!(clock_check("", None, 0, 0, 0));
	assert!(clock_check("wall", Some(13), 10, 12, 0));
	assert!(!clock_check("wall", Some(14), 10, 12, 0));
	assert!(!clock_check("wall", Some(9), 10, 12, 0));
	assert!(clock_check("monotonic", Some(6), 0, 0, 5));
	assert!(!clock_check("monotonic", Some(7), 0, 0, 5));
	assert!(!clock_check("monotonic", Some(-1), 0, 0, 5));
	assert!(!clock_check("wall", None, 0, 0, 5));
}
#[test]
fn pair_teeth() {
	for function in ["time::monotonic_ms", "time::monotonic_ns"] {
		let p = Property::for_function(function).unwrap();
		let scale = if function.ends_with("_ns") {
			1_000_000
		} else {
			1
		};
		let good = [Some(10 * scale), Some(30 * scale)];
		assert!(p.check(&good, [0, 0], 20_000_000, 22_000_000));
		for bad in [
			[Some(10), Some(10)],
			[None, Some(30 * scale)],
			[Some(30), Some(10)],
		] {
			assert!(!p.check(&bad, [0, 0], 20_000_000, 22_000_000));
		}
		let wrong = if scale == 1 {
			[Some(10_000_000), Some(30_000_000)]
		} else {
			[Some(10), Some(30)]
		};
		assert!(!p.check(&wrong, [0, 0], 20_000_000, 22_000_000));
	}
}

#[test]
fn wall_property_teeth() {
	let property = Property::for_function("time::now_ms").unwrap();
	assert!(property.check(&[Some(13)], [10, 12], 0, 0));
	for bad in [Some(9), Some(14), None] {
		assert!(!property.check(&[bad], [10, 12], 0, 0));
	}
}
