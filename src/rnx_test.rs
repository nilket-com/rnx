//! Feature-gated fixtures, never part of the production module inventory.
use rune::{Context, Module};

pub(crate) fn install(context: &mut Context) -> crate::Result<Vec<crate::host::HostFunction>> {
	let mut module = Module::with_crate("rnx_test")?;
	async fn pending(ms: u64) -> u64 {
		tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
		ms
	}
	module.function("test_pending", pending).build()?;
	// A deliberately ownerless task makes final drain fail without changing
	// its allowance. Only the test-support binary can create this fixture.
	module
		.function("test_shutdown_task", || async {
			tokio::spawn(std::future::pending::<()>());
		})
		.build()?;
	module
		.function("test_allocation_peak", || crate::memory::peak() as u64)
		.build()?;
	module
		.function("test_reset_allocation_peak", crate::memory::reset_peak)
		.build()?;
	context.install(module)?;
	Ok([
		(
			"test_shutdown_task",
			"test_shutdown_task().await: inject a task that prevents clean shutdown",
		),
		(
			"test_pending",
			"test_pending(ms): pending for ms milliseconds, then ms",
		),
		(
			"test_allocation_peak",
			"test_allocation_peak(): allocator peak",
		),
		(
			"test_reset_allocation_peak",
			"test_reset_allocation_peak(): reset allocator peak",
		),
	]
	.into_iter()
	.map(|(name, doc)| crate::host::HostFunction {
		path: format!("rnx_test::{name}"),
		doc,
	})
	.collect())
}
