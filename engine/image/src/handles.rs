//! ABI v2 uses the generation and slot together as a thread-affine capability.
use super::provider;
pub use super::provider::Provider;
use rnx::rune;
use rune::experimental_core::{
	domain::{
		StartError,
		file::{CheckedMain, FileModules, FileOutcome, FileProgram, FileRun, Installer},
	},
	execute::PrepareError,
	heap::{Limits, ViewKind, ViewLimits},
	value::Scalar,
};
use rune::{Context, Diagnostics};
use std::{
	cell::Cell,
	ffi::OsStr,
	os::unix::ffi::OsStrExt,
	path::Path,
	ptr, slice,
	sync::atomic::{AtomicU64, Ordering},
};

const OPEN: u32 = 1;
const COMPILE: u32 = 2;
const PREPARE: u32 = 3;
const DRIVE: u32 = 4;
const REPORT: u32 = 5;
const DISCARD_PROGRAM: u32 = 6;
const RELEASE_INVOCATION: u32 = 7;
const CLOSE: u32 = 8;

const INVALID: i32 = 1;
const REFUSED: i32 = 2;
const FAILED: i32 = 3;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
#[repr(C)]
pub struct Handle {
	slot: usize,
	generation: u64,
}
#[repr(C)]
pub struct Argument {
	data: *const u8,
	len: usize,
}
#[repr(C)]
pub struct Request {
	version: u32,
	size: u32,
	operation: u32,
	flags: u32,
	context: Handle,
	child: Handle,
	provider: *const Provider,
	path: *const u8,
	path_len: usize,
	argv: *const Argument,
	argc: usize,
	bytes: usize,
	fuel: usize,
	output: *mut Response,
	buffer: *mut u8,
	capacity: usize,
}
#[repr(C)]
#[derive(Default)]
pub struct Response {
	handle: Handle,
	scalar: i64,
	written: usize,
	needed: usize,
}

const _: () = assert!(size_of::<Request>() == 128 && size_of::<Response>() == 40);

#[derive(Clone, Copy)]
enum Owner {
	Probe(Provider),
	Adapters(crate::adapters::Callbacks),
}
struct Sealed {
	#[cfg(feature = "image-controls")]
	effects: Option<ptr::NonNull<crate::provider_abi::Effects>>,
	context: Context,
	files: FileModules,
	provider: Owner,
}
enum Object {
	Context(Box<Sealed>),
	Program(Box<CheckedMain>),
	Invocation {
		file: Box<FileRun>,
		terminal: Option<Verdict>,
	},
}
#[derive(Default)]
struct Slot {
	generation: u64,
	parent: Handle,
	object: Option<Object>,
}
#[derive(Default)]
struct Registry(Vec<Slot>);
thread_local! { static TABLE: Cell<Registry> = const { Cell::new(Registry(Vec::new())) }; }
static NEXT: AtomicU64 = AtomicU64::new(1);
type Error = (i32, String);
type Result<T> = std::result::Result<T, Error>;
fn invalid() -> Error {
	(INVALID, String::new())
}
fn failed(e: impl std::fmt::Debug) -> Error {
	(FAILED, format!("{e:?}"))
}

impl Registry {
	fn reserve(&mut self, parent: Handle) -> Result<Handle> {
		let generation = NEXT
			.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
			.map_err(failed)?;
		let index = match self.0.iter().position(|s| s.object.is_none()) {
			Some(index) => index,
			None => {
				self.0.try_reserve(1).map_err(failed)?;
				self.0.push(Slot::default());
				self.0.len() - 1
			}
		};
		self.0[index] = Slot {
			generation,
			parent,
			object: None,
		};
		Ok(Handle {
			slot: index,
			generation,
		})
	}
	fn slot(&mut self, handle: Handle, parent: Handle) -> Result<&mut Option<Object>> {
		let slot = self
			.0
			.get_mut(handle.slot)
			.filter(|s| {
				s.generation == handle.generation && s.parent == parent && s.object.is_some()
			})
			.ok_or_else(invalid)?;
		Ok(&mut slot.object)
	}
	fn context(&mut self, token: Handle) -> Result<&Sealed> {
		match self.slot(token, Handle::default())? {
			Some(Object::Context(c)) => Ok(c),
			_ => Err(invalid()),
		}
	}
}

struct Verdict {
	status: i32,
	scalar: i64,
	diagnostic: String,
}
fn drive(file: &mut FileRun, fuel: usize) -> Result<i64> {
	match file.drive(fuel, |_| true, || false) {
		Ok(FileOutcome::Complete) => {
			let view = file
				.result(ViewLimits { nodes: 1, depth: 0 })
				.map_err(failed)?;
			match view.root().and_then(|v| v.kind()).map_err(failed)? {
				ViewKind::Scalar(Scalar::I64(n)) => Ok(n),
				_ => Err((FAILED, "checked file did not return an integer".into())),
			}
		}
		Err(StartError::Prepare(PrepareError::Refused(why))) => Err((REFUSED, why.name().into())),
		Ok(FileOutcome::Failed { message, .. }) => Err((FAILED, message.into_std())),
		Ok(FileOutcome::Interrupted) => Err((FAILED, "interrupted".into())),
		Err(e) => Err(failed(e)),
	}
}

