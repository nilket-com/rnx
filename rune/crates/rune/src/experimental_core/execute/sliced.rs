//! Sliced execution; fixed semantics are guarded by ci/execute_guard.
use super::*;

/// An invocation suspended only between charged instructions.
///
/// This token exclusively owns execution access until completion or abandonment.
/// Dropping it releases the continuation without executing another instruction.
#[must_use = "resume or cancel the invocation"]
pub struct SlicedInvocation<'a> {
	pub(super) executor: &'a mut Executor,
	pub(super) program: &'a Program,
	pub(super) store: &'a mut Store,
	pub(super) remaining: usize,
	pub(super) active: bool,
	pub(super) cross: Option<CrossState<'a>>,
}

pub(super) struct CrossState<'a> {
	pub(super) programs: &'a [crate::experimental_core::domain::Member],
	pub(super) owners: Vec<u32>,
	pub(super) failure_frames: Vec<(u32, Option<Location>)>,
	pub(super) truncated: bool,
	pub(super) last_owner: u32,
	pub(super) departed: Option<Frame>,
}

impl core::fmt::Debug for SlicedInvocation<'_> {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("SlicedInvocation")
			.field("remaining", &self.remaining)
			.field("active", &self.active)
			.finish_non_exhaustive()
	}
}

/// A soft suspension or terminal execution result. Only `Limited` can resume.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)] // Keep terminal Failure inline; boxing adds an allocation per failure.
pub enum SliceOutcome {
	/// The next charged instruction has not executed.
	Limited,
	/// The entry returned; its result is transferred exactly once.
	Complete(Root),
	/// A terminal error, including hard exhaustion or cancellation.
	Failed(Failure),
}

#[allow(clippy::result_large_err)]
impl<'a> PreparedInvocation<'a> {
	/// Enter once with fixed total fuel. Sliced execution is interpreted, even
	/// when this executor permits native execution for complete invocations.
	pub fn start_sliced(self, fuel: usize) -> core::result::Result<SlicedInvocation<'a>, Failure> {
		// usize::MAX is the ambient unlimited sentinel; match the finite eval cap.
		let fuel = fuel.min(usize::MAX - 1);
		self.executor.active_host_runtime = self.executor.configured_host_runtime.clone();
		if let Err(failure) = self.executor.enter(
			&self.program.code,
			self.store,
			self.function,
			Args::Roots(self.args),
			None,
		) {
			let failure = self.executor.sliced_failure(
				&self.program.code,
				self.store,
				failure.error,
				failure.cleanup_error,
				false,
				fuel,
			);
			self.executor.active_host_runtime = None;
			return Err(failure);
		}
		Ok(SlicedInvocation {
			executor: self.executor,
			program: self.program,
			store: self.store,
			remaining: fuel,
			active: true,
			cross: None,
		})
	}
}

impl SlicedInvocation<'_> {
	/// Hard work remaining. Resuming never replenishes it.
	pub fn remaining_work(&self) -> usize {
		self.remaining
	}

	/// Execute up to this many charged instructions, plus uncharged bookkeeping.
	/// Host operations are not preempted. A terminal token rejects further work.
	pub fn resume(&mut self, slice: core::num::NonZeroUsize) -> SliceOutcome {
		if !self.active {
			return SliceOutcome::Failed(sliced_lifecycle_failure(self.remaining));
		}
		let result = self.executor.resume_sliced(
			&self.program.code,
			self.store,
			&mut self.remaining,
			slice.get(),
			&mut self.cross,
		);
		if !matches!(result, SliceOutcome::Limited) {
			self.active = false;
			self.executor.active_host_runtime = None;
		}
		result
	}

	/// Abandon the continuation without replay or rollback of committed effects.
	/// The returned failure preserves any cleanup failure beside cancellation.
	pub fn cancel(&mut self) -> Failure {
		if !self.active {
			return sliced_lifecycle_failure(self.remaining);
		}
		self.active = false;
		let code=if let Some(state)=&mut self.cross {
			state.capture(&self.executor.frames);
			if let Some(owner)=state.owners.last(){state.last_owner = *owner;}
			state.current().map(|p|&p.code).unwrap_or(&self.program.code)
		}else{&self.program.code};
		let failure = self.executor.sliced_failure(
			code,
			self.store,
			error(ErrorKind::Cancelled),
			None,
			true,
			self.remaining,
		);
		self.executor.active_host_runtime = None;
		failure
	}
}

