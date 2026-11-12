//! Persistent synchronous session. No legacy values cross this boundary.
use super::*;
use rune::experimental_core::{
	domain::{CrossOutcome, DomainRoot, ProgramDomain},
	heap::{ViewKind, ViewLimits},
	value::Scalar,
};

pub(crate) struct DomainSession {
	context: Context,
	domain: ProgramDomain,
	state: BTreeMap<String, DomainRoot>,
	declarations: BTreeMap<String, Declaration>,
	names: BTreeSet<String>,
	inputs: Vec<String>,
	numberings: Vec<usize>,
	maps: BTreeMap<u64, SourceMap>,
	last_generated: String,
	ceiling: usize,
	over_ceiling: bool,
	budget: usize,
	http: crate::http::State,
	lifecycle: crate::lifecycle::Lifecycle,
	runtime: crate::execute::Runtime,
	// Kept with the installed context, including across reset. Native values
	// are refused by frontend admission; no presenter is invoked on this route.
	_presenters: crate::present::Presenters,
	cleanup_notes: Vec<String>,
	#[cfg(test)]
	collect_fail_after: Option<usize>,
}
fn fault(e: impl std::fmt::Display) -> Failure {
	Failure::Runtime {
		message: e.to_string(),
		origin: None,
	}
}
impl DomainSession {
	pub(crate) fn new(context: Context, ceiling: usize) -> Result<Self> {
		let runtime = context.execution_runtime()?;
		let domain = ProgramDomain::new(Default::default(), Default::default(), runtime)
			.map_err(|e| format!("{e:?}"))?;
		Ok(Self {
			context,
			domain,
			state: BTreeMap::new(),
			declarations: BTreeMap::new(),
			names: BTreeSet::new(),
			inputs: Vec::new(),
			numberings: Vec::new(),
			maps: BTreeMap::new(),
			last_generated: String::new(),
			ceiling,
			over_ceiling: false,
			budget: BUDGET,
			http: Default::default(),
			lifecycle: Default::default(),
			runtime: crate::execute::Runtime::new()?,
			_presenters: Default::default(),
			cleanup_notes: Vec::new(),
			#[cfg(test)]
			collect_fail_after: None,
		})
	}
	pub(crate) fn with_presenters(mut self, presenters: crate::present::Presenters) -> Self {
		self._presenters = presenters;
		self
	}

	pub(crate) fn with_http(mut self, http: crate::http::State) -> Self {
		self.http = http;
		self
	}
	pub(crate) fn with_lifecycle(mut self, lifecycle: crate::lifecycle::Lifecycle) -> Self {
		self.lifecycle = lifecycle;
		self
	}
	pub(crate) fn lifecycle_failed(&self) -> bool {
		self.lifecycle.failed()
	}
	pub(crate) fn close(&mut self) -> std::result::Result<(), String> {
		let cleanup = self.lifecycle.close();
		self.http.cancel();
		self.state.clear();
		let domain = self.domain.close().map_err(|e| format!("{e:?}"));
		let drained = self.runtime.drain_shutdown();
		cleanup.and(domain).and(drained)
	}
	pub(crate) fn reset(&mut self) {
		if let Err(error) = self.reset_fallible() {
			eprintln!("{error}");
		}
	}
	pub(crate) fn reset_fallible(&mut self) -> std::result::Result<(), String> {
		// Allocate the replacement before retiring the old epoch. Refusal keeps it usable.
		let runtime = self
			.context
			.execution_runtime()
			.map_err(|e| e.to_string())?;
		let replacement = ProgramDomain::new(Default::default(), Default::default(), runtime)
			.map_err(|e| format!("{e:?}"))?;
		let cleanup = self.lifecycle.clear();
		self.http.cancel();
		self.state.clear();
		let closed = self.domain.close().map_err(|e| format!("{e:?}"));
		self.domain = replacement;
		self.declarations.clear();
		self.names.clear();
		self.inputs.clear();
		self.numberings.clear();
		self.maps.clear();
		self.last_generated.clear();
		self.over_ceiling = false;
		cleanup.and(closed)
	}

