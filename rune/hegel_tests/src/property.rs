//! One deterministic configuration and a digest of actual top-level draws.
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::fmt::Debug;
use std::io::Write;

use hegel::generators::Generator;
use sha2::{Digest, Sha256};

/// Seed 2 also found the committed `crate ! { ` formatter counterexample.
pub const GATE_SEED: u64 = 2;

fn settings(minimum_cases: u64) -> hegel::Settings {
	#[cfg(feature = "explore")]
	let (seed, cases) = {
		let seed = std::env::var("RNX_HEGEL_SEED")
			.expect("explore requires RNX_HEGEL_SEED")
			.parse::<u64>()
			.expect("RNX_HEGEL_SEED must be a u64");
		let requested = std::env::var("RNX_HEGEL_CASES")
			.map(|s| s.parse::<u64>().expect("RNX_HEGEL_CASES must be a u64"))
			.unwrap_or(0);
		(seed, requested.max(minimum_cases))
	};
	#[cfg(not(feature = "explore"))]
	let (seed, cases) = {
		assert!(
			std::env::var_os("RNX_HEGEL_SEED").is_none()
				&& std::env::var_os("RNX_HEGEL_CASES").is_none(),
			"exploration overrides require the explicit explore feature"
		);
		(GATE_SEED, minimum_cases)
	};

	hegel::Settings::new()
		.seed(Some(seed))
		.derandomize(false)
		.database(None)
		.backend(hegel::Backend::Default)
		.test_cases(cases)
		// Host load must not change the verdict; retain case-count health checks.
		.suppress_health_check([hegel::HealthCheck::TooSlow])
		.report_multiple_failures(true)
}

#[derive(Default)]
struct Trace {
	hash: RefCell<Sha256>,
	cases: Cell<u64>,
	draws: Cell<u64>,
}

impl Trace {
	fn write(&self, bytes: &[u8]) {
		let mut hash = self.hash.borrow_mut();
		hash.update((bytes.len() as u64).to_le_bytes());
		hash.update(bytes);
	}

	fn draw<T: Debug + 'static>(&self, value: &T) {
		self.draws.set(self.draws.get() + 1);
		self.write(std::any::type_name::<T>().as_bytes());
		// Debug's NaN spelling omits payload/sign, so floats bind their bits.
		if let Some(value) = (value as &dyn Any).downcast_ref::<f64>() {
			self.write(&value.to_bits().to_le_bytes());
		} else if let Some(value) = (value as &dyn Any).downcast_ref::<f32>() {
			self.write(&value.to_bits().to_le_bytes());
		} else {
			self.write(format!("{value:?}").as_bytes());
		}
	}
}

/// The properties' existing draw/assume/reject surface, recording every draw.
pub struct Case<'a> {
	inner: hegel::TestCase,
	trace: &'a Trace,
}

impl Case<'_> {
	pub fn draw<T: Debug + 'static>(&self, generator: impl Generator<T>) -> T {
		let value = self.inner.draw(generator);
		self.trace.draw(&value);
		value
	}

	pub fn assume(&self, condition: bool) {
		self.inner.assume(condition);
	}

	pub fn reject(&self) -> ! {
		self.inner.reject()
	}
}

/// Run one named property with a preserved lower bound on its case count.
pub fn property(name: &str, minimum_cases: u64, mut test: impl FnMut(Case<'_>)) {
	let trace = Trace::default();
	hegel::Hegel::new(|inner| {
		trace.cases.set(trace.cases.get() + 1);
		trace.write(b"case");
		test(Case {
			inner,
			trace: &trace,
		});
	})
	.settings(settings(minimum_cases))
	.run();

	assert!(
		trace.cases.get() > 0 && trace.draws.get() > 0,
		"empty property: {name}"
	);
	let hash = trace.hash.into_inner().finalize();
	// Use one direct write so parallel libtest captures cannot hide/interleave it.
	writeln!(
		std::io::stderr().lock(),
		"HEGEL_CASE_DIGEST {name} {} {} {hash:x}",
		trace.cases.get(),
		trace.draws.get()
	)
	.expect("write property digest");
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn digest_binds_draws_and_float_payloads() {
		let a = Trace::default();
		let b = Trace::default();
		a.draw(&f64::from_bits(0x7ff8_0000_0000_0001));
		b.draw(&f64::from_bits(0x7ff8_0000_0000_0002));
		assert_ne!(
			a.hash.into_inner().finalize(),
			b.hash.into_inner().finalize()
		);
	}
}
