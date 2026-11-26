//! Cold, fixed-arity string intrinsics. No runtime routing into legacy handlers.
use super::*;
impl Executor {
	#[cold]
	#[inline(never)]
	#[allow(clippy::too_many_arguments)]
	pub(super) fn text_call(
		&mut self,
		store: &mut Store,
		f: &super::super::code::Function,
		base: usize,
		out: u32,
		id: u32,
		start: u32,
		len: u32,
	) -> Result<()> {
		let invalid = || error(ErrorKind::InvalidCode);
		if id == super::super::code::DEBUG_LINES {
			return self.debug_lines(store, f, base, out, start, len);
		}
		if id >= 0x8000_0070 {
			return self.pipeline_call(store, f, base, out, id, start, len);
		}
		if out >= f.slots || len > 3 {
			return Err(invalid());
		}
		let end = start.checked_add(len).ok_or_else(invalid)?;
		let args = f
			.args
			.get(start as usize..end as usize)
			.ok_or_else(invalid)?;
		let mut values = [Value::Unit; 3];
		for (v, &slot) in values.iter_mut().zip(args) {
			if slot >= f.slots {
				return Err(invalid());
			}
			*v = *self
				.slots
				.get(base.checked_add(slot as usize).ok_or_else(invalid)?)
				.ok_or_else(invalid)?;
		}
		let args = &values[..len as usize];
		let result = if let Some(op) = crate::function::string_builder::Operation::from_id(id) {
			if len != op.arity() {
				return Err(invalid());
			}
			store.string_builder(op, values[0], values[1])?
		} else {
			let method = crate::function::text::Method::ALL
				.into_iter()
				.find(|m| m.id() == id && m.arity() == len)
				.ok_or_else(invalid)?;
			store.text_method(method, args)?
		};
		self.put(
			store,
			base.checked_add(out as usize).ok_or_else(invalid)?,
			result,
		)
	}
}

impl Executor {
	#[cold]
	#[inline(never)]
	#[allow(clippy::too_many_arguments)]
	fn pipeline_call(
		&mut self,
		store: &mut Store,
		f: &super::super::code::Function,
		base: usize,
		out: u32,
		id: u32,
		start: u32,
		len: u32,
	) -> Result<()> {
		let invalid = || error(ErrorKind::InvalidCode);
		let end = start.checked_add(len).ok_or_else(invalid)?;
		let args = f
			.args
			.get(start as usize..end as usize)
			.ok_or_else(invalid)?;
		if args.len() > 2 || out >= f.slots {
			return Err(invalid());
		}
		let mut v = [Value::Unit; 2];
		for (dst, &slot) in v.iter_mut().zip(args) {
			if slot >= f.slots {
				return Err(invalid());
			}
			*dst = *self
				.slots
				.get(base.checked_add(slot as usize).ok_or_else(invalid)?)
				.ok_or_else(invalid)?;
		}
		let result = match id {
			0x8000_0070 => store.pipeline_chars(v[0])?,
			0x8000_0071 => store.pipeline_unwrap(v[0], v[1])?,
			0x8000_0072 | 0x8000_0073 => store.pipeline_adapter(v[0], v[1], id == 0x8000_0073)?,
			0x8000_0075 => {
				let next = store.iterator_next(v[0])?;
				let result = store.variant(super::super::value::Type::Option, next.is_some(), next);
				if let Some(n) = next {
					store.release(n)?;
				}
				result?
			}
			0x8000_0078 => store.pipeline_child(v[0])?,
			0x8000_0079 => store.pipeline_remaining(v[0], false)?,
			0x8000_007a => store.pipeline_remaining(v[0], true)?,
			0x8000_007b => store.pipeline_append(v[0], v[1])?,
			_ => return Err(invalid()),
		};
		self.put(
			store,
			base.checked_add(out as usize).ok_or_else(invalid)?,
			result,
		)
	}
}

impl Executor {
	#[cold]
	fn debug_lines(
		&mut self,
		store: &mut Store,
		f: &super::super::code::Function,
		base: usize,
		out: u32,
		start: u32,
		len: u32,
	) -> Result<()> {
		use std::io::Write;
		let invalid = || error(ErrorKind::InvalidCode);
		let end = start.checked_add(len).ok_or_else(invalid)?;
		let args = f
			.args
			.get(start as usize..end as usize)
			.ok_or_else(invalid)?;
		let dest = base.checked_add(out as usize).ok_or_else(invalid)?;
		if out >= f.slots || dest >= self.slots.len() {
			return Err(invalid());
		}
		let mut stdout = std::io::stdout().lock();
		for &slot in args {
			if slot >= f.slots {
				return Err(invalid());
			}
			let value = *self
				.slots
				.get(base.checked_add(slot as usize).ok_or_else(invalid)?)
				.ok_or_else(invalid)?;
			let text = store.debug_line(value, &self.format_limits)?;
			#[cfg(test)]
			if let Some(capture) = self.test_output.as_mut() {
				capture.extend_from_slice(text.as_bytes());
				capture.push(b'\n');
				continue;
			}
			writeln!(stdout, "{text}").map_err(|e| {
				error(ErrorKind::Output).with_detail(Detail::Io {
					kind: e.kind(),
					os: e.raw_os_error(),
				})
			})?;
			#[cfg(test)]
			if let Some(hook) = self.test_after_print {
				hook();
			}
		}
		self.put(store, dest, Value::Unit)
	}
}
