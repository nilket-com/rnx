//! A compiled caller is shared by the two execution modes. Its values and
//! instruction allowance belong to one invocation.
use super::Result;
use crate::{execute, lifecycle::Lifecycle};
use rune::experimental_core::{
	execute::Executor,
	frontend::{self, FileEntry, Program},
	heap::{FormatLimits, Limits, Store},
	value::Type,
};
use rune::sync::Arc;
use rune::{Context, Diagnostics, Hash, Source, Sources, Vm};

fn sources(text: &str) -> Result<Sources> {
	let mut sources = Sources::new();
	sources.insert(Source::new("host-path.rn", text)?)?;
	Ok(sources)
}

pub(super) struct Engines {
	pub context: Context,
	stock: Arc<rune::runtime::RuntimeContext>,
	fresh: rune::host::ExecutionRuntime,
	lifecycle: Lifecycle,
	driver: execute::Runtime,
	_http: crate::http::State,
}

impl Engines {
	pub fn new() -> Result<Self> {
		let lifecycle = Lifecycle::new(true)?;
		let (mut context, _, http) = crate::serving_context(&lifecycle, true)?;
		crate::env::install(&mut context, std::sync::Arc::from([]))?;
		Ok(Self {
			stock: Arc::try_new(context.runtime()?)?,
			fresh: context.execution_runtime()?,
			context,
			lifecycle,
			driver: execute::Runtime::new()?,
			_http: http,
		})
	}
	pub fn compile(&self, text: &str) -> Result<Option<(Program, FileEntry)>> {
		match frontend::compile_file(
			&mut sources(text)?,
			&self.context,
			&mut Diagnostics::new(),
			1,
		) {
			Ok((program, mode))
				if crate::engine::legacy_alias_entry_refusal(&program, text).is_none() =>
			{
				Ok(Some((program, mode)))
			}
			Ok(_) => Ok(None),
			Err(error) if error.refusal.is_some() => Ok(None),
			Err(error) => Err(format!("host probe compilation failed: {error:?}: {text}").into()),
		}
	}

	pub fn stock(&mut self, text: &str, fuel: usize) -> Result<bool> {
		let mut source = sources(text)?;
		let unit = match rune::prepare(&mut source)
			.with_context(&self.context)
			.build()
		{
			Ok(unit) => unit,
			Err(_) => return Ok(false),
		};
		let mut vm = Vm::new(self.stock.clone(), Arc::try_new(unit)?);
		self.lifecycle.begin()?;
		let result = execute::drive_async(
			&self.driver,
			&mut vm,
			["main"],
			(rune::to_value(Vec::<String>::new())?,),
			fuel,
			execute::WhenInterrupted::Finish,
		);
		let ok = matches!(result, execute::Outcome::Complete(_));
		self.lifecycle.finish(!ok)?;
		Ok(ok)
	}
	pub fn execution(
		&mut self,
		program: &Program,
		mode: FileEntry,
		native: bool,
		fuel: usize,
	) -> Result<bool> {
		use super::super::census_execution::opaque_error;
		let mut store = Store::new(Limits::default()).map_err(opaque_error)?;
		let mut executor = Executor::new().map_err(opaque_error)?;
		executor
			.set_host_runtime(Some(self.fresh.clone()))
			.map_err(opaque_error)?;
		executor.set_native_enabled(native);
		executor.set_format_limits(FormatLimits::permissive());
		let argv = store.root_list(Type::Vec, &[]).map_err(opaque_error)?;
		let main = program
			.function(Hash::type_hash(["main"]))
			.ok_or("missing main")?;
		let args = [&argv];
		let prepared = executor
			.prepare(program, &mut store, main, &args)
			.map_err(opaque_error)?;
		self.lifecycle.begin()?;
		let result = match mode {
			FileEntry::Sync => self.driver.run_sync(|| prepared.invoke(fuel)),
			FileEntry::Async => self.driver.with_host_wait(|| prepared.invoke(fuel))?,
		};
		let ok = result.is_ok();
		self.lifecycle.finish(!ok)?;
		if let Ok(root) = result {
			store.root_release(root).map_err(opaque_error)?;
		}
		store.root_release(argv).map_err(opaque_error)?;
		store.close().map_err(opaque_error)?;
		Ok(ok)
	}
}