impl Drop for SlicedInvocation<'_> {
	fn drop(&mut self) {
		if self.active {
			let _ = self.cancel();
		}
	}
}

fn sliced_lifecycle_failure(fuel: usize) -> Failure {
	Failure {
		remaining_work: Some(fuel),
		numeric_context: None,
		error: error(ErrorKind::InvalidCode),
		missing_key: None,
		panic_message: None,
		host_message: None,
		call_chain: Vec::new(),
		cleanup_error: None,
	}
}

impl Executor {
	#[allow(clippy::too_many_arguments)]
	pub(super) fn sliced_failure(
		&mut self,
		code: &Code,
		store: &mut Store,
		mut failure: Error,
		setup_cleanup_error: Option<Error>,
		entered: bool,
		fuel: usize,
	) -> Failure {
		let missing_key = None;
		let mut panic_message = None;
		let result: core::result::Result<Root, Failure> = {
			let numeric_context =
				invocation_numeric_context(&failure, code, store, &self.frames, &self.slots);
			self.resolve_failure_location(code, &mut failure);
			let mut cleanup_error = setup_cleanup_error.or(self.pending_host_cleanup.take());
			let mut host_message = self.pending_host_message.take();
			if failure.kind == ErrorKind::Panic
				&& failure.detail == Detail::PanicMessage
				&& panic_message.is_none()
			{
				panic_message = host_message.take();
			}

			self.collect_failure_chain(code);

			// Includes a partially filled callee reserve if retaining an arg failed.
			while let Some(value) = self.slots.pop() {
				if let Err(e) = store.release(value) {
					cleanup_error.get_or_insert(e);
				}
			}

			self.frames.clear();
			Err(Failure {
				remaining_work: entered.then_some(fuel),
				numeric_context,
				error: failure,
				missing_key,
				panic_message,
				host_message,
				call_chain: core::mem::take(&mut self.diagnostics),
				cleanup_error,
			})
		};
		match result {
			Err(failure) => failure,
			Ok(_) => unreachable!(),
		}
	}

	// Only the sliced operation expansion calls this boundary. The complete
	// intrinsic/host helpers remain unchanged. The scope restores ambient TLS
	// before returning, including host panics converted to ordinary failures.
	#[allow(clippy::too_many_arguments)]
	fn budgeted_sliced_intrinsic(
		&mut self,
		code: &Code,
		store: &mut Store,
		function: &super::super::code::Function,
		base: usize,
		out: u32,
		id: u32,
		start: u32,
		count: u32,
		fuel: &mut usize,
		cross: &Option<CrossState<'_>>,
	) -> Result<()> {
		if let Some(state)=cross {
			if id==super::super::code::SORT_BEGIN && count==2 {
				let target=function.args.get(start as usize+1).and_then(|n|self.slots.get(base+*n as usize)).copied();
				let owner=target.and_then(|v|match v {Value::Function{code,..}=>Some(code),Value::Heap(_)=>store.closure_parts(v).ok().map(|p|p.0),_=>None});
				if let Some(program)=owner.and_then(|id|state.program(id).ok()) {
					return self.sort_call(&program.code,store,function,base,out,id,start,count);
				}
			}
		}
		if code.host(id).is_none() {
			return self.intrinsic(code, store, function, base, out, id, start, count);
		}
		use crate::runtime::budget;
		let granted = *fuel;
		let (result, remaining) = budget::with(granted, || {
			let result = self.intrinsic(code, store, function, base, out, id, start, count);
			(result, budget::remaining())
		})
		.call();
		match remaining {
			Some(left) if left <= granted => {
				// Local opcode fuel was already charged. Only host consumption is
				// reconciled here; nested ambient scopes cannot replenish the token.
				*fuel -= granted - left;
				result
			}
			_ => {
				// An escaped acquire guard or budget replacement cannot mint fuel.
				*fuel = 0;
				match result {
					Err(primary) => Err(primary),
					Ok(()) => Err(error(ErrorKind::FuelExhausted)),
				}
			}
		}
	}

