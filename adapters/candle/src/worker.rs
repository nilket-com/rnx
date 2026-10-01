//! Record 0129: Candle's joined worker. Each call runs on a scoped thread
//! that is joined before the call returns, so no work outlives it; a panic
//! in Candle becomes a script error naming the operation. (rnx has no
//! shared engine API: Polars' engine is private to that adapter.)
pub(crate) fn run<T: Send>(op: &str, f: impl FnOnce() -> T + Send) -> Result<T, String> {
	std::thread::scope(|s| {
		std::thread::Builder::new()
			.name("rnx-candle-worker".into())
			.spawn_scoped(s, f)
			.map_err(|e| format!("{op}: could not start the worker: {e}"))?
			.join()
			.map_err(|_| format!("{op}: Candle panicked"))
	})
}
