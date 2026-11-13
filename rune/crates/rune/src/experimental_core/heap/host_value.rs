//! Store ownership of a native object, without exposing an owning native handle.
use super::*;
use crate::experimental_core::value::Root;
use crate::function::host::opaque::Descriptor;
use crate::runtime;
use crate::sync::Arc;

#[derive(Debug)]
pub(super) struct HostValue {
	value: Option<runtime::Value>,
	pub(super) descriptor: Arc<Descriptor>,
	pub(super) bytes: usize,
}

/// Taking the native payload marks every temporary alias moved before Drop runs.
/// Even a panicking destructor is never called twice by the remaining handle.
pub(crate) fn dispose(value: &runtime::Value) -> Result<()> {
	match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| value.clone().drop())) {
		Ok(Ok(())) => Ok(()),
		Ok(Err(native)) if matches!(native.error().kind(), runtime::VmErrorKind::AccessError { error } if error.is_moved()) => {
			Ok(())
		}
		Ok(Err(_)) => Err(err(ErrorKind::AccessConflict)),
		Err(_) => Err(err(ErrorKind::Host).with_detail(super::super::error::Detail::HostPanicked)),
	}
}

impl HostValue {
	pub(super) fn inert(descriptor: Arc<Descriptor>, bytes: usize) -> Self {
		Self {
			value: None,
			descriptor,
			bytes,
		}
	}

	pub(super) fn dispose(&mut self) -> Result<()> {
		match self.value.take() {
			Some(value) => dispose(&value),
			None => Ok(()),
		}
	}
	pub(super) fn value(&self) -> Result<&runtime::Value> {
		self.value
			.as_ref()
			.ok_or_else(|| err(ErrorKind::AccessConflict))
	}
}

impl Drop for HostValue {
	fn drop(&mut self) {
		// Failed admission/unwinding still destroys the unpublished payload once.
		let _ = self.dispose();
	}
}

impl Store {
	pub(crate) fn host_display(&self, value: Value, limit: usize) -> Result<String> {
		let Value::Heap(handle) = value else {
			return Err(err(ErrorKind::Type));
		};
		let Payload::Host { value: host, .. } = self.payload(handle)? else {
			return Err(err(ErrorKind::Type));
		};
		let native = host.value()?;
		let called = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
			runtime::Formatter::bounded(limit, |formatter| {
				host.descriptor.display.call(native, formatter)
			})
		}));
		let (result, text, exceeded) = called.map_err(|_| {
			err(ErrorKind::Host).with_detail(super::super::error::Detail::HostPanicked)
		})?;
		if exceeded {
			return Err(err(ErrorKind::FormatLimit));
		}
		result.map_err(|_| err(ErrorKind::Host))?;
		Ok(text)
	}
	/// Only the cold typed host output bridge may call this constructor.
	pub(crate) fn host_value(
		&mut self,
		value: &runtime::Value,
		descriptor: Arc<Descriptor>,
	) -> Result<Value> {
		if value.type_hash() != descriptor.hash
			|| value.as_any().and_then(|v| v.owned_ref_count()) != Some(1)
		{
			return Err(err(ErrorKind::InvalidCode));
		}
		// The typed inspector checks native TypeId and holds only a shared guard.
		let declared = descriptor
			.inspect
			.call(value)
			.map_err(|_| err(ErrorKind::Host))?;
		let bytes = declared
			.checked_add(size_of::<HostValue>())
			.ok_or_else(|| err(ErrorKind::Overflow))?;
		self.host_room(bytes, 1)?;
		let nominal = super::super::nominal::intern_fmt(
			descriptor.hash.into_inner(),
			format_args!("{}", descriptor.info),
		)?;
		// Finish every allocation and bookkeeping check with an inert payload.
		// On failure, the caller still owns the native output and can report
		// a destructor panic as secondary rather than losing it in Drop.
		let host = HostValue::inert(descriptor, bytes);
		let payload = Payload::Host {
			tag: Type::Host(nominal),
			value: Box::try_new(host).map_err(crate::alloc::Error::from)?,
		};
		// The sole native constructor activates the side array before admission.
		// Plain allocations therefore need only the store-level mode test.
		self.prepare_host_contents(&payload)?;
		let admitted = self.admit(payload)?;
		let Value::Heap(handle) = admitted else {
			return Err(err(ErrorKind::InvalidCode));
		};
		let entry = self
			.entry_mut(handle)
			.map_err(|_| err(ErrorKind::InvalidCode))?;
		let Some(Payload::Host { value: host, .. }) = entry.payload.as_mut() else {
			return Err(err(ErrorKind::InvalidCode));
		};
		host.value = Some(value.clone());
		Ok(admitted)
	}

	pub(crate) fn host_snapshot(&self, value: Value) -> Result<runtime::Value> {
		let Value::Heap(handle) = value else {
			return Err(err(ErrorKind::Type));
		};
		let Payload::Host { value: host, .. } = self.payload(handle)? else {
			return Err(err(ErrorKind::Type));
		};
		Ok(host.value()?.clone())
	}
}