	fn resume_sliced(
		&mut self,
		code: &Code,
		store: &mut Store,
		remaining: &mut usize,
		slice: usize,
		cross: &mut Option<CrossState<'_>>,
	) -> SliceOutcome {
		let mut fuel = *remaining;
		let mut soft = slice;
		let entered = true;
		let mut setup_cleanup_error = None;
		let mut missing_key = None;
		let mut panic_message = None;
		let result: Result<Option<Value>> = (|| {
			'frames: loop {
				let code=if let Some(state)=cross { let program=state.current()?;state.last_owner=program.code.id(); &program.code }else{code};
				let frame = *self
					.frames
					.last()
					.ok_or_else(|| error(ErrorKind::InvalidCode))?;
				let function = code
					.function(frame.function)
					.ok_or_else(|| error(ErrorKind::InvalidCode))?;
				let frame_index = self.frames.len() - 1;
				let mut pc = frame.pc;
				loop {
					let source_pc = pc;
					let instruction = match function.cached_ref(source_pc) {
						Some(instruction) => instruction,
						None => {
							self.commit_pc(frame_index, pc)?;
							return Err(error(ErrorKind::InvalidCode));
						}
					};
					if instruction.charged && soft == 0 {
						self.commit_pc(frame_index, pc)?;
						return Ok(None);
					}
					let mut transferred = false;
					let result = (|| {
						if instruction.charged {
							#[cfg(test)]
							if cancel_tick_for_test() {
								self.canceled = true;
							}
							if self.canceled {
								self.canceled = false;
								return Err(error(ErrorKind::Cancelled));
							}
							fuel = fuel
								.checked_sub(1)
								.ok_or_else(|| error(ErrorKind::FuelExhausted))?;
							soft -= 1;
						}
						{
							pc += 1;
							let get = |n: u32| -> Result<Value> {
								self.slots
									.get(frame.base + n as usize)
									.copied()
									.ok_or_else(|| error(ErrorKind::InvalidCode))
							};

							// D20: operands are bound per arm from the borrowed cached instruction (no
							// whole-Op temporary). Call and CallValue name their call here; the shared
							// Call body runs after the match, so CallValue's checks still precede
							// commit_pc and every retain/reserve/frame publication.
							let mut call = None;
							match instruction.op {
								Op::Template { .. } => {
									self.template_instruction(
										store,
										function,
										instruction,
										frame.base,
									)?;
								}

								Op::StringFrom { out, value } => {
									let value = store.string_from(get(value)?)?;
									self.put(store, frame.base + out as usize, value)?;
								}
								Op::Panic { value } => {
									// Full copy is required language payload, not best-effort evidence.
									panic_message = Some(copy_panic_message(store, get(value)?)?);
									return Err(
										error(ErrorKind::Panic).with_detail(Detail::PanicMessage)
									);
								}

								Op::Print { value } => {
									let value = get(value)?;
									self.print_instruction(store, value)?;
								}

								Op::Variant {
									out,
									tag,
									success,
									payload,
								} => {
									let value =
										store.variant(tag, success, payload.map(get).transpose()?)?;
									self.put(store, frame.base + out as usize, value)?;
								}

								Op::Try { out, value } => {
									let value = get(value)?;
									let (_, success, payload) = store.variant_parts(value)?;

									if success {
										let payload = payload.ok_or_else(|| error(ErrorKind::Type))?;
										store.retain(payload)?;
										self.put(store, frame.base + out as usize, payload)?;
									} else {
										store.retain(value)?;
										let out = frame.out;
										self.commit_pc(frame_index, pc)?;
										self.sliced_leave(store,cross)?;
										transferred = true;

										if let Some(out) = out {
											self.put(store, out, value)?;
										} else {
											return Ok(Some(value));
										}
									}
								}

								Op::CallValue {
									out,
									value,
									args_start,
									args_len,
								} => {
                                    self.sliced_call_value(code,store,function,frame.base,frame_index,pc,source_pc,out,get(value)?,args_start,args_len,cross)
                                        .map_err(|failure| { setup_cleanup_error=failure.cleanup_error;failure.error })?;
                                    transferred=true;
								}

								Op::Range { out, start, end } => {
									let value = store.range_values(get(start)?, get(end)?)?;
									self.put(store, frame.base + out as usize, value)?;
								}

								Op::IterStart { out, value } => {
									let value = store.iterator(get(value)?)?;
									self.put(store, frame.base + out as usize, value)?;
								}

								Op::IterNext {
									out,
									iterator,
									none,
								} => {
									if let Some(value) = store.iterator_next(get(iterator)?)? {
										self.put(store, frame.base + out as usize, value)?;
									} else {
										pc = none as usize;
									}
								}

								Op::String { out, text } => {
									let v = store.string(
										function
											.strings
											.get(text as usize)
											.ok_or_else(|| error(ErrorKind::InvalidCode))?,
									)?;
									self.put(store, frame.base + out as usize, v)?;
								}

								Op::Clone { out, value, mode } => {
									let value = get(value)?;
									if mode == CloneMode::Associated
										&& !matches!(value, Value::Function { .. })
										&& !matches!(
											store.tag(value)?,
											super::super::value::Type::Vec
												| super::super::value::Type::Tuple | super::super::value::Type::Object
												| super::super::value::Type::String | super::super::value::Type::Unit
												| super::super::value::Type::I64 | super::super::value::Type::U64
												| super::super::value::Type::F64 | super::super::value::Type::Option
												| super::super::value::Type::Result | super::super::value::Type::Function
										) {
										return Err(error(ErrorKind::Type));
									}
									let v = store.clone_value(value)?;
									self.put(store, frame.base + out as usize, v)?;
								}

								Op::AssignBinary { out, op, lhs, rhs } => {
									// D21: borrow both operand slots (lhs then rhs, the same
									// InvalidCode order as `get`); two I64 values go to fast_i64
									// using their borrowed payloads, every other pair is copied and
									// takes the unchanged fallback. Both borrows end before `put`.
									let base = frame.base;
									let slots = &self.slots;
									let left = slots
										.get(base + lhs as usize)
										.ok_or_else(|| error(ErrorKind::InvalidCode))?;
									let right = slots
										.get(base + rhs as usize)
										.ok_or_else(|| error(ErrorKind::InvalidCode))?;
									let fast = match (left, right) {
										(&Value::I64(a), &Value::I64(b)) => {
											fast_i64(op, Value::I64(a), Value::I64(b))
										}
										_ => None,
									};

									let v = if let Some(result) = fast {
										result?
									} else {
										let (a, b) = (*left, *right);
										if op == arithmetic::Binary::Add
											&& store.tag(a)? == super::super::value::Type::String
										{
											store.append_string(a, b)?;
											store.retain(a)?;
											a
										} else {
											binary(store, op, a, b)?
										}
									};

									self.put(store, base + out as usize, v)?;
								}

								Op::Literal { out, value } => {
									self.put(store, frame.base + out as usize, value)?
								}

								Op::Copy { out, value } => {
									let v = get(value)?;
									store.retain(v)?;
									self.put(store, frame.base + out as usize, v)?;
								}

								Op::Release { value } => {
									self.put(store, frame.base + value as usize, Value::Unit)?
								}

								Op::Unary { out, op, value } => {
									let v = arithmetic::unary(op, get(value)?)?;
									self.put(store, frame.base + out as usize, v)?;
								}

								Op::Binary { out, op, lhs, rhs } => {
									// D21: as AssignBinary, without the string append arm.
									let base = frame.base;
									let slots = &self.slots;
									let left = slots
										.get(base + lhs as usize)
										.ok_or_else(|| error(ErrorKind::InvalidCode))?;
									let right = slots
										.get(base + rhs as usize)
										.ok_or_else(|| error(ErrorKind::InvalidCode))?;
									let fast = match (left, right) {
										(&Value::I64(a), &Value::I64(b)) => {
											fast_i64(op, Value::I64(a), Value::I64(b))
										}
										_ => None,
									};

									let v = if let Some(result) = fast {
										result?
									} else {
										binary(store, op, *left, *right)?
									};

									self.put(store, base + out as usize, v)?;
								}

								Op::Cast { out, value, tag } => {
									let v = arithmetic::cast(get(value)?, tag)?;
									self.put(store, frame.base + out as usize, v)?;
								}

								Op::Is {
									out,
									value,
									tag,
									negate,
								} => {
									let operand = get(value)?;
									let matches = match operand {
										Value::Function { .. } => tag == super::super::value::Type::Function,
										Value::Type(_) => false,
										_ => store.tag(operand)? == tag,
									};
									let v = Value::Bool(matches ^ negate);
									self.put(store, frame.base + out as usize, v)?;
								}
								Op::Jump { target } => pc = target as usize,
								Op::Branch { condition, yes, no } => {
									let Value::Bool(condition) = get(condition)? else {
										return Err(error(ErrorKind::Type));
									};

									pc = if condition { yes as usize } else { no as usize };
								}

								Op::Call {
									out,
									function: callee,
									args_start,
									args_len,
								} => call = Some((out, callee, args_start, args_len)),
								Op::Return { value } => {
									#[cfg(all(test, feature = "native-code"))]
									super::super::region::recursion::observe_return(
										frame.function,
										self.frames.len(),
										source_pc,
										fuel,
										&self.slots[frame.base..frame.base + function.slots as usize],
									);
									let value = get(value)?;
									store.retain(value)?;
									let out = frame.out;
									self.commit_pc(frame_index, pc)?;
									self.sliced_leave(store,cross)?;
									transferred = true;

									if let Some(out) = out {
										self.put(store, out, value)?;
									} else {
										return Ok(Some(value));
									}
								}

								Op::Record {
									out,
									tag,
									args_start,
									args_len,
								} => {
									let descriptors = &function.args
										[args_start as usize..(args_start + args_len) as usize];
									self.record_instruction(
										store,
										&function.strings,
										descriptors,
										frame.base,
										out,
										tag,
									)?;
								}

								Op::Field { out, value, key } => {
									let value = store
										.field(get(value)?, function.strings[key as usize].as_str())?;
									self.put(store, frame.base + out as usize, value)?;
								}

								Op::FieldGetMut { out, value, key } => {
									let value = store.field_get_mut(
										get(value)?,
										function.strings[key as usize].as_str(),
									)?;
									self.put(store, frame.base + out as usize, value)?;
								}
								Op::FieldSetMut { value, key, new } => store.field_set_mut(
									get(value)?,
									function.strings[key as usize].as_str(),
									get(new)?,
								)?,
								Op::SetField { value, key, new } => store.set_field(
									get(value)?,
									function.strings[key as usize].as_str(),
									get(new)?,
								)?,
								Op::List {
									out,
									tag,
									args_start,
									args_len,
								} => {
									// No temporary proportional argument allocation: the slots themselves
									// are not necessarily contiguous, so reserve only the bounded list.
									let indices = &function.args
										[args_start as usize..args_start as usize + args_len as usize];
									let mut values = Vec::try_with_capacity(indices.len())?;

									for &i in indices {
										values.try_push(get(i)?)?;
									}

									let value = store.list(tag, &values)?;
									self.put(store, frame.base + out as usize, value)?;
								}

								Op::TupleFieldGetMut { out, value, index } => {
									let v =
										store.tuple_get_mut(get(value)?, index_value(get(index)?)?)?;
									self.put(store, frame.base + out as usize, v)?;
								}
								Op::TupleFieldSetMut { value, index, new } => store.tuple_set_mut(
									get(value)?,
									index_value(get(index)?)?,
									get(new)?,
								)?,
								Op::TupleField { out, value, index } => {
									let v = store.tuple_get(get(value)?, index_value(get(index)?)?)?;
									self.put(store, frame.base + out as usize, v)?;
								}
								Op::SetTupleField { value, index, new } => store.tuple_set(
									get(value)?,
									index_value(get(index)?)?,
									get(new)?,
								)?,
								Op::Get { out, value, index } => {
									let v = store.dynamic_get(get(value)?, get(index)?)?;
									self.put(store, frame.base + out as usize, v)?;
								}
								Op::Set { value, index, new } => {
									store.dynamic_set(get(value)?, get(index)?, get(new)?)?
								}

								Op::Len { out, value } => {
									if !matches!(
										store.tag(get(value)?)?,
										super::super::value::Type::Vec
											| super::super::value::Type::Tuple | super::super::value::Type::String
											| super::super::value::Type::Object | super::super::value::Type::Unit
									) {
										return Err(error(ErrorKind::Type));
									}

									let n = u64::try_from(store.len(get(value)?)?)
										.map_err(|_| error(ErrorKind::Overflow))?;
									self.put(store, frame.base + out as usize, Value::U64(n))?;
								}
								Op::Push { value, new } => {
									let value = get(value)?;
									if !matches!(value, Value::Heap(_))
										|| !matches!(
											store.tag(value)?,
											super::super::value::Type::Vec | super::super::value::Type::String
										) {
										return Err(error(ErrorKind::Type));
									}
									store.push(value, get(new)?)?;
								}
								Op::Drop { value } => store.invalidate(get(value)?)?,
							}

							if let Some((out, callee, args_start, args_len)) = call {
								self.commit_pc(frame_index, pc)?;
								match code.function(callee) {
									Some(callee_code) => {
                                        Self::cross_prepare_owner(cross,code.id())?;
										let source_args = &function.args[args_start as usize
											..args_start as usize + args_len as usize];
										self.push_call(
											store,
											callee_code.slots as usize,
											Args::Slots {
												base: frame.base,
												indices: source_args,
											},
											Frame {
												function: callee,
												pc: 0,
												base: 0,
												out: Some(frame.base + out as usize),
												callsite_pc: source_pc as u32,
											},
										)
										.map_err(|f| {
											setup_cleanup_error = f.cleanup_error;
											f.error
										})?;
                                        Self::cross_publish_owner(cross,code.id());
										transferred = true;
									}
									None => self.budgeted_sliced_intrinsic(
										code, store, function, frame.base, out, callee, args_start,
										args_len,
									 &mut fuel, cross, )?,
								}
							}

							Ok(None)
						}
					})();
					match result {
						Ok(Some(v)) => return Ok(Some(v)),
						Ok(None) if transferred => continue 'frames,
						Ok(None) => (),
						Err(e) => {
							// A failed return can already have popped this frame.
							// Never overwrite the parent's pc in that case. A failed
							// call can have reserved slots without publishing a frame.
							if let Some(current) = self.frames.get_mut(frame_index) {
								current.pc = pc;
							}

							let e = enrich_error(
								e,
								store,
								&instruction.op,
								code,
								frame.function,
								&self.slots,
								frame.base,
							);
							if matches!(e.detail, Detail::MissingKey { .. }) {
								if let Op::Get { index, .. } | Op::Set { index, .. } =
									instruction.op
								{
									if let Some(value) = self.slots.get(frame.base + index as usize)
									{
										#[cfg(test)]
										let fail = core::mem::take(&mut self.test_fail_key_copy);
										#[cfg(not(test))]
										let fail = false;
										missing_key = if fail {
											#[cfg(test)]
											{
												rune_alloc::limit::with(0, || {
													copy_missing_key(store, *value)
												})
												.call()
											}
											#[cfg(not(test))]
											{
												None
											}
										} else {
											copy_missing_key(store, *value)
										};
									}
								}
							}
							return Err(e.at(function.locations[source_pc]));
						},
					}
				}
			}
		})();
		let code=if let Some(state)=cross {
			if result.is_err(){state.capture(&self.frames);}
			state.program(state.last_owner).map(|p| &p.code).unwrap_or(code)
		}else{code};
		*remaining = fuel;
		match result {
			Ok(None) => SliceOutcome::Limited,
			Ok(Some(value)) => SliceOutcome::Complete(store.export(value)),
			Err(mut failure) => {
				let result: core::result::Result<Root, Failure> = {
					let numeric_context =
						invocation_numeric_context(&failure, code, store, &self.frames, &self.slots);
					self.resolve_failure_location(code, &mut failure);
					let mut cleanup_error = setup_cleanup_error.or(self.pending_host_cleanup.take());
					let mut host_message = self.pending_host_message.take();
					if failure.kind == ErrorKind::Panic
						&& failure.detail == Detail::PanicMessage
						&& panic_message.is_none()
					{
						panic_message = host_message.take();
					}

					self.collect_failure_chain(code);

					// Includes a partially filled callee reserve if retaining an arg failed.
					while let Some(value) = self.slots.pop() {
						if let Err(e) = store.release(value) {
							cleanup_error.get_or_insert(e);
						}
					}

					self.frames.clear();
					Err(Failure {
						remaining_work: entered.then_some(fuel),
						numeric_context,
						error: failure,
						missing_key,
						panic_message,
						host_message,
						call_chain: core::mem::take(&mut self.diagnostics),
						cleanup_error,
					})
				};
				match result {
					Err(failure) => SliceOutcome::Failed(failure),
					Ok(_) => unreachable!(),
				}
			}
		}
	}
}