	pub(crate) fn next_number(&self) -> usize {
		self.inputs.len() - self.numberings.last().copied().unwrap_or(0) + 1
	}
	pub(crate) fn renumber(&mut self) {
		self.numberings.push(self.inputs.len());
	}
	pub(crate) fn last_generated(&self) -> &str {
		&self.last_generated
	}
	pub(crate) fn ceiling(&self) -> usize {
		self.ceiling
	}
	pub(crate) fn over_ceiling(&self) -> bool {
		self.over_ceiling
	}
	pub(crate) fn sample(&mut self) -> Option<usize> {
		let live = crate::memory::live()?;
		if live >= self.ceiling {
			self.over_ceiling = true;
		}
		Some(live)
	}
	pub(crate) fn retained_bytes(&self) -> usize {
		self.inputs.iter().map(String::len).sum::<usize>()
			+ self
				.declarations
				.values()
				.map(|d| d.source.len())
				.sum::<usize>()
			+ self.maps.values().map(SourceMap::bytes).sum::<usize>()
	}
	pub(crate) fn retained_units(&self) -> usize {
		self.domain.live_counts().0
	}
	pub(crate) fn binding_names(&self) -> Vec<String> {
		self.names.iter().cloned().collect()
	}
	pub(crate) fn declaration_names(&self) -> Vec<String> {
		self.declarations.keys().cloned().collect()
	}
	pub(crate) fn declaration(&self, name: &str) -> Option<(&'static str, &str)> {
		self.declarations
			.get(name)
			.map(|d| (d.kind, d.source.as_str()))
	}
	pub(crate) fn fields(&self) -> crate::declared::Fields {
		let mut fields = crate::declared::Fields::default();
		for d in self.declarations.values().filter(|d| d.is_type) {
			crate::declared::into_fields(&d.source, &mut fields);
		}
		fields
	}
	pub(crate) fn binding_count(&self) -> usize {
		self.names.len()
	}
	pub(crate) fn visit_bindings(&self, mut visit: impl FnMut(&str, &DomainRoot) -> bool) -> usize {
		let mut n = 0;
		for (name, root) in &self.state {
			n += 1;
			if !visit(name, root) {
				break;
			}
		}
		n
	}
	pub(crate) fn binding(&self, name: &str) -> Option<&DomainRoot> {
		self.state.get(name)
	}
	pub(crate) fn type_of(&self, root: &DomainRoot) -> std::result::Result<String, String> {
		let view = self
			.domain
			.view(root, ViewLimits { nodes: 1, depth: 1 })
			.map_err(|e| format!("{e:?}"))?;
		Ok(crate::format::type_name(
			view.root()
				.and_then(|v| v.type_name())
				.map_err(|e| format!("{e:?}"))?
				.to_string(),
		))
	}
	pub(crate) fn document(
		&self,
		root: &DomainRoot,
		limits: &crate::format::Limits,
	) -> std::result::Result<(String, Vec<crate::presentation::Span>), String> {
		let view = self
			.domain
			.view(
				root,
				ViewLimits {
					nodes: usize::MAX,
					depth: usize::MAX,
				},
			)
			.map_err(|e| format!("{e:?}"))?;
		crate::format::execution::document(
			view.root().map_err(|e| format!("{e:?}"))?,
			Some(&self.fields()),
			limits,
			crate::presentation::stdout(),
		)
	}
	pub(crate) fn eval_rendered(
		&mut self,
		input: &str,
		limits: &crate::format::Limits,
	) -> std::result::Result<Option<String>, Failure> {
		let root = self.eval(input)?;
		self.render_root(root, limits)
	}
	pub(crate) fn eval_rendered_with_armed(
		&mut self,
		input: &str,
		limits: &crate::format::Limits,
		armed: &mut dyn FnMut(usize) -> std::io::Result<()>,
	) -> std::io::Result<std::result::Result<Option<String>, Failure>> {
		let result = self.eval_interruptible_armed(input, crate::host::interrupted, armed)?;
		Ok(result.and_then(|root| self.render_root(root, limits)))
	}
	fn render_root(
		&mut self,
		root: DomainRoot,
		limits: &crate::format::Limits,
	) -> std::result::Result<Option<String>, Failure> {
		let rendered = (|| {
			let view = self
				.domain
				.view(
					&root,
					ViewLimits {
						nodes: usize::MAX,
						depth: usize::MAX,
					},
				)
				.map_err(|e| fault(format!("{e:?}")))?;
			let value = view.root().map_err(|e| fault(format!("{e:?}")))?;
			if matches!(
				value.kind().map_err(|e| fault(format!("{e:?}")))?,
				ViewKind::Scalar(Scalar::Unit)
			) {
				return Ok(None);
			}
			crate::format::execution::render(
				value,
				Some(&self.fields()),
				limits,
				crate::presentation::stdout(),
			)
			.map(Some)
			.map_err(fault)
		})();
		drop(root);
		let collected = self.collect();
		self.after_cleanup(rendered, collected)
	}

