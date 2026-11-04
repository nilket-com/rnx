use super::*;

#[test]
fn consume_boundaries_are_transactional() {
	let _outer = replace(usize::MAX);
	assert_eq!(remaining(), None);
	assert_eq!(consume(usize::MAX), Ok(()));
	assert_eq!(consume(0), Ok(()));
	assert_eq!(remaining(), None);
	for initial in [0, 1, 17, usize::MAX - 1] {
		with(initial, || {
			assert_eq!(consume(0), Ok(()));
			assert_eq!(remaining(), Some(initial));
			assert_eq!(consume(initial + 1), Err(BudgetExhausted));
			assert_eq!(remaining(), Some(initial));
			assert_eq!(consume(initial), Ok(()));
			assert_eq!(remaining(), Some(0));
			assert_eq!(consume(1), Err(BudgetExhausted));
			assert_eq!(consume(0), Ok(()));
			assert_eq!(remaining(), Some(0));
		})
		.call();
		assert_eq!(remaining(), None);
	}
}

#[test]
fn successful_bulk_debit_matches_individual_permits() {
	for initial in 0..12 {
		for n in 0..=initial {
			let bulk = with(initial, || {
				consume(n).unwrap();
				remaining()
			})
			.call();
			let single = with(initial, || {
				for _ in 0..n {
					assert!(take());
				}
				remaining()
			})
			.call();
			assert_eq!(bulk, single);
		}
	}
}

#[test]
fn consume_obeys_nested_scopes_and_acquire() {
	let _outer = replace(29);
	with(13, || {
		consume(3).unwrap();
		assert_eq!(remaining(), Some(10));
		with(4, || {
			consume(4).unwrap();
			assert_eq!(consume(1), Err(BudgetExhausted));
		})
		.call();
		assert_eq!(remaining(), Some(10));
		{
			let _replacement = replace(2);
			consume(1).unwrap();
			assert_eq!(remaining(), Some(1));
		}
		assert_eq!(remaining(), Some(10));
		{
			let mut held = acquire();
			assert_eq!(remaining(), None);
			consume(usize::MAX).unwrap();
			assert!(held.take());
		}
		assert_eq!(remaining(), Some(9));
		consume(9).unwrap();
	})
	.call();
	assert_eq!(remaining(), Some(29));
}

#[test]
fn consume_unwind_restores_outer_allowance() {
	let _outer = replace(31);
	let outcome = std::panic::catch_unwind(|| {
		with(9, || {
			consume(7).unwrap();
			let _inner = replace(3);
			consume(2).unwrap();
			panic!("budget unwind control");
		})
		.call();
	});
	assert!(outcome.is_err());
	assert_eq!(remaining(), Some(31));
}

#[test]
fn consume_preserves_slice_threshold() {
	let _outer = replace(41);
	with_slice(12, 4, || {
		let threshold = || {
			let point = YieldPoint::take();
			point.threshold()
		};
		assert_eq!(threshold(), 8);
		consume(5).unwrap();
		assert_eq!(remaining(), Some(7));
		assert_eq!(threshold(), 8);
		assert_eq!(consume(8), Err(BudgetExhausted));
		assert_eq!(remaining(), Some(7));
		with_slice(4, 1, || {
			assert_eq!(threshold(), 3);
			consume(4).unwrap();
		})
		.call();
		assert_eq!(threshold(), 8);
		assert_eq!(remaining(), Some(7));
		consume(7).unwrap();
		assert!(!take());
	})
	.call();
	assert_eq!(remaining(), Some(41));
}

#[test]
fn consume_future_polls_keep_the_same_hard_allowance() {
	use core::task::{Context, Poll, Waker};
	let _outer = replace(43);
	let mut first = true;
	let work = core::future::poll_fn(move |_| {
		if first {
			first = false;
			consume(3).unwrap();
			assert_eq!(remaining(), Some(4));
			Poll::Pending
		} else {
			assert_eq!(remaining(), Some(4));
			assert_eq!(consume(5), Err(BudgetExhausted));
			consume(4).unwrap();
			Poll::Ready(remaining())
		}
	});
	let mut work = std::boxed::Box::pin(with(7, work));
	let mut cx = Context::from_waker(Waker::noop());
	assert_eq!(work.as_mut().poll(&mut cx), Poll::Pending);
	assert_eq!(remaining(), Some(43));
	assert_eq!(work.as_mut().poll(&mut cx), Poll::Ready(Some(0)));
	assert_eq!(remaining(), Some(43));
}
