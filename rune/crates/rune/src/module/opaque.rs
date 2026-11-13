//! Typed native-object enrollment; no owned or mutable receiver API is provided.
use super::*;
use crate::function::host::opaque::{Descriptor, NativeType, Registration};
use crate::function::host::{
	HostEnrollment, HostOutput, HostOutputKind, HostSignature, HostType, OutputCertificate,
};
use crate::function::Ref;
use crate::runtime::{Formatter, Value};

impl Module {
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