	// Preserve the primary failure category and origin; the REPL prints these
	// secondary notes only after reporting that failure.
	fn after_cleanup<T>(
		&mut self,
		result: std::result::Result<T, Failure>,
		cleanup: std::result::Result<(), Failure>,
	) -> std::result::Result<T, Failure> {
		match (result, cleanup) {
			(Err(primary), Err(secondary)) => {
				self.cleanup_notes.push(secondary.to_string());
				Err(primary)
			}
			(Ok(_), Err(secondary)) => Err(secondary),
			(result, Ok(())) => result,
		}
	}
	pub(crate) fn take_cleanup_notes(&mut self) -> Vec<String> {
		std::mem::take(&mut self.cleanup_notes)
	}
	pub(crate) fn report_cleanup(&mut self) {
		for note in self.take_cleanup_notes() {
			eprintln!("cleanup error: {}", crate::format::terminal_safe(&note));
		}
	}

	fn collect(&mut self) -> std::result::Result<(), Failure> {
		#[cfg(test)]
		if let Some(remaining) = self.collect_fail_after.as_mut() {
			if *remaining == 0 {
				self.collect_fail_after = None;
				return Err(fault("injected program collection failure"));
			}
			*remaining -= 1;
		}
		self.domain
			.collect_programs()
			.map_err(|e| fault(format!("program collection: {e:?}")))?;
		let retained = self
			.domain
			.retained_revisions()
			.map_err(|e| fault(format!("program collection: {e:?}")))?;
		self.maps.retain(|revision, _| retained.contains(revision));
		Ok(())
	}
	pub(crate) fn eval(&mut self, input: &str) -> std::result::Result<DomainRoot, Failure> {
		self.eval_interruptible(input, crate::host::interrupted)
	}
	fn eval_interruptible(
		&mut self,
		input: &str,
		interrupted: impl FnMut() -> bool,
	) -> std::result::Result<DomainRoot, Failure> {
		self.eval_interruptible_armed(input, interrupted, &mut |_| Ok(()))
			.expect("infallible arming")
	}
	fn eval_interruptible_armed(
		&mut self,
		input: &str,
		mut interrupted: impl FnMut() -> bool,
		armed: &mut dyn FnMut(usize) -> std::io::Result<()>,
	) -> std::io::Result<std::result::Result<DomainRoot, Failure>> {
		let admission = (|| {
			self.collect()?;
			if self.over_ceiling {
				return Err(Failure::OverCeiling {
					live: super::super::memory::live().unwrap_or(0),
					ceiling: self.ceiling,
				});
			}
			if input.len() > INPUT_CAP {
				return Err(Failure::Refused(format!("input exceeds {INPUT_CAP} bytes")));
			}
			Ok(())
		})();
		if let Err(error) = admission {
			return Ok(Err(error));
		}
		super::super::host::clear_interrupt();
		self.inputs.push(input.to_owned());
		let number = self.inputs.len();
		// Same boundary as legacy: a numbered input is armed before compilation.
		// Transport failure has no compiled entry, lifecycle begin or script effect.
		armed(number)?;
		let result = self.eval_inner(input, number, &mut interrupted);
		let finished = self.lifecycle.finish(result.is_err()).map_err(fault);
		if finished.is_err() {
			self.http.cancel();
		}
		let result = self.after_cleanup(result, finished);
		let collected = self.collect();
		Ok(self.after_cleanup(result, collected))
	}

