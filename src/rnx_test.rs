//! Feature-gated fixtures, never part of the production module inventory.
use rune::{Context, Module};

/// A native value with a presenter (record 0068), so the REPL and worker
/// presentation paths can be driven in the test-support binary without an
/// adapter. `Broken` fails with a control-laden error; `Loud` writes far
/// more than any budget.
#[derive(rune::Any)]
#[rune(item = ::rnx_test)]
pub(crate) struct Presented {
	rows: i64,
}
#[derive(rune::Any)]
#[rune(item = ::rnx_test)]
pub(crate) struct Broken;
#[derive(rune::Any)]
#[rune(item = ::rnx_test)]
pub(crate) struct Loud;

/// Registered after the extensions' own presenters, into the same registry.
pub(crate) fn present(presenters: &mut crate::present::Presenters) -> crate::Result<()> {
	presenters.set_owner("rnx_test");
	presenters.register::<Presented>(|value, out| {
		out.push(&format!("Presented with {} rows\n", value.rows));
		Ok(())
	})?;
	presenters.register::<Broken>(|_, _| Err("\u{1b}[31mbroken\u{1b}[0m ".repeat(20_000)))?;
	presenters.register::<Loud>(|_, out| {
		// Pushing past the budget is refused token by token; the caller's
		// text is still bounded and escaped.
		let token = "\u{1b}]0;title\u{7}".repeat(64);
		while out.push(&token) {}
		Ok(())
	})?;
	Ok(())
}

pub(crate) fn install(context: &mut Context) -> crate::Result<Vec<crate::host::HostFunction>> {
	let mut module = Module::with_crate("rnx_test")?;
	module.ty::<Presented>()?;
	module.ty::<Broken>()?;
	module.ty::<Loud>()?;
	module
		.function("test_presented", |rows: i64| Presented { rows })
		.build()?;
	module.function("test_broken", || Broken).build()?;
	module.function("test_loud", || Loud).build()?;
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
		(
			"test_presented",
			"test_presented(rows): a native value whose presenter prints its rows",
		),
		(
			"test_broken",
			"test_broken(): a native value whose presenter fails",
		),
		(
			"test_loud",
			"test_loud(): a native value whose presenter exceeds every budget",
		),
	]
	.into_iter()
	.map(|(name, doc)| crate::host::HostFunction {
		path: format!("rnx_test::{name}"),
		doc,
	})
	.collect())
}
