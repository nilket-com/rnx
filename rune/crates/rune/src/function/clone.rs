//! Exact stock CLONE providers used by certified resize fill proofs.
use crate::runtime as rt;
use crate::Hash;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Provider {
	String,
	Vec,
	Object,
	Tuple,
	Option,
	Result,
	Function,
}
impl Provider {
	pub(crate) const ALL: [Self; 7] = [
		Self::String,
		Self::Vec,
		Self::Object,
		Self::Tuple,
		Self::Option,
		Self::Result,
		Self::Function,
	];
	pub(crate) const fn bit(self) -> u8 {
		1 << self as u8
	}
	pub(crate) const fn receiver(self) -> Hash {
		match self {
			Self::String => crate::hash_in!(crate, ::std::string::String),
			Self::Vec => crate::hash_in!(crate, ::std::vec::Vec),
			Self::Object => crate::hash_in!(crate, ::std::object::Object),
			Self::Tuple => crate::hash_in!(crate, ::std::tuple::Tuple),
			Self::Option => crate::hash_in!(crate, ::std::option::Option),
			Self::Result => crate::hash_in!(crate, ::std::result::Result),
			Self::Function => crate::hash_in!(crate, ::std::ops::Function),
		}
	}
	pub(crate) const fn protocol(self) -> Hash {
		Hash::new(
			0x5ea7_7ffb_cdf5_f302
				^ self.receiver().into_inner()
				^ rt::Protocol::CLONE.hash.into_inner(),
		)
	}
	pub(crate) fn unit_absent(self, items: &crate::function::methods::Instances) -> bool {
		items.complete() && items.get(self.protocol()).is_none()
	}
	pub(crate) fn from_receiver(hash: Hash) -> Option<Self> {
		Self::ALL.into_iter().find(|p| p.receiver() == hash)
	}
}

/// The std integer module registers its checked CLONE and its inherent pow
/// together. The existing pow certificate witnesses successful installation
/// of that module. The named alias must be that exact protocol handler.
pub(crate) fn integer_alias(context: &crate::Context, owner: Hash, alias: Hash) -> bool {
	use super::number::{Method, Pair, Receiver};
	let Some(receiver @ (Receiver::I64 | Receiver::U64)) = Receiver::from_hash(owner) else {
		return false;
	};
	let protocol = Hash::associated_function(owner, rt::Protocol::CLONE.hash);
	Pair::new(Method::Pow, receiver).is_some_and(|p| context.number_pair(p))
		&& context
			.lookup_function(protocol)
			.zip(context.lookup_function(alias))
			.is_some_and(|(original, named)| original.same_allocation(named))
}

pub(crate) const RESIZE: Hash = crate::hash_in!(crate, ::std::vec::Vec.resize);
pub(crate) const RESIZE_NAME: Hash =
	Hash::new(RESIZE.into_inner() ^ Provider::Vec.receiver().into_inner() ^ 0x5ea7_7ffb_cdf5_f302);

#[cfg(test)]
mod tests {
	use super::*;
	use crate::TypeHash;
	#[test]
	fn clone_provider_identities_match_runtime_hashes() {
		let runtime = [
			<crate::alloc::String>::HASH,
			rt::Vec::HASH,
			rt::Object::HASH,
			rt::OwnedTuple::HASH,
			<Option<rt::Value>>::HASH,
			<Result<rt::Value, rt::Value>>::HASH,
			rt::Function::HASH,
		];
		for (p, hash) in Provider::ALL.into_iter().zip(runtime) {
			assert_eq!(p.receiver(), hash);
			assert_eq!(
				p.protocol(),
				Hash::associated_function(hash, rt::Protocol::CLONE.hash)
			);
		}
		assert_eq!(RESIZE_NAME, Hash::ident("resize"));
		assert_eq!(
			RESIZE,
			Hash::associated_function(rt::Vec::HASH, Hash::ident("resize"))
		);
	}
	#[test]
	fn default_clone_providers_and_resize_are_certified() {
		let context = crate::Context::with_default_modules().unwrap();
		for p in Provider::ALL {
			assert!(context.clone_provider(p), "{p:?}");
		}
		assert!(context.vec_resize());
		let empty = crate::Context::new();
		for p in Provider::ALL {
			assert!(!empty.clone_provider(p));
		}
		assert!(!empty.vec_resize());
	}
}

#[cfg(test)]
mod unmarked_tests {
	use super::*;
	#[test]
	fn unmarked_clone_and_resize_providers_stay_uncertified() {
		let mut module = crate::Module::with_crate_item("std", ["vec"]).unwrap();
		module.ty::<rt::Vec>().unwrap();
		module
			.associated_function("resize", |_: &mut rt::Vec, _: usize, _: rt::Value| ())
			.unwrap();
		module
			.associated_function(&rt::Protocol::CLONE, |_: &rt::Vec| rt::Vec::new())
			.unwrap();
		let mut context = crate::Context::new();
		context.install(module).unwrap();
		assert!(context.lookup_function(RESIZE).is_some());
		assert!(context.lookup_function(Provider::Vec.protocol()).is_some());
		assert!(!context.vec_resize());
		assert!(!context.clone_provider(Provider::Vec));
	}
}

#[cfg(test)]
mod absence_tests {
	use super::*;
	#[test]
	fn clone_unit_absence_is_exact_and_fails_closed() {
		let p = Provider::Vec;
		let mut items = crate::function::methods::Instances::default();
		assert!(p.unit_absent(&items));
		let named = Hash::associated_function(p.receiver(), Hash::ident("clone"));
		assert_ne!(named, p.protocol());
		items.insert(named, Hash::EMPTY, 1);
		assert!(p.unit_absent(&items));
		items.insert(p.protocol(), Hash::EMPTY, 1);
		assert!(!p.unit_absent(&items));
		let mut incomplete = crate::function::methods::Instances::default();
		incomplete.mark_incomplete();
		assert!(!p.unit_absent(&incomplete));
	}
}

#[cfg(test)]
mod integer_alias_tests {
	use super::*;
	use crate::TypeHash;

	#[test]
	fn only_the_installed_integer_protocol_alias_gets_the_conversion() {
		let context = crate::Context::with_default_modules().unwrap();
		for owner in [i64::HASH, u64::HASH] {
			let named = Hash::associated_function(owner, Hash::ident("clone"));
			assert!(integer_alias(&context, owner, named));
			let other = Hash::associated_function(owner, Hash::ident("pow"));
			assert!(!integer_alias(&context, owner, other));
			assert!(!integer_alias(&crate::Context::new(), owner, named));
		}
	}
}