#[cfg(test)]
mod sliced_tests {
	use super::*;
	use crate::{Context, Diagnostics, Hash, Source, Sources};
	use core::num::NonZeroUsize;

	fn program(text: &str) -> Program {
		let mut sources = Sources::new();
		sources.insert(Source::memory(text).unwrap()).unwrap();
		super::super::super::frontend::compile(
			&mut sources,
			&Context::with_default_modules().unwrap(),
			&mut Diagnostics::new(),
			1,
		)
		.unwrap()
	}
	fn run(
		p: &Program,
		total: usize,
		slice: Option<usize>,
	) -> (Result<Value>, usize, Option<Location>, usize) {
		let mut executor = Executor::new().unwrap();
		executor.set_native_enabled(false);
		let mut store = Store::new(Default::default()).unwrap();
		let entry = p.function(Hash::type_hash(["main"])).unwrap();
		let (result, remaining) = if let Some(slice) = slice {
			let mut invocation = executor
				.prepare(p, &mut store, entry, &[])
				.unwrap()
				.start_sliced(total)
				.unwrap();
			let result = loop {
				match invocation.resume(NonZeroUsize::new(slice).unwrap()) {
					SliceOutcome::Limited => (),
					SliceOutcome::Complete(value) => break Ok(value),
					SliceOutcome::Failed(failure) => break Err(failure),
				}
			};
			let left = invocation.remaining_work();
			assert!(matches!(
				invocation.resume(NonZeroUsize::new(1).unwrap()),
				SliceOutcome::Failed(_)
			));
			assert_eq!(invocation.remaining_work(), left);
			(result, left)
		} else {
			let result = executor.invoke(p, &mut store, entry, &[], total);
			let remaining = take_remaining_work_for_test().unwrap();
			(result, remaining)
		};
		assert!(executor.is_idle());
		assert!(executor.active_host_runtime.is_none());
		let (value, location, depth) = match result {
			Ok(root) => (Ok(store.import(&root).unwrap()), None, 0),
			Err(failure) => {
				let location = failure.error.location;
				(Err(failure.error), location, failure.call_chain.len())
			}
		};
		store.close().unwrap();
		(value, remaining, location, depth)
	}

