//! Typed native-object enrollment; no owned or mutable receiver API is provided.
use super::*;
use crate::function::host::opaque::{Descriptor, NativeType, Registration};
use crate::function::host::{
	HostEnrollment, HostOutput, HostOutputKind, HostSignature, HostType, OutputCertificate,
};
use crate::function::Ref;
use crate::function_meta::{OpaqueAssociatedData, OpaqueAssociatedKind};
use crate::runtime::{Formatter, Value};

impl Module {
	pub(super) fn insert_opaque_associated(
		&mut self,
		metadata: OpaqueAssociatedData,
		docs: Docs,
		deprecated: Option<Box<str>>,
	) -> Result<ItemFnMut<'_>, ContextError> {
		let OpaqueAssociatedData { data, native, kind } = metadata;
		let association = &data.associated;
		let registered = self.opaque.types.get(&native.hash);
		if association.container != native.hash
			|| !registered.is_some_and(|r| r.descriptor.type_id == native.id)
		{
			return Err(ContextError::OpaqueType {
				declared: native.info.into(),
				registered: registered.map(|r| r.descriptor.info.into()),
			});
		}
		if !matches!(association.name.kind, meta::AssociatedKind::Instance(_))
			|| association.name.function_parameters != Hash::EMPTY
		{
			return Err(ContextError::InternalAlreadyPresent {
				name: "nonstandard opaque association",
			});
		}
		let name = Name::Associated(association.as_key()?);
		let key = association.name.kind.hash(association.container);
		if self.names.contains(&name)
			|| self.opaque.factories.contains_key(&key)
			|| self.opaque.methods.contains_key(&key)
		{
			return Err(Self::associated_conflict(association)?);
		}
		let (map, arity, input, output) = match kind {
			OpaqueAssociatedKind::Factory => (
				&mut self.opaque.factories,
				0,
				HostType::Unit,
				HostOutputKind::OpaqueOwned,
			),
			OpaqueAssociatedKind::Method(result) => (
				&mut self.opaque.methods,
				1,
				HostType::OpaqueShared,
				HostOutputKind::Scalar(result),
			),
		};
		map.try_reserve(1)?;
		self.names.try_reserve(1)?;
		self.associated.try_reserve(1)?;
		self.names.try_insert(name)?;
		map.try_insert(key, native)?;
		self.push_associated_function(
			data,
			docs,
			deprecated,
			Some(HostEnrollment {
				signature: HostSignature {
					arity,
					arguments: [input, HostType::Unit, HostType::Unit, HostType::Unit],
					output,
				},
				certificate: None,
			}),
		)
	}

	/// Register a host value type, its logical payload charge and bounded display.
	/// The host promises no hidden execution Roots or callbacks into the engine.
	pub fn opaque_type<T>(
		&mut self,
		bytes: fn(&T) -> usize,
		display: fn(&T, &mut Formatter) -> Result<(), VmError>,
	) -> Result<(), ContextError>
	where
		T: crate::Any + TypeOf + Named + InstallWith + MaybeTypeOf + crate::runtime::UnsafeToRef,
	{
		if crate::function::host::opaque::reserved(T::HASH) {
			return Err(ContextError::OpaqueBuiltin {
				info: T::ANY_TYPE_INFO.into(),
			});
		}
		self.ty::<T>()?;
		let descriptor = Descriptor::new(bytes, display)?;
		self.associated_function(&Protocol::DISPLAY_FMT, display)?;
		let Some(ModuleAssociated {
			kind: ModuleAssociatedKind::Function(function),
			..
		}) = self.associated.last()
		else {
			return Err(ContextError::InternalAlreadyPresent {
				name: "host value display registration",
			});
		};
		self.opaque.types.try_insert(
			T::HASH,
			Registration {
				descriptor,
				display_handler: function.handler.clone(),
			},
		)?;
		// A compiler-selected scalar adapter renders once, after template
		// operands have run. It never passes the native payload to Template.
		self.opaque_method::<T, _, _>(
			crate::function::host::opaque::DISPLAY,
			move |value: &T| -> Result<std::string::String, VmError> {
				let (result, text, exceeded) = Formatter::bounded(262_144, |f| display(value, f));
				if exceeded {
					return Err(VmError::panic("host value display byte limit"));
				}
				result?;
				Ok(text.into_std())
			},
			None,
		)?;
		Ok(())
	}

	/// Register a zero-argument factory returning a uniquely owned native T.
	/// Execution admission is restricted to the checked session entry.
	///
	/// ```
	/// #[derive(rune::Any)] struct Object;
	/// let mut module = rune::Module::new();
	/// module.opaque_factory::<Object, _, _>("make", || Object).unwrap();
	/// ```
	/// ```compile_fail
	/// #[derive(rune::Any)] struct Object;
	/// let mut module = rune::Module::new();
	/// module.opaque_factory::<Object, _, _>("make", |_: i64| Object).unwrap();
	/// ```
	pub fn opaque_factory<T, F, N>(&mut self, name: N, factory: F) -> Result<(), ContextError>
	where
		T: crate::Any,
		F: Fn() -> T + Send + Sync + 'static,
		N: IntoComponent,
	{
		if let Some(registered) = self
			.opaque
			.types
			.get(&T::HASH)
			.filter(|r| r.descriptor.type_id != core::any::TypeId::of::<T>())
		{
			return Err(ContextError::OpaqueType {
				declared: T::ANY_TYPE_INFO.into(),
				registered: Some(registered.descriptor.info.into()),
			});
		}
		let mut data =
			FunctionData::new::<N, _, (), Plain>(name, move || -> Result<Value, VmError> {
				Ok(Value::new(factory())?)
			})?;
		let key = Hash::type_hash(&self.item.join(&data.item)?);
		data.host = Some(HostEnrollment {
			signature: HostSignature {
				arity: 0,
				arguments: [HostType::Unit; 4],
				output: HostOutputKind::OpaqueOwned,
			},
			certificate: None,
		});
		self.function_inner(data, Docs::EMPTY, None)?;
		self.opaque
			.factories
			.try_insert(key, NativeType::of::<T>())?;
		Ok(())
	}

	/// Register a checked session producer beside this function's legacy handler.
	/// The checked callable only sees a string snapshot and an owned tree packet.
	/// Both handlers are installed under the same name.
	///
	/// ```compile_fail,E0631
	/// #[derive(rune::Any)] struct Native;
	/// let mut module = rune::Module::new();
	/// module.opaque_result_function("borrowed", |_, _, _, _| Ok(()),
	///     |_: &str, _: &rune::host::OwnedTreeInput| Ok(Ok(Native)));
	/// ```
	/// ```compile_fail,E0631
	/// #[derive(rune::Any)] struct Native;
	/// let mut module = rune::Module::new();
	/// module.opaque_result_function("mutable", |_, _, _, _| Ok(()),
	///     |_: &str, _: &mut rune::host::OwnedTreeInput| Ok(Ok(Native)));
	/// ```
	/// ```compile_fail,E0631
	/// #[derive(rune::Any)] struct Native;
	/// let mut module = rune::Module::new();
	/// module.opaque_result_function("value", |_, _, _, _| Ok(()),
	///     |_: &str, _: rune::Value| Ok(Ok(Native)));
	/// ```
	/// ```compile_fail,E0593
	/// let mut module = rune::Module::new();
	/// module.opaque_result_function("raw", |_, _, _, _| Ok(()), |_, _, _, _| Ok(()));
	/// ```
	/// ```compile_fail,E0308
	/// #[derive(rune::Any)] struct Native;
	/// let mut module = rune::Module::new();
	/// module.opaque_result_function("non_string_error", |_, _, _, _| Ok(()),
	///     |_, _| Ok(Err::<Native, i64>(7)));
	/// ```
	#[doc(hidden)]
	pub fn opaque_result_function<N, F, T, G>(
		&mut self,
		name: N,
		legacy: F,
		producer: G,
	) -> Result<ItemFnMut<'_>, ContextError>
	where
		N: IntoComponent,
		F: Fn(&mut dyn Memory, Address, usize, Output) -> Result<(), VmError>
			+ 'static
			+ Send
			+ Sync,
		T: crate::Any,
		G: Fn(
				&str,
				crate::host::OwnedTreeInput,
			) -> Result<Result<T, std::string::String>, crate::host::OpaqueProducerLimit>
			+ 'static
			+ Send
			+ Sync,
	{
		use crate::host::{AlternateHost, OwnedTreeInput};
		use crate::runtime::FunctionHandler;

		let native = NativeType::of::<T>();
		let registered = self.opaque.types.get(&native.hash);
		if !registered.is_some_and(|entry| entry.descriptor.type_id == native.id) {
			return Err(ContextError::OpaqueType {
				declared: native.info.into(),
				registered: registered.map(|entry| entry.descriptor.info.into()),
			});
		}
		let item = ItemBuf::with_item([name])?;
		let path = self.item.join(&item)?;
		let hash = Hash::type_hash(&path);
		if self.names.contains(&Name::Item(hash))
			|| self.opaque.factories.contains_key(&hash)
			|| self.opaque.methods.contains_key(&hash)
		{
			return Err(ContextError::ConflictingFunctionName { item: path, hash });
		}

		let checked = move |name: &str, tree: OwnedTreeInput| -> Result<Value, VmError> {
			let result = match producer(name, tree) {
				Ok(Ok(native)) => Ok(Value::new(native)?),
				Ok(Err(message)) => Err(crate::to_value(message)?),
				Err(limit) => return Ok(Value::new(limit)?),
			};
			Ok(Value::try_from(result)?)
		};
		let handler = FunctionHandler::new(move |stack, addr, args, out| {
			Function::<(Ref<str>, OwnedTreeInput), Plain>::call(&checked, stack, addr, args, out)
		})?;
		let mut data = FunctionData::from_raw(item, FunctionHandler::new(legacy)?);
		data.alternate_host = Some(AlternateHost {
			await_only: false,
			handler,
			enrollment: HostEnrollment {
				signature: HostSignature {
					arity: 2,
					arguments: [
						HostType::String,
						HostType::OwnedTreeInput,
						HostType::Unit,
						HostType::Unit,
					],
					output: HostOutputKind::OpaqueResultString,
				},
				certificate: None,
			},
		});
		self.names.try_reserve(1)?;
		self.items.try_reserve(1)?;
		self.opaque.factories.try_reserve(1)?;
		self.opaque.factories.try_insert(hash, native)?;
		self.insert_function_item(data, Docs::EMPTY, None, path, hash)
	}

	/// Register a shared receiver with no additional arguments. R uses the
	/// existing closed scalar or explicitly certified builtin output domain.
	///
	/// ```
	/// #[derive(rune::Any)] struct Object;
	/// let mut module = rune::Module::new();
	/// module.opaque_method::<Object, _, _>("read", |_: &Object| 7i64, None).unwrap();
	/// ```
	/// ```compile_fail
	/// #[derive(rune::Any)] struct Object;
	/// let mut module = rune::Module::new();
	/// module.opaque_method::<Object, _, _>("read", |_: Object| 7i64, None).unwrap();
	/// ```
	/// ```compile_fail
	/// #[derive(rune::Any)] struct Object;
	/// let mut module = rune::Module::new();
	/// module.opaque_method::<Object, _, _>("read", |_: &mut Object| 7i64, None).unwrap();
	/// ```
	/// ```compile_fail
	/// #[derive(rune::Any)] struct Object;
	/// let mut module = rune::Module::new();
	/// module.opaque_method::<Object, _, _>("read", |_: &Object, _: i64| 7i64, None).unwrap();
	/// ```
	/// ```compile_fail
	/// #[derive(rune::Any)] struct Object;
	/// let mut module = rune::Module::new();
	/// module.opaque_method::<Object, _, _>("read", |_: &Object| Object, None).unwrap();
	/// ```
	pub fn opaque_method<T, F, R>(
		&mut self,
		name: &'static str,
		method: F,
		certificate: Option<OutputCertificate>,
	) -> Result<(), ContextError>
	where
		T: crate::Any + TypeOf + MaybeTypeOf,
		F: for<'a> Fn(&'a T) -> R + InstanceFunction<(Ref<T>,), Plain, Return = R>,
		R: HostOutput + MaybeTypeOf,
	{
		if let Some(registered) = self
			.opaque
			.types
			.get(&T::HASH)
			.filter(|r| r.descriptor.type_id != core::any::TypeId::of::<T>())
		{
			return Err(ContextError::OpaqueType {
				declared: T::ANY_TYPE_INFO.into(),
				registered: Some(registered.descriptor.info.into()),
			});
		}
		let data = AssociatedFunctionData::from_instance_function(name.to_instance()?, method)?;
		self.insert_associated_function(data, Docs::EMPTY, None)?;
		let Some(associated) = self.associated.last_mut() else {
			return Err(ContextError::InternalAlreadyPresent {
				name: "host value method registration",
			});
		};
		let key = associated
			.name
			.kind
			.hash(associated.container)
			.with_function_parameters(associated.name.function_parameters);
		let ModuleAssociatedKind::Function(function) = &mut associated.kind else {
			return Err(ContextError::InternalAlreadyPresent {
				name: "host value method kind",
			});
		};
		function.host = Some(HostEnrollment {
			signature: HostSignature {
				arity: 1,
				arguments: [
					HostType::OpaqueShared,
					HostType::Unit,
					HostType::Unit,
					HostType::Unit,
				],
				output: R::SHAPE,
			},
			certificate,
		});
		self.opaque.methods.try_insert(key, NativeType::of::<T>())?;
		Ok(())
	}
}