	fn eval_inner(
		&mut self,
		input: &str,
		number: usize,
		interrupted: &mut impl FnMut() -> bool,
	) -> std::result::Result<DomainRoot, Failure> {
		let p = prepare_input(
			input,
			number,
			&self.declarations,
			&self.names,
			|message, at| Failure::Compile {
				message,
				origin: at.and_then(|(i, o)| self.origin(i, o)),
			},
			|message, i, o| self.refused_at(message, i, o),
		)?;
		let generated = p.generate(false);
		self.last_generated = generated.text.clone();
		if crate::engine::requires_loader(&generated.text) {
			return Err(Failure::Refused(
				"execution session does not support source loading".into(),
			));
		}
		let mut sources = Sources::new();
		sources
			.insert(Source::memory(&generated.text).map_err(|e| fault(e.to_string()))?)
			.map_err(|e| fault(e.to_string()))?;
		let mut diagnostics = rune::Diagnostics::new();
		let key = match self.domain.compile_insert(
			&self.context,
			&mut sources,
			&mut diagnostics,
			number as u64,
		) {
			Ok(k) => k,
			Err(e) => {
				use rune::experimental_core::error::Refusal;
				let first = diagnostics.diagnostics().iter().find_map(|d| match d {
					rune::diagnostics::Diagnostic::Fatal(f) => f
						.as_compile_error()
						.map(|e| (e.to_string(), e.span().range().start)),
					_ => None,
				});
				let at = first
					.as_ref()
					.map(|(_, at)| *at)
					.or(e.location.map(|l| l.start as usize));
				let at = at
					.and_then(|at| generated.map.locate(at))
					.unwrap_or((number, 0));
				if let Some(construct) = diagnostics.diagnostics().iter().find_map(|d| match d {
					rune::diagnostics::Diagnostic::Fatal(f) => {
						f.as_compile_error().and_then(|e| e.execution_refusal())
					}
					_ => None,
				}) {
					return Err(self.refused_at(
						&format!(
							"execution session does not support {construct}; start `rnx repl` without --engine=execution"
						),
						at.0,
						at.1,
					));
				}
				if let Some(refusal) = e.refusal {
					if matches!(
						refusal,
						Refusal::AsyncBlock
							| Refusal::AsyncFunction
							| Refusal::Await | Refusal::Select
					) {
						return Err(self.unsupported("async input", at));
					}
					if refusal != Refusal::FrontendDiagnostic {
						if refusal == Refusal::Native {
							let host = diagnostics.diagnostics().iter().find_map(|d| match d {
								rune::diagnostics::Diagnostic::Fatal(f) => f
									.as_compile_error()
									.filter(|c| {
										e.location.is_some_and(|l| {
											c.span().range().start == l.start as usize
										})
									})
									.and_then(|c| c.unenrolled_host()),
								_ => None,
							});
							if let Some(host) = host {
								let name = host.to_string();
								let name = name.trim_start_matches("::");
								return Err(self.refused_at(&format!("{name} needs {}, not yet available in an execution session; start `rnx repl` without --engine=execution", host_wait_reason(name)), at.0, at.1));
							}
						}
						return Err(self.unsupported(refused_construct(refusal), at));
					}
				}
				// Wrapper compilation only establishes legacy admission, not async use.
				// Use the compiler's structured category, never diagnostic prose.
				if diagnostics.diagnostics().iter().any(|d| match d {
					rune::diagnostics::Diagnostic::Fatal(f) => f
						.as_compile_error()
						.is_some_and(|e| e.requires_async_context()),
					_ => false,
				}) {
					return Err(self.unsupported("async input", at));
				}
				let origin = first
					.as_ref()
					.and_then(|(_, o)| generated.map.locate(*o))
					.and_then(|(i, o)| self.origin(i, o));
				return Err(Failure::Compile {
					message: first
						.map(|(m, _)| m)
						.unwrap_or_else(|| format!("{}: {:?}", e.operation, e.kind)),
					origin,
				});
			}
		};
		self.maps.insert(number as u64, generated.map);
		let result = (|| {
			let fields: Vec<_> = p
				.referenced
				.iter()
				.filter_map(|n| self.state.get(n).map(|v| (n.as_str(), v)))
				.collect();
			let restored = self
				.domain
				.record_from_fields(&fields)
				.map_err(|e| fault(format!("{e:?}")))?;
			let entry = self
				.domain
				.entry(&key, rune::Hash::type_hash(["main"]))
				.map_err(|e| fault(format!("{e:?}")))?;
			self.lifecycle.begin().map_err(fault)?;
			let output = {
				let mut invocation = self
					.domain
					.start(&key, entry, &[&restored], self.budget)
					.map_err(|e| fault(format!("{e:?}")))?;
				// At most 1024 fresh work units between interrupt checks in a pure loop.
				// Host calls have their own cancellation contract; no wall-time promise.
				loop {
					if interrupted() {
						let _failure = invocation.cancel();
						break Err(Failure::Interrupted);
					}
					match invocation.resume(core::num::NonZeroUsize::new(1024).unwrap()) {
						CrossOutcome::Limited => {}
						CrossOutcome::Complete(root) => break Ok(root),
						CrossOutcome::Failed(f) => {
							if matches!(
								f.failure.error.kind,
								rune::experimental_core::error::ErrorKind::FuelExhausted
							) {
								break Err(Failure::Budget(self.budget));
							}
							let at = f.failure.error.location;
							let origin = at
								.and_then(|loc| {
									self.maps
										.get(&loc.revision)
										.and_then(|m| m.locate(loc.start as usize))
								})
								.and_then(|(i, o)| origin_in(&self.inputs, &self.numberings, i, o));
							let message =
								frames::render(&f, &self.maps, &self.inputs, &self.numberings);
							break Err(Failure::Runtime { message, origin });
						}
					}
				}
			}?;
			let delta = self
				.domain
				.get(&output, 0)
				.map_err(|e| fault(format!("{e:?}")))?;
			let value = self
				.domain
				.get(&output, 1)
				.map_err(|e| fault(format!("{e:?}")))?;
			let mut staged = Vec::new();
			for name in &p.delta_names {
				staged.push((
					name.clone(),
					self.domain
						.field(&delta, name)
						.map_err(|e| fault(format!("{e:?}")))?,
				));
			}
			for (name, value) in staged {
				self.state.insert(name, value);
			}
			self.declarations = p.declarations;
			self.names = p.names;
			Ok(value)
		})();
		let release = self
			.domain
			.release_program(key)
			.map_err(|e| fault(format!("entry release: {e:?}")));
		self.after_cleanup(result, release)
	}
	fn unsupported(&self, construct: &str, at: (usize, usize)) -> Failure {
		self.refused_at(&format!("{construct} is not available in an execution session; start `rnx repl` without --engine=execution"),at.0,at.1)
	}

