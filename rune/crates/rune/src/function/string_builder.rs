//! Certified constructors and methods of the installed std String provider.
use crate::Hash;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Operation {
	New,
	WithCapacity,
	PushStr,
	Capacity,
	Clear,
	Reserve,
	ReserveExact,
	ShrinkToFit,
}

impl Operation {
	pub(crate) const fn id(self) -> u32 {
		0x8000_0067 + self as u32
	}

	pub(crate) fn from_id(id: u32) -> Option<Self> {
		Some(match id {
			0x8000_0067 => Self::New,
			0x8000_0068 => Self::WithCapacity,
			0x8000_0069 => Self::PushStr,
			0x8000_006a => Self::Capacity,
			0x8000_006b => Self::Clear,
			0x8000_006c => Self::Reserve,
			0x8000_006d => Self::ReserveExact,
			0x8000_006e => Self::ShrinkToFit,
			_ => return None,
		})
	}

	pub(crate) const fn bit(self) -> u8 {
		1 << self as u8
	}

	pub(crate) const fn arity(self) -> u32 {
		match self {
			Self::New => 0,
			Self::WithCapacity | Self::Capacity | Self::Clear | Self::ShrinkToFit => 1,
			Self::PushStr | Self::Reserve | Self::ReserveExact => 2,
		}
	}

	pub(crate) fn from_name(name: &str) -> Option<Self> {
		Some(match name.as_bytes() {
			b"new" => Self::New,
			b"with_capacity" => Self::WithCapacity,
			b"push_str" => Self::PushStr,
			b"capacity" => Self::Capacity,
			b"clear" => Self::Clear,
			b"reserve" => Self::Reserve,
			b"reserve_exact" => Self::ReserveExact,
			b"shrink_to_fit" => Self::ShrinkToFit,
			_ => return None,
		})
	}

	pub(crate) fn single_provider_name(self) -> bool {
		matches!(
			self,
			Self::PushStr | Self::ShrinkToFit | Self::Reserve | Self::ReserveExact
		)
	}

	pub(crate) fn constructor(hash: Hash) -> Option<Self> {
		const NEW_ASSOC: Hash = crate::hash_in!(crate, ::std::string::String.new);
		const WITH_CAPACITY_ASSOC: Hash =
			crate::hash_in!(crate, ::std::string::String.with_capacity);
		const NEW_ITEM: Hash = crate::hash_in!(crate, ::std::string::String::new);
		const WITH_CAPACITY_ITEM: Hash =
			crate::hash_in!(crate, ::std::string::String::with_capacity);
		match hash {
			NEW_ITEM | NEW_ASSOC => Some(Self::New),
			WITH_CAPACITY_ITEM | WITH_CAPACITY_ASSOC => Some(Self::WithCapacity),
			_ => None,
		}
	}

	pub(crate) fn method(name: Hash) -> Option<Self> {
		const CAPACITY_ASSOC: Hash = crate::hash_in!(crate, ::std::string::String.capacity);
		const CLEAR_ASSOC: Hash = crate::hash_in!(crate, ::std::string::String.clear);
		const PUSH_STR_ASSOC: Hash = crate::hash_in!(crate, ::std::string::String.push_str);
		const RESERVE_ASSOC: Hash = crate::hash_in!(crate, ::std::string::String.reserve);
		const RESERVE_EXACT_ASSOC: Hash =
			crate::hash_in!(crate, ::std::string::String.reserve_exact);
		const SHRINK_TO_FIT_ASSOC: Hash =
			crate::hash_in!(crate, ::std::string::String.shrink_to_fit);
		let associated =
			Hash::associated_function(crate::hash_in!(crate, ::std::string::String), name);
		match associated {
			PUSH_STR_ASSOC => Some(Self::PushStr),
			CAPACITY_ASSOC => Some(Self::Capacity),
			CLEAR_ASSOC => Some(Self::Clear),
			RESERVE_ASSOC => Some(Self::Reserve),
			RESERVE_EXACT_ASSOC => Some(Self::ReserveExact),
			SHRINK_TO_FIT_ASSOC => Some(Self::ShrinkToFit),
			_ => None,
		}
	}
}