/// Native identity is meaningful only after the Store owner/generation check.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Identity {
	pub(crate) hash: crate::Hash,
	pub(crate) native: Option<core::any::TypeId>,
}
impl Store {
	pub(crate) fn session_identity(&self, root: &Root) -> Result<Option<Identity>> {
		self.session_value_identity(self.import(root)?)
	}
	fn session_value_identity(&self, value: Value) -> Result<Option<Identity>> {
		if let Value::Heap(handle) = value {
			let payload = self
				.entry(handle)?
				.payload
				.as_ref()
				.ok_or_else(|| err(ErrorKind::InvalidHandle))?;
			match payload {
				Payload::Host { value: host, .. } => {
					host.value()?;
					return Ok(Some(Identity {
						hash: host.descriptor.hash,
						native: Some(host.descriptor.type_id),
					}));
				}
				Payload::Moved(Type::Host(nominal)) => {
					let hash = crate::Hash::new(
						super::super::nominal::hash_copy(*nominal)
							.ok_or_else(|| err(ErrorKind::InvalidCode))?,
					);
					return Ok(Some(Identity { hash, native: None }));
				}
				Payload::Moved(_) => return Ok(None),
				_ => {}
			}
		}
		// Construction propagates this mark from direct children. No data walk
		// or graph-sized scratch is needed at a session boundary.
		if self.contains_host(value)? {
			return Err(Error::refused(
				super::super::error::Refusal::OpaqueContainer,
			));
		}
		Ok(None)
	}
	pub(crate) fn validate_session_record(
		&self,
		root: &Root,
		expected: &[(String, Option<Identity>)],
	) -> Result<()> {
		let Value::Heap(handle) = self.import(root)? else {
			return Err(err(ErrorKind::Type));
		};
		let Payload::Record {
			tag: Type::Object,
			fields,
		} = self.payload(handle)?
		else {
			return Err(err(ErrorKind::Type));
		};
		if fields.len() != expected.len() {
			return Err(Error::refused(super::super::error::Refusal::OpaqueEnvelope));
		}
		for ((name, identity), (key, value)) in expected.iter().zip(fields) {
			if name != key || self.session_value_identity(*value)? != *identity {
				return Err(Error::refused(super::super::error::Refusal::OpaqueEnvelope));
			}
		}
		Ok(())
	}
}

impl Store {
	fn contains_host(&self, value: Value) -> Result<bool> {
		let Value::Heap(handle) = value else {
			return Ok(false);
		};
		self.entry(handle)?;
		Ok(self
			.host_contents
			.as_ref()
			.and_then(|marks| marks.get(handle.slot as usize))
			.copied()
			.unwrap_or(false))
	}
	pub(super) fn plain_host_child(&self, value: Value) -> Result<()> {
		if self.contains_host(value)? {
			return Err(Error::refused(
				super::super::error::Refusal::OpaqueContainer,
			));
		}
		Ok(())
	}
	pub(super) fn prepare_host_contents(&mut self, payload: &Payload) -> Result<bool> {
		let direct = matches!(payload, Payload::Host { .. });
		if self.host_contents.is_none() && !direct {
			return Ok(false);
		}
		let mut marked = direct;
		for child in payload.admission_children() {
			marked |= self.contains_host(child)?;
		}
		let required = self
			.slots
			.len()
			.checked_add(1)
			.ok_or_else(|| err(ErrorKind::Overflow))?;
		if self.host_contents.is_none() {
			self.host_contents = Some(Box::try_new(Vec::new()).map_err(crate::alloc::Error::from)?);
		}
		let Some(marks) = &mut self.host_contents else {
			return Err(err(ErrorKind::InvalidCode));
		};
		let additional = required.saturating_sub(marks.len());
		marks.try_reserve(additional)?;
		// The first host allocation realizes the implicit false marks for prior
		// slots. This one-time metadata initialization does not walk payloads.
		while marks.len() < required {
			marks.try_push(false)?;
		}
		Ok(marked)
	}
}