impl Request {
	fn arguments(&self) -> Result<Vec<String>> {
		if self.argc == 0 {
			return Ok(Vec::new());
		}
		if self.argv.is_null()
			|| !self.argv.is_aligned()
			|| self.argc > isize::MAX as usize / size_of::<Argument>()
		{
			return Err(invalid());
		}
		let mut args = Vec::new();
		args.try_reserve_exact(self.argc).map_err(failed)?;
		for arg in unsafe { slice::from_raw_parts(self.argv, self.argc) } {
			if arg.len > isize::MAX as usize || (arg.len != 0 && arg.data.is_null()) {
				return Err(invalid());
			}
			let bytes = if arg.len == 0 {
				&[]
			} else {
				unsafe { slice::from_raw_parts(arg.data, arg.len) }
			};
			args.push(
				std::str::from_utf8(bytes)
					.map_err(|_| invalid())?
					.to_owned(),
			);
		}
		Ok(args)
	}
	fn execute(&self, registry: &mut Registry, out: &mut Response) -> Result<()> {
		let root = Handle::default();
		if self.operation == OPEN {
			if self.context != root || self.child != root || self.provider.is_null() {
				return Err(invalid());
			}
			let (provider, module) = if self.version == 3 {
				let (owner, module) = unsafe { crate::adapters::install(self.provider.cast()) }
					.map_err(|e| (REFUSED, e))?;
				(Owner::Adapters(owner), module)
			} else {
				let provider = unsafe { *self.provider };
				if provider.create.is_none()
					|| provider.read.is_none()
					|| provider.release.is_none()
				{
					return Err(invalid());
				}
				(
					Owner::Probe(provider),
					vec![provider::module().map_err(failed)?],
				)
			};
			let mut context = Context::with_default_modules().map_err(failed)?;
			let mut installer = Installer::new(&mut context);
			for module in module {
				installer.install(module).map_err(failed)?;
			}

			let files = installer.finish().map_err(failed)?;
			rune::experimental_core::domain::file::initialize_image_thread();
			let token = registry.reserve(root)?;
			registry.0[token.slot].object = Some(Object::Context(Box::new(Sealed {
				#[cfg(feature = "image-controls")]
				effects: crate::OPEN_EFFECTS.get(),
				context,
				files,
				provider,
			})));
			out.handle = token;
			return Ok(());
		}
		let sealed = registry.context(self.context)?;
		if matches!(sealed.provider, Owner::Adapters(_)) != (self.version == 3) {
			return Err(invalid());
		}
		match self.operation {
			COMPILE => {
				if self.child != root
					|| self.path.is_null()
					|| self.path_len == 0
					|| self.path_len > isize::MAX as usize
				{
					return Err(invalid());
				}
				let path = unsafe { slice::from_raw_parts(self.path, self.path_len) };
				if path.contains(&0) {
					return Err(invalid());
				}
				let (mut sources, mut loader) =
					rnx::image_file::checked_sources(Path::new(OsStr::from_bytes(path)))
						.map_err(|e| (FAILED, e))?;
				let compiled = sealed
					.files
					.compile(
						&mut sources,
						&sealed.context,
						&mut Diagnostics::new(),
						1,
						&mut loader,
					)
					.map_err(|e| match e.refusal {
						Some(why) if self.version == 3 && self.flags == 1 => {
							let detail = match e.location {
								Some(loc) => format!(
									"{} source={} revision={} bytes={}..{}",
									why.name(),
									loc.source,
									loc.revision,
									loc.start,
									loc.end
								),
								None => format!("{} unlocated", why.name()),
							};
							(REFUSED, detail)
						}
						Some(why) => (REFUSED, why.name().into()),
						None => failed(e),
					})?;
				let FileProgram::Checked(program) = compiled else {
					return Err((REFUSED, "requires checked file main".into()));
				};
				let token = registry.reserve(self.context)?;
				registry.0[token.slot].object = Some(Object::Program(Box::new(program)));
				out.handle = token;
			}
			PREPARE => {
				if !matches!(
					registry.slot(self.child, self.context)?,
					Some(Object::Program(_))
				) {
					return Err(invalid());
				}
				let args = self.arguments()?;
				let token = registry.reserve(self.context)?;
				let Some(Object::Program(program)) = registry.0[self.child.slot].object.take()
				else {
					return Err(invalid());
				};
				let limits = Limits {
					bytes: self.bytes,
					..Limits::default()
				};
				let file = program
					.prepare(&args, limits, self.flags != 0)
					.map_err(failed)?;
				registry.0[token.slot].object = Some(Object::Invocation {
					file: Box::new(file),
					terminal: None,
				});
				out.handle = token;
			}
			DRIVE | REPORT => {
				let provider = sealed.provider;
				let Some(Object::Invocation { file, terminal }) =
					registry.slot(self.child, self.context)?
				else {
					return Err(invalid());
				};
				if self.operation == DRIVE {
					if self.fuel == 0 || self.fuel == usize::MAX || terminal.is_some() {
						return Err(invalid());
					}
					// Mark terminal before any effect, including on Rust panic.
					*terminal = Some(Verdict {
						status: FAILED,
						scalar: 0,
						diagnostic: "engine panicked".into(),
					});
					let result = match provider {
						Owner::Probe(p) => provider::during(p, || drive(file, self.fuel)),
						Owner::Adapters(p) => p.during(|| {
							let outcome = file.drive(self.fuel, |_| true, || false);
							if let Some(e) = crate::adapters::take_fault() {
								return Err(failed(e));
							}
							let outcome = outcome.map_err(|e| match e {
								StartError::Prepare(PrepareError::Refused(why)) => {
									(REFUSED, why.name().into())
								}
								other => failed(other),
							})?;
							let (code, report) =
								rnx::image_file::checked_result(file, outcome, self.fuel);
							if code == 0 {
								Ok(0)
							} else {
								Err((FAILED, String::from_utf8_lossy(report.bytes()).into_owned()))
							}
						}),
					};

					*terminal = Some(match result {
						Ok(scalar) => Verdict {
							status: 0,
							scalar,
							diagnostic: String::new(),
						},
						Err((status, diagnostic)) => Verdict {
							status,
							scalar: 0,
							diagnostic,
						},
					});
				}
				let verdict = terminal.as_ref().ok_or_else(invalid)?;
				out.scalar = verdict.scalar;
				if verdict.status != 0 {
					return Err((verdict.status, verdict.diagnostic.clone()));
				}
			}
			DISCARD_PROGRAM | RELEASE_INVOCATION => {
				let object = registry.slot(self.child, self.context)?;
				match (self.operation, &object) {
					(DISCARD_PROGRAM, Some(Object::Program(_)))
					| (RELEASE_INVOCATION, Some(Object::Invocation { .. })) => {}
					_ => return Err(invalid()),
				}
				if let Some(Object::Invocation { file, .. }) = object.take() {
					file.close().map_err(failed)?;
				}
			}
			CLOSE => {
				if self.child != root
					|| registry
						.0
						.iter()
						.any(|s| s.parent == self.context && s.object.is_some())
				{
					return Err(invalid());
				}
				registry.slot(self.context, root)?.take();
			}
			_ => return Err(invalid()),
		}
		Ok(())
	}
}

