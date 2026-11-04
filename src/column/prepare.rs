use super::*;
use rune::{Context, Diagnostics, Source, SourceId, Vm};

impl Kernel {
	/// Compile a named, synchronous function through Rune's ordinary frontend.
	/// Preparation executes no script instructions, even for refused graphs.
	#[allow(clippy::result_large_err)]
	pub fn prepare(
		source: &str,
		entry: &str,
		inputs: &[ColumnType],
		output: ColumnType,
		limits: Limits,
	) -> Result<Self, Error> {
		let fail = |cause| Error {
			row: None,
			remaining_work: 0,
			cause,
		};
		if inputs.is_empty() || inputs.len() > 3 {
			return Err(fail(Cause::Invalid("one to three inputs required")));
		}
		// Parse the identifier with Rune's grammar. This validates the public
		// surface only; graph admission never recognizes source expressions.
		rune::parse::parse_all::<rune::ast::Ident>(entry, SourceId::EMPTY, false)
			.map_err(|_| fail(Cause::Invalid("entry must be a top-level function name")))?;
		let room = limits
			.bytes
			.checked_sub(size_of::<Self>())
			.ok_or_else(|| fail(Cause::Memory))?;
		let (scalar, prepared, refusal, owned) = rune::alloc::limit::with(room, || {
			let (scalar, prepared, refusal) = compile(source, entry, inputs, output, limits)?;
			// All temporary Context/compiler objects have dropped. This is the
			// capacity actually retained through Rune's allocation API, including
			// shared code/runtime/source owners, not a source-length estimate.
			let owned = room
				.checked_sub(rune::alloc::limit::get())
				.ok_or(Cause::Memory)?;
			Ok::<_, Cause>((scalar, prepared, refusal, owned))
		})
		.call()
		.map_err(fail)?;
		let retained_bytes = size_of::<Self>()
			.checked_add(owned)
			.and_then(|n| {
				n.checked_add(
					prepared
						.as_ref()
						.map_or(0, core::PreparedColumn::retained_bytes),
				)
			})
			.filter(|n| *n <= limits.bytes)
			.ok_or_else(|| fail(Cause::Memory))?;
		let mut types = [ColumnType::I64; 3];
		types[..inputs.len()].copy_from_slice(inputs);
		Ok(Self {
			inputs: types,
			arity: inputs.len(),
			output,
			scalar,
			prepared,
			refusal,
			limits,
			retained_bytes,
			last: Observation::default(),
		})
	}
}

fn compile(
	source: &str,
	entry: &str,
	inputs: &[ColumnType],
	output: ColumnType,
	limits: Limits,
) -> Result<(Scalar, Option<core::PreparedColumn>, Option<ColumnRefusal>), Cause> {
	let context = Context::with_default_modules().map_err(diagnostic)?;
	let mut sources = Sources::new();
	sources
		.insert(Source::new("column.rn", source).map_err(diagnostic)?)
		.map_err(diagnostic)?;
	let mut diagnostics = Diagnostics::new();
	let unit = rune::prepare(&mut sources)
		.with_context(&context)
		.with_diagnostics(&mut diagnostics)
		.build()
		.map_err(|error| Cause::Compile(format!("{error}: {diagnostics:?}")))?;
	let unit = Arc::try_new(unit).map_err(diagnostic)?;
	let runtime = Arc::try_new(context.runtime().map_err(diagnostic)?).map_err(diagnostic)?;
	let hash = Hash::type_hash([entry]);
	// execute performs entry/arity setup only. Dropping its execution without
	// resuming cannot run a source instruction or an effect.
	{
		let mut vm = Vm::new(runtime.clone(), unit.clone());
		match inputs.len() {
			1 => {
				vm.execute(hash, (0i64,)).map_err(Cause::Stock)?;
			}
			2 => {
				vm.execute(hash, (0i64, 0i64)).map_err(Cause::Stock)?;
			}
			3 => {
				vm.execute(hash, (0i64, 0i64, 0i64)).map_err(Cause::Stock)?;
			}
			_ => unreachable!(),
		}
	}
	let mut diagnostics = Diagnostics::new();
	let refusal = match rune::experimental_core::frontend::compile(
		&mut sources,
		&context,
		&mut diagnostics,
		0,
	) {
		Ok(program) => {
			if let Some(function) = program.function(hash) {
				match core::PreparedColumn::prepare(&program, function, inputs, output, limits) {
					Ok(prepared) => {
						let charge = prepared.row_charge();
						return Ok((
							Scalar::Core {
								program,
								function,
								charge,
							},
							Some(prepared),
							None,
						));
					}
					Err(reason) => reason,
				}
			} else {
				ColumnRefusal::Unsupported
			}
		}
		Err(_) => ColumnRefusal::Unsupported,
	};
	Ok((
		Scalar::Stock {
			runtime,
			unit,
			sources,
			hash,
		},
		None,
		Some(refusal),
	))
}

fn diagnostic(error: impl std::fmt::Display) -> Cause {
	Cause::Compile(error.to_string())
}