	#[test]
	fn sliced_partition_preserves_results_and_fuel() {
		for source in [
			"pub fn main(){42}",
			"pub fn main(){let n=0; for i in 0..20 {n+=i;} n}",
			"fn f(n){if n==0 {0} else {f(n-1)+1}} pub fn main(){f(8)}",
			"pub fn main(){let x=3; let f=|a|a+x; f(5)}",
			"pub fn main(){let v=[1,2]; v[0]=5; v[0]+v[1]}",
		] {
			let p = program(source);
			for total in [0, 1, 2, 7, 100, 10000] {
				let reference = run(&p, total, None);
				for slice in [1, 2, 3, 10000, 20000] {
					let actual = run(&p, total, Some(slice));
					assert_eq!(
						format!("{actual:?}"),
						format!("{reference:?}"),
						"{source}, total {total}, slice {slice}"
					);
				}
			}
		}
	}

	#[test]
	fn sliced_error_keeps_location_chain_and_last_charge() {
		let p = program("fn f(n){if n==0 {1/0} else {f(n-1)}} pub fn main(){f(4)}");
		let reference = run(&p, 10000, None);
		assert!(reference.0.is_err());
		let used = 10000 - reference.1;
		for total in [used - 1, used, used + 1, 10000] {
			for slice in [1, 2, 3, 10000] {
				assert_eq!(
					format!("{:?}", run(&p, total, Some(slice))),
					format!("{:?}", run(&p, total, None))
				);
			}
		}
	}

