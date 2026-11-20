//! File-only host registrations and the one-program checked main entry.
use super::*;
use crate::compile::{ContextError, SourceLoader};
use crate::experimental_core::{
	frontend::{FileEntry, SourceRecord},
	heap::{FormatLimits, RootView, ViewLimits},
	value::Type,
};
use crate::function::host::HostOutputKind;
use crate::runtime::FunctionHandler;
use crate::{Hash, Module};

pub(crate) struct Reader {
	pub(crate) hash: Hash,
	pub(crate) original: FunctionHandler,
	pub(crate) session: FunctionHandler,
	pub(crate) file: FunctionHandler,
}

/// Install modules once while holding the exclusive file context. The ordinary
/// Context itself always retains its original and session-safe registrations.
pub struct Installer<'a> {
	context: &'a mut Context,
	readers: Vec<Reader>,
}
impl<'a> Installer<'a> {
	/// Begin after installing the file's standard modules and argv environment.
	pub fn new(context: &'a mut Context) -> Self {
		Self {
			context,
			readers: Vec::new(),
		}
	}
	/// A failed installation cannot contribute a file reader.
	pub fn install(&mut self, mut module: Module) -> core::result::Result<(), ContextError> {
		self.readers.try_reserve(module.file_readers.len())?;
		let readers = core::mem::take(&mut module.file_readers);
		self.context.install(module)?;
		for reader in readers {
			self.readers.try_push(reader)?;
		}
		Ok(())
	}
	/// Seal the exact final context revision after all builders ran.
	pub fn finish(self) -> Result<FileModules> {
		Ok(FileModules {
			epoch: self.context.execution_epoch.snapshot()?,
			readers: self.readers,
		})
	}
}