#[cfg(test)]
mod associated_metadata_tests {
	use super::*;
	use crate::Context;

	#[derive(rune::Any)]
	#[rune(item = ::opaque_test)]
	struct Native(i64);

	#[derive(rune::Any)]
	#[rune(item = ::opaque_test, name = Native)]
	struct Impostor;

	#[rune::function(free, path = Native::make, opaque_factory)]
	fn make() -> Native {
		Native(42)
	}

	#[rune::function(instance, path = get, opaque_method)]
	fn get(value: &Native) -> i64 {
		value.0
	}

	#[rune::function(free, path = Impostor::make, opaque_factory)]
	fn impostor_factory() -> Impostor {
		Impostor
	}

	#[test]
	fn fallible_native_producer_checks_nominal_type_before_registration() {
		let mut module = Module::with_crate("opaque_test").unwrap();
		let legacy = |_: &mut dyn Memory, _: Address, _: usize, _: Output| Ok(());
		assert!(matches!(
			module.opaque_result_function("load", legacy, |_, _| Ok(Ok(Native(7)))),
			Err(ContextError::OpaqueType {
				registered: None,
				..
			})
		));
		assert!(module.items.is_empty());
		assert!(module.opaque.factories.is_empty());
		module.opaque_type::<Native>(|_| 0, |_, _| Ok(())).unwrap();
		assert!(matches!(
			module.opaque_result_function("load", legacy, |_, _| Ok(Ok(Impostor))),
			Err(ContextError::OpaqueType {
				registered: Some(_),
				..
			})
		));
		assert!(module.items.is_empty());
		assert!(module.opaque.factories.is_empty());
		module
			.opaque_result_function("load", legacy, |_, _| Ok(Ok(Native(7))))
			.unwrap();
		assert!(matches!(
			module.opaque_result_function("load", legacy, |_, _| Ok(Ok(Native(8)))),
			Err(ContextError::ConflictingFunctionName { .. })
		));
		assert_eq!(module.items.len(), 1);
		assert_eq!(module.opaque.factories.len(), 1);
	}