	fn origin(&self, input: usize, offset: usize) -> Option<Origin> {
		origin_in(&self.inputs, &self.numberings, input, offset)
	}
	fn refused_at(&self, message: &str, input: usize, offset: usize) -> Failure {
		match self.origin(input, offset) {
			Some(o) => Failure::Refused(format!(
				"{message} (input {}, line {}, column {})",
				o.number, o.line, o.column
			)),
			None => Failure::Refused(message.to_owned()),
		}
	}
}
fn refused_construct(refusal: rune::experimental_core::error::Refusal) -> &'static str {
	use rune::experimental_core::error::Refusal::*;
	match refusal {
		AsyncBlock | AsyncFunction | Await | Select => "async input",
		Arity | CallClosure => "a call with unsupported or unknown closure arity",
		Assignment => "this assignment target",
		ByteStr => "byte-string literals",
		Call => "this indirect function call",
		CodeLimit => "a program exceeding the execution capacity",
		Condition => "this condition form",
		ConstCall | ConstCollection | ConstCycle | ConstFn | UnresolvedConstant => {
			"this constant expression"
		}
		DepthLimit => "expressions exceeding the nesting limit",
		Field | FieldIndex => "this field access",
		Format | FormatParts | FormatSpec => "this formatting expression",
		FrontendDiagnostic => "this source form",
		GeneratorFunction | Yield => "generators and yield",
		Impl => "impl declarations",
		Import => "imports",
		LabelledBlock | LoopControl => "this loop-control form",
		Macro => "this macro",
		Match => "this match expression",
		Method => "this method call",
		Module => "module declarations",
		Native => "native host values or uncertified host calls",
		ObjectConstructor => "this object constructor",
		Operator => "this operator",
		Pattern => "this pattern",
		Range => "this range",
		SourceLimit => "sources exceeding the size limit",
		Static => "static declarations",
		TypeExpr => "this type expression",
		TypeTable => "types exceeding the identity-table capacity",
		UnresolvedFunction => "this unresolved function",
		Variable => "this unresolved variable",
		Entry => "this host entry requiring a receiver proof",
	}
}