/// Dispatch a validated, non-reentrant call from the image entry.
///
/// # Safety
/// The caller satisfies the memory, thread, and lifetime contract in engine_image.h.
pub unsafe fn dispatch(p: *const Request, version: u32) -> i32 {
	if p.is_null() || !p.is_aligned() {
		return INVALID;
	}
	let header = p.cast::<u32>();
	if unsafe { header.read() } != version
		|| (unsafe { header.add(1).read() } as usize) < size_of::<Request>()
	{
		return INVALID;
	}
	let r = unsafe { &*p };
	if r.output.is_null()
		|| !r.output.is_aligned()
		|| r.capacity > isize::MAX as usize
		|| (r.capacity != 0 && r.buffer.is_null())
		|| r.flags > u32::from(r.operation == PREPARE || (version == 3 && r.operation == COMPILE))
		|| r.bytes > isize::MAX as usize
		|| (!r.provider.is_null() && (!r.provider.is_aligned() || r.operation != OPEN))
		|| (r.operation != COMPILE && (!r.path.is_null() || r.path_len != 0))
		|| (r.operation != PREPARE && (r.bytes != 0 || r.argc != 0 || !r.argv.is_null()))
		|| (r.operation != DRIVE && r.fuel != 0)
	{
		return INVALID;
	}
	struct Restore(Registry);
	impl Drop for Restore {
		fn drop(&mut self) {
			TABLE.set(std::mem::take(&mut self.0));
		}
	}
	let mut registry = Restore(TABLE.take());
	#[cfg(feature = "image-controls")]
	let effects = match (crate::OPEN_EFFECTS.get(), r.operation) {
		(opening, OPEN) => opening,
		(Some(_), _) => return INVALID,
		(None, _) => registry
			.0
			.context(r.context)
			.ok()
			.and_then(|sealed| sealed.effects),
	};
	let mut out = Response::default();
	let status = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
		#[cfg(feature = "image-controls")]
		if let Some(p) = effects {
			// The caller retains this storage until CLOSE has returned.
			let counter = &unsafe { p.as_ref() }.host_bytes;
			return rune::experimental_core::heap::count_image_host_debits(counter, || {
				r.execute(&mut registry.0, &mut out)
			});
		}
		r.execute(&mut registry.0, &mut out)
	}))
	.unwrap_or_else(|_| Err((FAILED, "engine panicked".into())))
	{
		Ok(()) => 0,
		Err((INVALID, _)) => return INVALID,
		Err((status, diagnostic)) => {
			out.needed = diagnostic.len();
			out.written = r.capacity.min(out.needed);
			if out.written != 0 {
				unsafe { ptr::copy_nonoverlapping(diagnostic.as_ptr(), r.buffer, out.written) };
			}
			status
		}
	};
	unsafe { r.output.write(out) };
	status
}