/// Does not expose file-specific handlers or an unrestricted runtime snapshot.
pub struct FileModules {
	epoch: Epoch,
	readers: Vec<Reader>,
}
/// A compiled ordinary entry or a sealed entry with no transferable program.
pub enum FileProgram {
	/// Ordinary sync or async file entry.
	Ordinary(Program, FileEntry),
	/// Checked synchronous main with sealed invocation authority.
	Checked(CheckedMain),
}
impl FileModules {
	/// Compile a single execution attempt with the actual file loader.
	pub fn compile(
		&self,
		sources: &mut Sources,
		context: &Context,
		diagnostics: &mut Diagnostics,
		revision: u64,
		loader: &mut dyn SourceLoader,
	) -> Result<FileProgram> {
		if context.execution_epoch.snapshot()? != self.epoch {
			return Err(invalid());
		}
		let (mut program, entry, checked) = frontend::compile_with_entry(
			sources,
			context,
			diagnostics,
			revision,
			None,
			Some(FileEntry::Sync),
			!self.readers.is_empty(),
			Some(loader),
		)?;
		let entry = entry.unwrap_or(FileEntry::Sync);
		if !checked {
			return Ok(FileProgram::Ordinary(program, entry));
		}
		let refuse = || Error::refused(super::super::error::Refusal::OpaqueEntry);
		if !matches!(entry, FileEntry::Sync) || program.epoch != Some(self.epoch) {
			return Err(refuse());
		}
		let mut runtime = context.execution_runtime()?;
		let rt = Arc::get_mut(&mut runtime.inner).ok_or_else(invalid)?;
		let mut selected = false;
		for binding in program.code.file_host_bindings_mut() {
			let unary = context.opaque.file_unary.get(&binding.hash);
			if context.opaque.producer_inputs.contains_key(&binding.hash)
				|| (context.opaque.producer_outputs.contains_key(&binding.hash)
					&& unary.is_none()
					&& !binding.signature.file_pair())
			{
				continue;
			}
			match binding.signature.output {
				HostOutputKind::OpaqueResultString => {
					let reader = self
						.readers
						.iter()
						.find(|r| {
							r.hash == binding.hash
								&& binding.handler.same_allocation(&r.session)
								&& context
									.lookup_function(r.hash)
									.is_some_and(|h| h.same_allocation(&r.original))
								&& context.host(r.hash).is_some_and(|b| binding.same(b))
						})
						.ok_or_else(refuse)?;
					if let Some(handler) = unary {
						if !handler.same_allocation(&reader.file)
							|| binding.signature.arity != 1
							|| !binding.signature.shared_producer()
						{
							return Err(refuse());
						}
					} else if binding.signature.arity == 1 {
						let valid = context
							.opaque
							.factories
							.get(&reader.hash)
							.and_then(|ty| context.opaque.types.get(&ty.hash))
							.is_some_and(|r| {
								r.descriptor.display.is_none()
									&& r.witness_key == reader.hash
									&& r.witness.same_allocation(&reader.file)
							});
						if !valid {
							return Err(refuse());
						}
					}
					rt.replace_execution_handler(reader.hash, reader.file.clone())?;
					binding.handler = reader.file.clone();
					selected = true;
				}
				HostOutputKind::OpaqueOwned | HostOutputKind::ScalarResultNative(_) => {
					return Err(refuse())
				}
				_ => {}
			}
		}
		if !selected {
			return Err(refuse());
		}
		let function = program
			.function(Hash::type_hash(["main"]))
			.ok_or_else(refuse)?;
		Ok(FileProgram::Checked(CheckedMain {
			program,
			runtime,
			function,
		}))
	}
}
/// This entry can only start public main with a vector built from plain text.
pub struct CheckedMain {
	program: Program,
	runtime: ExecutionRuntime,
	function: u32,
}
/// A terminal outcome reported through the ordinary file renderer.
pub enum FileOutcome {
	/// The file returned a plain result.
	Complete,
	/// The invocation failed after preparation.
	Failed {
		/// Located engine error.
		error: super::super::error::Error,
		/// Primary diagnostic materialized against the owning member.
		message: String,
		/// Remaining hard fuel.
		remaining: Option<usize>,
	},
	/// The caller canceled at a slice boundary.
	Interrupted,
}
/// One context-bound invocation, with all roots kept private.
pub struct FileRun {
	domain: ProgramDomain,
	key: Option<ProgramKey>,
	function: u32,
	argv: DomainRoot,
	result: Option<DomainRoot>,
}
impl CheckedMain {
	/// Construct the private Store and argument ownership before effects.
	pub fn prepare(
		self,
		argv: &[std::string::String],
		limits: Limits,
		native: bool,
	) -> Result<FileRun> {
		let mut domain = ProgramDomain::new(limits, ExecutorLimits::default(), self.runtime)?;
		domain.executor.set_native_enabled(native);
		domain
			.executor
			.set_format_limits(FormatLimits::permissive());
		let key = ProgramKey {
			domain: domain.id,
			code: self.program.code.id(),
		};
		domain.programs.try_push(Member {
			program: self.program,
			leased: true,
			session: None,
			may_escape: false,
		})?;
		let mut children = Vec::try_with_capacity(argv.len())?;
		for text in argv {
			let slot = domain.reserve_root()?;
			let root = domain.store.root_string(text)?;
			children.try_push(domain.publish(slot, root))?;
		}
		let slot = domain.reserve_root()?;
		let vector = {
			let reg = domain.registry.borrow();
			let mut values = Vec::try_with_capacity(children.len())?;
			for child in &children {
				values.try_push(domain.check(&reg, child)?)?
			}
			domain.store.root_list(Type::Vec, &values)?
		};
		let argv = domain.publish(slot, vector);
		drop(children);
		domain.drain()?;
		Ok(FileRun {
			domain,
			key: Some(key),
			function: self.function,
			argv,
			result: None,
		})
	}
}
impl FileRun {
	/// File and span data without exposing the underlying program.
	pub fn sources(&self) -> &[SourceRecord] {
		self.domain.programs[0].program.sources()
	}
	/// Start once with the caller's full fuel. The ready hook runs only after
	/// preparation, before the first instruction. A false return cancels.
	#[allow(clippy::result_large_err)] // Preserve the typed pre-effect admission failure.
	pub fn drive(
		&mut self,
		fuel: usize,
		ready: impl FnOnce(&[SourceRecord]) -> bool,
		mut interrupted: impl FnMut() -> bool,
	) -> core::result::Result<FileOutcome, StartError> {
		let key = self.key.take().ok_or_else(invalid)?;
		let mut invocation = self
			.domain
			.start(&key, self.function, &[&self.argv], fuel)?;
		if !ready(invocation.programs[0].program.sources()) {
			invocation.cancel();
			return Ok(FileOutcome::Interrupted);
		}
		loop {
			if interrupted() {
				invocation.cancel();
				return Ok(FileOutcome::Interrupted);
			}
			match invocation.resume(core::num::NonZeroUsize::new(1024).unwrap()) {
				CrossOutcome::Limited => {}
				CrossOutcome::Complete(root) => {
					self.result = Some(root);
					return Ok(FileOutcome::Complete);
				}
				CrossOutcome::Failed(f) => {
					let message =
						crate::alloc::fmt::try_format(format_args!("{}", f.primary_message()))
							.map_err(Error::from)?;
					return Ok(FileOutcome::Failed {
						error: f.failure.error,
						message,
						remaining: f.failure.remaining_work,
					});
				}
			}
		}
	}
	/// A read-only result view tied to this invocation.
	pub fn result(&self, limits: ViewLimits) -> Result<RootView<'_>> {
		self.domain
			.view(self.result.as_ref().ok_or_else(invalid)?, limits)
	}
	/// Release every root and the single program, including after failure.
	pub fn close(mut self) -> Result<()> {
		self.domain.close()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate as rune;
	use crate::experimental_core::error::Refusal;

	#[derive(crate::Any)]
	#[rune(item = ::fixture)]
	struct Item;

	#[test]
	fn path_constructor_admits_only_its_sealed_file_handler() {
		use crate::host::OutputCertificate;
		use core::sync::atomic::{AtomicUsize, Ordering};
		static STARTS: AtomicUsize = AtomicUsize::new(0);
		fn load(path: &str) -> core::result::Result<Item, std::string::String> {
			STARTS.fetch_add(1, Ordering::SeqCst);
			assert_eq!(path, "weights");
			Ok(Item)
		}

		let mut context = Context::with_default_modules().unwrap();
		let mut module = Module::with_crate("fixture").unwrap();
		module
			.file_path_constructor("load", load, |_: &Item| 32)
			.unwrap();
		module
			.opaque_method::<Item, _, _>(
				"dims",
				|_: &Item| (3i64, 8i64, 1i64),
				Some(OutputCertificate::I64_TRIPLE),
			)
			.unwrap();
		let mut installer = Installer::new(&mut context);
		installer.install(module).unwrap();
		let mut files = installer.finish().unwrap();
		let compile = |files: &FileModules, source: &str| {
			let mut src = Sources::new();
			src.insert(crate::Source::memory(source).unwrap()).unwrap();
			files.compile(
				&mut src,
				&context,
				&mut Diagnostics::new(),
				1,
				&mut crate::compile::NoopSourceLoader::default(),
			)
		};
		const MAIN: &str = "pub fn main(args){let m=fixture::Item::load(args[0])?; let (i,h,o)=m.dims(); if i != 3 || h != 8 || o != 1 {panic!(\"bad dims\");} 42}";
		let FileProgram::Checked(main) = compile(&files, MAIN).unwrap() else {
			panic!("ordinary file");
		};
		let mut run = main
			.prepare(&["weights".into()], Limits::default(), false)
			.unwrap();
		assert!(matches!(
			run.drive(10000, |_| true, || false).unwrap(),
			FileOutcome::Complete
		));
		run.close().unwrap();
		assert_eq!(STARTS.load(Ordering::SeqCst), 1);
		assert_eq!(
			compile(
				&files,
				"pub fn main(args){let m=fixture::Item::load(args[0])?; let s=`model ${m}`; 42}"
			)
			.err()
			.unwrap()
			.refusal,
			Some(Refusal::OpaqueOperation)
		);
		assert!(compile(&files, "pub fn main(args){fixture::Item::load(args[0])?}").is_err());
		assert!(compile(
			&files,
			"pub fn main(args){let f=fixture::Item::load; f(args[0])}"
		)
		.is_err());
		for source in [
			"pub async fn main(args){let m=fixture::Item::load(args[0])?; m.dims()}",
			"fn main(args){let m=fixture::Item::load(args[0])?; m.dims()}",
			"pub fn main(args){let m=fixture::Item::load(args[0])?; m.load(args[0])}",
		] {
			assert!(compile(&files, source).is_err(), "{source}");
		}
		let mut ordinary = Sources::new();
		ordinary
			.insert(crate::Source::memory(MAIN).unwrap())
			.unwrap();
		assert!(frontend::compile(&mut ordinary, &context, &mut Diagnostics::new(), 1).is_err());
		let mut unsealed = Sources::new();
		unsealed
			.insert(crate::Source::memory(MAIN).unwrap())
			.unwrap();
		assert!(
			frontend::compile_file(&mut unsealed, &context, &mut Diagnostics::new(), 1).is_err()
		);
		let original = files.readers[0].file.clone();
		files.readers[0].file = files.readers[0].session.clone();
		assert_eq!(
			compile(&files, MAIN).err().unwrap().refusal,
			Some(Refusal::OpaqueEntry)
		);
		files.readers[0].file = original;
		files.readers.clear();
		assert!(compile(&files, MAIN).is_err());
		assert_eq!(STARTS.load(Ordering::SeqCst), 1);
	}

	#[test]
	fn file_native_retention_is_charged_once_per_model() {
		use crate::experimental_core::heap::Store;
		use crate::function::host::opaque::Descriptor;
		use crate::runtime::Value;
		use core::sync::atomic::{AtomicUsize, Ordering::SeqCst};
		static DROPS: AtomicUsize = AtomicUsize::new(0);
		#[derive(crate::Any)]
		struct Model;
		impl Drop for Model {
			fn drop(&mut self) {
				DROPS.fetch_add(1, SeqCst);
			}
		}
		let desc = Descriptor::new::<Model>(|_| 256, None).unwrap();
		let import =
			|store: &mut Store| store.host_value(&Value::new(Model).unwrap(), desc.clone());
		let mut measure = Store::new(Limits::default()).unwrap();
		let a = import(&mut measure).unwrap();
		let bytes = measure.payload_bytes();
		assert!(bytes >= 256);
		measure.release(a).unwrap();
		let mut short = Store::new(Limits {
			bytes: bytes - 1,
			..Limits::default()
		})
		.unwrap();
		assert_eq!(
			import(&mut short).unwrap_err().kind,
			super::super::super::error::ErrorKind::StoreCapacity
		);
		assert_eq!((short.live_entries(), short.payload_bytes()), (0, 0));
		let mut store = Store::new(Limits {
			bytes: 2 * bytes,
			..Limits::default()
		})
		.unwrap();
		let a = import(&mut store).unwrap();
		store.retain(a).unwrap();
		let b = import(&mut store).unwrap();
		assert_eq!(store.payload_bytes(), 2 * bytes);
		store.release(a).unwrap();
		assert_eq!(store.payload_bytes(), 2 * bytes);
		store.release(a).unwrap();
		assert_eq!(store.payload_bytes(), bytes);
		let _c = import(&mut store).unwrap();
		store.release(b).unwrap();
		store.close().unwrap();
		assert_eq!(DROPS.load(SeqCst), 5);
	}

	#[test]
	fn file_registration_requires_both_matching_handler_allocations() {
		let mut context = Context::with_default_modules().unwrap();
		let mut module = Module::with_crate("fixture").unwrap();
		module.opaque_type::<Item>(|_| 8, |_, _| Ok(())).unwrap();
		module
			.file_opaque_result_function(
				"read",
				|_, _, _, _| Ok(()),
				|_, _| Ok(Ok(Item)),
				|_, _| Ok(Ok(Item)),
			)
			.unwrap();
		let mut installer = Installer::new(&mut context);
		installer.install(module).unwrap();
		let mut files = installer.finish().unwrap();
		let compile = |files: &FileModules| {
			let mut src = Sources::new();
			src.insert(
				crate::Source::memory("pub fn main(args){let local=fixture::read(args[0],[])?;42}")
					.unwrap(),
			)
			.unwrap();
			files.compile(
				&mut src,
				&context,
				&mut Diagnostics::new(),
				1,
				&mut crate::compile::NoopSourceLoader::default(),
			)
		};
		assert!(matches!(compile(&files), Ok(FileProgram::Checked(_))));
		let reader = &mut files.readers[0];
		let original = reader.original.clone();
		reader.original = reader.file.clone();
		assert_eq!(
			compile(&files).err().unwrap().refusal,
			Some(Refusal::OpaqueEntry)
		);
		let reader = &mut files.readers[0];
		reader.original = original;
		reader.session = reader.file.clone();
		assert_eq!(
			compile(&files).err().unwrap().refusal,
			Some(Refusal::OpaqueEntry)
		);
		files.readers.clear();
		assert_eq!(
			compile(&files).err().unwrap().refusal,
			Some(Refusal::OpaqueEntry)
		);
	}
}

#[cfg(test)]
mod pair_tests;