fn origin_in(
	inputs: &[String],
	numberings: &[usize],
	input: usize,
	offset: usize,
) -> Option<Origin> {
	let text = inputs.get(input.checked_sub(1)?)?;
	let (line, column, text) = position(text, offset);
	let epoch = numberings.partition_point(|&boundary| boundary < input);
	let base = epoch.checked_sub(1).map_or(0, |i| numberings[i]);
	Some(Origin {
		input,
		number: InputNumber {
			position: input - base,
			older_numbering: (epoch != numberings.len()).then_some(epoch + 1),
		},
		line,
		column,
		text,
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn every_refusal_has_truthful_async_classification() {
		use rune::experimental_core::error::Refusal::*;
		for refusal in [
			Arity,
			Assignment,
			AsyncBlock,
			AsyncFunction,
			Await,
			ByteStr,
			Call,
			CallClosure,
			CodeLimit,
			Condition,
			ConstCall,
			ConstCollection,
			ConstCycle,
			ConstFn,
			DepthLimit,
			Field,
			FieldIndex,
			Format,
			FormatParts,
			FormatSpec,
			FrontendDiagnostic,
			GeneratorFunction,
			Impl,
			Import,
			LabelledBlock,
			LoopControl,
			Macro,
			Match,
			Method,
			Module,
			Native,
			ObjectConstructor,
			Operator,
			Pattern,
			Range,
			Select,
			SourceLimit,
			Static,
			TypeExpr,
			TypeTable,
			UnresolvedConstant,
			UnresolvedFunction,
			Variable,
			Yield,
			Entry,
		] {
			assert_eq!(
				refused_construct(refusal) == "async input",
				matches!(refusal, AsyncBlock | AsyncFunction | Await | Select),
				"{refusal:?}"
			);
		}
	}

	#[test]
	fn constant_data_is_fresh_recursively() {
		for (source, expected) in [
			("const A=[];A", "[]"),
			("const A=[1,\"x\",true];A", "[1, \"x\", true]"),
			("const A=(1,\"x\");A", "(1, \"x\")"),
			(
				"const A=[1,2];let x=A;x.push(3);(x,A)",
				"([1, 2, 3], [1, 2])",
			),
			(
				"const A=[[1]];let x=A;let y=x[0];y.push(2);(x,A)",
				"([[1, 2]], [[1]])",
			),
			(
				"const A=#{a:[1]};let x=A;x.a.push(2);(x,A)",
				"({\"a\": [1, 2]}, {\"a\": [1]})",
			),
			(
				"const A=[1];const B=[A,A];let x=B;x[0].push(2);(x,B)",
				"([[1, 2], [1]], [[1], [1]])",
			),
			("const A=\"x\";let x=A;x.push('y');(x,A)", "(\"xy\", \"x\")"),
			("const A=2;const B=A*3+1;B", "7"),
			("const A=[1,2];const B=A[0];B", "1"),
			("const A={let x=2;x+1};A", "3"),
			("const A=if true {1}else{2};A", "1"),
		] {
			let mut s = DomainSession::new(context(), DEFAULT_CEILING).unwrap();
			assert_eq!(
				s.eval_rendered(source, &crate::format::Limits::default())
					.unwrap(),
				Some(expected.into()),
				"{source}"
			);
		}
	}

	#[test]
	fn constant_deferred_forms_are_not_async() {
		for (source, construct) in [
			("const fn f(x){x+1} const A=f(2);A", "constant functions"),
			("const A=Some(1);A", "option construction"),
			("const A=b\"ab\";A", "byte-string constants"),
		] {
			let mut s = DomainSession::new(context(), DEFAULT_CEILING).unwrap();
			let Err(Failure::Refused(message)) = s.eval(source) else {
				panic!("expected refusal for {source}");
			};
			assert!(message.contains(construct), "{source}: {message}");
			assert!(!message.contains("async"), "{source}: {message}");
		}
	}

	#[test]
	fn constant_type_diagnostic_preserves_old_bindings() {
		let mut s = DomainSession::new(context(), DEFAULT_CEILING).unwrap();
		let limits = crate::format::Limits::default();
		s.eval_rendered("const A=1;const C=A+1;C", &limits).unwrap();
		let Err(Failure::Compile { message, origin }) = s.eval("const A=\"x\";A") else {
			panic!("expected type error");
		};
		assert_eq!(
			message,
			"Expected type `std::string::String` but found `std::i64`"
		);
		assert_eq!(origin.unwrap().input, 1);
		assert_eq!(
			s.eval_rendered("(A,C)", &limits).unwrap(),
			Some("(1, 2)".into())
		);
	}

	#[test]
	fn constant_diagnostic_async_categories_are_truthful() {
		for source in ["async {1}.await", "async fn f(){1} f().await", "async {1}"] {
			let mut s = DomainSession::new(context(), DEFAULT_CEILING).unwrap();
			let Err(Failure::Refused(message)) = s.eval(source) else {
				panic!("expected async refusal: {source}");
			};
			assert!(message.contains("async input"), "{source}: {message}");
		}
		let mut s = DomainSession::new(context(), DEFAULT_CEILING).unwrap();
		assert_eq!(
			s.eval_rendered(
				"const A=\"async await select\";/* async */ A",
				&crate::format::Limits::default()
			)
			.unwrap(),
			Some("\"async await select\"".into())
		);
	}

	#[test]
	fn constant_expansion_bound_is_named_and_transactional() {
		let mut s = DomainSession::new(context(), DEFAULT_CEILING).unwrap();
		let mut source = String::from("const A0=[1];");
		for n in 1..5 {
			let children = vec![format!("A{}", n - 1); 16].join(",");
			source.push_str(&format!("const A{n}=[{children}];"));
		}
		source.push_str("A4");
		let message = match s.eval(&source) {
			Err(Failure::Refused(message)) => message,
			Err(e) => panic!("unexpected failure: {e:?}"),
			Ok(_) => panic!("expected expansion bound"),
		};
		assert!(message.contains("65536"), "{message}");
		assert_eq!(
			s.eval_rendered("7", &crate::format::Limits::default())
				.unwrap(),
			Some("7".into())
		);
	}

	fn context() -> Context {
		let mut c = Context::with_default_modules().unwrap();
		crate::install_core(&mut c).unwrap();
		c
	}
	#[test]
	fn armed_transport_failure_and_prearmed_interrupt_preserve_state() {
		// This ownership/arming test does not need host signal registration.
		let mut s =
			DomainSession::new(Context::with_default_modules().unwrap(), DEFAULT_CEILING).unwrap();
		let limits = crate::format::Limits::default();
		s.eval_rendered("let a=[1];let alias=a;0", &limits).unwrap();
		let before = s.next_number();
		let generated = s.last_generated().to_owned();
		let error = s
			.eval_rendered_with_armed("alias.push(2);9", &limits, &mut |number| {
				assert_eq!(number, before);
				Err(std::io::Error::other("lost armed transport"))
			})
			.unwrap_err();
		assert_eq!(error.to_string(), "lost armed transport");
		assert_eq!(s.last_generated(), generated);
		assert_eq!(
			s.eval_rendered("a.len()", &limits).unwrap(),
			Some("1".into())
		);
		let interrupted = std::cell::Cell::new(false);
		let result = s
			.eval_interruptible_armed("alias.push(2);9", || interrupted.get(), &mut |_| {
				interrupted.set(true);
				Ok(())
			})
			.unwrap();
		assert!(matches!(result, Err(Failure::Interrupted)));
		assert_eq!(
			s.eval_rendered("a.len()", &limits).unwrap(),
			Some("1".into())
		);
		let mut armed = 0;
		let result = s
			.eval_rendered_with_armed("a.push(2);async {1}.await", &limits, &mut |_| {
				armed += 1;
				Ok(())
			})
			.unwrap();
		assert_eq!(armed, 1);
		assert!(matches!(result, Err(Failure::Refused(_))));
		assert_eq!(
			s.eval_rendered("a.len()", &limits).unwrap(),
			Some("1".into())
		);
	}
	#[test]
	fn refusal_policy_and_budget_preserve_state() {
		let mut s = DomainSession::new(context(), DEFAULT_CEILING).unwrap();
		s.eval_rendered(
			"let a=[1];let f=|n|n+1;0",
			&crate::format::Limits::default(),
		)
		.unwrap();
		for text in [
			"a.push(2); async {1}.await",
			"a.push(2); [1].iter().map(f).collect::<Vec>()",
			"mod M {}",
		] {
			let failure = s.eval(text).expect_err(text);
			assert!(matches!(failure, Failure::Refused(_)), "{text}: {failure}");
			assert_eq!(
				s.eval_rendered("a.len()", &crate::format::Limits::default())
					.unwrap(),
				Some("1".into())
			);
		}
		s.budget = 1024;
		assert!(matches!(s.eval("loop {}"), Err(Failure::Budget(1024))));
		s.budget = BUDGET;
		assert_eq!(
			s.eval_rendered("a.len()", &crate::format::Limits::default())
				.unwrap(),
			Some("1".into())
		);
	}
	#[test]
	fn cleanup_failure_preserves_primary_error() {
		let mut s = DomainSession::new(context(), DEFAULT_CEILING).unwrap();
		let expected = s.eval("1/0").expect_err("division");
		s.reset();
		s.collect_fail_after = Some(1);
		let actual = s.eval("1/0").expect_err("division with cleanup failure");
		assert_eq!(actual.to_string(), expected.to_string());
		assert!(matches!(
			actual,
			Failure::Runtime {
				origin: Some(_),
				..
			}
		));
		assert_eq!(s.cleanup_notes.len(), 1);
		assert!(s.cleanup_notes[0].contains("injected program collection failure"));
		s.cleanup_notes.clear();
		s.collect_fail_after = Some(1);
		assert!(
			s.eval("7")
				.expect_err("cleanup failure on success")
				.to_string()
				.contains("injected program collection failure")
		);
		assert!(s.cleanup_notes.is_empty());
	}
	#[test]
	fn pure_loop_interrupt_is_checked_at_next_slice_and_collects() {
		let mut s = DomainSession::new(context(), DEFAULT_CEILING).unwrap();
		let initial = s.domain.live_counts();
		for _ in 0..4 {
			let mut checks = 0;
			let outcome = s.eval_interruptible("loop {}", || {
				checks += 1;
				checks == 2
			});
			assert!(matches!(outcome, Err(Failure::Interrupted)));
			assert_eq!(checks, 2);
			assert_eq!(s.domain.live_counts(), initial);
		}
		assert_eq!(
			s.eval_rendered("7", &crate::format::Limits::default())
				.unwrap(),
			Some("7".into())
		);
	}
	#[test]
	fn collection_plateaus_and_reset_releases_closure_programs() {
		let mut s = DomainSession::new(context(), DEFAULT_CEILING).unwrap();
		let empty = s.domain.live_counts();
		let limits = crate::format::Limits::default();
		s.eval_rendered("let f=||7;let alias=f;0", &limits).unwrap();
		let retained = s.domain.live_counts();
		assert!(retained.0 > 0);
		s.eval_rendered("[1,2,3]", &limits).unwrap();
		let capacity = s.domain.capacity_counts().unwrap();
		for _ in 0..8 {
			s.eval_rendered("[1,2,3]", &limits).unwrap();
			assert_eq!(s.domain.live_counts(), retained);
			assert_eq!(s.domain.capacity_counts().unwrap(), capacity);
			assert!(s.eval("async {1}.await").is_err());
			assert_eq!(s.domain.live_counts(), retained);
		}
		s.eval_rendered("f=0;0", &limits).unwrap();
		assert_eq!(s.domain.live_counts().0, retained.0);
		s.eval_rendered("alias=0;0", &limits).unwrap();
		assert_eq!(s.domain.live_counts().0, 0);
		s.reset();
		assert_eq!(s.domain.live_counts(), empty);
		assert_eq!(s.next_number(), 1);
		assert!(s.maps.is_empty());
		assert_eq!(s.eval_rendered("9", &limits).unwrap(), Some("9".into()));
	}
	#[test]
	fn inspection_is_read_only_bounded_and_works_at_latched_ceiling() {
		let mut old = Session::new(context()).unwrap();
		let mut s = DomainSession::new(context(), DEFAULT_CEILING).unwrap();
		let text = "struct P{x} let v=P{x:[1,2]};let n=7;0";
		drop(old.eval(text).unwrap());
		s.eval_rendered(text, &crate::format::Limits::default())
			.unwrap();
		let limits = crate::inspect::InspectLimits::default();
		let before = s.domain.live_counts();
		s.over_ceiling = true;
		assert_eq!(
			crate::inspect::vars(&old, &limits),
			crate::inspect::domain_vars(&s, &limits).0
		);
		for name in [None, Some("v"), Some("P"), Some("missing")] {
			assert_eq!(
				crate::inspect::help(&old, &[], &[], name, &limits),
				crate::inspect::domain_help(&s, &[], &[], name, &limits).0
			);
		}
		assert_eq!(s.domain.live_counts(), before);
		assert!(matches!(s.eval("7"), Err(Failure::OverCeiling { .. })));
		s.reset();
		assert!(!s.over_ceiling());
		assert!(s.eval("7").is_ok());
	}
	#[test]
	fn native_presenter_input_refuses_before_host_effects() {
		#[derive(rune::Any)]
		struct Native;
		let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
		let observed = calls.clone();
		let mut c = context();
		let mut m = rune::Module::new();
		m.ty::<Native>().unwrap();
		m.function("native", move || {
			observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
			Native
		})
		.build()
		.unwrap();
		c.install(m).unwrap();
		let mut presenters = crate::present::Presenters::default();
		presenters
			.register::<Native>(|_, _| panic!("presenter must never run"))
			.unwrap();
		let mut s = DomainSession::new(c, DEFAULT_CEILING)
			.unwrap()
			.with_presenters(presenters);
		let mut armed = 0;
		let refused = s
			.eval_rendered_with_armed("native()", &crate::format::Limits::default(), &mut |_| {
				armed += 1;
				Ok(())
			})
			.unwrap();
		assert_eq!(armed, 1);
		assert!(matches!(refused, Err(Failure::Refused(_))));
		assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
		assert_eq!(
			s.eval_rendered("7", &crate::format::Limits::default())
				.unwrap(),
			Some("7".into())
		);
		s.reset();
		assert!(matches!(s.eval("native()"), Err(Failure::Refused(_))));
		assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
	}
}

#[cfg(test)]
pub(super) mod transcripts;

mod frames;

/// Small admission-work grouping shared by the pinned inventory.
pub(crate) fn host_wait_reason(name: &str) -> &'static str {
	match name {
		"time::sleep"
		| "http::get"
		| "http::get_bytes"
		| "http::request"
		| "http::request_bytes" => "async",
		"fs::read_bytes" => "bytes",
		"time::from_parts" => "borrowed object input",
		"process::run" => "argument-vector and options-object input",
		"process::run_bytes" => "argument-vector/options input and bytes output",
		"web::response" => "dynamic body/headers and response output",
		_ => "host enrollment",
	}
}