	#[test]
	fn sliced_drop_and_cancel_release_continuations_and_allow_reuse() {
		let p = program("pub fn main(){let v=[1,2,3]; loop {v[0]+=1;}}");
		let entry = p.function(Hash::type_hash(["main"])).unwrap();
		let mut executor = Executor::new().unwrap();
		let mut store = Store::new(Default::default()).unwrap();
		for explicit in [false, true] {
			{
				let mut invocation = executor
					.prepare(&p, &mut store, entry, &[])
					.unwrap()
					.start_sliced(1000)
					.unwrap();
				for _ in 0..10 {
					assert!(matches!(
						invocation.resume(NonZeroUsize::new(1).unwrap()),
						SliceOutcome::Limited
					));
				}
				if explicit {
					assert_eq!(invocation.cancel().error.kind, ErrorKind::Cancelled);
				}
			}
			assert!(executor.is_idle());
			assert!(executor.active_host_runtime.is_none());
			assert_eq!(store.live_entries(), 0);
		}
		store.close().unwrap();
	}

	#[test]
	fn sliced_output_is_not_replayed_after_resume_or_failure() {
		let p = program(
			"pub fn main(){println!(\"first\"); let n=0; while n<3 {println!(\"tick\"); n+=1;} 1/0}",
		);
		let entry = p.function(Hash::type_hash(["main"])).unwrap();
		let mut outputs = std::vec::Vec::new();
		for slice in [None, Some(1), Some(2), Some(10000)] {
			let mut executor = Executor::new().unwrap();
			executor.set_native_enabled(false);
			executor.test_output = Some(std::vec::Vec::new());
			let mut store = Store::new(Default::default()).unwrap();
			if let Some(slice) = slice {
				let mut invocation = executor
					.prepare(&p, &mut store, entry, &[])
					.unwrap()
					.start_sliced(10000)
					.unwrap();
				loop {
					match invocation.resume(NonZeroUsize::new(slice).unwrap()) {
						SliceOutcome::Limited => (),
						SliceOutcome::Failed(_) => break,
						SliceOutcome::Complete(_) => panic!("expected failure"),
					}
				}
			} else {
				assert!(executor.invoke(&p, &mut store, entry, &[], 10000).is_err());
			}
			assert!(executor.is_idle());
			outputs.push(executor.test_output.take().unwrap());
			store.close().unwrap();
		}
		assert!(!outputs[0].is_empty());
		for output in &outputs[1..] {
			assert_eq!(output, &outputs[0]);
		}
	}

