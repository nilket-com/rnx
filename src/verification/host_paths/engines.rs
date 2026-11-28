//! A compiled caller is shared by the two execution modes. Its values and
//! instruction allowance belong to one invocation.
use super::{Result, outcome::Outcome};
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

	pub fn stock(&mut self, text: &str, fuel: usize) -> Result<Outcome> {
		let mut source = sources(text)?;
		let unit = match rune::prepare(&mut source)
			.with_context(&self.context)
			.build()
		{
			Ok(unit) => unit,
			Err(_) => return Ok(Outcome::CompileError),
		};
		let unit = Arc::try_new(unit)?;
		let mut vm = Vm::new(self.stock.clone(), unit.clone());
		self.lifecycle.begin()?;
		let result = execute::drive_async(
			&self.driver,
			&mut vm,
			["main"],
			(rune::to_value(Vec::<String>::new())?,),
			fuel,
			execute::WhenInterrupted::Finish,
		);
		self.lifecycle
			.finish(!matches!(result, execute::Outcome::Complete(_)))?;
		Ok(match result {
			execute::Outcome::Complete(_) => Outcome::Complete,
			execute::Outcome::Failed { error, .. } => {
				let offset = error
					.first_location()
					.filter(|l| Arc::ptr_eq(&unit, &l.unit))
					.and_then(|l| l.unit.debug_info()?.instruction_at(l.ip))
					.filter(|i| {
						source
							.get(i.source_id)
							.is_some_and(|s| s.name() == "host-path.rn")
					})
					.map(|i| i.span.range().start);
				Outcome::stock_error(&error, text, offset)
			}
			execute::Outcome::Budget => Outcome::budget(fuel),
			_ => return Err("unexpected host-path stock outcome".into()),
		})
	}
	pub fn execution(
		&mut self,
		program: &Program,
		mode: FileEntry,
		native: bool,
		fuel: usize,
	) -> Result<Outcome> {
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
		self.lifecycle.finish(result.is_err())?;
		let outcome = match result {
			Ok(root) => {
				store.root_release(root).map_err(opaque_error)?;
				Outcome::Complete
			}
			Err(failure) => Outcome::execution_error(&failure, program, fuel),
		};
		store.root_release(argv).map_err(opaque_error)?;
		store.close().map_err(opaque_error)?;
		Ok(outcome)
	}
}
