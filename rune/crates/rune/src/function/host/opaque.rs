//! Explicit native-type enrollment. All erased casts use the existing Any TypeId guard.
use crate as rune;
use crate::alloc;
use crate::runtime::{AnyTypeInfo, Formatter, Value, VmError};
use crate::sync::Arc;
use crate::{Any, Hash};

pub(crate) const DISPLAY: &str = "__opaque_display";
pub(crate) fn display_hash(hash: Hash) -> Hash {
	Hash::associated_function(hash, Hash::ident(DISPLAY))
}

pub(crate) fn reserved(hash: Hash) -> bool {
	use crate::TypeHash;
	crate::function::methods::RECEIVERS.contains(&hash)
		|| hash == <()>::HASH
		|| [
			crate::runtime::Bytes::HASH,
			crate::modules::collections::HashMap::HASH,
			crate::modules::collections::HashSet::HASH,
		]
		.contains(&hash)
}

crate::declare_dyn_fn! {
	struct InspectVTable;
	pub(crate) struct Inspect {
		fn call(value: &Value) -> Result<usize, VmError>;
	}
}
crate::declare_dyn_fn! {
	struct DisplayVTable;
	pub(crate) struct Display {
		fn call(value: &Value, formatter: &mut Formatter) -> Result<(), VmError>;
	}
}

/// Immutable authority retained with every imported host payload.
pub(crate) struct Descriptor {
	pub(crate) hash: Hash,
	pub(crate) type_id: core::any::TypeId,
	pub(crate) info: AnyTypeInfo,
	pub(crate) inspect: Inspect,
	pub(crate) display: Display,
}

impl Descriptor {
	pub(crate) fn new<T: Any>(
		bytes: fn(&T) -> usize,
		display: fn(&T, &mut Formatter) -> Result<(), VmError>,
	) -> alloc::Result<Arc<Self>> {
		Arc::try_new(Self {
			hash: T::HASH,
			type_id: core::any::TypeId::of::<T>(),
			info: T::ANY_TYPE_INFO,
			inspect: Inspect::new(move |value| Ok(bytes(&*value.borrow_ref::<T>()?)))?,
			display: Display::new(move |value, formatter| {
				display(&*value.borrow_ref::<T>()?, formatter)
			})?,
		})
	}
}

impl core::fmt::Debug for Descriptor {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("HostValueDescriptor")
			.field("hash", &self.hash)
			.finish()
	}
}

#[derive(Clone)]
pub(crate) struct Registration {
	pub(crate) descriptor: Arc<Descriptor>,
	pub(crate) display_handler: crate::runtime::FunctionHandler,
}
impl crate::alloc::clone::TryClone for Registration {
	fn try_clone(&self) -> alloc::Result<Self> {
		Ok(self.clone())
	}
}

#[derive(Default, crate::alloc::prelude::TryClone)]
pub(crate) struct State {
	pub(crate) types: crate::hash::Map<Registration>,
	pub(crate) factories: crate::hash::Map<NativeType>,
	pub(crate) methods: crate::hash::Map<NativeType>,
}

#[derive(Clone, Copy, crate::alloc::prelude::TryClone)]
#[try_clone(copy)]
pub(crate) struct NativeType {
	pub(crate) hash: Hash,
	pub(crate) id: core::any::TypeId,
	pub(crate) info: AnyTypeInfo,
}
impl NativeType {
	pub(crate) fn of<T: Any>() -> Self {
		Self {
			hash: T::HASH,
			id: core::any::TypeId::of::<T>(),
			info: T::ANY_TYPE_INFO,
		}
	}
}
impl State {
	pub(crate) fn validate(&self) -> Result<(), crate::ContextError> {
		for (hash, registered) in &self.types {
			if reserved(*hash) {
				return Err(crate::ContextError::OpaqueBuiltin {
					info: registered.descriptor.info.into(),
				});
			}
		}
		for native in self.factories.values().chain(self.methods.values()) {
			let registered = self.types.get(&native.hash);
			if !registered.is_some_and(|r| r.descriptor.type_id == native.id) {
				return Err(crate::ContextError::OpaqueType {
					declared: native.info.into(),
					registered: registered.map(|r| r.descriptor.info.into()),
				});
			}
		}
		Ok(())
	}
	pub(crate) fn merged(&self, other: &Self) -> alloc::Result<Self> {
		use crate::alloc::clone::TryClone;
		let mut next = self.try_clone()?;
		for (hash, value) in &other.types {
			next.types.try_insert(*hash, value.clone())?;
		}
		for (hash, value) in &other.factories {
			next.factories.try_insert(*hash, *value)?;
		}
		for (hash, value) in &other.methods {
			next.methods.try_insert(*hash, *value)?;
		}
		Ok(next)
	}
}