	#[test]
	fn sliced_host_debits_restore_ambient_budget_on_success_panic_and_invalid_balance() {
		use crate::runtime::budget;
		for mode in [0u8, 1, 2] {
			let mut context = Context::with_default_modules().unwrap();
			let mut module = crate::Module::with_item(["sliced_host"]).unwrap();
			module
				.host_function("work", move || -> i64 {
					budget::consume(5).unwrap();
					if mode == 1 {
						panic!("after debit");
					}
					if mode == 2 {
						core::mem::forget(budget::acquire());
					}
					7
				})
				.unwrap();
			context.install(module).unwrap();
			let mut sources = Sources::new();
			sources
				.insert(Source::memory("pub fn main(){sliced_host::work()}").unwrap())
				.unwrap();
			let p =
				super::super::super::frontend::compile(&mut sources, &context, &mut Diagnostics::new(), 1)
					.unwrap();
			let entry = p.function(Hash::type_hash(["main"])).unwrap();
			let mut executor = Executor::new().unwrap();
			executor.set_native_enabled(false);
			executor
				.set_host_runtime(Some(
					crate::sync::Arc::try_new(context.runtime().unwrap()).unwrap(),
				))
				.unwrap();
			let mut store = Store::new(Default::default()).unwrap();
			let (result, left) = budget::with(999, || {
				let mut invocation = executor
					.prepare(&p, &mut store, entry, &[])
					.unwrap()
					.start_sliced(100)
					.unwrap();
				loop {
					let outcome = invocation.resume(NonZeroUsize::new(1).unwrap());
					assert_eq!(budget::remaining(), Some(999));
					if !matches!(outcome, SliceOutcome::Limited) {
						break (outcome, invocation.remaining_work());
					}
				}
			})
			.call();
			assert_eq!(budget::remaining(), None);
			match mode {
				0 => {
					let SliceOutcome::Complete(root) = result else {
						panic!("host failed")
					};
					assert_eq!(store.import(&root).unwrap(), Value::I64(7));
					// The unchanged complete path does not debit its local counter.
					let _complete = executor.invoke(&p, &mut store, entry, &[], 100).unwrap();
					let complete_left = take_remaining_work_for_test().unwrap();
					assert_eq!(complete_left - left, 5);
				}
				1 => {
					let SliceOutcome::Failed(failure) = result else {
						panic!("panic lost")
					};
					assert!(left < 95);
					assert_ne!(failure.error.kind, ErrorKind::FuelExhausted);
				}
				_ => {
					let SliceOutcome::Failed(failure) = result else {
						panic!("invalid balance accepted")
					};
					assert_eq!(left, 0);
					assert_eq!(failure.error.kind, ErrorKind::FuelExhausted);
				}
			}
			assert!(executor.is_idle());
			store.close().unwrap();
		}
	}

	#[test]
	fn sliced_pending_cancel_is_consumed_only_by_execution() {
		let p = program("pub fn main(){42}");
		let entry = p.function(Hash::type_hash(["main"])).unwrap();
		let mut executor = Executor::new().unwrap();
		let mut store = Store::new(Default::default()).unwrap();
		executor.request_cancel();
		assert!(executor.prepare(&p, &mut store, u32::MAX, &[]).is_err());
		{
			let mut invocation = executor
				.prepare(&p, &mut store, entry, &[])
				.unwrap()
				.start_sliced(100)
				.unwrap();
			let SliceOutcome::Failed(failure) = invocation.resume(NonZeroUsize::new(1).unwrap())
			else {
				panic!("cancel missed")
			};
			assert_eq!(failure.error.kind, ErrorKind::Cancelled);
			assert_eq!(invocation.remaining_work(), 100);
		}
		assert!(!executor.canceled);
		assert!(executor.is_idle());
	}
}
