//! Cold graph traversal for the sealed program domain; never a root-subset GC API.
use super::*;
use crate::alloc::HashSet;
impl Store {
	pub(crate) fn domain_identity(&self) -> u64 {
		self.id
	}
	pub(crate) fn reachable_programs<'a>(
		&self,
		roots: impl Iterator<Item = &'a super::super::value::Root>,
	) -> Result<HashSet<u32>> {
		if self.closed {
			return Err(err(ErrorKind::ClosedStore));
		}
		let mut todo = Vec::new();
		let mut visited = HashSet::new();
		let mut owners = HashSet::new();
		for root in roots {
			todo.try_push(self.import(root)?)?;
		}
		for value in &self.work {
			todo.try_push(*value)?;
		}
		while let Some(value) = todo.pop() {
			match value {
				Value::Function { code, .. } => {
					owners.try_insert(code)?;
				}
				Value::Heap(h) => {
					if !visited.try_insert(h)? {
						continue;
					}
					let entry = self.entry(h)?;
					let Some(payload) = &entry.payload else {
						continue;
					};
					if let Payload::Closure { code, .. } = payload {
						owners.try_insert(*code)?;
					}
					for child in payload.children() {
						todo.try_push(child)?;
					}
				}
				Value::Unit
				| Value::Bool(_)
				| Value::I64(_)
				| Value::U64(_)
				| Value::F64(_)
				| Value::Char(_)
				| Value::Ordering(_)
				| Value::Type(_) => {}
			}
		}
		Ok(owners)
	}
}