	#[test]
	fn mismatched_and_duplicate_proofs_leave_the_original_association() {
		let mut module = Module::with_crate("opaque_test").unwrap();
		assert!(matches!(
			module.function_meta(make),
			Err(ContextError::OpaqueType {
				registered: None,
				..
			})
		));
		assert!(module.opaque.factories.is_empty());
		module.opaque_type::<Native>(|_| 0, |_, _| Ok(())).unwrap();
		assert_eq!(Native::HASH, Impostor::HASH);
		assert!(matches!(
			module.function_meta(impostor_factory),
			Err(ContextError::OpaqueType {
				registered: Some(_),
				..
			})
		));
		assert!(module.opaque.factories.is_empty());
		module.function_meta(make).unwrap();
		module.function_meta(get).unwrap();
		let before = (module.associated.len(), module.names.len());
		for meta in [make as FunctionMeta, get] {
			assert!(matches!(
				module.function_meta(meta),
				Err(ContextError::ConflictingInstanceFunction { .. })
			));
		}
		assert_eq!((module.associated.len(), module.names.len()), before);
		let associated = Hash::associated_function(Native::HASH, Hash::ident("make"));
		let path = ItemBuf::with_crate_item("opaque_test", ["Native", "make"]).unwrap();
		let path = Hash::type_hash(&path);
		let mut context = Context::new();
		context.install(module).unwrap();
		let binding = context.host(associated).unwrap();
		assert!(binding
			.handler
			.same_allocation(context.lookup_function(path).unwrap()));
		assert_eq!(binding.signature.output, HostOutputKind::OpaqueOwned);
	}
}
