//! Owner resolution for the single sliced operation loop.
use super::sliced::{CrossState, SlicedInvocation};
use super::*;
use crate::experimental_core::domain::Member;

#[cfg(test)]
std::thread_local! { static INJECT_STAGE: core::cell::Cell<u8> = const { core::cell::Cell::new(0) }; }
#[cfg(test)]
pub(crate) fn inject_stage(stage: u8) {
	INJECT_STAGE.with(|s| s.set(stage));
}
#[cfg(test)]
fn injected(stage: u8) -> bool {
	INJECT_STAGE.with(|s| {
		if s.get() == stage {
			s.set(0);
			true
		} else {
			false
		}
	})
}

impl<'a> CrossState<'a> {
	pub(super) fn program(&self, code: u32) -> Result<&'a Program> {
		self.programs
			.iter()
			.find(|m| m.program.code.id() == code)
			.map(|m| &m.program)
			.ok_or_else(|| error(ErrorKind::Type).with_detail(Detail::ForeignFunction))
	}
	pub(super) fn current(&self) -> Result<&'a Program> {
		self.program(
			*self
				.owners
				.last()
				.ok_or_else(|| error(ErrorKind::InvalidCode))?,
		)
	}
	// Keep measured lazy failure-frame capture; this annotation has no runtime effect.
	#[allow(clippy::unnecessary_lazy_evaluations)]
	pub(super) fn capture(&mut self, frames: &[Frame]) {
		self.failure_frames.clear();
		self.truncated = false;
		let total = frames.len() + usize::from(self.departed.is_some());
		for index in (0..total).rev() {
			let frame_at = |index| {
				frames.get(index).or_else(|| {
					if index == frames.len() {
						self.departed.as_ref()
					} else {
						None
					}
				})
			};
			let Some(frame) = frame_at(index) else {
				self.truncated = true;
				break;
			};
			let owner = if index == frames.len() {
				self.last_owner
			} else {
				let Some(&owner) = self.owners.get(index) else {
					self.truncated = true;
					break;
				};
				owner
			};
			let location = if let Some(next) = frame_at(index + 1) {
				self.program(owner)
					.ok()
					.and_then(|p| p.code.function(frame.function))
					.and_then(|f| f.locations.get(next.callsite_pc as usize))
					.copied()
			} else {
				None
			};
			if self.failure_frames.try_push((owner, location)).is_err() {
				self.truncated = true;
				break;
			}
		}
	}
}
impl Executor {
	#[allow(clippy::result_large_err)]
	pub(crate) fn start_cross<'a>(
		&'a mut self,
		programs: &'a mut [Member],
		entry: u32,
		store: &'a mut Store,
		function: u32,
		args: &[&Root],
		fuel: usize,
	) -> core::result::Result<
		(SlicedInvocation<'a>, &'a [Member]),
		crate::experimental_core::domain::StartError,
	> {
		let index = programs
			.iter()
			.position(|m| m.program.code.id() == entry)
			.ok_or_else(|| error(ErrorKind::InvalidCode))?;
		// Exactly one complete preparation. Refusal preserves the lease and roots;
		// later setup failures have the existing post-admission semantics.
		{
			let _prepared = self.prepare(&programs[index].program, store, function, args)?;
		}
		programs[index].consume_entry_lease();
		// Return this shared reborrow for both invocation and diagnostic owners.
		let programs: &'a [Member] = programs;
		let program = &programs[index].program;
		for member in programs.iter() {
			member
				.program
				.code
				.validate_hosts(self.configured_host_runtime.as_deref())?;
			if member.program.epoch != program.epoch || program.epoch.is_none() {
				return Err(error(ErrorKind::InvalidCode).into());
			}
		}
		let mut owners = Vec::new();
		owners.try_push(entry).map_err(Error::from)?;
		let mut failure_frames = Vec::new();
		failure_frames.try_reserve(1).map_err(Error::from)?;
		self.active_host_runtime = self.configured_host_runtime.clone();
		if let Err(failure) = self.enter(&program.code, store, function, Args::Roots(args), None) {
			let failure = self.sliced_failure(
				&program.code,
				store,
				failure.error,
				failure.cleanup_error,
				false,
				fuel.min(usize::MAX - 1),
			);
			self.active_host_runtime = None;
			return Err(crate::experimental_core::domain::StartError::Failed(
				failure,
			));
		}
		Ok((
			SlicedInvocation {
				executor: self,
				program,
				store,
				remaining: fuel.min(usize::MAX - 1),
				active: true,
				cross: Some(CrossState {
					programs,
					owners,
					failure_frames,
					truncated: false,
					last_owner: entry,
					departed: None,
				}),
			},
			programs,
		))
	}
	pub(super) fn cross_prepare_owner(
		cross: &mut Option<CrossState<'_>>,
		owner: u32,
	) -> Result<()> {
		if let Some(state) = cross {
			state.program(owner)?;
			#[cfg(test)]
			if injected(1) {
				return Err(error(ErrorKind::Allocation));
			}
			state.owners.try_reserve(1)?;
			state.failure_frames.try_reserve(state.owners.len() + 1)?;
		}
		Ok(())
	}
	pub(super) fn cross_publish_owner(cross: &mut Option<CrossState<'_>>, owner: u32) {
		if let Some(state) = cross {
			let result = state.owners.push_within_capacity(owner);
			debug_assert!(result.is_ok());
		}
	}
	pub(super) fn sliced_leave(
		&mut self,
		store: &mut Store,
		cross: &mut Option<CrossState<'_>>,
	) -> Result<()> {
		#[cfg(test)]
		if cross.is_some() && injected(3) {
			assert_eq!(self.frames.len(), cross.as_ref().unwrap().owners.len());
			return Err(error(ErrorKind::Allocation));
		}
		let departed = self.frames.last().copied();
		let result = self.leave(store);
		if let Some(state) = cross {
			if result.is_err() {
				state.departed = departed;
			}
			state.owners.truncate(self.frames.len());
			#[cfg(test)]
			if injected(4) {
				state.departed = departed;
				return Err(error(ErrorKind::Allocation));
			}
		}
		result
	}
	#[allow(clippy::too_many_arguments)]
	// Preserve the allocation-free setup failure, as in complete call setup.
	#[allow(clippy::result_large_err)]
	pub(super) fn sliced_call_value(
		&mut self,
		code: &Code,
		store: &mut Store,
		function: &super::super::code::Function,
		base: usize,
		frame_index: usize,
		pc: usize,
		source_pc: usize,
		out: u32,
		called: Value,
		start: u32,
		len: u32,
		cross: &mut Option<CrossState<'_>>,
	) -> core::result::Result<(), CallSetupFailure> {
		let (owner, id, captures) = match called {
			Value::Function { code, id } => (code, id, None),
			Value::Heap(_) => {
				let (owner, id, captures) = store.closure_parts(called)?;
				(owner, id, Some(captures.len()))
			}
			_ => {
				return Err(error(ErrorKind::Type)
					.with_detail(Detail::NotCallable {
						found: store.diagnostic_type(called).unwrap_or(TypeName::Unit),
					})
					.into());
			}
		};
		let callee = if let Some(state) = cross.as_ref() {
			&state.program(owner)?.code
		} else {
			if owner != code.id() {
				return Err(error(ErrorKind::Type)
					.with_detail(Detail::ForeignFunction)
					.into());
			}
			code
		};
		let target = callee
			.function(id)
			.ok_or_else(|| error(ErrorKind::InvalidCode))?;
		let capture_count =
			u32::try_from(captures.unwrap_or(0)).map_err(|_| error(ErrorKind::InvalidCode))?;
		let expected = target
			.parameters
			.checked_sub(capture_count)
			.ok_or_else(|| error(ErrorKind::InvalidCode))?;
		if expected != len {
			return Err(error(ErrorKind::Type)
				.with_detail(Detail::Arity {
					given: len,
					expected,
				})
				.into());
		}
		let slots = target.slots as usize;
		let end = start
			.checked_add(len)
			.ok_or_else(|| error(ErrorKind::InvalidCode))?;
		let indices = function
			.args
			.get(start as usize..end as usize)
			.ok_or_else(|| error(ErrorKind::InvalidCode))?;
		Self::cross_prepare_owner(cross, owner)?;
		self.commit_pc(frame_index, pc)?;
		let frame = Frame {
			function: id,
			pc: 0,
			base: 0,
			out: Some(base + out as usize),
			callsite_pc: source_pc as u32,
		};
		if let Some(captures) = captures {
			self.push_closure_call(store, slots, called, captures, base, indices, frame)?;
		} else {
			self.push_call(store, slots, Args::Slots { base, indices }, frame)?;
		}
		#[cfg(test)]
		if cross.is_some() && injected(2) {
			let cleanup_error = self.leave(store).err();
			assert_eq!(self.frames.len(), cross.as_ref().unwrap().owners.len());
			return Err(CallSetupFailure {
				error: error(ErrorKind::Allocation),
				cleanup_error,
			});
		}
		Self::cross_publish_owner(cross, owner);
		Ok(())
	}
}
impl<'a> SlicedInvocation<'a> {
	pub(crate) fn cross_diagnostics(
		&self,
	) -> (Option<&'a Program>, &[(u32, Option<Location>)], bool) {
		match &self.cross {
			Some(s) => (s.program(s.last_owner).ok(), &s.failure_frames, s.truncated),
			None => (Some(self.program), &[], false),
		}
	}
}
