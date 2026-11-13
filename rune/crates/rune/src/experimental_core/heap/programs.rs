//! Cold traversal for the sealed program domain; never a root-subset GC API.
use super::*;
use crate::alloc::HashSet;

#[cfg(test)]
std::thread_local! {
 static PROBE: core::cell::Cell<(usize, usize)> = const { core::cell::Cell::new((0, 0)) };
}
impl Store {
	pub(crate) fn domain_identity(&self) -> u64 {
		self.id
	}
	#[cfg(test)]
	pub(crate) fn collection_probe() -> (usize, usize) {
		PROBE.with(|probe| probe.replace((0, 0)))
	}
	// Only heap handles belong on the work list: scalar/function bits need no copies.
	fn program_root(&self, value: Value, missing: &mut HashSet<u32>) -> Result<Option<Handle>> {
		#[cfg(test)]
		PROBE.with(|probe| {
			let (walks, values) = probe.get();
			probe.set((walks, values + 1));
		});
		match value {
			Value::Function { code, .. } => {
				missing.remove(&code);
				Ok(None)
			}
			Value::Heap(handle) => {
				if let Some(Payload::Closure { code, .. }) = &self.entry(handle)?.payload {
					missing.remove(code);
				}
				Ok(Some(handle))
			}
			Value::Unit
			| Value::Bool(_)
			| Value::I64(_)
			| Value::U64(_)
			| Value::F64(_)
			| Value::Char(_)
			| Value::Ordering(_)
			| Value::Type(_) => Ok(None),
		}
	}
	/// Missing IDs are dead only after success; allocation failure commits no decision.
	pub(crate) fn find_programs<'a>(
		&self,
		roots: impl Iterator<Item = &'a super::super::value::Root> + Clone,
		missing: &mut HashSet<u32>,
	) -> Result<()> {
		if self.closed {
			return Err(err(ErrorKind::ClosedStore));
		}
		#[cfg(test)]
		PROBE.with(|probe| {
			let (walks, values) = probe.get();
			probe.set((walks + 1, values));
		});
		// Inspect all direct roots/work before any descent, independent of binding order.
		// No work-list allocation is needed to find a top-level live closure.
		for root in roots.clone() {
			self.program_root(self.import(root)?, missing)?;
		}
		for value in &self.work {
			self.program_root(*value, missing)?;
		}
		if missing.is_empty() {
			return Ok(());
		}
		let mut todo = Vec::new();
		let mut visited = HashSet::new();
		for root in roots {
			if let Value::Heap(handle) = self.import(root)? {
				todo.try_push(handle)?;
			}
		}
		for value in &self.work {
			if let Value::Heap(handle) = *value {
				todo.try_push(handle)?;
			}
		}
		while let Some(handle) = todo.pop() {
			if !visited.try_insert(handle)? {
				continue;
			}
			let entry = self.entry(handle)?;
			let Some(payload) = &entry.payload else {
				continue;
			};
			for child in payload.children() {
				let next = self.program_root(child, missing)?;
				if missing.is_empty() {
					return Ok(());
				}
				if let Some(handle) = next {
					todo.try_push(handle)?;
				}
			}
		}
		Ok(())
	}
}
